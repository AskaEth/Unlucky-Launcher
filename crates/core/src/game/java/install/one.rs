//! 回退链里的「一家」怎么装：下载 → 解压 → 探测，外加幂等复用。
//!
//! 从 `install.rs` 拆出来是为了守住 400 行红线（`guards.yml` 的 `file-size`）：
//! `install.rs` 只保留「按优先级链挑一家 / 记降级原因」的编排，这里放单家的机械步骤。

use std::path::PathBuf;

use super::super::error::{io_error, JavaError, JavaErrorKind};
use super::super::extract;
use super::super::location::JavaInstallRoot;
use super::super::probe::{self, JavaOrigin, JavaRuntime};
use super::super::release::JdkRelease;
use super::{archive_path, archive_request};
use crate::download::DownloadEngine;

/// 某一家的中间结果（把七元组元组换成具名结构，读起来才不费劲）。
pub(super) struct Prepared {
    pub(super) root: PathBuf,
    pub(super) archive: PathBuf,
    pub(super) runtime: JavaRuntime,
    pub(super) downloaded: Option<String>,
    pub(super) bytes: u64,
    pub(super) files: usize,
    pub(super) extracted: bool,
}

/// 装某一家的那一份（下载 → 解压 → 探测）。
pub(super) async fn install_one(
    engine: &DownloadEngine,
    location: &JavaInstallRoot,
    release: &JdkRelease,
) -> Result<Prepared, JavaError> {
    let root = location.jdk_dir(release.major, release.vendor, &release.version);
    let binary = extract::java_binary(&root);
    let archive = archive_path(&location.root, release);

    // 幂等第一层：戳记里的 sha256 与本次一致，且 bin/java 在 → 一个字节都不动。
    if extract::is_ready(&root, release.sha256()) {
        let runtime = probe::probe(&binary, JavaOrigin::Managed);
        if runtime.major == Some(release.major) {
            let mut runtime = runtime;
            runtime.vendor = Some(release.vendor.id().to_string());
            return Ok(Prepared {
                root,
                archive,
                runtime,
                downloaded: None,
                bytes: 0,
                files: 0,
                extracted: false,
            });
        }
        note(&format!(
            "托管目录 {} 里的 java 跑不起来，重装一份",
            root.display()
        ));
    }

    if let Some(parent) = archive.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| io_error("创建归档缓存目录", parent.display(), &error))?;
    }
    let outcome = engine
        .download(archive_request(release, archive.clone()))
        .await?;
    let report = extract::extract_archive(&archive, release.sha256(), release.major, &root)?;
    let mut runtime = probe::probe(&binary, JavaOrigin::Managed);
    runtime.vendor = Some(release.vendor.id().to_string());
    if runtime.major != Some(release.major) {
        return Err(JavaError::new(
            JavaErrorKind::Probe,
            format!(
                "装好的 JDK 探测结果是 Java {}，与请求的 {} 不符（{}）",
                runtime
                    .major
                    .map_or_else(|| "未知".to_string(), |major| major.to_string()),
                release.major,
                binary.display()
            ),
        )
        .with_hint("删掉该托管目录后重跑 `ull java install`；或手动设 JAVA_HOME"));
    }
    Ok(Prepared {
        root,
        archive,
        runtime,
        downloaded: Some(outcome.source),
        bytes: outcome.bytes,
        files: report.files,
        extracted: report.wrote(),
    })
}

fn note(message: &str) {
    if cfg!(debug_assertions) {
        eprintln!("[ull::game::java] {message}");
    } else {
        let _ = message;
    }
}
