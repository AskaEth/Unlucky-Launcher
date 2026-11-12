//! 宿主平台探测与标准目录解析。
//!
//! 环境变量读取走 [`EnvSource`] 注入，因此三模式落点可以在单测里确定性地覆盖各 OS。
//! 设计出处：`DevDocs/PlanDoc/04-data-and-config.md` §1（三模式落点）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 需要分支处理的宿主平台族。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Windows。
    Windows,
    /// macOS。
    MacOs,
    /// Linux 及其它 Unix。
    Linux,
    /// 其它（按 Linux 的 XDG 习惯兜底）。
    Other,
}

impl Platform {
    /// 编译目标决定的当前平台。
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(unix) {
            Self::Linux
        } else {
            Self::Other
        }
    }

    /// 稳定名（写日志 / 序列化用）。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::MacOs => "macos",
            Self::Linux => "linux",
            Self::Other => "other",
        }
    }
}

/// 环境变量来源（生产用 [`ProcessEnv`]，测试用 [`MapEnv`]）。
pub trait EnvSource {
    /// 读一个环境变量；空串按不存在处理。
    fn var(&self, name: &str) -> Option<String>;
}

/// 真实进程环境。
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessEnv;

impl ProcessEnv {
    /// 构造。
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl EnvSource for ProcessEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    }
}

/// 固定环境（测试 / 便携化封装用）。
#[derive(Debug, Clone, Default)]
pub struct MapEnv {
    vars: BTreeMap<String, String>,
}

impl MapEnv {
    /// 空环境。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 链式塞一条变量：`MapEnv::new().with("HOME", "/home/u")`。
    #[must_use]
    pub fn with(mut self, name: &str, value: &str) -> Self {
        self.vars.insert(name.to_owned(), value.to_owned());
        self
    }
}

impl EnvSource for MapEnv {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }
}

/// 用户主目录。
#[must_use]
pub fn home_dir(env: &dyn EnvSource) -> Option<PathBuf> {
    if cfg!(windows) {
        env.var("USERPROFILE")
            .or_else(|| env.var("HOME"))
            .map(PathBuf::from)
    } else {
        env.var("HOME").map(PathBuf::from)
    }
}

/// 可执行文件所在目录（Portable 模式的根）。
///
/// Linux 上 `/proc/self/exe` 由 `std` 处理；AppImage 场景下指向挂载点内的 exe，
/// 因此 AppImage 想要「便携到 exe 旁边」需由打包层设置环境变量接管（见 §1 细节）。
#[must_use]
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
}

/// Standard 模式下的启动器数据目录（`.ULL` 的等价物）。
///
/// - Windows：`%APPDATA%\ULL`
/// - macOS：`~/Library/Application Support/ULL`
/// - Linux / 其它：`$XDG_DATA_HOME/ull`，回落 `~/.local/share/ull`
#[must_use]
pub fn standard_launcher_home(platform: Platform, env: &dyn EnvSource) -> Option<PathBuf> {
    match platform {
        Platform::Windows => roaming_app_data(env).map(|dir| dir.join("ULL")),
        Platform::MacOs => home_dir(env).map(|home| home.join("Library/Application Support/ULL")),
        Platform::Linux | Platform::Other => xdg_data_home(env).map(|dir| dir.join("ull")),
    }
}

/// Standard 模式下的游戏目录（Mojang 约定，与 HMCL 的 HOME 目录一致）。
///
/// - Windows：`%APPDATA%\.minecraft`
/// - macOS：`~/Library/Application Support/minecraft`
/// - Linux / 其它：`~/.minecraft`
#[must_use]
pub fn standard_game_home(platform: Platform, env: &dyn EnvSource) -> Option<PathBuf> {
    match platform {
        Platform::Windows => roaming_app_data(env).map(|dir| dir.join(".minecraft")),
        Platform::MacOs => {
            home_dir(env).map(|home| home.join("Library/Application Support/minecraft"))
        }
        Platform::Linux | Platform::Other => home_dir(env).map(|home| home.join(".minecraft")),
    }
}

/// `%APPDATA%`，回落 `%USERPROFILE%\AppData\Roaming`。
fn roaming_app_data(env: &dyn EnvSource) -> Option<PathBuf> {
    env.var("APPDATA").map(PathBuf::from).or_else(|| {
        env.var("USERPROFILE")
            .map(|profile| PathBuf::from(profile).join("AppData/Roaming"))
    })
}

/// `$XDG_DATA_HOME`，回落 `~/.local/share`。
fn xdg_data_home(env: &dyn EnvSource) -> Option<PathBuf> {
    env.var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| home_dir(env).map(|home| home.join(".local/share")))
}

/// 写成可搬运字符串：相对路径统一用 `/`（HMCL `PortablePath` 做法），绝对路径原样保留。
#[must_use]
pub fn to_portable_string(path: &Path) -> String {
    let text = path.to_string_lossy();
    if path.is_absolute() {
        text.into_owned()
    } else {
        text.replace('\\', "/")
    }
}

/// 解析可搬运字符串。
#[must_use]
pub fn from_portable_string(text: &str) -> PathBuf {
    PathBuf::from(text)
}
