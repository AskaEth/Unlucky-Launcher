//! `SettingsBridge` 的单测（M300 从 `settings_bridge.rs` **纯搬移**，守 400 行硬线）。
//!
//! 搬移理由：M300 给桥加了 `load_nav_collapsed` / `persist_nav_collapsed` 两件（折叠档落盘），
//! 原文件 434 行已破门禁。判据与注释一字未动，只是换文件——`settings_bridge.rs` 里现在是一行
//! `#[cfg(test)] #[path = "settings_bridge/tests.rs"] mod tests;`，模块路径仍是
//! `settings_bridge::tests`。

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

/// **M300 落盘往返**：`ui.nav.collapsed` 写进文件 → **重新构造桥**（= 重启）→ 读回。
///
/// 这条用例守的是任务单里那句「不然重启就回去了」：只断言内存字段的用例对它零约束
/// （字段本来就地改，重启后 `UiPrefs::default()` 照样是展开）。
///
/// **抓得住**：①`persist_nav_collapsed` 忘了 `store.save()`（第二次构造读回 false）；
/// ②键名两侧不一致（core schema 叫 `ui.nav.collapsed`、桥写成别的串 ⇒ `store.set` 被
/// schema 拒 ⇒ 第一次写返回 false）；③core 没登记这个键（`UnknownKey` ⇒ 同样第一次写就 false）；
/// ④`load_nav_collapsed` 读的是内存快照而不是刚 reload 的文档（跑完这次进程内的旧值还能骗过它，
/// 但「换一个桥对象」那一步就露馅）。
/// **抓不住**：真实数据根解析（`SettingsBridge::open()` 那条路，见本文件另一组用例）、
/// 并发写盘（`save_worker` 的锁），以及 UI 那一侧有没有真的调它（那归 app 侧的
/// `app/update/nav.rs` 用例）。
#[test]
fn nav_collapsed_survives_a_restart() {
    let path = temp_config("nav-collapsed");

    // ① 出厂（文件不存在）：展开。
    let mut bridge = SettingsBridge::with_path(&path);
    assert!(!bridge.load_nav_collapsed(), "缺键 = 出厂展开");

    // ② 折叠 → 真的落盘。
    assert!(
        bridge.persist_nav_collapsed(true),
        "第一次写 `ui.nav.collapsed` 必须真的落盘（false = 被 schema 拒 / 没存）"
    );

    // ③ 「重启」：换一个桥对象读同一份文件。
    let reopened = SettingsBridge::with_path(&path);
    assert!(
        reopened.load_nav_collapsed(),
        "重启后必须读回折叠档（否则折叠是假的）"
    );

    // ④ 幂等：同一个值再写一次不该产生写盘（`Edit::Unchanged`）。
    let mut bridge = reopened;
    assert!(!bridge.persist_nav_collapsed(true), "值没变时不该再写盘");

    // ⑤ 展开也要落盘（只写 true 的实现会在这里红）。
    assert!(bridge.persist_nav_collapsed(false));
    assert!(!SettingsBridge::with_path(&path).load_nav_collapsed());

    // ⑥ 内存态桥（数据根解析不出来）：不 panic、也不假装写成功。
    let mut memory = SettingsBridge::memory_only();
    assert!(!memory.load_nav_collapsed());
    assert!(!memory.persist_nav_collapsed(true));

    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}
