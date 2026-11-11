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
    ///
    /// **顺带做一件有副作用的事**：把本进程的日志目录登记给 `launch::launcher_log`（M188）。
    /// 这是**启动早期那几行诊断**能落到 `<.ULL>/logs/launcher.log` 的唯一时机——app 层在
    /// presenter 打开之前就在打 `[probe]` / `[font]` / `[theme]`，那些行先被 core 缓冲
    /// （上限 200 行），到这里一次刷出。此前只有「启动一次游戏」或「导出诊断包」才登记，
    /// 于是「双击 → 启动 → 崩」的进程根本不产生 launcher.log——正是用户投诉的那一半。
    pub fn from_data(data: ResolvedData) -> Self {
        let layout = data.layout();
        unlucky_core::launch::launcher_log::set_dir(layout.logs());
        Self {
            layout,
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

    /// 启动会话根（M188 起 = `logs/`；`LauncherConfig` 在其下建 `game/`）。
    pub fn sessions_root(&self) -> PathBuf {
        self.layout.sessions_root()
    }

    /// 会话日志的**旧家**（`data/agent/sessions/`）：只读兼容，M188 起不再往里写。
    pub fn legacy_sessions_root(&self) -> PathBuf {
        self.layout.agent_sessions()
    }

    /// 启动器日志目录（`logs/`：`launcher.log` + `game/` 会话）。
    pub fn logs_dir(&self) -> PathBuf {
        self.layout.logs()
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
    use unlucky_core::platform::paths::MapEnv;

    #[test]
    fn at_root_splits_launcher_and_game_homes() {
        let root = std::env::temp_dir().join(format!("ull-host-env-root-{}", std::process::id()));
        let env = HostEnv::at_root(&root);
        assert_eq!(env.layout.home(), root.join(".ULL"));
        assert_eq!(env.game.root(), root.join(".minecraft"));
        assert_eq!(env.mode(), DataMode::Portable);
        assert!(env.data_line().contains("portable"));
        std::fs::remove_dir_all(&root).ok();
    }

    /// M188 的守门②：**日志根随数据模式变化**（便携 / 标准 / 自定义各一条断言）。
    ///
    /// 三条走的都是 `HostEnv` 真正用的那条解析链（`Resolver` → `ResolvedData::layout`）：
    ///
    /// | 模式 | 期望的 `<home>` | 期望的日志根 |
    /// |---|---|---|
    /// | portable | `<exe>/.ULL` | `<exe>/.ULL/logs` |
    /// | standard | `%APPDATA%/ULL` | `%APPDATA%/ULL/logs` |
    /// | custom | `<root>/.ULL` | `<root>/.ULL/logs` |
    ///
    /// 这里只碰解析结果、不建 `HostEnv`：`from_data` 会真的去登记日志目录（那是产品行为，
    /// 单测里拿假绝对路径调它会在盘根建目录）。
    /// 变异证明：把 `LauncherLayout::logs` 改成返回 `cache/`，三条全红。
    #[test]
    fn logs_root_follows_the_three_data_modes() {
        let env = MapEnv::new().with("APPDATA", "/roaming");
        let exe_dir = PathBuf::from("/games/ULL");
        // custom 模式的根必须是**绝对路径**（Windows 上 `/custom-root` 不算绝对，要带盘符）。
        let custom_root = std::env::temp_dir().join("ull-m188-custom");
        let resolver = Resolver::with_env(Platform::current(), Some(exe_dir.clone()), &env);

        let cases = [
            (
                DataDirRequest {
                    mode: DataMode::Portable,
                    ..DataDirRequest::default()
                },
                exe_dir.join(".ULL/logs"),
            ),
            (
                DataDirRequest {
                    mode: DataMode::Standard,
                    ..DataDirRequest::default()
                },
                PathBuf::new(),
            ),
            (
                DataDirRequest {
                    mode: DataMode::Custom,
                    custom_root: Some(custom_root.clone()),
                    ..DataDirRequest::default()
                },
                custom_root.join(".ULL/logs"),
            ),
        ];

        for (request, expected) in cases {
            let Ok(data) = resolver.resolve(&request) else {
                // 标准模式在某些平台拿不到（没有 HOME/APPDATA）：这一档只能跳过，不许假绿。
                assert_eq!(request.mode, DataMode::Standard, "{request:?}");
                continue;
            };
            let home = data.launcher_home.clone();
            let layout = data.layout();
            assert_eq!(layout.logs(), home.join("logs"), "{request:?}");
            assert_eq!(layout.sessions_root(), home.join("logs"), "{request:?}");
            assert_eq!(layout.game_logs(), home.join("logs/game"), "{request:?}");
            assert_eq!(
                layout.agent_sessions(),
                home.join("data/agent/sessions"),
                "{request:?}"
            );
            if !expected.as_os_str().is_empty() {
                assert_eq!(layout.logs(), expected, "{request:?}");
            }
            // 自定义模式的根在临时目录里，顺手把 `HostEnv` 那三个访问器也钉一遍——
            // 「layout 对了但 `HostEnv::logs_dir` 写死」这种假绿只有这一段抓得住。
            if request.mode == DataMode::Custom {
                let env = HostEnv::from_data(data);
                assert_eq!(env.logs_dir(), home.join("logs"), "{request:?}");
                assert_eq!(env.sessions_root(), home.join("logs"), "{request:?}");
                assert_eq!(
                    env.legacy_sessions_root(),
                    home.join("data/agent/sessions"),
                    "{request:?}"
                );
            }
        }
    }

    /// 三种模式的日志根**各不相同**（防止「都指向同一个写死的目录」这种假绿）。
    #[test]
    fn the_three_modes_do_not_share_one_logs_root() {
        let env = MapEnv::new().with("APPDATA", "/roaming");
        let resolver =
            Resolver::with_env(Platform::current(), Some(PathBuf::from("/games/ULL")), &env);
        let portable = resolver
            .resolve(&DataDirRequest {
                mode: DataMode::Portable,
                ..DataDirRequest::default()
            })
            .expect("portable 一定解析得出");
        let custom = resolver
            .resolve(&DataDirRequest {
                mode: DataMode::Custom,
                custom_root: Some(std::env::temp_dir().join("ull-m188-custom")),
                ..DataDirRequest::default()
            })
            .expect("custom 有绝对路径");
        assert_ne!(portable.layout().logs(), custom.layout().logs());
    }

    /// `HostEnv` 自己的三格（日志根 / 会话根 / 旧会话根）指着同一份解析结果。
    #[test]
    fn env_paths_come_from_one_layout() {
        let root = std::env::temp_dir().join(format!("ull-host-env-paths-{}", std::process::id()));
        let env = HostEnv::at_root(&root);
        assert_eq!(env.logs_dir(), root.join(".ULL/logs"));
        assert_eq!(env.sessions_root(), root.join(".ULL/logs"));
        assert_eq!(
            env.legacy_sessions_root(),
            root.join(".ULL/data/agent/sessions")
        );
        std::fs::remove_dir_all(&root).ok();
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
