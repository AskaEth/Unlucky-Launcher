//! host/install.rs 的单测（拆文件守 400 行上限）。

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use unlucky_core::game::install::{FileOutcome, FileRole, FileStatus};

use super::{run, supported, unsupported_reason};
use crate::dto::{CreateRequest, LoaderKind};
use crate::host::env::HostEnv;
use crate::host::meter::Meter;

#[test]
fn loader_support_matrix_is_explicit() {
    assert!(supported(LoaderKind::Vanilla));
    assert!(supported(LoaderKind::Fabric));
    assert!(supported(LoaderKind::Quilt));
    assert!(!supported(LoaderKind::Forge));
    assert!(!supported(LoaderKind::NeoForge));
    assert!(unsupported_reason(LoaderKind::Forge).contains("ull install"));
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
    let dir = std::env::temp_dir().join(format!("ull-host-install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = HostEnv::at_root(&dir);
    let (tx, _rx) = super::super::job::channel();
    let request = CreateRequest {
        version: String::new(),
        loader: LoaderKind::Vanilla,
        loader_version: String::new(),
        name: String::new(),
    };
    let meter = Arc::new(std::sync::Mutex::new(Meter::new(&request)));
    let cancel = Arc::new(AtomicBool::new(false));
    let err = run(&env, &tx, &request, &meter, &cancel).unwrap_err();
    assert!(err.contains("没有选择版本"), "{err}");

    let forge = CreateRequest {
        version: "1.20.1".to_string(),
        loader: LoaderKind::Forge,
        ..request
    };
    let meter = Arc::new(std::sync::Mutex::new(Meter::new(&forge)));
    let cancel = Arc::new(AtomicBool::new(false));
    assert!(run(&env, &tx, &forge, &meter, &cancel)
        .unwrap_err()
        .contains("installer processors"));
    std::fs::remove_dir_all(&dir).ok();
}
