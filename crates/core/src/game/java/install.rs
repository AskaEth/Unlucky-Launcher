//! 托管 JDK 安装编排：发行版回退链 → 下载引擎 → 解压 → `java -version` 验证。
//!
//! ```text
//! [Termux] termux_install::attempt            系统包（.deb）——与下面这条链不共用任何一步
//! vendor::order(用户优先级, 地区)                  D23 的尝试顺序
//!   → source::fetch(vendor, major, 版本)           该家最新 GA，或 `--version` 指定的那一版
//!   → location::JavaInstallRoot::jdk_dir          按落点模式算目录
//!   → [戳记 sha256 相同 && bin/java 在] 复用      幂等的第一层
//!   → engine.download(JavaRuntime 请求)           镜像/健康度/退避/续传/边下边算全在引擎
//!   → extract::extract_archive                    zip / tar.gz + 幂等戳记
//!   → probe::probe(bin/java -version)             幂等的第二层，也是「装完真能用」的证据
//! ```
//!
//! **回退的粒度是发行版**（D23）：该家没有这个大版本、镜像全 4xx、sha256 取不到、下载校验失败，
//! 都记一条降级原因然后换下一家；只有全部失败才整体报错，错误里带上每一跳的原因。
//!
//! **平台参数（`AdoptiumTarget::host()`）失败不提前退出**：Termux/Android 这类没有归档产物的
//! 宿主上，早退只会在错误里留一句「不支持该平台」，看不出是哪几家、各卡在哪；现在它降级成
//! 每家一条原因，与其它失败同形。
//!
//! 这一步**不拼任何镜像 URL**（Oracle 的哈希段不可推导，见 `oracle` 模块）：Temurin 的
//! GitHub 直链交给 `MirrorTable::expand` 展开成院校镜像，Oracle/MS/Zulu 的链接由各自模块
//! 从目录页/模板/API 拿到最终形态，引擎只负责「把这个 URL 下下来并校验」。
//!
//! 这一步**不拼任何镜像 URL**（Oracle 的哈希段不可推导，见 `oracle` 模块）：Temurin 的
//! GitHub 直链交给 `MirrorTable::expand` 展开成院校镜像，Oracle/MS/Zulu 的链接由各自模块
//! 从目录页/模板/API 拿到最终形态，引擎只负责「把这个 URL 下下来并校验」。

use std::path::{Path, PathBuf};

use super::catalog;
use super::error::{JavaError, JavaErrorKind};
use super::extract;
use super::location::{JavaInstallRoot, LocationMode};
use super::probe::{self, JavaRuntime};
use super::release::JdkRelease;
use super::source::{self, ChainLog};
use super::target::AdoptiumTarget;
use super::termux;
use super::vendor::{self, JdkVendor};
use crate::download::{DownloadEngine, DownloadRequest, ResourceKind};

use one::install_one;

mod chain;
mod one;
mod plan;
mod termux_install;

use chain::resolve_one;

/// 归档缓存目录名（在安装根下；托管目录扫描跳过它）。
pub use super::location::CACHE_DIR;

/// 一次安装请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRequest {
    /// 目标大版本。
    pub major: u32,
    /// 钉死某一家发行版（`--vendor`）；`None` = 按回退链自己选。
    pub vendor: Option<JdkVendor>,
    /// 用户优先级（`java.vendorPriority`，空 = 按地区默认序）。
    pub priority: Vec<JdkVendor>,
    /// 钉死具体版本（`--version`）；`None` = 该家**最新 GA**（M280 之前的唯一行为）。
    ///
    /// 能不能钉由各家的上游决定（Temurin/Oracle 能、Microsoft 只认它的内置快照、
    /// Zulu 只有 latest）——不能钉时**报错**，不会回落到最新还声称装成了这一版。
    pub version: Option<String>,
}

impl InstallRequest {
    /// 只要大版本（走回退链，优先级用地区默认）。
    #[must_use]
    pub const fn new(major: u32) -> Self {
        Self {
            major,
            vendor: None,
            priority: Vec::new(),
            version: None,
        }
    }

    /// 钉死发行版。
    #[must_use]
    pub const fn with_vendor(major: u32, vendor: JdkVendor) -> Self {
        Self {
            major,
            vendor: Some(vendor),
            priority: Vec::new(),
            version: None,
        }
    }

    /// 带上用户优先级（只影响没有 `--vendor` 的情形）。
    #[must_use]
    pub fn with_priority(mut self, priority: Vec<JdkVendor>) -> Self {
        self.priority = priority;
        self
    }

    /// 钉死具体版本（`--version`；`None` 与不调用本方法完全等价）。
    #[must_use]
    pub fn with_version(mut self, version: Option<String>) -> Self {
        self.version = version;
        self
    }

    /// 实际尝试顺序（`--vendor` 钉死时链里只放它）。
    #[must_use]
    pub fn order(&self, domestic: bool) -> Vec<JdkVendor> {
        match self.vendor {
            Some(vendor) => vec![vendor],
            None => vendor::order(&self.priority, domestic),
        }
    }
}

/// 会真正进入回退链的发行版（顺序 + 能力过滤）。
///
/// 与 [`install`] 的循环同源，但**纯**：不联网、不落盘，给 `--dry-run` 与离线测试用。
/// 过滤条件只有一条——该家发不发这个 GA 大版本（MS 没有 8、Oracle 从 9 起）。
/// `--vendor` 钉死时**不过滤**：链里就它一个，由 `install` 明确报「不支持 Java X」，
/// 而不是悄悄把用户点名的发行版换成别的。
#[must_use]
pub fn chain_order(request: &InstallRequest, domestic: bool) -> Vec<JdkVendor> {
    if request.vendor.is_some() {
        return request.order(domestic);
    }
    request
        .order(domestic)
        .into_iter()
        .filter(|vendor| vendor.supports(request.major))
        .collect()
}

/// 一次成功安装的结果。
#[derive(Debug, Clone)]
pub struct InstalledJdk {
    /// 大版本号。
    pub major: u32,
    /// 实际装上的发行版。
    pub vendor: JdkVendor,
    /// 版本查询结果（含归档名与 sha256）。
    pub release: JdkRelease,
    /// 托管目录（Portable：`java-managed/jdk-21-temurin`；Standard：`Java/oracle/21.0.2`）。
    pub root: PathBuf,
    /// 落点模式（报告里要说清「装到哪」）。
    pub mode: LocationMode,
    /// 归档路径（缓存）。
    pub archive: PathBuf,
    /// 探测到的运行时。
    pub runtime: JavaRuntime,
    /// 实际下载的源标识（`None` = 复用已装，没走网络）。
    pub downloaded: Option<String>,
    /// 本次下载字节数（复用已装时为 0）。
    pub bytes: u64,
    /// 本次解压的文件数（复用已装时可能为 0）。
    pub files: usize,
    /// 本次是否真的解压了。
    pub extracted: bool,
    /// 回退链记录（命中与降级原因）。
    pub chain: ChainLog,
}

impl InstalledJdk {
    /// 是否完全复用了已装的那一份。
    #[must_use]
    pub fn reused(&self) -> bool {
        self.downloaded.is_none()
    }

    /// `bin/java[.exe]` 路径。
    #[must_use]
    pub fn binary(&self) -> PathBuf {
        extract::java_binary(&self.root)
    }

    /// 实际落点（人读一行）。
    #[must_use]
    pub fn location_line(&self) -> String {
        format!("{} · {}", self.mode.as_str(), self.root.display())
    }
}

/// 托管目录路径（Portable 布局 `java-managed/jdk-<major>-<vendor>`）。
#[must_use]
pub fn managed_root_dir(managed_root: &Path, major: u32, vendor: &str) -> PathBuf {
    managed_root.join(probe::managed_dir_name(major, vendor))
}

/// 归档缓存路径（`<root>/.cache/<归档名>`）。
#[must_use]
pub fn archive_path(root: &Path, release: &JdkRelease) -> PathBuf {
    root.join(CACHE_DIR).join(&release.archive.name)
}

/// 归档下载请求（`ResourceKind::JavaRuntime` + sha256 + 期望大小）。
#[must_use]
pub fn archive_request(release: &JdkRelease, dest: PathBuf) -> DownloadRequest {
    let request =
        DownloadRequest::new(ResourceKind::JavaRuntime, release.archive.url.clone(), dest)
            .with_sha256(release.archive.sha256.clone());
    if release.archive.size > 0 {
        request.with_size(release.archive.size)
    } else {
        request
    }
}

/// 按优先级链装一个托管 JDK（幂等：同一份归档装过就直接复用）。
///
/// # Errors
///
/// 全部发行版都失败（没有该大版本 / 平台无产物 / 网络 / 校验 / 解压 / 装完跑不起来）时
/// 返回错误，`message()` 里带着每一跳的降级原因。
pub async fn install(
    engine: &DownloadEngine,
    location: &JavaInstallRoot,
    request: InstallRequest,
) -> Result<InstalledJdk, JavaError> {
    let domestic = engine.region().is_domestic();
    let priority = request.order(domestic);
    let wanted = request.version.as_deref();
    let mut reasons: Vec<String> = Vec::new();
    // 清掉上次中断留下的 .staging-* 与空壳（只碰我们自己的形状，见 location 模块）。
    let _ = super::location::cleanup_orphans(&location.root);

    // Termux 那一条路（系统包）单独成模块：它与归档链不共用任何一步，见 `termux_install`。
    // 它把 chain 还回来（没走成时），归档链接着往上记。
    // 系统包的版本由包管理器决定，所以钉了版本（`--version`）就不走它：`--vendor termux`
    // 直接报「不能指定版本」，其余情况记一条跳过原因后交给归档链。
    let mut chain = match wanted {
        Some(wanted) if request.vendor == Some(JdkVendor::Termux) => {
            return Err(catalog::version_pin_error(
                JdkVendor::Termux,
                request.major,
                wanted,
                &[],
                catalog::VersionListScope::Complete,
            ));
        }
        Some(_) => {
            let mut chain = ChainLog::new();
            // 只有真在 Termux 上才需要解释「为什么不走系统包」（别的平台这条路本来就不存在）。
            if termux::available() {
                chain.skip(
                    JdkVendor::Termux,
                    "系统包不能指定版本（--version），改走归档链",
                );
            }
            chain
        }
        None => match termux_install::attempt(
            request.vendor,
            request.major,
            location,
            ChainLog::new(),
            &mut reasons,
        )
        .await?
        {
            termux_install::Outcome::Done(installed) => return Ok(*installed),
            termux_install::Outcome::Skipped(chain) => chain,
        },
    };

    // 归档发行版需要 Adoptium 的平台参数；Termux/Android 上这里会 Unsupported，
    // 不提前 `?` 退出——让每一家都记一条降级原因，错误里能看见全貌。
    let target = AdoptiumTarget::host();

    for candidate in &priority {
        let vendor = *candidate;
        if !vendor.supports(request.major) {
            let reason = format!(
                "不支持 Java {}（{} 只发 {}）",
                request.major,
                vendor.id(),
                majors_text(vendor)
            );
            chain.skip(vendor, reason.clone());
            reasons.push(format!("{}：{reason}", vendor.id()));
            continue;
        }
        let target = match &target {
            Ok(target) => *target,
            Err(error) => {
                let message = error.message().to_string();
                chain.failed(vendor, message.clone());
                reasons.push(format!("{}：{message}", vendor.id()));
                continue;
            }
        };
        // 取版本 + 兜底闸（版本必须真是点名的那一版，任何一家都不许回落 latest）：
        // 判据在 `chain::resolve_one` 里，`--dry-run` 的 `plan` 走同一个函数。
        let release = match resolve_one(wanted, || {
            source::fetch(engine, vendor, request.major, target, wanted)
        })
        .await
        {
            Ok(release) => release,
            Err(reason) => {
                chain.failed(vendor, reason.clone());
                reasons.push(format!("{}：{reason}", vendor.id()));
                continue;
            }
        };
        match install_one(engine, location, &release).await {
            Ok(prepared) => {
                chain.hit(vendor, format!("命中 {}", release.summary()));
                return Ok(InstalledJdk {
                    major: release.major,
                    vendor,
                    release,
                    root: prepared.root,
                    mode: location.mode,
                    archive: prepared.archive,
                    runtime: prepared.runtime,
                    downloaded: prepared.downloaded,
                    bytes: prepared.bytes,
                    files: prepared.files,
                    extracted: prepared.extracted,
                    chain,
                });
            }
            Err(error) => {
                chain.failed(vendor, error.message().to_string());
                reasons.push(format!("{}：{}", vendor.id(), error.message()));
            }
        }
    }

    // 逐家试过的记录**跟着错误走**（M334 / 复审 r2 P2-1）：调用方只调一次 `install`，
    // 失败时从 `error.chain()` 拿结构化的「哪一家、为什么」，不必自己重写一遍回退链。
    let error = JavaError::new(
        JavaErrorKind::Metadata,
        format!(
            "没有装成 Java {}（试过 {}）：{}",
            request.major,
            priority.len(),
            reasons.join("；")
        ),
    )
    .with_chain(chain);
    // 钉了版本时给一句能照着做的：哪几家真能按版本装（默认路径的提示一个字都不动）。
    Err(if wanted.is_some() {
        error.with_hint(
            "能按版本装的是 `--vendor temurin` 与 `--vendor oracle`；\
             microsoft 只认它内置的那份快照，zulu 上游只提供 latest",
        )
    } else {
        error.with_hint("`ull java vendors` 看各发行版的可用性；或手动装 JDK 后设 JAVA_HOME")
    })
}

pub use plan::plan;

fn majors_text(vendor: JdkVendor) -> String {
    vendor
        .hint_majors()
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join("/")
}
