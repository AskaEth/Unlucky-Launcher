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
mod tests {
    use super::*;
    use crate::prefs::PaletteSource;
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

    /// **M216 ①**：数据根四键的写回落点 = 启动解析读的那份文件。
    ///
    /// 抓得住：四键 → 便携那份、其余键 → 解析后那份、拿不到 exe 目录时退回。抓不住：真实
    /// `current_exe()` 的取值与 `Resolver` 的 `redirect.json` 跟随（那两条靠 `set` 的回合用例）。
    #[test]
    fn data_root_keys_are_written_where_startup_reads_them() {
        let exe = PathBuf::from("/games/ULL");
        let portable = crate::host::env::portable_config_path(&exe);
        let resolved = PathBuf::from("/appdata/ULL/config/launcher.json");
        for key in crate::host::env::DATA_ROOT_KEYS {
            assert_eq!(setting_path(&resolved, Some(&exe), key), portable, "{key}");
        }
        for key in ["java.default_memory_mb", "ai.model", "core.unknown_suffix"] {
            assert_eq!(setting_path(&resolved, Some(&exe), key), resolved, "{key}");
        }
        // 拿不到 exe 目录：退回解析后那份（宁可写「没人读的那份」也不丢写、不 panic）。
        assert_eq!(setting_path(&resolved, None, "core.data_mode"), resolved);
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

    /// **P2-4 回归**：外观写回不许盖掉别人刚写的键（`ai.api_key` 最贵）。修之前必红。
    #[test]
    fn persist_keeps_keys_another_writer_just_saved() {
        let path = temp_config("persist-keeps-ai");
        let mut bridge = SettingsBridge::with_path(&path);
        // 先让本桥真的加载过一份文件（旧快照从这里来）。
        bridge.persist(&AppearancePrefs::factory(), ThemeMode::Light);

        {
            use unlucky_core::setting::keys::launcher_schema;
            use unlucky_core::setting::store::SettingStore;
            // 另一个写者（M174 的助手设置页 / `ull config set`）写进三个 AI 键。
            let mut store = SettingStore::load(&path, launcher_schema());
            for (key, value) in [
                ("ai.base_url", "https://api.deepseek.com/v1"),
                ("ai.model", "deepseek-chat"),
                ("ai.api_key", "sk-must-survive"),
            ] {
                store.set(key, value.to_string().into()).expect("写 AI 键");
            }
            store.save().expect("落盘");
        }

        // 同一秒内改主题：外观写回。
        bridge.persist(&AppearancePrefs::factory(), ThemeMode::Dark);

        let text = std::fs::read_to_string(&path).expect("读文件");
        assert!(
            text.contains("sk-must-survive"),
            "外观写回把刚保存的密钥盖掉了：{text}"
        );
        assert!(text.contains("deepseek-chat"), "model 也被盖掉了：{text}");
        assert!(text.contains("\"dark\""), "主题这一项该写进去：{text}");
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
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

    /// `load_base()` 是「重启后读回整窗 α / 配色来源 / 窗口圆角」的唯一通道（M93 · M116）。
    ///
    /// 抓得住：①文件里写着 `window_opacity=0.6` / `palette_source=wallpaper` 时能被**新开的**
    /// bridge 读到（= 重启语义）；②缺键时回出厂（1.0 / preset），而不是 `0.0` 或随机档。
    /// 抓不住：UI 侧是否真的用它来下发窗口 α（那由 `app-iced` 的日志与单测证明）。
    #[test]
    fn load_base_reads_the_four_read_only_keys_and_defaults_when_missing() {
        let path = temp_config("base");
        // 文件由「外部写者」（CLI / AI / 设置页的 SetSetting）产生，这里直接落 JSON。
        std::fs::write(
            &path,
            r#"{"appearance":{"theme_mode":"dark","window_opacity":0.6,"palette_source":"wallpaper","panel_alpha":0.5}}"#,
        )
        .expect("写配置文件");

        let bridge = SettingsBridge::with_path(&path);
        let base = bridge.load_base();
        assert_eq!(base.window_opacity, 0.6, "重启后读回整窗 α");
        assert_eq!(base.palette_source, PaletteSource::Wallpaper);
        assert!(base.is_transparent());

        // 缺键 → 出厂值（与 `AppearanceBase::factory()` 逐字段相等）。
        let empty = path.with_file_name("launcher-empty.json");
        std::fs::write(&empty, "{}").expect("写空配置");
        let factory = SettingsBridge::with_path(&empty).load_base();
        assert_eq!(factory, AppearanceBase::factory());
        assert_eq!(factory.window_opacity, 1.0);
        assert_eq!(factory.palette_source, PaletteSource::Preset);

        // 数据根解析不出来的内存态桥也要给得出出厂值（不 panic、不返回 0）。
        assert_eq!(
            SettingsBridge::memory_only().load_base(),
            AppearanceBase::factory()
        );

        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }
}
