//! 数据根解析与目录视图。
//!
//! 三模式（D6 / `04-data-and-config.md` §1）全部交给 `core::platform::data_dir`：
//! - `--data-dir <dir>` → custom 模式（`<dir>/.ULL` + `<dir>/.minecraft`）；
//! - 否则先按便携目录的 `launcher.json` 读 `core.data_mode` / `core.data_custom_root` /
//!   `core.launcher_home` / `core.game_home`，再交 [`Resolver`] 解析（含 `redirect.json` 跟随）。
//!
//! CLI 不自己拼路径：目录形状的真相在 `core::platform::layout`。
//!
//! `ull config get/set` 的落点也从这里出：[`Ctx::setting_path_for`] 让决定数据根的
//! `core.*` 键落在**便携目录那份** `launcher.json`（与解析同源），其余键落解析后的数据根。

use std::path::{Path, PathBuf};

use unlucky_core::game::GameLayout;
use unlucky_core::platform::data_dir::{DataDirRequest, DataMode, ResolvedData, Resolver};
use unlucky_core::platform::layout::{
    LauncherLayout, CONFIG_DIR, LAUNCHER_CONFIG_FILE, LAUNCHER_HOME_DIR,
};
use unlucky_core::platform::paths::Platform;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::cli::Globals;
use crate::error::CliError;
use crate::os;

/// 决定数据根本身的配置键（`request_from_portable_config` 读的就是它们）。
///
/// `ull config get/set` 对这几个键必须读写**同一份文件**（便携目录那份），否则会出现
/// 「`config get core.data_mode` 说 portable、实际按 standard 跑」这种两套真相
/// （finding 20261030-worker-java2-improve-ull-config-get-set-launcher-json）。
pub const DATA_ROOT_KEYS: [&str; 4] = [
    "core.data_mode",
    "core.data_custom_root",
    "core.launcher_home",
    "core.game_home",
];

/// 该键是否决定数据根。
#[must_use]
pub fn is_data_root_key(key: &str) -> bool {
    DATA_ROOT_KEYS.contains(&key)
}

/// 便携目录那份 `launcher.json` 的路径（`<exe 目录>/.ULL/config/launcher.json`）。
#[must_use]
pub fn portable_config_path(exe_dir: &Path) -> PathBuf {
    exe_dir
        .join(LAUNCHER_HOME_DIR)
        .join(CONFIG_DIR)
        .join(LAUNCHER_CONFIG_FILE)
}

/// 一次命令执行的上下文：数据根 + 平台 + 系统摘要。
#[derive(Debug, Clone)]
pub struct Ctx {
    /// 解析后的数据目录（launcher home / game home）。
    pub data: ResolvedData,
    /// 宿主平台族。
    pub platform: Platform,
    /// 系统信息与支持等级（D19 探测 / D21 Win10+ 单基线）。
    pub os: unlucky_core::platform::os_info::OsInfo,
    /// exe 目录（便携目录那份 `launcher.json` 的根；拿不到时为 `None`）。
    exe_dir: Option<PathBuf>,
    /// `--data-dir` 是否由命令行强制指定（此时数据根不看 `core.*` 键）。
    data_dir_forced: bool,
}

impl Ctx {
    /// 解析数据根。
    ///
    /// # Errors
    ///
    /// `--data-dir` 非绝对路径、exe 目录拿不到、`redirect.json` 损坏等返回用户错误。
    pub fn resolve(globals: &Globals) -> Result<Self, CliError> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf));
        Self::resolve_in(globals, exe_dir)
    }

    /// 指定 exe 目录的解析入口（单测用它把便携目录钉在临时目录里）。
    ///
    /// # Errors
    ///
    /// 同 [`Self::resolve`]。
    pub fn resolve_in(globals: &Globals, exe_dir: Option<PathBuf>) -> Result<Self, CliError> {
        let platform = Platform::current();
        let data_dir_forced = globals.data_dir.is_some();
        let request = match &globals.data_dir {
            Some(dir) => {
                if !dir.is_absolute() {
                    return Err(CliError::usage(format!(
                        "--data-dir 需要绝对路径，收到 `{}`",
                        dir.display()
                    )));
                }
                DataDirRequest {
                    mode: DataMode::Custom,
                    custom_root: Some(dir.clone()),
                    ..DataDirRequest::default()
                }
            }
            None => request_from_portable_config(exe_dir.clone()),
        };
        let resolver = Resolver::new(platform, exe_dir.clone());
        let data = resolver
            .resolve_default(&request)
            .map_err(|error| CliError::user("data_dir", error.to_string()))?;
        Ok(Self {
            data,
            platform,
            os: os::detect(),
            exe_dir,
            data_dir_forced,
        })
    }

    /// 启动器数据布局（`.ULL/` 四分区）。
    #[must_use]
    pub fn layout(&self) -> LauncherLayout {
        self.data.layout()
    }

    /// 游戏目录布局（`.minecraft/`）。
    #[must_use]
    pub fn game(&self) -> GameLayout {
        GameLayout::new(self.data.game_home.clone())
    }

    /// `launcher.json` 路径。
    #[must_use]
    pub fn launcher_config(&self) -> PathBuf {
        self.layout().launcher_config()
    }

    /// 便携目录那份 `launcher.json`（数据根解析的唯一输入；拿不到 exe 目录时为 `None`）。
    #[must_use]
    pub fn portable_launcher_config(&self) -> Option<PathBuf> {
        self.exe_dir.as_deref().map(portable_config_path)
    }

    /// `--data-dir` 是否由命令行强制指定（此时 `core.*` 数据根键本轮不参与解析）。
    #[must_use]
    pub const fn data_dir_forced(&self) -> bool {
        self.data_dir_forced
    }

    /// 某个配置键的读写落点。
    ///
    /// 数据根键（[`DATA_ROOT_KEYS`]）走便携目录那份——和 [`Self::resolve`] 读的是同一份文件，
    /// 这样 `config get core.data_mode` 展示的就是真正生效的值，`config set` 也真的会影响
    /// 下次解析；其余键走解析后的数据根（`<data>/.ULL/config/launcher.json`）。
    #[must_use]
    pub fn setting_path_for(&self, key: &str) -> PathBuf {
        if is_data_root_key(key) {
            if let Some(path) = self.portable_launcher_config() {
                return path;
            }
        }
        self.launcher_config()
    }

    /// `profiles.json` 路径。
    #[must_use]
    pub fn profiles_config(&self) -> PathBuf {
        self.layout().profiles_config()
    }

    /// 会话根（M188 起 = `<data>/logs`；launch 的事件流与索引都在这下面）。
    #[must_use]
    pub fn sessions_root(&self) -> PathBuf {
        self.layout().sessions_root()
    }

    /// 会话日志的**旧家**（`<data>/data/agent/sessions`）：只读兼容，M188 起不再往里写。
    #[must_use]
    pub fn legacy_sessions_root(&self) -> PathBuf {
        self.layout().agent_sessions()
    }

    /// 启动器日志目录（`<data>/logs`：`launcher.log` + `game/` 会话）。
    #[must_use]
    pub fn logs_dir(&self) -> PathBuf {
        self.layout().logs()
    }

    /// 账户数据目录（`data/auth/`：`accounts.json` + `credentials.dat`）。
    #[must_use]
    pub fn auth_data(&self) -> PathBuf {
        self.layout().auth_data()
    }

    /// 会话索引文件（`<sessions_root>/game/index.jsonl`）。
    #[must_use]
    pub fn session_index(&self) -> PathBuf {
        unlucky_core::launch::session_index::index_path(&self.sessions_root())
    }

    /// 旧家的会话索引（`data/agent/sessions/game/index.jsonl`）：只读兼容。
    #[must_use]
    pub fn legacy_session_index(&self) -> PathBuf {
        unlucky_core::launch::session_index::index_path(&self.legacy_sessions_root())
    }

    /// D19/D21：低于基线（Win10+ / macOS 12+ / Linux 达标）时拒绝执行。
    ///
    /// # Errors
    ///
    /// 系统不支持时返回 `unsupported_os` 用户错误（文案与核心自检同一份）。
    pub fn ensure_supported(&self) -> Result<(), CliError> {
        let check = os::check();
        if check.is_usable() {
            return Ok(());
        }
        Err(CliError::user("unsupported_os", check.message)
            .with_hint(format!("最低要求：{}", os::BASELINE)))
    }

    /// 环境级提示（低于基线时的升级指引；一等基线时为空）。
    #[must_use]
    pub fn notes(&self) -> Vec<String> {
        os::notes(&self.os)
    }
}

/// 便携目录的 `launcher.json` → 三模式请求。
///
/// 只读 `effective`（文件值 → 默认值），坏文件也永不 panic（`SettingStore` 的加载降级）。
/// 路径由 [`portable_config_path`] 决定，`config get/set` 用的是同一个函数（同源）。
fn request_from_portable_config(exe_dir: Option<PathBuf>) -> DataDirRequest {
    let mut request = DataDirRequest::default();
    let Some(exe_dir) = exe_dir else {
        return request;
    };
    let path = portable_config_path(&exe_dir);
    if !path.is_file() {
        return request;
    }
    let store = SettingStore::load(&path, launcher_schema());
    let text = |key: &str| store.doc().effective_str(key).map(str::to_string);
    if let Some(mode) = text("core.data_mode").as_deref().and_then(DataMode::parse) {
        request.mode = mode;
    }
    if let Some(root) = text("core.data_custom_root") {
        request.custom_root = Some(PathBuf::from(root));
    }
    if let Some(home) = text("core.launcher_home") {
        request.launcher_home = Some(PathBuf::from(home));
    }
    if let Some(home) = text("core.game_home") {
        request.game_home = Some(PathBuf::from(home));
    }
    request
}
