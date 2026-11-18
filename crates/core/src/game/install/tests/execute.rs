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
use crate::game::install::execute::{
    current_bytes, execute, execute_with_cancel, FileOutcome, FileStatus,
};
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

/// **取消：一个文件都不下、版本清单不落盘**（M105 的「半成品不许变成品」）。
///
/// 标志**开工前**就置位 ⇒ 走「一个文件都不起」那条路，全程零网络（所以这条能进 CI，
/// 真网络的那条在 `crates/presenter/src/host/cancel_tests.rs`，`#[ignore]` 守着）。
///
/// 抓得住：取消返回 `Cancelled` 大类、`version_json` 没写、盘上没多出 jar。
/// 抓不住：在飞的文件被掐断的那一刻（那要真网络 + 真字节流，见 presenter 的实测装置）。
#[tokio::test]
async fn cancelling_before_the_first_file_downloads_nothing_and_writes_no_manifest() {
    let root = temp_root("cancel");
    let layout = GameLayout::new(&root);
    // jar **不在**盘上：真跑就会去下（本用例不许它跑起来）
    let sha1 = "3f".repeat(20);
    let resolved = VersionJson::from_json_str(&format!(
        r#"{{"id": "1.20.1", "mainClass": "net.minecraft.client.main.Main",
            "downloads": {{"client": {{"sha1": "{sha1}", "size": 32, "url": "https://piston-data.mojang.com/v1/objects/{sha1}/client.jar"}}}}}}"#
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

    let cancel = std::sync::atomic::AtomicBool::new(true);
    let error = execute_with_cancel(&engine(&root), &plan, 4, None, Some(&cancel))
        .await
        .expect_err("取消了就不要返回 report");
    assert_eq!(
        error.kind(),
        crate::game::install::InstallErrorKind::Cancelled,
        "取消是独立大类，不是网络/IO 失败"
    );
    assert!(
        !plan.version_json_path.is_file(),
        "取消后不许落版本清单（否则半成品会被当成已安装）"
    );
    assert!(!layout.version_jar("1.20.1").is_file(), "一个文件都不该下");
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
        manifest: None,
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

/// **取消之后，剩下的文件连 `install_one` 都不许进**（复审 P2-1 的守门，p1）。
///
/// 现场（修前）：标志检查只写在 `select` 的右支，而 `select` 先轮询左支 —— 左支第一次 poll
/// 就把 `engine.download(…)` 派出去了。于是取消后 `buffer_unordered` 立刻让出槽位、流继续被拉，
/// **每一个剩余 item 都会各发一次整文件 sha1 + 一次请求**，1.20.1 那种四位数文件的 plan 会一直
/// 走到列表末尾；盘上已就绪的文件第一 poll 就 `Ready` ⇒ 还会各回调一次 `sink`。
///
/// 这条用例把它钉成可数的东西：三个文件全在盘上（`install_one` 直接判 Skipped、零网络），
/// 并发 1，`sink` 第一次回调时置取消标志 —— 于是「取消后还剩几个文件被处理」= `sink` 的调用次数。
/// 修好后必须是 **1**（只有第一个），修前是 **3**。
///
/// 抓得住：`async move` 开头那句 `if is_cancelled(cancel) { return None; }` 被删掉/挪到 `select` 右支。
/// 抓不住：在飞的那一个文件被掐断的时机（那要真字节流，见 presenter 的真网络装置）。
#[tokio::test]
async fn cancelling_mid_run_stops_creating_remaining_files() {
    let root = temp_root("cancel-mid");
    let layout = GameLayout::new(&root);
    let jar = b"client jar payload".to_vec();
    let sha1 = sha1_of(&jar);
    write_file(&layout.version_jar("1.20.1"), &jar);
    let resolved = VersionJson::from_json_str(&format!(
        r#"{{"id": "1.20.1", "mainClass": "net.minecraft.client.main.Main",
            "downloads": {{"client": {{"sha1": "{sha1}", "size": {}, "url": "https://piston-data.mojang.com/v1/objects/{sha1}/client.jar"}}}}}}"#,
        jar.len()
    ))
    .expect("清单应可解析");
    let mut plan = InstallPlan::build(PlanInput {
        layout: &layout,
        version_id: "1.20.1",
        resolved: &resolved,
        asset_index: None,
        environment: &Environment::host(),
    })
    .expect("组计划应成功");

    // 再挂两个"已在盘上"的库文件：内容与声明的 sha1 一致 ⇒ 跳过判定成立、零网络。
    for name in ["org.example:a:1.0", "org.example:b:1.0"] {
        let bytes = format!("payload for {name}").into_bytes();
        let dest = root
            .join("libraries")
            .join(format!("{}.jar", name.replace(':', "-")));
        write_file(&dest, &bytes);
        plan.files.push(PlanFile {
            role: FileRole::Library,
            name: name.to_string(),
            url: format!("https://example.invalid/{name}.jar"),
            sha1: Some(sha1_of(&bytes)),
            size: Some(bytes.len() as u64),
            dest,
            resource: ResourceKind::Library,
        });
    }
    assert_eq!(plan.files.len(), 3, "三个文件全在盘上 ⇒ 全程零网络");

    let seen = Arc::new(AtomicUsize::new(0));
    // `sink` 是 `Fn + 'static`，所以标志也必须 `Arc` 进来；第一次回调就置取消。
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let sink_seen = Arc::clone(&seen);
    let sink_flag = Arc::clone(&flag);
    let sink: crate::game::install::ProgressSink = Arc::new(move |_outcome: &FileOutcome| {
        sink_seen.fetch_add(1, Ordering::SeqCst);
        sink_flag.store(true, Ordering::SeqCst);
    });

    let error = execute_with_cancel(&engine(&root), &plan, 1, Some(sink), Some(flag.as_ref()))
        .await
        .expect_err("取消之后不许返回成功 report");
    assert_eq!(
        error.kind(),
        crate::game::install::InstallErrorKind::Cancelled
    );
    assert_eq!(
        seen.load(Ordering::SeqCst),
        1,
        "取消只该让**第一个**文件走完回调；剩下的 item 必须连 install_one 都不进\
         （修前这里会是 3：每个剩余文件各多算一次 sha1、各多发一次请求）"
    );
    assert!(!plan.version_json_path.is_file(), "取消后不许落版本清单");
    std::fs::remove_dir_all(&root).ok();
}
