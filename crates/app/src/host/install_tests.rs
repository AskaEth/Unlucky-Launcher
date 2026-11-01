//! host/install.rs 的单测（拆文件守 400 行上限）。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use unlucky_core::game::install::{FileOutcome, FileRole, FileStatus};

use super::{loader_label, run, supported, unsupported_reason, Job};
use crate::dto::{CreateRequest, LoaderKind};
use crate::host::env::HostEnv;
use crate::host::install_journal::Journal;
use crate::host::job::{channel, JobSender};
use crate::host::meter::Meter;

static SEQ: AtomicUsize = AtomicUsize::new(0);

/// 一个装在临时根里的任务上下文（只走「联网前就返回」的用例）。
struct Fixture {
    dir: PathBuf,
    env: HostEnv,
    tx: JobSender,
    meter: Arc<Mutex<Meter>>,
    cancel: Arc<AtomicBool>,
    journal: Journal,
}

impl Fixture {
    fn new(request: &CreateRequest) -> Self {
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("ull-host-install-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let env = HostEnv::at_root(&dir);
        let journal = Journal::open(&env);
        Self {
            dir,
            env,
            tx: channel().0,
            meter: Arc::new(Mutex::new(Meter::new(request))),
            cancel: Arc::new(AtomicBool::new(false)),
            journal,
        }
    }

    fn job(&self) -> Job<'_> {
        Job {
            env: &self.env,
            tx: &self.tx,
            meter: &self.meter,
            cancel: &self.cancel,
            journal: &self.journal,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn request(loader: LoaderKind, version: &str) -> CreateRequest {
    CreateRequest {
        version: version.to_string(),
        loader,
        loader_version: String::new(),
        name: String::new(),
    }
}

#[test]
fn loader_support_matrix_is_explicit() {
    // M45 语义变更：Forge/NeoForge 从「GUI 显式拒装」扩进受支持集合（走 installer processors）。
    for kind in [
        LoaderKind::Vanilla,
        LoaderKind::Fabric,
        LoaderKind::Quilt,
        LoaderKind::Forge,
        LoaderKind::NeoForge,
    ] {
        assert!(supported(kind), "{} 应当受支持", kind.label());
    }
    // 这两家 core 侧还没有安装管线，仍然显式拒装并说清去哪装。
    for kind in [LoaderKind::OptiFine, LoaderKind::LiteLoader] {
        assert!(!supported(kind), "{} 不该被当成可装", kind.label());
        let reason = unsupported_reason(kind);
        assert!(reason.contains(kind.label()), "{reason}");
        assert!(reason.contains("还没有接入"), "{reason}");
    }
}

#[test]
fn forge_install_is_no_longer_refused_at_the_door() {
    // 旧文案是把用户赶去终端跑 `ull install`；M45 之后 GUI 与 CLI 同一条路径。
    assert!(supported(LoaderKind::Forge));
    assert!(supported(LoaderKind::NeoForge));
    assert_eq!(loader_label(LoaderKind::Forge, "47.2.0"), "Forge 47.2.0");
    assert_eq!(
        loader_label(LoaderKind::NeoForge, "21.1.80"),
        "NeoForge 21.1.80"
    );
}

#[test]
fn meter_counts_files_and_skips_skipped_bytes() {
    let request = CreateRequest {
        version: "1.20.4".to_string(),
        loader: LoaderKind::Fabric,
        loader_version: "0.16.5".to_string(),
        name: "生存".to_string(),
    };
    let mut meter = Meter::new(&request);
    assert_eq!(meter.progress.loader, "Fabric 0.16.5");
    assert_eq!(meter.progress.instance, "生存");
    meter.progress.files_total = 2;

    let mut outcome = FileOutcome {
        role: FileRole::Library,
        name: "a.jar".to_string(),
        dest: PathBuf::from("a.jar"),
        status: FileStatus::Downloaded,
        bytes: 100,
        source: Some("bmclapi".to_string()),
        failure: None,
    };
    meter.note(&outcome);
    assert_eq!(meter.progress.files_done, 1);
    assert_eq!(meter.progress.bytes_done, 100);

    outcome.status = FileStatus::Skipped;
    outcome.bytes = 500;
    meter.note(&outcome);
    assert_eq!(meter.progress.files_done, 2);
    assert_eq!(meter.progress.bytes_done, 100, "跳过的文件不计入已下载字节");
    assert!((meter.progress.ratio() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn empty_version_is_rejected_before_network() {
    let request = request(LoaderKind::Vanilla, "");
    let fixture = Fixture::new(&request);
    let err = run(&fixture.job(), &request).unwrap_err();
    assert!(err.contains("没有选择版本"), "{err}");
}

#[test]
fn unsupported_loader_is_rejected_before_network() {
    let request = request(LoaderKind::OptiFine, "1.20.1");
    let fixture = Fixture::new(&request);
    let err = run(&fixture.job(), &request).unwrap_err();
    assert!(err.contains("还没有接入"), "{err}");
}
