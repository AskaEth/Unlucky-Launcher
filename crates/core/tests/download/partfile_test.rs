//! `download::atomic::PartFile`：中间文件的进程/实例隔离、续传收养与孤儿清理。
//!
//! 背景（M35 真机 finding）：中间文件路径原先只由目标路径决定，两个启动器进程装同一
//! 数据根时共用一个 `.ullpart`，先提交的一方 `rename` 走，另一方 `os error 2`
//! （实测两次 `ull install` 各失败 819 / 1116 个资源）。这里守住三条：名字带会话标识、
//! 旧材料能收养、孤儿清理不许误删有人在用的中间文件。

mod support;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use unlucky_core::download::backoff::BackoffPolicy;
use unlucky_core::download::cache::ContentCache;
use unlucky_core::download::chunked::ChunkedConfig;
use unlucky_core::download::config::EngineConfig;
use unlucky_core::download::engine::DownloadEngine;
use unlucky_core::download::health::HealthRegistry;
use unlucky_core::download::mirrors::MirrorTable;
use unlucky_core::download::region::RegionProfile;
use unlucky_core::download::types::{DownloadRequest, ResourceKind};
use unlucky_core::download::{PartFile, ADOPT_MIN_AGE, META_SUFFIX, ORPHAN_MAX_AGE, PART_SUFFIX};

use support::{age_file, part_files, sha1_hex, temp_dir, MockResponse, MockServer};

/// 另一个会话号（保证与本进程不同）。
fn foreign_session() -> u32 {
    std::process::id() ^ 1
}

fn part_name(name: &str, session: u32, seq: u64) -> String {
    format!("{name}{PART_SUFFIX}-{session}-{seq}")
}

fn write_part(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, b"0123456789").expect("写中间文件失败");
    path
}

#[test]
fn part_file_names_carry_session_and_sequence() {
    let dir = temp_dir("partfile-identity");
    let dest = dir.join("client.jar");

    let first = PartFile::for_dest(&dest);
    let second = PartFile::for_dest(&dest);

    assert_ne!(
        first.path(),
        second.path(),
        "同一 dest 的两个守卫不能共用文件"
    );
    assert_eq!(first.path().parent(), Some(dir.as_path()));
    let name = first.path().file_name().unwrap().to_string_lossy();
    assert!(
        name.starts_with(&format!("client.jar{PART_SUFFIX}-")),
        "中间文件名要带进程标识：{name}"
    );
    assert!(
        name.contains(&std::process::id().to_string()),
        "进程号要在名字里（跨进程隔离的依据）：{name}"
    );
}

#[test]
fn two_threads_writing_the_same_dest_commit_independently() {
    let dir = temp_dir("partfile-threads");
    let dest = dir.join("client.jar");

    let handles: Vec<_> = (0..2_u8)
        .map(|index| {
            let dest = dest.clone();
            std::thread::spawn(move || {
                let payload = vec![b'a' + index; 4096];
                let part = PartFile::for_dest(&dest);
                part.open_truncate()
                    .expect("打开中间文件失败")
                    .write_all(&payload)
                    .expect("写中间文件失败");
                part.commit(&dest).expect("原子落盘失败");
                payload
            })
        })
        .collect();

    let payloads: Vec<Vec<u8>> = handles
        .into_iter()
        .map(|handle| handle.join().expect("下载线程 panic"))
        .collect();
    let written = fs::read(&dest).expect("读落盘文件失败");
    assert!(
        payloads.contains(&written),
        "落盘内容必须是某个线程的完整产物"
    );
    assert!(
        part_files(&dir).is_empty(),
        "两线程各自的中间文件都要被提交掉：{:?}",
        part_files(&dir)
    );
}

#[test]
fn legacy_part_file_is_adopted_for_resume() {
    let dir = temp_dir("partfile-legacy");
    let dest = dir.join("client.jar");
    let legacy = write_part(&dir, &format!("client.jar{PART_SUFFIX}"));
    let legacy_meta = dir.join(format!("client.jar{PART_SUFFIX}{META_SUFFIX}"));
    fs::write(&legacy_meta, b"{}").expect("写元数据失败");

    let part = PartFile::for_dest(&dest);

    assert_ne!(part.path(), legacy.as_path(), "新守卫要换自己的名字");
    assert_eq!(part.len(), 10, "旧版中间文件必须被收养（续传语义不变）");
    assert!(!legacy.exists(), "收养后不许留下旧名字");
    assert!(!legacy_meta.exists(), "元数据要跟着中间文件一起搬");
    assert!(part.meta_path().is_file(), "收养后的元数据路径要就位");
}

#[test]
fn foreign_part_file_is_adopted_only_after_it_cools_down() {
    let dir = temp_dir("partfile-foreign");
    let dest = dir.join("client.jar");
    let foreign = write_part(&dir, &part_name("client.jar", foreign_session(), 1));

    let fresh = PartFile::for_dest(&dest);
    assert!(!fresh.exists(), "外进程刚写过的中间文件不能抢");
    assert!(foreign.is_file(), "更不能删（可能正被那个进程写着）");

    age_file(&foreign, ADOPT_MIN_AGE + Duration::from_secs(60));
    let adopted = PartFile::for_dest(&dest);
    assert_eq!(adopted.len(), 10, "静置够久的外来中间文件应被收养续传");
    assert!(!foreign.exists(), "收养后旧名字应消失");
}

#[test]
fn sweeps_ownerless_orphans_but_keeps_fresh_and_live_files() {
    let dir = temp_dir("partfile-sweep");
    let stale = write_part(&dir, &format!("other.jar{PART_SUFFIX}"));
    let stale_meta = dir.join(format!("other.jar{PART_SUFFIX}{META_SUFFIX}"));
    fs::write(&stale_meta, b"{}").expect("写元数据失败");
    age_file(&stale, ORPHAN_MAX_AGE + Duration::from_secs(60));
    let fresh = write_part(&dir, &format!("fresh.jar{PART_SUFFIX}"));

    let _guard = PartFile::for_dest(&dir.join("client.jar"));

    assert!(!stale.exists(), "无主且超龄的中间文件应被清理");
    assert!(!stale_meta.exists(), "它的元数据也要一起清");
    assert_eq!(
        fs::read(&fresh).unwrap().len(),
        10,
        "还新鲜的无主文件不能删"
    );
}

#[test]
fn keeps_another_live_guard_of_the_same_dest() {
    let dir = temp_dir("partfile-live");
    let dest = dir.join("client.jar");
    let running = PartFile::for_dest(&dest);
    running
        .open_truncate()
        .expect("打开中间文件失败")
        .write_all(&[b'x'; 1024])
        .expect("写中间文件失败");

    let other = PartFile::for_dest(&dest);

    assert!(
        running.path().is_file(),
        "有人在用的中间文件不能被清理或收养"
    );
    assert_eq!(running.len(), 1024);
    assert_ne!(other.path(), running.path());
}

fn fast_engine(cache: &Path) -> DownloadEngine {
    DownloadEngine::new(
        EngineConfig {
            max_attempts_per_source: 1,
            backoff: BackoffPolicy {
                base_ms: 1,
                max_ms: 2,
                jitter_ratio: 0.0,
                max_attempts: 1,
            },
            read_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(5),
            chunked: ChunkedConfig::disabled(),
            ..EngineConfig::default()
        },
        Arc::new(MirrorTable::builtin()),
        Arc::new(RegionProfile::direct(0)),
        Arc::new(HealthRegistry::in_memory()),
        Arc::new(ContentCache::open(cache)),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_engines_downloading_the_same_dest_do_not_share_parts() {
    let payload = b"same destination payload".to_vec();
    let server = MockServer::start(vec![
        MockResponse::ok(payload.clone()),
        MockResponse::ok(payload.clone()),
    ])
    .await;
    let url = server.url("/client.jar");
    let dir = temp_dir("partfile-engine");
    let dest = dir.join("client.jar");
    // 两份独立的缓存目录：否则后到的那次会直接吃内容缓存，绕过中间文件路径。
    let left = fast_engine(&dir.join("cache-a"));
    let right = fast_engine(&dir.join("cache-b"));
    let request = || {
        DownloadRequest::new(ResourceKind::ClientJar, url.as_str(), &dest)
            .with_sha1(sha1_hex(&payload))
            .with_size(payload.len() as u64)
    };

    let (first, second) = tokio::join!(left.download(request()), right.download(request()));

    first.expect("并行下载 A 失败");
    second.expect("并行下载 B 失败");
    assert_eq!(fs::read(&dest).expect("读落盘文件失败"), payload);
    assert!(
        part_files(&dir).is_empty(),
        "并发下载不能留下中间文件：{:?}",
        part_files(&dir)
    );
    server.stop();
}
