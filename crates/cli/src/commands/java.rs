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
use unlucky_core::download::{DownloadEngine, RegionProfile};
use unlucky_core::game::java::{
    self, location, vendor, JavaOrigin, JavaRuntime, JdkVendor, LocationMode, DEFAULT_MAJOR,
};
use unlucky_core::launch::request::JavaSelection;
use unlucky_core::platform::paths::ProcessEnv;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::catalog::java_origin_text;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

pub use super::java_install::install;
pub use super::java_launch::{resolve_for_launch, LaunchJava};
pub use super::java_vendors::vendors;

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
        human.push(candidate_line(candidate));
    }
    Ok(Report::new(data, human))
}

/// 一个候选的人读一行：`版本  可执行文件  [来源]`（脱出 `format!` 是为了让来源列有判据）。
fn candidate_line(runtime: &JavaRuntime) -> String {
    format!(
        "  {}  {}  [{}]",
        runtime
            .version
            .clone()
            .unwrap_or_else(|| "版本未知".to_string()),
        runtime.binary.display(),
        source_label(runtime)
    )
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
    pub(super) fn location(&self, ctx: &Ctx) -> Result<java::JavaInstallRoot, CliError> {
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
pub(super) fn describe_mode(mode: LocationMode, override_root: Option<&Path>) -> String {
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
pub(super) fn candidate_json(runtime: &JavaRuntime) -> Value {
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
pub(super) fn map_java_error(error: java::JavaError) -> CliError {
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

/// 运行时 → 来源那一列（托管项带上 vendor）。
///
/// **M327/P30 最后一条**：来源取目录里那句人话（以前直接印 core 的 slug：`instance` /
/// `managed` / `JAVA_HOME` / `PATH` / `system`），五条与 `pack import` 处理器行**同一批**
/// （[`crate::catalog::java_origin_text`]，零新增文案）；`--json` 的 `source` 仍印稳定 id。
fn source_label(runtime: &JavaRuntime) -> String {
    let source = java_origin_text(runtime.origin);
    match &runtime.vendor {
        Some(vendor) => format!("{source} · {vendor}"),
        None => source.to_string(),
    }
}

/// 当前地区档案（`java vendors` 展示「国内/国外」口径；也给 GUI 复用）。
#[must_use]
pub fn region_profile(ctx: &Ctx) -> RegionProfile {
    build_engine(ctx).region().clone()
}

#[cfg(test)]
mod tests {
    use unlucky_core::game::java::{JavaOrigin, JavaRuntime};
    use unlucky_i18n::{Catalog, Key, Lang};

    use super::candidate_line;

    /// `java list` 的来源列说人话，五个 slug 一个都不许露出来（P30 最后一条）。
    ///
    /// 断言四件事，对应四种真会发生的坏法：
    /// 1. **没有 slug**（`instance` / `managed` / `JAVA_HOME` / `PATH` / `system`）——
    ///    `source_label` 改回 `runtime.origin.as_str()` 就红；
    /// 2. **来源信息还在**（行里仍是目录那句人话）——为了去 slug 把来源整段删掉也红；
    /// 3. **逐档对账目录文案**——`java_origin_key` 两档指到同一个 key（或 `System` 指成
    ///    `FromPath`）⇒ 该档那句对不上；删掉某个臂则**编译不过**（穷尽匹配）；
    /// 4. **两条分支都过**（带 vendor 的托管项 / 不带的其余四项）——`Some` 那条臂最容易被漏
    ///    （那时标签是 `managed · temurin`，slug 又从这条缝里回来）。
    ///
    /// 输入是 [`JavaRuntime::unknown`]——`candidate_line` 能拿到的最小输入（不碰盘、不联网、不看
    /// 这台机器装了哪些 JDK）；`list()` 本身要探本机 JVM，用例不能挂在「这台机器上有没有 Java」上。
    #[test]
    fn the_java_list_source_column_names_the_source_in_plain_words() {
        let catalog = Catalog::of(Lang::ZhCn);
        let expected = [
            (JavaOrigin::Instance, Key::PackProcessorJavaFromInstance),
            (JavaOrigin::Managed, Key::PackProcessorJavaFromLauncher),
            (JavaOrigin::JavaHome, Key::PackProcessorJavaFromEnvVar),
            (JavaOrigin::Path, Key::PackProcessorJavaFromPath),
            (JavaOrigin::System, Key::PackProcessorJavaFromSystem),
        ];
        let slugs = ["instance", "managed", "JAVA_HOME", "PATH", "system"];
        let mut lines = Vec::new();

        for (origin, key) in expected {
            let mut runtime = JavaRuntime::unknown("C:/fake/java/bin/java.exe", origin);
            let bare = candidate_line(&runtime);
            runtime.vendor = Some("temurin".to_string());
            let with_vendor = candidate_line(&runtime);

            for line in [&bare, &with_vendor] {
                assert!(
                    line.contains(catalog.text(key)),
                    "{origin:?} 那一档没取到目录文案「{}」：{line}",
                    catalog.text(key)
                );
                for slug in slugs {
                    assert!(
                        !line.contains(slug),
                        "{origin:?} 那一行把内部 slug `{slug}` 印给了用户：{line}"
                    );
                }
            }
            assert!(
                with_vendor.contains("temurin") && with_vendor.contains(" · "),
                "托管项的 vendor 后缀丢了：{with_vendor}"
            );
            assert!(
                !lines.contains(&bare),
                "两档来源印成了同一句（`java_origin_key` 指错 key）：{bare}"
            );
            lines.push(bare);
        }
        assert_eq!(lines.len(), 5, "{lines:#?}");
    }
}
