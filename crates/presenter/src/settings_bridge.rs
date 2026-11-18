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

use serde_json::Value;
use unlucky_core::platform::data_dir::Resolver;
use unlucky_core::platform::paths::Platform;
use unlucky_core::setting::appearance::Appearance;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::prefs::{AppearanceBase, AppearancePrefs, ThemeMode};

#[path = "settings_bridge/write.rs"]
mod write;

pub(crate) use write::Edit;

/// 一次设置写回该落到哪份 `launcher.json`：数据根四键（`DATA_ROOT_KEYS`，判据只留
/// `host::env` 那一份）→ 便携那份 `<exe>/.ULL/config/launcher.json`，启动解析读的就是它；
/// 其余键 → `env_config`（解析后数据根那份）。
///
/// **写侧与读侧同源就是这一个函数**（M216 ①）：老代码在 Standard 模式下把 `core.data_mode`
/// 写进 `%APPDATA%/ULL/config/launcher.json`，而下次启动只读 `<exe>/.ULL/…` 里的旧值 ⇒
/// 切到 portable 后切不回来。CLI 的 `Ctx::setting_path_for` 早就是这个契约；`exe_dir = None`
/// 时退回 `env_config`（不 panic、不丢写）。
#[must_use]
pub(crate) fn setting_path(env_config: &Path, exe_dir: Option<&Path>, key: &str) -> PathBuf {
    match exe_dir {
        Some(dir) if crate::host::env::DATA_ROOT_KEYS.contains(&key) => {
            crate::host::env::portable_config_path(dir)
        }
        _ => env_config.to_path_buf(),
    }
}

/// 设置文件桥（一个进程一份）。
#[derive(Debug)]
pub struct SettingsBridge {
    /// `config/launcher.json`；`None` = 数据根解析不出来（内存态运行，不落盘）
    pub(super) path: Option<PathBuf>,
    /// 设置文档（schema 校验 + 原子写的唯一入口）
    pub(super) store: Option<SettingStore>,
    /// 上次读/写之后的文件 `mtime`（热切探测用）
    pub(super) last_mtime: Option<SystemTime>,
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

    /// 外观**下层**的只读快照（D36·D37·D38 · M116）：整窗 α / 标题栏三态 / 配色来源档 /
    /// 窗口圆角策略（`AppearanceBase` 的**四个**字段）。
    ///
    /// 为什么不并进 [`SettingsBridge::load`] 的返回值：那个结构体（`AppearancePrefs`）是
    /// **UI 写入用**的（每个字段都会被 `persist` 回写），而这**四个**键**只读不写**——
    /// 落盘走 core 的 `Appearance::write`。并进去会让每个 `AppearancePrefs` 构造点都要补字段，
    /// 还会让「这四项只读」这件事在类型上消失。
    ///
    /// UI 侧（`crates/app-iced`）拿它读启动值：`app-iced` 不直接依赖 `unlucky-core`，
    /// 所以「把 core 的 `SettingDocument` 翻译成只读快照」这一步必须留在本 crate。
    #[must_use]
    pub fn load_base(&self) -> AppearanceBase {
        match &self.store {
            Some(store) => AppearanceBase::from_document(store.doc()),
            None => AppearanceBase::factory(),
        }
    }

    /// 左侧导航轨的折叠档（**M300**）：`UiPrefs::nav_collapsed` 的**唯一读入口**。
    ///
    /// `ui.*` 这一族偏好此前**全部只在内存里**（见 `prefs.rs` 模块文档的「其余 UI 偏好键仍只在
    /// 内存里」）——M300 起那句对 `nav_collapsed` 不再成立：它在 core 的 schema 里登记了
    /// （`ui.nav.collapsed`）、在这里读、在 [`SettingsBridge::persist_nav_collapsed`] 写。
    ///
    /// 缺键 / 值被手改成非布尔 → **`false`（展开）**：出厂态是展开，坏配置不许把用户关进图标条。
    #[must_use]
    pub fn load_nav_collapsed(&self) -> bool {
        self.store
            .as_ref()
            .and_then(|store| {
                store
                    .doc()
                    .effective_bool(crate::prefs::keys::NAV_COLLAPSED)
            })
            .unwrap_or(false)
    }

    /// 写回左侧导航轨的折叠档（**M300**）。返回 `true` = 磁盘上真的有新内容。
    ///
    /// 走 [`SettingsBridge::write_with`]（唯一写入口：现读 → 改 → 落盘 → 刷 mtime），于是
    /// 「刚存完 AI 配置就点折叠」不会把 `ai.*` 连密钥一起抹掉（M174 两轮复审的教训）。
    /// 值没变时返回 `Edit::Unchanged`：重复点同一个档位不该产生一次写盘 + `mtime` 抖动。
    pub fn persist_nav_collapsed(&mut self, collapsed: bool) -> bool {
        self.write_with(|store| {
            if store
                .doc()
                .effective_bool(crate::prefs::keys::NAV_COLLAPSED)
                == Some(collapsed)
            {
                return Edit::Unchanged;
            }
            match store.set(crate::prefs::keys::NAV_COLLAPSED, Value::from(collapsed)) {
                Ok(()) => Edit::Changed,
                Err(_) => Edit::Rejected,
            }
        })
        .is_saved()
    }

    /// 落盘（失败静默——配置写不进去不该把 UI 卡住，诊断走日志）。
    ///
    /// **写走 [`SettingsBridge::write_with`]**（M174 r1/r2 复审）：本桥的 store 是长生命的
    /// （启动时 load 一次），而 `SettingStore::save` 是**整份文档**落盘。不先重读就会拿「≤1 秒前
    /// 的旧快照」整体覆盖文件：用户刚存下的 `ai.base_url` / `ai.model` / `ai.api_key` 会在下一秒
    /// 被外观写回抹掉。现在「现读」这一步收在唯一写入口里，本函数只管改哪些键。
    pub fn persist(&mut self, appearance: &AppearancePrefs, theme: ThemeMode) {
        let _ = self.write_with(|store| {
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
                return Edit::Rejected;
            }
            Edit::Changed
        });
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
///
/// **M216 ①**：读这一步不再自带一份实现，直接用 [`crate::host::env::request_from_portable`]
/// ——`HostEnv::resolve` 与 [`setting_path`] 读的也是它，于是「启动读到的」与「设置页写回的」
/// 是同一份文件、同一段解析（presenter 侧原先两份逐字拷贝，现合一处）。
fn resolve_config_path() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))?;
    let request = crate::host::env::request_from_portable(&exe_dir);
    let resolver = Resolver::new(Platform::current(), Some(exe_dir.clone()));
    match resolver.resolve_default(&request) {
        Ok(resolved) => Some(resolved.layout().launcher_config()),
        // 连 exe 目录都解析不了（极端情况）：仍旧用便携落点，至少设置能存下来
        Err(_) => Some(crate::host::env::portable_config_path(&exe_dir)),
    }
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}
#[cfg(test)]
#[path = "settings_bridge/tests.rs"]
mod tests;
