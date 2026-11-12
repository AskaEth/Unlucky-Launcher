//! 主题三档（Ink / Silver / 跟随系统）的落点测试（M229；从 `app/tests.rs` 拆出守 400 行）。
//!
//! 这一份只管**用户点设置页那三枚 chip 之后**发生的事：换色板、同步设置组状态、落盘。
//! `Auto`（跟随系统）那条路在 `app::tests::the_system_theme_never_overrides_an_explicit_one`。

use crate::app::message::Message;
use crate::app::state::App;
use crate::cli::Options;
use crate::pages;
use crate::theme::Families;
use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::mock::MockBackend;
use unlucky_presenter::prefs::ThemeMode;
use unlucky_presenter::settings_bridge::SettingsBridge;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 一个把配置写在临时文件里的 App：测「落盘」必须真的过文件（内存桥读回来永远是出厂值）。
fn with_config() -> (App, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ull-theme-m229-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("config")).expect("临时目录");
    let path = dir.join("config/launcher.json");
    std::fs::write(&path, "{}").expect("写配置文件");

    let app = App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(MockBackend::new()),
        SettingsBridge::with_path(&path),
    );
    (app, dir)
}

/// M229：设置页「通用 → 主题」三档（Ink / Silver / 跟随系统）真的换色板、真的落盘。
///
/// 抓得住：①`pages::Message::Theme` 没接进 `on_page`（点了没反应）②接了但不换 `Ui`
/// （只有选中态变了，界面颜色还是旧的）③同步设置组状态漏了（设置页首帧/刷新后画错档）
/// ④落盘漏了（重启回旧值——这里**真的过文件**：`with_config` 用 `SettingsBridge::with_path` 写临时
/// 配置，再用同一路径重新 `load()` 读回；内存桥读回来永远是出厂值，验不了落盘）。
/// 抓不住：色板好不好看、像素级差异（那要截图，见 `DevDocs/artifacts/m229/`）。
#[test]
fn the_theme_message_switches_the_board_and_persists_it() {
    let (mut app, dir) = with_config();
    assert_eq!(app.board_name(), "silver", "首帧是浅色主方案（Silver）");

    // 选 Ink（= 深色主方案）
    let _ = app.update(Message::Page(pages::Message::Theme(ThemeMode::Dark)));
    assert_eq!(app.theme_mode(), ThemeMode::Dark);
    assert_eq!(app.board_name(), "ink", "Ink 深板要真切过去");
    assert_eq!(app.ui().flavor(), SchemeFlavor::Dark);
    assert_eq!(
        app.settings.language_appearance.theme,
        ThemeMode::Dark,
        "设置页那一行的档位要跟着改"
    );
    // 落盘：真的过了文件（重新打开同一个配置读回来就是 dark）
    assert_eq!(
        SettingsBridge::with_path(dir.join("config/launcher.json"))
            .load()
            .1,
        ThemeMode::Dark,
        "主题档要落盘"
    );

    // 选 Silver（= 浅色主方案）
    let _ = app.update(Message::Page(pages::Message::Theme(ThemeMode::Light)));
    assert_eq!(app.board_name(), "silver");
    assert_eq!(app.ui().flavor(), SchemeFlavor::Light);
    assert_eq!(
        SettingsBridge::with_path(dir.join("config/launcher.json"))
            .load()
            .1,
        ThemeMode::Light
    );

    // 同一档再点一次：不许白重建
    let refreshes = app.refreshes;
    let _ = app.update(Message::Page(pages::Message::Theme(ThemeMode::Light)));
    assert_eq!(app.refreshes, refreshes, "同一档不该再重建视图模型");
    std::fs::remove_dir_all(&dir).ok();
}

/// **截图通道把主题档钉死在 `--screenshot-theme`**，不读机器上的 `launcher.json`（D30 可复现）。
///
/// 复审 r2 P2-a 指出：`construct.rs` 里新加的这条分支没有回归测试——于是「哪天有人把它改回
/// `bridge.load().1`」不会有人发现，两次不同配置的机器又会出不同颜色的图。
///
/// 判据：**同一份截图参数 + 两份不同的机器配置 ⇒ 同一个档位**，且等于参数（浅→`Light`、深→`Dark`）；
/// 反证：非截图通道下同样两份配置**确实**分出两个档（否则这条测试等于没测到分支）。
#[test]
fn the_screenshot_channel_pins_the_theme_mode_instead_of_reading_the_file() {
    use std::path::{Path, PathBuf};

    /// 写一份只含 `appearance.theme_mode` 的临时配置，并用它构造 App。
    fn with_mode(
        dir_name: &str,
        json: &str,
        options: Options,
        flavor: SchemeFlavor,
    ) -> (App, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("ull-shot-theme-{dir_name}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("config")).expect("临时目录");
        let path = dir.join("config/launcher.json");
        std::fs::write(&path, json).expect("写配置文件");
        let app = App::with_presenter(
            options,
            FAMILIES,
            flavor,
            Box::new(MockBackend::new()),
            SettingsBridge::with_path(&path),
        );
        (app, dir)
    }

    fn cleanup(dir: &Path) {
        std::fs::remove_dir_all(dir).ok();
    }

    let dark_file = r#"{"appearance":{"theme_mode":"dark"}}"#;
    let light_file = r#"{"appearance":{"theme_mode":"light"}}"#;

    // ① 截图参数给浅色：机器配置是 dark 也一样出 Light（钉死值赢）
    let shot_light = Options {
        screenshot: Some(PathBuf::from("unused.png")),
        screenshot_theme: Some(SchemeFlavor::Light),
        ..Options::default()
    };
    let (from_dark_file, d1) = with_mode(
        "light-a",
        dark_file,
        shot_light.clone(),
        SchemeFlavor::Light,
    );
    let (from_light_file, d2) = with_mode("light-b", light_file, shot_light, SchemeFlavor::Light);
    assert_eq!(
        from_dark_file.theme_mode(),
        ThemeMode::Light,
        "截图模式下档位必须由 --screenshot-theme 决定，不许读文件里的 dark"
    );
    assert_eq!(
        from_dark_file.theme_mode(),
        from_light_file.theme_mode(),
        "两份机器配置必须出同一个档位"
    );
    assert_eq!(from_dark_file.board_name(), "silver");
    assert_eq!(from_light_file.board_name(), "silver");

    // ② 反方向：截图参数给深色 ⇒ 即使文件写 light 也是 Dark/Ink
    let shot_dark = Options {
        screenshot: Some(PathBuf::from("unused.png")),
        screenshot_theme: Some(SchemeFlavor::Dark),
        ..Options::default()
    };
    let (from_light_file_dark, d3) = with_mode("dark-a", light_file, shot_dark, SchemeFlavor::Dark);
    assert_eq!(from_light_file_dark.theme_mode(), ThemeMode::Dark);
    assert_eq!(from_light_file_dark.board_name(), "ink");

    // ③ 反证：非截图通道下，同样的两份配置**确实**分出两个档（证明这条分支真的存在）
    let (interactive_dark, d4) =
        with_mode("plain-a", dark_file, Options::default(), SchemeFlavor::Dark);
    let (interactive_light, d5) = with_mode(
        "plain-b",
        light_file,
        Options::default(),
        SchemeFlavor::Light,
    );
    assert_eq!(
        interactive_dark.theme_mode(),
        ThemeMode::Dark,
        "产品路径要读文件"
    );
    assert_eq!(
        interactive_light.theme_mode(),
        ThemeMode::Light,
        "产品路径要读文件"
    );

    for dir in [d1, d2, d3, d4, d5] {
        cleanup(&dir);
    }
}
