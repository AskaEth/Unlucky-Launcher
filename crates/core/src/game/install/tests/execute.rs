//! 落盘执行：跳过判定（三级）、幂等重装、预取元数据落盘、失败分类聚合。
//!
//! 全部用例都**已把文件放好**或**只做纯函数判定**，因此不发网络请求。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::download::verify;
use crate::download::ResourceKind;
use crate::download::{
    ContentCache, DownloadEngine, DownloadErrorKind, EngineConfig, HashAlgo, HealthRegistry,
    MirrorTable, RegionProfile,
};
use crate::game::arguments::Environment;
use crate::game::install::assets::{AssetIndex, AssetObject};
use crate::game::install::execute::{current_bytes, execute, FileOutcome, FileStatus};
use crate::game::install::plan::{FileRole, InstallPlan, PlanFile, PlanInput};
use crate::game::layout::GameLayout;
use crate::game::manifest::VersionJson;

fn temp_root(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "ull-install-exec-{}-{tag}-{unique}",
        std::process::id()
    ))
}

fn engine(root: &Path) -> DownloadEngine {
    DownloadEngine::new(
        EngineConfig::default(),
        Arc::new(MirrorTable::builtin()),
        Arc::new(RegionProfile::direct(0)),
        Arc::new(HealthRegistry::in_memory()),
        Arc::new(ContentCache::open(root.join("cache"))),
    )
}

fn sha1_of(bytes: &[u8]) -> String {
    verify::digest_bytes(HashAlgo::Sha1, bytes)
}

fn write_file(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().expect("有父目录")).expect("建目录");
    std::fs::write(path, bytes).expect("写文件");
}

#[test]
fn current_bytes_matches_by_sha1_then_size_then_existence() {
    let root = temp_root("current");
    let payload = b"client jar payload";
    let dest: PathBuf = root.join("versions/1.20.1/1.20.1.jar");
    write_file(&dest, payload);

    let with_sha1 = PlanFile {
        role: FileRole::ClientJar,
        name: "1.20.1".to_string(),
        url: "https://example.invalid/x.jar".to_string(),
        sha1: Some(sha1_of(payload)),
        size: Some(payload.len() as u64),
        dest: dest.clone(),
        resource: ResourceKind::ClientJar,
    };
    assert_eq!(current_bytes(&with_sha1), Some(payload.len() as u64));

    let wrong_sha1 = PlanFile {
        sha1: Some("0".repeat(40)),
        ..with_sha1.clone()
    };
    assert_eq!(current_bytes(&wrong_sha1), None, "sha1 不符必须重下");

    let size_only = PlanFile {
        sha1: None,
        size: Some(payload.len() as u64),
        ..with_sha1.clone()
    };
    assert_eq!(current_bytes(&size_only), Some(payload.len() as u64));

    let size_mismatch = PlanFile {
        sha1: None,
        size: Some(1),
        ..with_sha1.clone()
    };
    assert_eq!(current_bytes(&size_mismatch), None);

    let bare = PlanFile {
        sha1: None,
        size: None,
        ..with_sha1.clone()
    };
    assert_eq!(current_bytes(&bare), Some(payload.len() as u64), "存在即信");

    let missing = PlanFile {
        dest: root.join("nope.jar"),
        ..with_sha1
    };
    assert_eq!(current_bytes(&missing), None);
    std::fs::remove_dir_all(&root).ok();
}

/// 一份"客户端 jar 已存在"的清单 + 计划，用于幂等与跳过用例。
fn plan_with_existing_jar(root: &Path) -> (InstallPlan, Vec<u8>) {
    let payload = b"already installed client jar".to_vec();
    let layout = GameLayout::new(root);
    let sha1 = sha1_of(&payload);
    write_file(&layout.version_jar("1.20.1"), &payload);

    let resolved = VersionJson::from_json_str(&format!(
        r#"{{"id": "1.20.1", "mainClass": "net.minecraft.client.main.Main",
            "downloads": {{"client": {{"sha1": "{sha1}", "size": {}, "url": "https://piston-data.mojang.com/v1/objects/{sha1}/client.jar"}}}}}}"#,
        payload.len()
    ))
    .expect("清单应可解析");

    let plan = InstallPlan::build(PlanInput {
        layout: &layout,
        version_id: "1.20.1",
        resolved: &resolved,
        asset_index: None,
        environment: &Environment::host(),
    })
    .expect("组计划应成功");
    (plan, payload)
}

#[tokio::test]
async fn skips_existing_files_and_writes_version_json() {
    let root = temp_root("skip");
    let (plan, payload) = plan_with_existing_jar(&root);
    let engine = engine(&root);
    let calls = Arc::new(AtomicUsize::new(0));
    let sink_calls = Arc::clone(&calls);

    let report = execute(
        &engine,
        &plan,
        4,
        Some(Arc::new(move |_outcome: &FileOutcome| {
            sink_calls.fetch_add(1, Ordering::Relaxed);
        })),
    )
    .await
    .expect("执行应成功");

    assert!(report.is_complete());
    assert_eq!(report.downloaded(), 0, "已存在且校验通过的文件不该重下");
    assert_eq!(report.skipped(), 1);
    assert_eq!(report.bytes(), 0);
    assert_eq!(calls.load(Ordering::Relaxed), 1, "每个文件回调一次");

    // 版本清单必须落盘且能被重新解析。
    let version_json = report.version_json.clone().expect("全量成功应写版本清单");
    let text = std::fs::read_to_string(&version_json).expect("版本清单应已写盘");
    let written = VersionJson::from_json_str(&text).expect("写出的清单应可解析");
    assert_eq!(written.id.as_deref(), Some("1.20.1"));
    assert_eq!(
        written.main_class.as_deref(),
        Some("net.minecraft.client.main.Main")
    );

    // 盘上内容没被改动（跳过 = 不动文件）。
    assert_eq!(
        std::fs::read(GameLayout::new(&root).version_jar("1.20.1")).expect("读 jar"),
        payload
    );
    std::fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn reinstalling_twice_is_idempotent() {
    let root = temp_root("idempotent");
    let (plan, _) = plan_with_existing_jar(&root);
    let engine = engine(&root);
    let first = execute(&engine, &plan, 2, None).await.expect("第一次");
    let second = execute(&engine, &plan, 2, None).await.expect("第二次");

    assert!(first.is_complete() && second.is_complete());
    assert_eq!(second.downloaded(), 0);
    assert_eq!(second.skipped(), second.files.len());
    std::fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn prefetched_metadata_is_written_without_network() {
    let root = temp_root("prefetch");
    let layout = GameLayout::new(&root);
    let hash = "0f9d6b1c0d5f4e3b2a1908070605040302010f0e";
    // 规划期"已经下载到内存"的资源索引字节（sha1 必须与清单声明一致，否则跳过判定不成立）。
    let index_bytes = br#"{"objects":{}}"#.to_vec();
    let index_sha1 = sha1_of(&index_bytes);
    let resolved = VersionJson::from_json_str(&format!(
        r#"{{"id": "1.20.1", "assetIndex": {{"id": "17", "sha1": "{index_sha1}", "size": {}, "url": "https://piston-meta.mojang.com/v1/packages/{index_sha1}/17.json"}}}}"#,
        index_bytes.len()
    ))
    .expect("清单应可解析");
    let mut index = AssetIndex::default();
    index.objects.insert(
        "minecraft/lang/zh_cn.json".to_string(),
        AssetObject {
            hash: hash.to_string(),
            size: 3,
        },
    );

    let mut plan = InstallPlan::build(PlanInput {
        layout: &layout,
        version_id: "1.20.1",
        resolved: &resolved,
        asset_index: Some(&index),
        environment: &Environment::host(),
    })
    .expect("组计划应成功");

    let index_dest = layout.asset_index("17");
    plan.prefetch(index_dest.clone(), index_bytes.clone());

    // 只跑索引这一个文件：把资源对象从计划里摘掉，避免真发网络请求。
    plan.files.retain(|file| file.role != FileRole::Asset);
    let engine = engine(&root);
    let report = execute(&engine, &plan, 2, None).await.expect("执行应成功");

    assert!(report.is_complete(), "不该有失败：{:?}", report.failures(3));
    assert_eq!(report.downloaded(), 1);
    assert_eq!(
        std::fs::read(&index_dest).expect("索引应已写盘"),
        index_bytes
    );
    let outcome = report
        .files
        .iter()
        .find(|file| file.dest == index_dest)
        .expect("应有索引结果");
    assert_eq!(outcome.source.as_deref(), Some("metadata"));

    // 重跑：索引已在盘上且 sha1 一致 → 跳过，不重写、不算"下载"。
    let again = execute(&engine, &plan, 2, None).await.expect("第二次");
    assert_eq!(again.downloaded(), 0, "重装不该再写索引");
    assert_eq!(again.skipped(), 1);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn failure_kind_prefers_integrity() {
    let failed = |kind: DownloadErrorKind| FileOutcome {
        role: FileRole::Asset,
        name: "x".to_string(),
        dest: PathBuf::from("/x"),
        status: FileStatus::Failed,
        bytes: 0,
        source: None,
        failure: Some(crate::game::install::FileFailure {
            kind,
            message: "boom".to_string(),
        }),
    };
    let report = crate::game::install::InstallReport {
        files: vec![
            failed(DownloadErrorKind::AllSourcesFailed),
            failed(DownloadErrorKind::Integrity),
        ],
        version_json: None,
        warnings: Vec::new(),
    };
    assert_eq!(report.failed(), 2);
    assert!(!report.is_complete());
    assert_eq!(report.failure_kind(), Some(DownloadErrorKind::Integrity));
    assert_eq!(report.failures(1).len(), 1);
}
