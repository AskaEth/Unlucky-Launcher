//! 托管 JDK 安装编排：发行版回退链 → 下载引擎 → 解压 → `java -version` 验证。
//!
//! ```text
//! vendor::order(用户优先级, 地区)                  D23 的尝试顺序
//!   → source::fetch(vendor, major)                查到该家最新 GA（Temurin/Oracle/MS/Zulu）
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
//! 这一步**不拼任何镜像 URL**（Oracle 的哈希段不可推导，见 `oracle` 模块）：Temurin 的
//! GitHub 直链交给 `MirrorTable::expand` 展开成院校镜像，Oracle/MS/Zulu 的链接由各自模块
//! 从目录页/模板/API 拿到最终形态，引擎只负责「把这个 URL 下下来并校验」。

use std::path::{Path, PathBuf};

use super::error::{io_error, JavaError, JavaErrorKind};
use super::extract;
use super::location::{JavaInstallRoot, LocationMode};
use super::probe::{self, JavaOrigin, JavaRuntime};
use super::release::JdkRelease;
use super::source::{self, ChainLog};
use super::target::AdoptiumTarget;
use super::vendor::{self, JdkVendor};
use crate::download::{DownloadEngine, DownloadRequest, ResourceKind};

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
}

impl InstallRequest {
    /// 只要大版本（走回退链，优先级用地区默认）。
    #[must_use]
    pub const fn new(major: u32) -> Self {
        Self {
            major,
            vendor: None,
            priority: Vec::new(),
        }
    }

    /// 钉死发行版。
    #[must_use]
    pub const fn with_vendor(major: u32, vendor: JdkVendor) -> Self {
        Self {
            major,
            vendor: Some(vendor),
            priority: Vec::new(),
        }
    }

    /// 带上用户优先级（只影响没有 `--vendor` 的情形）。
    #[must_use]
    pub fn with_priority(mut self, priority: Vec<JdkVendor>) -> Self {
        self.priority = priority;
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
    let target = AdoptiumTarget::host()?;
    let domestic = engine.region().is_domestic();
    let priority = request.order(domestic);
    let mut chain = ChainLog::new();
    let mut reasons: Vec<String> = Vec::new();
    // 清掉上次中断留下的 .staging-* 与空壳（只碰我们自己的形状，见 location 模块）。
    let _ = super::location::cleanup_orphans(&location.root);

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
        let release = match source::fetch(engine, vendor, request.major, target).await {
            Ok(release) => release,
            Err(error) => {
                chain.failed(vendor, error.message().to_string());
                reasons.push(format!("{}：{}", vendor.id(), error.message()));
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

    Err(JavaError::new(
        JavaErrorKind::Metadata,
        format!(
            "没有装成 Java {}（试过 {}）：{}",
            request.major,
            priority.len(),
            reasons.join("；")
        ),
    )
    .with_hint("`ull java vendors` 看各发行版的可用性；或手动装 JDK 后设 JAVA_HOME"))
}

/// 某一家的中间结果（把七元组元组换成具名结构，读起来才不费劲）。
struct Prepared {
    root: PathBuf,
    archive: PathBuf,
    runtime: JavaRuntime,
    downloaded: Option<String>,
    bytes: u64,
    files: usize,
    extracted: bool,
}

/// 装某一家的那一份（下载 → 解压 → 探测）。
async fn install_one(
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

/// 只做规划不落盘：返回将要安装的版本、目标目录与「是否已经装好」。
///
/// # Errors
///
/// 与 [`install`] 的查询段相同（平台不支持 / 网络失败 / 所有发行版都没有该大版本）。
pub async fn plan(
    engine: &DownloadEngine,
    location: &JavaInstallRoot,
    major: u32,
) -> Result<(JdkRelease, PathBuf, bool), JavaError> {
    let target = AdoptiumTarget::host()?;
    let domestic = engine.region().is_domestic();
    let mut reasons: Vec<String> = Vec::new();
    for vendor in vendor::order(&[], domestic) {
        if !vendor.supports(major) {
            reasons.push(format!("{}：不支持 Java {major}", vendor.id()));
            continue;
        }
        match source::fetch(engine, vendor, major, target).await {
            Ok(release) => {
                let root = location.jdk_dir(release.major, release.vendor, &release.version);
                let ready = extract::is_ready(&root, release.sha256());
                return Ok((release, root, ready));
            }
            Err(error) => reasons.push(format!("{}：{}", vendor.id(), error.message())),
        }
    }
    Err(JavaError::new(
        JavaErrorKind::Metadata,
        format!("没有可用的 Java {major}（{} ）", reasons.join("；")),
    ))
}

fn majors_text(vendor: JdkVendor) -> String {
    vendor
        .hint_majors()
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

fn note(message: &str) {
    if cfg!(debug_assertions) {
        eprintln!("[ull::game::java] {message}");
    } else {
        let _ = message;
    }
}
