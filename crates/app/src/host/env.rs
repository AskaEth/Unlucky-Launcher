//! host 的环境快照：一份 `ResolvedData` + 由其派生的布局视图。
//!
//! app 自己不解析三模式数据根（那是 `core::platform::data_dir` 的活）；这里只做两件事：
//! ①按「exe 同级便携 `launcher.json` 的 `core.*` 四键」拼请求；②把解析结果与
//! `Platform` / `DataMode` 打成一个可克隆的结构，交给后台线程。
//!
//! 截图/演示通道用 [`HostEnv::at_root`] 直接钉一个根（`<root>/.ULL` + `<root>/.minecraft`）。

use std::path::{Path, PathBuf};

use unlucky_core::game::GameLayout;
use unlucky_core::platform::data_dir::{DataDirRequest, DataMode, ResolvedData, Resolver};
use unlucky_core::platform::layout::LauncherLayout;
use unlucky_core::platform::paths::Platform;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

/// 数据根决定键（与 CLI 的 `DATA_ROOT_KEYS` 同源同义）。
pub(crate) const DATA_ROOT_KEYS: [&str; 4] = [
    "core.data_mode",
    "core.data_custom_root",
    "core.launcher_home",
    "core.game_home",
];

/// host 运行期需要的一切路径/平台信息（可克隆进线程）。
#[derive(Debug, Clone)]
pub struct HostEnv {
    /// 数据根解析结果
    pub data: ResolvedData,
    /// `.ULL/` 布局
    pub layout: LauncherLayout,
    /// `.minecraft/` 布局
    pub game: GameLayout,
    /// 平台
    pub platform: Platform,
}

impl HostEnv {
    /// 按便携目录的 `core.*` 解析真实数据根；解析不出来返回 `None`（host 退回 mock）。
    pub fn resolve() -> Option<Self> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))?;
        let request = request_from_portable(&exe_dir);
        let resolver = Resolver::new(Platform::current(), Some(exe_dir));
        let data = resolver.resolve_default(&request).ok()?.clone();
        Some(Self::from_data(data))
    }

    /// 直接钉一个根：`<root>/.ULL` + `<root>/.minecraft`（截图/演示通道）。
    ///
    /// 根下已有 `config/launcher.json` 时**尊重其中的 `core.game_home` / `core.launcher_home`**：
    /// 演示根常用它把游戏目录指到既有真数据上（避免复制几 GB 的 assets/libraries）。
    pub fn at_root(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        let request = request_from_portable(root);
        let absolute = |path: Option<PathBuf>| path.filter(|path| path.is_absolute());
        Self::from_data(ResolvedData {
            mode: DataMode::Portable,
            launcher_home: absolute(request.launcher_home).unwrap_or_else(|| root.join(".ULL")),
            game_home: absolute(request.game_home).unwrap_or_else(|| root.join(".minecraft")),
            redirected_from: None,
        })
    }

    /// 由解析结果建环境。
    pub fn from_data(data: ResolvedData) -> Self {
        Self {
            layout: data.layout(),
            game: GameLayout::new(data.game_home.clone()),
            platform: Platform::current(),
            data,
        }
    }

    /// `config/launcher.json` 路径。
    pub fn launcher_config(&self) -> PathBuf {
        self.layout.launcher_config()
    }

    /// 设置存储（每次现读一份；写操作自己 save）。
    pub fn store(&self) -> SettingStore {
        SettingStore::load(self.launcher_config(), launcher_schema())
    }

    /// 启动会话根（`.ULL/data/agent/sessions`）。
    pub fn sessions_root(&self) -> PathBuf {
        self.layout.agent_sessions()
    }

    /// 数据目录模式。
    pub fn mode(&self) -> DataMode {
        self.data.mode
    }

    /// 人读一行（设置页「数据」分组展示）。
    pub fn data_line(&self) -> String {
        format!(
            "{} · 启动器 {} · 游戏 {}",
            self.data.mode,
            self.data.launcher_home.display(),
            self.data.game_home.display()
        )
    }
}

/// 便携目录那份 `launcher.json` → 数据根请求（只读 `core.*` 四键）。
pub(crate) fn request_from_portable(exe_dir: &Path) -> DataDirRequest {
    let mut request = DataDirRequest::default();
    let path = portable_config_path(exe_dir);
    if !path.is_file() {
        return request;
    }
    let store = SettingStore::load(&path, launcher_schema());
    let text = |key: &str| store.doc().effective_str(key).map(str::to_string);
    if let Some(mode) = text(DATA_ROOT_KEYS[0]).as_deref().and_then(DataMode::parse) {
        request.mode = mode;
    }
    if let Some(root) = text(DATA_ROOT_KEYS[1]) {
        request.custom_root = Some(PathBuf::from(root));
    }
    if let Some(home) = text(DATA_ROOT_KEYS[2]) {
        request.launcher_home = Some(PathBuf::from(home));
    }
    if let Some(home) = text(DATA_ROOT_KEYS[3]) {
        request.game_home = Some(PathBuf::from(home));
    }
    request
}

/// `<exe 目录>/.ULL/config/launcher.json`。
pub(crate) fn portable_config_path(exe_dir: &Path) -> PathBuf {
    LauncherLayout::new(exe_dir.join(".ULL")).launcher_config()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_root_splits_launcher_and_game_homes() {
        let env = HostEnv::at_root("D:/tmp/ull-demo");
        assert_eq!(env.layout.home(), Path::new("D:/tmp/ull-demo/.ULL"));
        assert_eq!(env.game.root(), Path::new("D:/tmp/ull-demo/.minecraft"));
        assert_eq!(env.mode(), DataMode::Portable);
        assert!(env.data_line().contains("portable"));
    }

    #[test]
    fn portable_request_reads_core_keys_only_when_file_exists() {
        let dir = std::env::temp_dir().join(format!("ull-host-env-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".ULL/config")).expect("建目录");
        let request = request_from_portable(&dir);
        assert_eq!(request, DataDirRequest::default());

        std::fs::write(
            portable_config_path(&dir),
            r#"{"core":{"data_mode":"custom","data_custom_root":"D:/data/ull"}}"#,
        )
        .expect("写配置");
        let request = request_from_portable(&dir);
        assert_eq!(request.mode, DataMode::Custom);
        assert_eq!(
            request.custom_root.as_deref(),
            Some(Path::new("D:/data/ull"))
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
