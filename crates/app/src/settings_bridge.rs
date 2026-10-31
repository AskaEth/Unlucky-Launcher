//! 设置持久化桥：把 UI 偏好落到 `core::setting` 的 schema 文件里，并支持**外部热改热切**。
//!
//! 三条纪律（`DevDocs/SubAgentsTalk/app-DTO通道约定.md` §6 / AGENTS §6）：
//!
//! 1. **不另立配置文件**：读写的就是 `ull config set/get` 与 AI `set_config` 用的那一份
//!    `<data root>/config/launcher.json`（`core::platform::data_dir` 解析数据根、`layout` 给路径）；
//! 2. **不自己拼 JSON**：一律过 `core::setting::store::SettingStore`（schema 校验 + 原子写）；
//! 3. **不 import 领域类型**：跨边界只看「键名 + 取值」，领域模型留在 core。
//!
//! 热切：文件 `mtime` 变了就整份重读（AI / CLI 改完下一拍即生效），见 [`SettingsBridge::reload_if_changed`]。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use unlucky_core::platform::data_dir::{DataDirRequest, DataMode, Resolver};
use unlucky_core::platform::layout::LauncherLayout;
use unlucky_core::platform::paths::Platform;
use unlucky_core::setting::appearance::Appearance;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::prefs::{AppearancePrefs, ThemeMode};

/// 数据根配置键（决定 `launcher.json` 落在哪个数据目录；与 CLI 的 `DATA_ROOT_KEYS` 同源同义）。
const DATA_ROOT_KEYS: [&str; 4] = [
    "core.data_mode",
    "core.data_custom_root",
    "core.launcher_home",
    "core.game_home",
];

/// 设置文件桥（一个进程一份）。
#[derive(Debug)]
pub struct SettingsBridge {
    /// `config/launcher.json`；`None` = 数据根解析不出来（内存态运行，不落盘）
    path: Option<PathBuf>,
    store: Option<SettingStore>,
    /// 上次读/写之后的文件 `mtime`（热切探测用）
    last_mtime: Option<SystemTime>,
}

impl SettingsBridge {
    /// 按三模式数据根打开真实配置文件（启动时调用一次）。
    #[must_use]
    pub fn open() -> Self {
        match resolve_config_path() {
            Some(path) => Self::with_path(path),
            None => Self::memory_only(),
        }
    }

    /// 指定路径打开（测试与特殊数据根用）。
    #[must_use]
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let store = SettingStore::load(&path, launcher_schema());
        let last_mtime = mtime(&path);
        Self {
            path: Some(path),
            store: Some(store),
            last_mtime,
        }
    }

    /// 不落盘（数据根解析失败 / 单测）。
    #[must_use]
    pub fn memory_only() -> Self {
        Self {
            path: None,
            store: None,
            last_mtime: None,
        }
    }

    /// 配置文件路径（诊断/日志用）。
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// 当前生效的外观配置与主题模式。
    #[must_use]
    pub fn load(&self) -> (AppearancePrefs, ThemeMode) {
        let Some(store) = &self.store else {
            return (AppearancePrefs::factory(), ThemeMode::Auto);
        };
        let appearance = Appearance::from_store(store);
        let theme = ThemeMode::from_key_value(&appearance.theme_mode);
        (
            AppearancePrefs {
                bg_image_enabled: appearance.bg_image_enabled,
                bg_image: appearance.bg_image,
                bg_fill: appearance.bg_fill,
                bg_dim: appearance.bg_dim as f32,
                panel_alpha: appearance.panel_alpha as f32,
                glass: appearance.glass,
                accent_color: appearance.accent_color,
            },
            theme,
        )
    }

    /// 落盘（只写出现过的键；失败静默——配置写不进去不该把 UI 卡住，诊断走日志）。
    pub fn persist(&mut self, appearance: &AppearancePrefs, theme: ThemeMode) {
        let Some(store) = self.store.as_mut() else {
            return;
        };
        let mut snapshot = Appearance::from_store(store);
        snapshot.theme_mode = theme.as_str().to_owned();
        snapshot.bg_image_enabled = appearance.bg_image_enabled;
        snapshot.bg_image = appearance.bg_image.clone();
        snapshot.bg_fill = appearance.bg_fill;
        snapshot.bg_dim = f64::from(appearance.bg_dim);
        snapshot.panel_alpha = f64::from(appearance.panel_alpha);
        snapshot.glass = appearance.glass;
        snapshot.accent_color = appearance.accent_color.clone();
        if snapshot.write(store).is_err() {
            return;
        }
        let _ = store.save();
        if let Some(path) = self.path() {
            self.last_mtime = mtime(path);
        }
    }

    /// 文件被外部（AI / `ull config set`）改过就重读；返回新的生效值。
    ///
    /// `mtime` 采样不到（某些文件系统精度/权限）时返回 `None`，退化为「只在启动时读一次」。
    pub fn reload_if_changed(&mut self) -> Option<(AppearancePrefs, ThemeMode)> {
        let path = self.path.clone()?;
        let current = mtime(&path);
        if current.is_none() || current == self.last_mtime {
            return None;
        }
        self.last_mtime = current;
        let store = self.store.as_mut()?;
        store.reload();
        Some(self.load())
    }
}

/// 数据根解析 → `config/launcher.json`。
///
/// 与 CLI 同一套语义：先读便携目录那份 `launcher.json` 的 `core.*`（数据根决定键），
/// 再交 `Resolver` 解析（含 `redirect.json` 跟随）。任何一步失败都退回内存态。
fn resolve_config_path() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let exe_dir = exe_dir?;
    let request = request_from_portable(&exe_dir);
    let resolver = Resolver::new(Platform::current(), Some(exe_dir.clone()));
    match resolver.resolve_default(&request) {
        Ok(resolved) => Some(resolved.layout().launcher_config()),
        // 连 exe 目录都解析不了（极端情况）：仍旧用便携落点，至少设置能存下来
        Err(_) => Some(LauncherLayout::new(exe_dir.join(".ULL")).launcher_config()),
    }
}

/// 便携目录那份 `launcher.json` → 三模式请求（只读 `core.*` 四个键）。
fn request_from_portable(exe_dir: &Path) -> DataDirRequest {
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
fn portable_config_path(exe_dir: &Path) -> PathBuf {
    LauncherLayout::new(exe_dir.join(".ULL")).launcher_config()
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use unlucky_core::setting::appearance::BgFill;

    fn temp_config(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ull-bridge-{label}-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("config")).expect("临时目录");
        let path = dir.join("config/launcher.json");
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn factory_defaults_when_file_missing() {
        let path = temp_config("factory");
        let bridge = SettingsBridge::with_path(&path);
        let (appearance, theme) = bridge.load();
        assert_eq!(appearance, AppearancePrefs::factory());
        assert_eq!(theme, ThemeMode::Auto);
        assert!(!path.exists(), "只读不写：缺文件不该被创建");
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }

    #[test]
    fn persist_then_reload_round_trips() {
        let path = temp_config("roundtrip");
        let mut bridge = SettingsBridge::with_path(&path);
        let mut appearance = AppearancePrefs::factory();
        appearance.bg_image_enabled = true;
        appearance.bg_image = "D:/wall/p.png".to_owned();
        appearance.bg_fill = BgFill::Contain;
        appearance.bg_dim = 0.5;
        appearance.panel_alpha = 0.8;
        appearance.glass = true;
        assert!(appearance.set_accent("#0f6cbd"));
        bridge.persist(&appearance, ThemeMode::Dark);

        let reloaded = SettingsBridge::with_path(&path);
        let (value, theme) = reloaded.load();
        assert_eq!(value, appearance);
        assert_eq!(theme, ThemeMode::Dark);
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }

    #[test]
    fn external_edit_is_picked_up_by_reload() {
        let path = temp_config("hotreload");
        let mut bridge = SettingsBridge::with_path(&path);
        // 先落一份（建立 mtime 基线）
        bridge.persist(&AppearancePrefs::factory(), ThemeMode::Light);
        assert!(bridge.reload_if_changed().is_none(), "自己写完不该报告变更");

        // 模拟外部写入（AI / `ull config set`）：改主题 + 打开背景图
        let mut external = SettingsBridge::with_path(&path);
        let mut appearance = AppearancePrefs::factory();
        appearance.bg_image_enabled = true;
        appearance.bg_image = "D:/wall/x.png".to_owned();
        external.persist(&appearance, ThemeMode::Dark);

        // mtime 精度：有的文件系统 1s 粒度，这里显式等到时间戳变化
        for _ in 0..50 {
            if let Some((value, theme)) = bridge.reload_if_changed() {
                assert_eq!(theme, ThemeMode::Dark);
                assert!(value.bg_image_enabled);
                assert_eq!(value.bg_image, "D:/wall/x.png");
                std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("外部修改没有被 mtime 热切捕获");
    }

    #[test]
    fn accent_preset_and_validation() {
        let mut appearance = AppearancePrefs::factory();
        assert_eq!(appearance.accent_color, "");
        assert!(appearance.set_accent_preset(1));
        assert_eq!(appearance.accent_color, "#0F6CBD");
        assert!(!appearance.set_accent("not-a-color"), "非法输入不改现状");
        assert_eq!(appearance.accent_color, "#0F6CBD");
        assert!(appearance.set_accent(""), "空串恢复出厂");
        assert_eq!(appearance.accent_color, "");
        assert!(!appearance.set_accent_preset(99));
    }

    #[test]
    fn bg_active_needs_both_switch_and_path() {
        let mut appearance = AppearancePrefs::factory();
        appearance.bg_image = "D:/wall/p.png".to_owned();
        assert!(!appearance.bg_active(), "开关没开不算生效");
        appearance.bg_image_enabled = true;
        assert!(appearance.bg_active());
        appearance.bg_image.clear();
        assert!(!appearance.bg_active(), "路径空也不算生效");
    }
}
