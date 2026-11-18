//! M321 r1：**装载器段的逐文件进度**（`run_plan` 的 sink 接线 + `file_progress` 映射）。
//!
//! 复审 r1 P2-1：`execute_with_cancel` 的 `sink` 形参一直传 `None`，于是原版 jar+assets
//! （M320 实测 ≈6.1 min）与 loader 库下载零输出。这里两条断言：
//!
//! | 用例 | 抓得住什么（变异） |
//! |------|--------------------|
//! | [`run_plan_feeds_the_loader_stage_progress_from_the_job`] | `run_plan` 里 `sink` 改回 `None`，或适配器没从 `job.progress` 取 ⇒ 一条 `ImportProgress` 都收不到 |
//! | [`file_progress_counts_and_names_every_file`] | `done` 不递增 / `current` 不填 / 没有观测者却造 sink |
//!
//! **全离线**：计划里那个文件的 `dest` 已经存在 ⇒ `execute_with_cancel` 走「已就绪」快路径，
//! 只回调一次、不建下载请求、不开 socket（引擎用 `game::install::tests::execute` 那套出厂件）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::addon::import::cancel::ImportCancel;
use crate::addon::import::report::{ImportProgress, ImportProgressSink, ImportStage};
use crate::addon::import::LoaderTarget;
use crate::addon::loader::LoaderKind;
use crate::download::blocking::block_on;
use crate::download::{
    ContentCache, DownloadEngine, EngineConfig, HealthRegistry, MirrorTable, RegionProfile,
    ResourceKind,
};
use crate::game::install::execute::{FileOutcome, FileStatus};
use crate::game::install::plan::{FileRole, InstallPlan, PlanFile};
use crate::game::layout::GameLayout;
use crate::game::manifest::VersionJson;

use super::super::engine::{file_progress, run_plan};
use super::super::LoaderInstallJob;

/// 录下来的一条进度：`(done, total, current)`。
type Seen = Arc<Mutex<Vec<(usize, usize, Option<String>)>>>;

// ============================ 夹具 ============================

fn temp_root(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "ull-import-progress-{}-{tag}-{unique}",
        std::process::id()
    ))
}

/// 出厂引擎（与 `game/install/tests/execute.rs` 同一套；**没有任何网络在飞**）。
fn engine(root: &Path) -> DownloadEngine {
    DownloadEngine::new(
        EngineConfig::default(),
        Arc::new(MirrorTable::builtin()),
        Arc::new(RegionProfile::direct(0)),
        Arc::new(HealthRegistry::in_memory()),
        Arc::new(ContentCache::open(root.join("cache"))),
    )
}

/// 一份只有一个文件的安装计划，且那个文件**已经在盘上**（摘要按大小兜底 ⇒ 走"已就绪"）。
fn ready_plan(root: &Path) -> InstallPlan {
    let dest = root.join("versions/1.20.1/1.20.1.jar");
    std::fs::create_dir_all(dest.parent().expect("有父目录")).expect("建目录");
    std::fs::write(&dest, b"payload").expect("写 jar");
    InstallPlan {
        version_id: "1.20.1".to_string(),
        jar_id: "1.20.1".to_string(),
        game_root: root.to_path_buf(),
        version_json_path: root.join("versions/1.20.1/1.20.1.json"),
        files: vec![PlanFile {
            role: FileRole::ClientJar,
            name: "1.20.1".to_string(),
            url: "https://example.invalid/never-fetched.jar".to_string(),
            sha1: None,
            size: Some(7),
            dest,
            resource: ResourceKind::ClientJar,
        }],
        version_json: VersionJson::from_json_str(r#"{"id":"1.20.1","mainClass":"a.B"}"#)
            .expect("版本 JSON"),
        prefetched: Vec::new(),
        warnings: Vec::new(),
    }
}

fn job<'a>(
    layout: &'a GameLayout,
    target: &'a LoaderTarget,
    cancel: &'a ImportCancel,
    progress: Option<ImportProgressSink>,
) -> LoaderInstallJob<'a> {
    LoaderInstallJob {
        layout,
        target,
        cancel,
        steps: None,
        progress,
    }
}

fn target() -> LoaderTarget {
    LoaderTarget {
        kind: LoaderKind::Forge,
        mc_version: "1.20.1".to_string(),
        loader_version: "47.1.39".to_string(),
        raw: "forge-47.1.39".to_string(),
        version_id: "1.20.1-forge-47.1.39".to_string(),
    }
}

// ============================ 用例 ============================

/// `run_plan` 把 `job.progress` 变成执行器的 `ProgressSink`（**整条链**都有断言）。
///
/// 变异：`run_plan` 里 `let sink = file_progress(...)` 改回 `None` ⇒ 记录器空 ⇒ 红；
/// 适配器不看 `job.progress`（传 `&None`）⇒ 同上。
#[test]
fn run_plan_feeds_the_loader_stage_progress_from_the_job() {
    let root = temp_root("run-plan");
    let plan = ready_plan(&root);
    let game = GameLayout::new(root.join("game"));
    let cancel = ImportCancel::default();
    let target = target();

    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&seen);
    let sink: ImportProgressSink = Arc::new(move |progress: &ImportProgress| {
        recorder.lock().expect("锁").push((
            progress.done,
            progress.total,
            progress.current.clone(),
        ));
        assert_eq!(progress.stage, ImportStage::Loader, "进度必须挂在装载器段");
    });
    let job = job(&game, &target, &cancel, Some(sink));

    let report = block_on(run_plan(&engine(&root), &plan, &job)).expect("已就绪的计划不该失败");
    assert!(report.is_complete(), "{:?}", report.failures(3));

    assert_eq!(
        seen.lock().expect("锁").clone(),
        vec![(1, 1, Some("1.20.1".to_string()))],
        "逐文件进度必须从 job.progress 走到执行器（改回 None ⇒ 一条都没有）"
    );
}

/// 适配器本身的三个字段：`done` 递增、`total` 是计划文件数、`current` 是文件名。
#[test]
fn file_progress_counts_and_names_every_file() {
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&seen);
    let progress: ImportProgressSink = Arc::new(move |event: &ImportProgress| {
        recorder
            .lock()
            .expect("锁")
            .push((event.done, event.total, event.current.clone()));
    });
    let sink = file_progress(&Some(progress), 3).expect("有观测者就必须造出 sink");

    for name in ["a.jar", "b.jar"] {
        sink(&FileOutcome {
            role: FileRole::Library,
            name: name.to_string(),
            dest: PathBuf::from(name),
            status: FileStatus::Skipped,
            bytes: 0,
            source: None,
            failure: None,
        });
    }

    assert_eq!(
        seen.lock().expect("锁").clone(),
        vec![
            (1, 3, Some("a.jar".to_string())),
            (2, 3, Some("b.jar".to_string())),
        ]
    );
    // 没有观测者 ⇒ 不造 sink（引擎侧就不会有回调开销）。
    assert!(file_progress(&None, 3).is_none());
}
