//! `install` 的 Termux 分支：JDK 由系统包管理器装进 `$PREFIX`，不是可下载的归档。
//!
//! 单独成模块有两个理由：`.deb` 这条路与归档链**没有共用步骤**（不查版本元数据、不下载、
//! 不解压、不校验 sha256），留在 `install()` 里只能是一段嵌在中间的 if；而 `install.rs`
//! 贴着 400 行红线（拆分前 456 行，见 `guards.yml` 的 `file-size`）。

use super::super::error::{JavaError, JavaErrorKind};
use super::super::extract;
use super::super::location::JavaInstallRoot;
use super::super::probe::{self, JavaOrigin};
use super::super::release::{JdkArchive, JdkRelease};
use super::super::source::ChainLog;
use super::super::vendor::JdkVendor;
use super::InstalledJdk;

/// Termux 分支的结果。
pub(super) enum Outcome {
    /// 系统包装成了，`install` 直接返回它（装箱只为让枚举别撑到 392 字节，
    /// 这条路一次安装才走一次）。
    Done(Box<InstalledJdk>),
    /// 不该走这条分支，或者走了但失败（原因已进 `chain` 与 `reasons`）——把 chain 还给调用方，
    /// 由它继续跑归档回退链。
    Skipped(ChainLog),
}

/// 尝试走 Termux 系统包。
///
/// 语义（D23「`--vendor` 钉死一家、不自动降级」）：
/// - `pinned = Some(Termux)`：只走这条，失败直接冒泡；
/// - `pinned = None`：Termux 上先试系统包，失败记一条降级原因后继续归档链；
/// - 其它平台：`Ok(Skipped(chain))`，一个字节都不动。
///
/// # Errors
///
/// 只有「钉死 termux」且失败时才返回错误；其余失败一律降级成 `reasons` 里的一行。
pub(super) async fn attempt(
    pinned: Option<JdkVendor>,
    major: u32,
    location: &JavaInstallRoot,
    chain: ChainLog,
    reasons: &mut Vec<String>,
) -> Result<Outcome, JavaError> {
    let wanted = match pinned {
        Some(JdkVendor::Termux) => true,
        Some(_) => false,
        None => super::super::termux::available(),
    };
    if !wanted {
        return Ok(Outcome::Skipped(chain));
    }
    if !super::super::termux::available() {
        return Err(
            JavaError::new(JavaErrorKind::Unsupported, "termux 源只在 Termux 里可用")
                .with_hint("普通 Linux/Windows 用 --vendor temurin|oracle|microsoft|zulu"),
        );
    }
    let mut chain = chain;
    match super::super::termux::install(major).await {
        Ok((home, reused)) => {
            let mut runtime = probe::probe(&extract::java_binary(&home), JavaOrigin::Managed);
            runtime.vendor = Some(JdkVendor::Termux.id().to_string());
            let version = runtime.version.clone().unwrap_or_else(|| major.to_string());
            let release = JdkRelease {
                vendor: JdkVendor::Termux,
                major,
                version,
                semver: None,
                release_name: format!("Termux openjdk-{major}"),
                archive: JdkArchive {
                    name: format!("openjdk-{major}.deb"),
                    url: String::new(),
                    sha256: String::new(),
                    size: 0,
                },
            };
            chain.hit(JdkVendor::Termux, format!("命中 {}", release.summary()));
            Ok(Outcome::Done(Box::new(InstalledJdk {
                major: release.major,
                vendor: JdkVendor::Termux,
                release,
                root: home,
                mode: location.mode,
                archive: std::path::PathBuf::new(),
                runtime,
                downloaded: if reused {
                    None
                } else {
                    Some("termux".to_string())
                },
                bytes: 0,
                files: 0,
                extracted: false,
                chain,
            })))
        }
        Err(error) => {
            if pinned == Some(JdkVendor::Termux) {
                return Err(error);
            }
            chain.failed(JdkVendor::Termux, error.message().to_string());
            reasons.push(format!("termux：{}", error.message()));
            Ok(Outcome::Skipped(chain))
        }
    }
}
