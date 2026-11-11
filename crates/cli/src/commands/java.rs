//! Java 运行时：`ull java list` / `install` / `vendors`（D13 v1 + D23 v2）。
//!
//! 四件事：建下载引擎、把 core 的结果排成人读与 `--json` 两种形态、把错误翻成稳定码、
//! 从设置里取 JDK 的落点与发行版优先级。探测/下载/解压/映射/回退链全在 `core::game::java`。
//!
//! D23 之后的三个用户可见变化：
//!
//! 1. **落点可选**：`java.installRoot` 设了就装到那里；否则 Standard 数据模式装到
//!    `%APPDATA%\Java\<发行方>\<版本>\`（mac/Linux 见 `game::java::location`），
//!    Portable 仍在数据根 `java-managed/`（v1 布局不变）；
//! 2. **发行版可选**：`--vendor temurin|oracle|microsoft|zulu` 钉死一家；不给就按
//!    `java.vendorPriority` → 地区默认序跑回退链，降级原因进 `--json` 的 `chain` 块；
//! 3. **`java vendors`** 一眼看清四家的可用性、镜像与当前顺序。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use unlucky_core::download::blocking::block_on;
use unlucky_core::download::{DownloadEngine, RegionProfile};
use unlucky_core::game::java::{
    self, location, vendor, InstallRequest, JavaOrigin, JavaRuntime, JdkVendor, LocationMode,
    DEFAULT_MAJOR,
};
use unlucky_core::launch::request::JavaSelection;
use unlucky_core::platform::paths::ProcessEnv;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

pub use super::java_launch::{resolve_for_launch, LaunchJava};
pub use super::java_vendors::vendors;

/// 可自动安装的最老大版本（再老的 Temurin 没有 GA 产物）。
const MIN_INSTALLABLE_MAJOR: u32 = 8;

/// 装配用的 Java 选择（`--java` > `JAVA_HOME` > PATH 上的 `java`）。
///
/// `snapshot::prepare` 调它拿 [`JavaSelection`]；真正的来源优先级在
/// [`resolve_for_launch`] 里求值（那一步还会看托管目录与版本要求）。
#[must_use]
pub fn selection(explicit: Option<&Path>) -> JavaSelection {
    let (binary, origin) = match explicit {
        Some(path) => (path.to_path_buf(), JavaOrigin::Instance),
        None => match env_java() {
            Some(binary) => (binary, JavaOrigin::JavaHome),
            None => (PathBuf::from(java::java_binary_name()), JavaOrigin::Path),
        },
    };
    let runtime = java::probe(&binary, origin);
    JavaSelection::new(binary, runtime.major.unwrap_or(DEFAULT_MAJOR), runtime.bits)
}

/// 托管 JDK 可能存在的全部安装根（Portable 数据根 + Standard/Custom 落点，D23 后不止一个）。
#[must_use]
pub fn managed_roots(ctx: &Ctx) -> Vec<PathBuf> {
    let settings = JavaSettings::load(ctx);
    location::scan_roots(
        ctx.data.mode,
        &ctx.layout(),
        settings.install_root.as_deref(),
        ctx.platform,
        &ProcessEnv,
    )
}

/// `ull java list`：探测托管目录 / `JAVA_HOME` / `PATH` / 实例覆盖，并显示各 JDK 的落点。
///
/// # Errors
///
/// 实例清单读不了时返回用户错误（其余探测失败只体现为缺项）。
pub fn list(ctx: &Ctx) -> Result<Report, CliError> {
    let settings = JavaSettings::load(ctx);
    let roots = managed_roots(ctx);
    // 托管 → JAVA_HOME → PATH → 标准安装位置（Program Files 系 / 官方启动器 runtime / 注册表），
    // 同一路径只列一次且保留优先级高者（`discover::dedupe` 负责）。
    let mut candidates = java::candidates_in_roots(&roots);
    let profiles = super::instances::read_profiles(ctx)?;
    for profile in &profiles.profiles {
        if let Some(java_path) = &profile.java_path {
            candidates.push(java::probe(java_path, JavaOrigin::Instance));
        }
    }
    let candidates = java::discover::dedupe(candidates);
    let candidates: Vec<JavaRuntime> = candidates
        .into_iter()
        .filter(|candidate| candidate.major.is_some())
        .collect();

    let mode = settings.location_mode(ctx);
    let installed_root = settings.location(ctx).map(|root| root.root).ok();
    let data = json!({
        // 兼容 v1 的 `managed_root`（首个扫描根），同时给出多根清单（D23 之后不止一个根）。
        "managed_root": roots.first().map(|root| root.display().to_string()),
        "managed_roots": roots.iter().map(|root| root.display().to_string()).collect::<Vec<_>>(),
        "install": {
            "mode": mode.as_str(),
            "root": installed_root.as_ref().map(|root| root.display().to_string()),
            "override": settings.install_root.as_ref().map(|root| root.display().to_string()),
        },
        "candidates": candidates.iter().map(candidate_json).collect::<Vec<_>>(),
    });
    let mut human = vec![format!("Java {} 个", candidates.len())];
    if candidates.is_empty() {
        human.push(
            "（没找到可用 Java）用 `%prog% java install 21` 装一个，或设 JAVA_HOME / `%prog% launch --java <path>`"
                .to_string(),
        );
    }
    human.push(format!(
        "落点模式：{}",
        describe_mode(mode, settings.install_root.as_deref())
    ));
    for root in &roots {
        human.push(format!("  扫描根：{}", root.display()));
    }
    for candidate in &candidates {
        human.push(format!(
            "  {}  {}  [{}]",
            candidate
                .version
                .clone()
                .unwrap_or_else(|| "版本未知".to_string()),
            candidate.binary.display(),
            source_label(candidate)
        ));
    }
    Ok(Report::new(data, human))
}

/// `ull java install <major> [--vendor X]`：回退链查版本 → 下载 → 解压 → `java -version` 验证。
///
/// 幂等：同一个归档装过就直接复用（不联网、不解压）。下载走 `core::download` 引擎，
/// 国内环境按 D7 的地区策略优先院校/国内镜像。
///
/// # Errors
///
/// 大版本不合法、平台无产物、所有发行版都失败（网络/校验/解压/跑不起来）时返回用户错误。
pub fn install(ctx: &Ctx, major: u32, vendor: Option<JdkVendor>) -> Result<Report, CliError> {
    if !(MIN_INSTALLABLE_MAJOR..=99).contains(&major) {
        return Err(CliError::usage(format!(
            "大版本号看起来不对：{major}（可用 8 / 11 / 17 / 21 / 25 这类 GA 大版本）"
        )));
    }
    let settings = JavaSettings::load(ctx);
    let engine = build_engine(ctx);
    let install_root = settings.location(ctx)?;
    let request = match vendor {
        Some(vendor) => {
            InstallRequest::with_vendor(major, vendor).with_priority(settings.priority.clone())
        }
        None => InstallRequest::new(major).with_priority(settings.priority.clone()),
    };
    let installed =
        block_on(java::install(&engine, &install_root, request)).map_err(map_java_error)?;

    let data = json!({
        "major": installed.major,
        "vendor": installed.vendor.id(),
        "release": installed.release.release_name,
        "version": installed.release.version,
        "semver": installed.release.semver,
        "archive": installed.release.archive.name,
        "sha256": installed.release.archive.sha256,
        "size": installed.release.archive.size,
        "root": installed.root.display().to_string(),
        "install_mode": installed.mode.as_str(),
        "binary": installed.binary().display().to_string(),
        "reused": installed.reused(),
        "source": installed.downloaded,
        "bytes": installed.bytes,
        "files": installed.files,
        "extracted": installed.extracted,
        "chain": installed.chain.to_json(),
        "runtime": candidate_json(&installed.runtime),
    });

    let mut human = Vec::new();
    if installed.reused() {
        human.push(format!(
            "Java {}（{}）已就绪（{}），未走网络",
            installed.major,
            installed.vendor.id(),
            installed.root.display()
        ));
    } else {
        human.push(format!(
            "已安装 Java {}（{}，来源 {}，{} 字节 / {} 个文件）",
            installed.major,
            installed.release.summary(),
            installed.downloaded.as_deref().unwrap_or("?"),
            installed.bytes,
            installed.files
        ));
    }
    human.push(format!("落点：{}", installed.location_line()));
    human.push(format!("可执行文件：{}", installed.binary().display()));
    if let Some(version) = &installed.runtime.version {
        human.push(format!("探测：{version}"));
    }
    for note in installed.chain.degradations() {
        human.push(format!("降级：{note}"));
    }
    human.push("`%prog% launch` 会自动优先用它".to_string());
    Ok(Report::new(data, human))
}

/// 设置里与 JDK 有关的两项（`java.installRoot` / `java.vendorPriority`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct JavaSettings {
    pub(crate) install_root: Option<PathBuf>,
    pub(crate) priority: Vec<JdkVendor>,
}

impl JavaSettings {
    /// 读 `<数据根>/.ULL/config/launcher.json`（坏文件退化成默认值，不阻断命令）。
    pub(crate) fn load(ctx: &Ctx) -> Self {
        let store = SettingStore::load(ctx.launcher_config(), launcher_schema());
        let install_root = store
            .doc()
            .effective_str("java.installRoot")
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(PathBuf::from);
        let priority = store
            .effective("java.vendorPriority")
            .and_then(Value::as_array)
            .map(|items| {
                let texts: Vec<String> = items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect();
                vendor::parse_priority(&texts)
            })
            .unwrap_or_default();
        Self {
            install_root,
            priority,
        }
    }

    /// 落点模式（只描述，不建目录）。
    fn location_mode(&self, ctx: &Ctx) -> LocationMode {
        if self.install_root.is_some() {
            LocationMode::Custom
        } else if ctx.data.mode == unlucky_core::platform::data_dir::DataMode::Standard {
            LocationMode::Standard
        } else {
            LocationMode::Portable
        }
    }

    /// 真正要装到哪（解析失败 → 用户错误，带可照着做的提示）。
    fn location(&self, ctx: &Ctx) -> Result<java::JavaInstallRoot, CliError> {
        java::JavaInstallRoot::resolve(
            ctx.data.mode,
            &ctx.layout(),
            self.install_root.as_deref(),
            ctx.platform,
            &ProcessEnv,
        )
        .map_err(map_java_error)
    }
}

/// 落点模式 → 人读一行（含「没设覆盖键时默认落在哪」的解释）。
fn describe_mode(mode: LocationMode, override_root: Option<&Path>) -> String {
    match mode {
        LocationMode::Custom => format!(
            "custom（java.installRoot = {}）",
            override_root.unwrap_or(Path::new("?")).display()
        ),
        LocationMode::Standard => {
            "standard（%APPDATA%\\Java\\<发行方>\\<版本> 等系统标准目录）".to_string()
        }
        LocationMode::Portable => "portable（<数据根>/.ULL/data/java-managed）".to_string(),
    }
}

/// 一个运行时的 `--json` 形状。
fn candidate_json(runtime: &JavaRuntime) -> Value {
    json!({
        "binary": runtime.binary.display().to_string(),
        "source": runtime.origin.as_str(),
        "source_category": runtime.origin.category(),
        "version": runtime.version,
        "major": runtime.major,
        "bits": runtime.bits,
        "vendor": runtime.vendor,
        "install_root": install_root_of(runtime),
    })
}

/// 托管 JDK 的实际目录（`bin/java` 的上两级）；非托管来源为 `None`。
fn install_root_of(runtime: &JavaRuntime) -> Option<String> {
    if runtime.origin != JavaOrigin::Managed {
        return None;
    }
    runtime
        .binary
        .parent()
        .and_then(Path::parent)
        .map(|dir| dir.display().to_string())
}

/// 建下载引擎：装配参数来自设置（并发 / 重试 / gh-proxy，读数口径见 [`crate::download`]），
/// 引擎本体与 `install` / `pack` 共用同一个（core 按缓存根共享）。
pub(crate) fn build_engine(ctx: &Ctx) -> Arc<DownloadEngine> {
    crate::download::engine(ctx)
}

/// JDK 下载/解压错误 → CLI 错误（保留稳定码与提示）。
fn map_java_error(error: java::JavaError) -> CliError {
    let cli = CliError::user(error.code(), error.message().to_string());
    match error.hint() {
        Some(hint) => cli.with_hint(hint.to_string()),
        None => cli,
    }
}

/// `JAVA_HOME/bin/java`（存在才返回）。
fn env_java() -> Option<PathBuf> {
    let home = std::env::var_os("JAVA_HOME")?;
    let binary = PathBuf::from(home)
        .join("bin")
        .join(java::java_binary_name());
    binary.is_file().then_some(binary)
}

/// 运行时 → 展示标签（托管项带上 vendor）。
fn source_label(runtime: &JavaRuntime) -> String {
    match &runtime.vendor {
        Some(vendor) => format!("{} · {vendor}", runtime.origin.as_str()),
        None => runtime.origin.as_str().to_string(),
    }
}

/// 当前地区档案（`java vendors` 展示「国内/国外」口径；也给 GUI 复用）。
#[must_use]
pub fn region_profile(ctx: &Ctx) -> RegionProfile {
    build_engine(ctx).region().clone()
}
