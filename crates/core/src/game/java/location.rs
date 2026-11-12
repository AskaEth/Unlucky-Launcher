//! 托管 JDK 的落点模式（D23 三模式，与 D6 数据目录三模式对齐）。
//!
//! | 模式 | 落点 | 说明 |
//! | --- | --- | --- |
//! | Portable | `<数据根>/.ULL/data/java-managed/jdk-<major>-<vendor>/` | v1 既有布局，M33/M35 的托管根解析语义不变 |
//! | Standard | Windows `%APPDATA%\Java\<vendor>\<version>\`；macOS `~/Library/Application Support/Java/<vendor>/<version>`；Linux `$XDG_DATA_HOME/java/<vendor>/<version>`（回落 `~/.local/share/java/…`） | 用户原话「安装到 %APPDATA%\Java\发行方\Java版本\」 |
//! | Custom | 设置键 `java.installRoot` 指定的目录，同样按 `<vendor>/<version>` 分层 | 覆盖前两种模式 |
//!
//! 三条纪律：
//!
//! 1. **不猜平台目录**：`%APPDATA%` / `XDG_DATA_HOME` / `HOME` 全走 `platform::paths::EnvSource`，
//!    测试能注入假环境确定性地覆盖各 OS 落点；
//! 2. **Portable 布局永不改**：`.ULL/data/java-managed/jdk-<major>-<vendor>` 是 M33 定下的，
//!    改成 `<vendor>/<version>` 会让既有托管 JDK 变成孤儿——迁移语义靠「两种形状都扫得到」兜住；
//! 3. **清理只碰自己的形状**：`cleanup_orphans` 只删 `.staging-*` 半成品与空目录，
//!    没有戳记的非空目录一律不动（用户可能自己往里塞了 JDK）。

use std::path::{Path, PathBuf};

use crate::platform::data_dir::DataMode;
use crate::platform::layout::LauncherLayout;
use crate::platform::paths::{home_dir, EnvSource, Platform};

use super::error::{JavaError, JavaErrorKind};
use super::vendor::JdkVendor;

/// Standard 模式下 JDK 的父目录名（Windows 是 `%APPDATA%\Java`）。
pub const JAVA_DIR_NAME: &str = "Java";

/// 归档缓存目录名（在安装根下；托管目录扫描跳过它）。
pub const CACHE_DIR: &str = ".cache";

/// 解压暂存目录前缀（中断后残留的就是孤儿）。
pub const STAGING_PREFIX: &str = ".staging-";

/// 落地模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationMode {
    /// 数据根内（v1 布局）。
    Portable,
    /// OS 标准应用数据目录。
    Standard,
    /// `java.installRoot` 指定。
    Custom,
}

impl LocationMode {
    /// 稳定名（JSON / 日志）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Portable => "portable",
            Self::Standard => "standard",
            Self::Custom => "custom",
        }
    }

    /// 从配置字符串解析。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "portable" => Some(Self::Portable),
            "standard" => Some(Self::Standard),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

/// 解析好的安装根 + 它是怎么来的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInstallRoot {
    /// 落地模式。
    pub mode: LocationMode,
    /// 安装根目录（各 JDK 目录在它下面）。
    pub root: PathBuf,
}

impl JavaInstallRoot {
    /// Portable：`<数据根>/.ULL/data/java-managed`。
    #[must_use]
    pub fn portable(launcher: &LauncherLayout) -> Self {
        Self {
            mode: LocationMode::Portable,
            root: launcher.java_managed(),
        }
    }

    /// Standard：OS 标准应用数据目录 + `Java`。
    ///
    /// 平台约定与 `platform::paths::standard_launcher_home` 同源：
    /// - Windows：`%APPDATA%\Java`（回落 `%USERPROFILE%\AppData\Roaming\Java`）
    /// - macOS：`~/Library/Application Support/Java`
    /// - Linux / 其它：`$XDG_DATA_HOME/java`（回落 `~/.local/share/java`）
    #[must_use]
    pub fn standard(platform: Platform, env: &dyn EnvSource) -> Option<Self> {
        standard_java_root(platform, env).map(|root| Self {
            mode: LocationMode::Standard,
            root,
        })
    }

    /// Custom：`java.installRoot` 指定的目录（相对路径按数据根解析）。
    #[must_use]
    pub fn custom(root: impl Into<PathBuf>) -> Self {
        Self {
            mode: LocationMode::Custom,
            root: root.into(),
        }
    }

    /// 按设置解析出真正的落点。
    ///
    /// 顺序：`java.installRoot`（设了就用，模式 Custom）→ Standard（数据目录 Standard 模式）
    /// → Portable（数据根内，含 `--data-dir` 的 Custom 数据模式）。
    ///
    /// # Errors
    ///
    /// 数据目录是 Standard 但拿不到系统标准目录时返回 [`JavaErrorKind::Unsupported`]
    /// （不静默落到别处——否则用户会在 `%APPDATA%\Java` 里找不到刚装的 JDK）。
    pub fn resolve(
        data_mode: DataMode,
        launcher: &LauncherLayout,
        override_root: Option<&Path>,
        platform: Platform,
        env: &dyn EnvSource,
    ) -> Result<Self, JavaError> {
        if let Some(root) = override_root.filter(|root| !root.as_os_str().is_empty()) {
            let root = if root.is_absolute() {
                root.to_path_buf()
            } else {
                launcher.home().join(root)
            };
            return Ok(Self::custom(root));
        }
        if data_mode == DataMode::Standard {
            return Self::standard(platform, env).ok_or_else(|| {
                JavaError::new(
                    JavaErrorKind::Unsupported,
                    "Standard 模式下拿不到系统标准目录（%APPDATA% / HOME 都读不到）",
                )
                .with_hint(
                    "设置 `java.installRoot` 指定一个目录，或把 `core.data_mode` 改回 portable",
                )
            });
        }
        Ok(Self::portable(launcher))
    }

    /// 某个 JDK 的目录。
    ///
    /// Standard / Custom 用 `<vendor>/<version>`（D23 用户原话的 `<发行方>\<Java版本>\`），
    /// Portable 保持 v1 的 `jdk-<major>-<vendor>`（改形状会让既有托管 JDK 变孤儿）。
    #[must_use]
    pub fn jdk_dir(&self, major: u32, vendor: JdkVendor, version: &str) -> PathBuf {
        match self.mode {
            LocationMode::Standard | LocationMode::Custom => {
                self.root.join(vendor.id()).join(sanitize_version(version))
            }
            LocationMode::Portable => self
                .root
                .join(super::probe::managed_dir_name(major, vendor.id())),
        }
    }

    /// 归档缓存目录（`<root>/.cache`）。
    #[must_use]
    pub fn cache_dir(&self) -> PathBuf {
        self.root.join(CACHE_DIR)
    }

    /// 人读描述（`Standard（%APPDATA%\Java）` 这种）。
    #[must_use]
    pub fn describe(&self) -> String {
        format!("{}（{}）", self.mode.as_str(), self.root.display())
    }
}

/// OS 标准 JDK 根（纯函数，测试注入环境）。
///
/// 注意 `HOME` 不用 `platform::paths::home_dir`：那个函数按**编译目标**分叉（Windows 上读
/// `USERPROFILE`），会让「在 Windows 上断言 macOS/Linux 落点」这种测试无法成立。
/// 这里按传入的 `platform` 参数走，语义与 `paths::standard_launcher_home` 的落点一致。
#[must_use]
pub fn standard_java_root(platform: Platform, env: &dyn EnvSource) -> Option<PathBuf> {
    match platform {
        Platform::Windows => roaming_app_data(env).map(|dir| dir.join(JAVA_DIR_NAME)),
        Platform::MacOs => {
            unix_home(env).map(|home| home.join("Library/Application Support").join(JAVA_DIR_NAME))
        }
        Platform::Linux | Platform::Other => env
            .var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .or_else(|| unix_home(env).map(|home| home.join(".local/share")))
            .map(|dir| dir.join("java")),
    }
}

/// Unix 家目录（`HOME`，缺了才退回 `platform::paths::home_dir`）。
fn unix_home(env: &dyn EnvSource) -> Option<PathBuf> {
    env.var("HOME").map(PathBuf::from).or_else(|| home_dir(env))
}

/// `%APPDATA%`，回落 `%USERPROFILE%\AppData\Roaming`。
fn roaming_app_data(env: &dyn EnvSource) -> Option<PathBuf> {
    env.var("APPDATA").map(PathBuf::from).or_else(|| {
        env.var("USERPROFILE")
            .map(|profile| PathBuf::from(profile).join("AppData/Roaming"))
    })
}

/// 版本号 → 目录名（只保留文件系统安全的字符）。
///
/// Temurin `21.0.13+9`、MS `21.0.13.0.2`、Zulu `21.0.13+3` 都原样可作目录名；
/// 遇到 `~` / `/` 这类才替换（Zulu 的 `21.0.13+3-x86_64` 之类不会出现，但别赌）。
#[must_use]
pub fn sanitize_version(version: &str) -> String {
    let cleaned: String = version
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '+') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

/// `java list` 要扫的全部安装根（Portable 数据根在前，去重）。
#[must_use]
pub fn scan_roots(
    data_mode: DataMode,
    launcher: &LauncherLayout,
    override_root: Option<&Path>,
    platform: Platform,
    env: &dyn EnvSource,
) -> Vec<PathBuf> {
    let mut roots = vec![launcher.java_managed()];
    if let Ok(resolved) =
        JavaInstallRoot::resolve(data_mode, launcher, override_root, platform, env)
    {
        if !roots.contains(&resolved.root) {
            roots.push(resolved.root);
        }
    }
    // 数据目录是 Standard 时，便携目录里的老装法仍然可能还在（用户从便携切过来的）。
    if let Some(standard) = standard_java_root(platform, env) {
        if !roots.contains(&standard) {
            roots.push(standard);
        }
    }
    roots
}

/// 清理结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CleanupReport {
    /// 删掉的路径（暂存目录、空壳目录）。
    pub removed: Vec<PathBuf>,
    /// 保留下来的条目数（都是我们没资格动的）。
    pub kept: usize,
}

impl CleanupReport {
    /// 是否什么都没删。
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.removed.is_empty()
    }
}

/// 清掉安装根下的半成品与空壳。
///
/// 只碰两种东西：①`.staging-*`（解压中断的残留，永远可以删）②空目录（含空 vendor 目录）。
/// 其余一律保留并计入 `kept`——**宁可不删，也不误删用户手工放进来的 JDK**。
#[must_use]
pub fn cleanup_orphans(root: &Path) -> CleanupReport {
    let mut report = CleanupReport::default();
    let Ok(entries) = std::fs::read_dir(root) else {
        return report;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !path.is_dir() || name == CACHE_DIR {
            report.kept += 1;
            continue;
        }
        if name.starts_with(STAGING_PREFIX) {
            if std::fs::remove_dir_all(&path).is_ok() {
                report.removed.push(path);
            } else {
                report.kept += 1;
            }
            continue;
        }
        if is_empty_dir(&path) {
            if std::fs::remove_dir(&path).is_ok() {
                report.removed.push(path);
            } else {
                report.kept += 1;
            }
            continue;
        }
        // 两层的 Standard/Custom 布局：`<vendor>/<version>` 里的空目录也要收。
        if JdkVendor::parse(&name).is_some() {
            for child in std::fs::read_dir(&path).into_iter().flatten().flatten() {
                let child_path = child.path();
                if child_path.is_dir()
                    && is_empty_dir(&child_path)
                    && std::fs::remove_dir(&child_path).is_ok()
                {
                    report.removed.push(child_path);
                }
            }
            if is_empty_dir(&path) && std::fs::remove_dir(&path).is_ok() {
                report.removed.push(path);
            }
        }
        report.kept += 1;
    }
    report
}

fn is_empty_dir(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_none())
}
