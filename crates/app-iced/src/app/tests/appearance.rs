//! 外观接线的单测：**配置 → 状态 → 下发字节 / 像素 / 落盘**。
//!
//! 每条都按本 mission 的验收纪律写「抓得住 / 抓不住」。真机才有的东西（`WS_EX_LAYERED` 是否补上、
//! `LWA_ALPHA` 的观感）不在这里假装——那是 `[appearance]` 日志与报告里「没验的部分」的事。

use std::path::PathBuf;

use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::mock::MockBackend;
use unlucky_presenter::prefs::{BgFill, PaletteSource};
use unlucky_presenter::settings_bridge::SettingsBridge;

use crate::cli::{Backend, Options};
use crate::pages::appearance::{self, Change, State};
use crate::pages::{self, Screen};

use super::{headless, FAMILIES};

use super::super::state::App;

/// 一个把配置写在临时文件里的 App（测「读回 / 写回」必须真的过文件）。
fn with_config(json: &str, backend: Backend, label: &str) -> (App, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "ull-appear-{label}-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(dir.join("config")).expect("临时目录");
    let path = dir.join("config/launcher.json");
    std::fs::write(&path, json).expect("写配置文件");

    let app = App::with_presenter(
        Options {
            backend,
            ..Options::default()
        },
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(MockBackend::new()),
        SettingsBridge::with_path(&path),
    );
    (app, dir)
}

fn cleanup(dir: &PathBuf) {
    std::fs::remove_dir_all(dir).ok();
}

/// 抓得住：配置里写 `window_opacity=0.8` → 启动读回 0.8 → 下发字节 205（钳位链的入口那一段）。
/// 抓不住：Win32 是否真的收下 204（`transparency` 的单测与本机日志各自举证）。
#[test]
fn the_configured_window_alpha_is_read_back_and_turned_into_a_byte() {
    let (app, dir) = with_config(
        r#"{"appearance":{"window_opacity":0.8,"palette_source":"preset"}}"#,
        Backend::Cpu,
        "read",
    );

    assert_eq!(app.appearance.window_opacity, 0.8);
    assert_eq!(
        app.appearance.window_alpha_byte(),
        Some(205),
        "f32 0.8 → f64 略大于 0.8 → ceil 205（不向下取整）"
    );
    // CPU 档：窗口 α 允许，面板 α 强制 1.0。
    assert_eq!(app.appearance.effective_panel_alpha(), 1.0);
    assert!(app.appearance.panel_is_forced());
    cleanup(&dir);
}

/// 抓得住：①面板 α 滑到 0.8 → 状态是 0.8、生效值 GPU 档也是 0.8；
/// ②CPU 档下滑到 0.5 → 生效值仍是 1.0，而**请求值**仍是 0.5（界面上的「requested → forced」靠这条）。
/// 抓不住：这层半透明在屏幕上到底多透（RDP 上不可信，见报告）。
#[test]
fn panel_alpha_is_independent_of_the_window_alpha_and_forced_on_cpu() {
    let mut gpu = headless(Options::default(), SchemeFlavor::Light);
    gpu.appearance.apply(Change::PanelAlpha(0.8));
    assert_eq!(gpu.appearance.panel_alpha, 0.8);
    assert_eq!(gpu.appearance.effective_panel_alpha(), 0.8);
    assert_eq!(gpu.appearance.window_opacity, 1.0, "面板 α 不许带动整窗 α");

    let mut cpu = headless(
        Options {
            backend: Backend::Cpu,
            ..Options::default()
        },
        SchemeFlavor::Light,
    );
    assert!(
        !cpu.appearance.apply(Change::PanelAlpha(0.5)),
        "只有整窗 α 需要立刻下发（面板 α 只重绘）"
    );
    assert_eq!(cpu.appearance.panel_alpha, 0.5, "请求值保留");
    assert_eq!(
        cpu.appearance.effective_panel_alpha(),
        1.0,
        "CPU 档强制 1.0"
    );
    assert_eq!(cpu.appearance.window_alpha_byte(), None, "整窗 α 仍是 1.0");
}

/// 抓得住：一次滑块改动 → 落盘 → **新开一个 bridge** 能读回（= 重启后还在）。
/// 抓不住：真 host 是否也写了同一份文件（`AppCommand::SetSetting` 走的是同一份 schema）。
#[test]
fn a_panel_alpha_change_is_written_back_to_the_config_file() {
    let (mut app, dir) = with_config("{}", Backend::Gpu, "write");
    app.on_appearance(Change::PanelAlpha(0.8));
    app.on_appearance(Change::BgFill(BgFill::Tile));
    app.on_appearance(Change::BgDim(0.5));

    let path = dir.join("config/launcher.json");
    let reopened = SettingsBridge::with_path(&path).load().0;
    assert_eq!(reopened.panel_alpha, 0.8);
    assert_eq!(reopened.bg_fill, BgFill::Tile);
    assert!((reopened.bg_dim - 0.5).abs() < 1e-6);
    assert_eq!(
        app.appearance_prefs.panel_alpha, 0.8,
        "内存里的「已落盘快照」也要跟上，否则下一次写回会拿旧基底覆盖"
    );
    cleanup(&dir);
}

/// 抓得住：路径读不出来时 ①不 panic ②`background_active()` 为假（静默回落）
/// ③诊断里有一句人话（不是空串）④配色回落到色卡档（不换 scheme）。
/// 抓不住：这张图在别的机器上路径存在时会怎样（那是成功路径，另一条测试管）。
#[test]
fn a_broken_background_path_falls_back_silently_but_says_why() {
    let mut app = headless(Options::default(), SchemeFlavor::Light);
    app.appearance
        .apply(Change::BgImage("D:/definitely/not/here.png".to_owned()));

    assert!(!app.appearance.background_active());
    assert!(app.appearance.image().is_none(), "解码失败 ⇒ 没有可用图");
    assert_eq!(app.appearance.diagnostics().len(), 1, "恰好一条诊断");
    assert!(
        app.appearance.diagnostics()[0].contains("could not be read"),
        "诊断要是人话：{:?}",
        app.appearance.diagnostics()
    );
    assert_eq!(app.appearance.palette_level(), "preset", "回落色卡");
    assert!(
        app.appearance.derived_scheme().is_none(),
        "色卡档不换 scheme"
    );
}

/// 抓得住：真 PNG → 背景真的生效（解出原图尺寸、路径留给渲染器、配色走「背景图取色」这一级）。
/// 抓不住：渲染出来的像素（由截图举证），以及三种填充的覆盖范围（由 `background::tiles` 的
/// 「覆盖 ≥ 视口」断言守）。
#[test]
fn a_decodable_png_makes_the_background_and_the_palette_active() {
    let dir = std::env::temp_dir().join(format!("ull-appear-png-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("临时目录");
    let path = dir.join("bg.png");
    let image = image::RgbImage::from_fn(64, 32, |x, _y| {
        if x < 32 {
            image::Rgb([200, 30, 30])
        } else {
            image::Rgb([20, 60, 200])
        }
    });
    image.save(&path).expect("写 png");

    let mut app = headless(Options::default(), SchemeFlavor::Light);
    app.appearance
        .apply(Change::PaletteSource(PaletteSource::BackgroundImage));
    app.appearance
        .apply(Change::BgImage(path.to_string_lossy().to_string()));
    app.rebuild_ui();

    assert!(app.appearance.background_active());
    assert!(app.appearance.image().is_some(), "解码成功 ⇒ 有可用图");
    assert!(
        !app.appearance.bg_path.trim().is_empty(),
        "渲染器按路径解码（见 picture::image_handle）"
    );
    assert_eq!(app.appearance.diagnostics().len(), 0, "成功时没有诊断");
    assert_eq!(app.appearance.palette_level(), "background-image");
    assert!(
        app.appearance.derived_scheme().is_some(),
        "取色成功后 Ui 要换成解析出的方案"
    );
    // 换 scheme 之后主题名仍是同一个（深浅没变），但 primary 必须与 baseline 不同。
    assert_eq!(app.ui().name(), "ull-m3-light");
    let baseline = crate::theme::Ui::new(SchemeFlavor::Light, FAMILIES);
    assert_ne!(
        app.ui().color(unlucky_design::color::Role::Primary),
        baseline.color(unlucky_design::color::Role::Primary)
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 抓得住：设置页在有外观状态时能真的构图（真控件树，含文本框/滑块/互斥按钮），并且
/// 「外观」分组的下标与 `appearance::APPEARANCE_GROUP` 一致（点左列能切过去）。
/// 抓不住：这些控件点下去之后的交互（`Message` 的落点由 `on_appearance` 的其它测试管）。
#[test]
fn the_settings_page_renders_the_real_appearance_group() {
    let mut app = headless(Options::default(), SchemeFlavor::Light);
    app.state
        .navigate(unlucky_presenter::router::Route::Settings);
    app.refresh_view();
    assert_eq!(app.view.screen, Screen::Settings);
    assert_eq!(app.appearance.selected_group, appearance::APPEARANCE_GROUP);

    let _tree = pages::page_with_appearance(&app.ui, &app.view, &app.appearance);
    drop(_tree);

    // 切分组：只改视图状态，不落盘（`Change::SelectGroup` 的 key() == None）。
    app.appearance.apply(Change::SelectGroup(0));
    assert_eq!(app.appearance.selected_group, 0);
    let _tree = pages::page_with_appearance(&app.ui, &app.view, &app.appearance);
}

/// **P2-2 的守门（复审 r1）**：面板 α 只作用到允许半透明的层。
///
/// 抓得住：`Ui::layer(Canvas)` 在 α=0.55 时仍是不透明（标题栏/导航轨/页头带都取 Canvas，
/// 它们一旦透底，三个窗口键与导航文字就压在背景照片上）；`Card` / `Overlay` 必须真的吃到 α。
/// 抓不住：真实合成后的观感（那由截图看）。
#[test]
fn the_canvas_layer_stays_opaque_while_panels_take_the_alpha() {
    use unlucky_design::elevation::MaterialLayer;

    let mut app = headless(Options::default(), SchemeFlavor::Light);
    app.appearance.apply(Change::PanelAlpha(0.55));
    app.rebuild_ui();

    let ui = app.ui();
    assert_eq!(ui.panel_alpha(), 0.55, "生效值进了 Ui");
    assert_eq!(
        ui.layer(MaterialLayer::Canvas).a,
        1.0,
        "窗口底（标题栏/导航轨/页头带）永远不透明——令牌的不变量"
    );
    for layer in [MaterialLayer::Card, MaterialLayer::Overlay] {
        assert!(
            (ui.layer(layer).a - 0.55).abs() < 1e-6,
            "{layer:?} 必须吃面板 α（实际 {}）",
            ui.layer(layer).a
        );
    }

    // CPU 档：α 被强制 1.0 ⇒ 连卡片也不透明。
    let mut cpu = headless(
        Options {
            backend: Backend::Cpu,
            ..Options::default()
        },
        SchemeFlavor::Light,
    );
    cpu.appearance.apply(Change::PanelAlpha(0.55));
    cpu.rebuild_ui();
    assert_eq!(cpu.ui().layer(MaterialLayer::Card).a, 1.0, "CPU 档强制 1.0");
}

/// 抓得住：**把滑块从头拖到尾**（0.00 → 1.00 的 11 个档）时，状态里的整窗 α 与下发字节逐档单调
/// 上升、且最低一档就是下限字节 21（= 0.0824 ≥ 0.08）、最后一档不透明（不给字节）。
/// 这就是「拖动 → 实际 α」的链条在数值上的等价物；真机拖动的那一次由二进制日志举证。
/// 抓不住：拖动的手感与真实观感（RDP 上不可信）。
#[test]
fn a_slider_sweep_moves_the_alpha_and_the_byte_monotonically() {
    let mut app = headless(Options::default(), SchemeFlavor::Light);
    let mut bytes = Vec::new();
    for step in 0..=10 {
        let fraction = step as f32 / 10.0;
        let alpha = crate::pages::appearance::alpha::window_alpha_from_fraction(fraction);
        app.appearance.apply(Change::WindowOpacity(alpha));
        let byte = app.appearance.window_alpha_byte();
        bytes.push(byte);
        // 每一档都要能打出一行（数值而不是观感）。
        assert!(app.appearance.log_line("sweep").contains("window_alpha="));
    }

    assert_eq!(bytes[0], Some(21), "0% 一档 = 下限 0.08 → 21");
    assert_eq!(bytes[10], None, "100% 一档 = 不透明 → 不下发");
    let sent: Vec<u8> = bytes.iter().flatten().copied().collect();
    assert!(
        sent.windows(2).all(|pair| pair[0] <= pair[1]),
        "字节必须单调不减：{sent:?}"
    );
    assert_eq!(sent.len(), 10, "除最后一档（不透明）之外每档都有字节");
    assert!(
        sent.iter().all(|byte| *byte >= 21),
        "任何一档都不许低于下限字节：{sent:?}"
    );
}

/// 抓得住：日志行里有请求值、生效值与下发字节（RDP 上唯一可信的证据形态）。
/// 抓不住：这行日志是否真的被写进了 stderr（本测试只钉格式，真机日志由跑起来的二进制举证）。
#[test]
fn the_log_line_carries_requests_effective_values_and_the_byte() {
    let mut cpu = headless(
        Options {
            backend: Backend::Cpu,
            ..Options::default()
        },
        SchemeFlavor::Light,
    );
    cpu.appearance.apply(Change::WindowOpacity(0.5));
    cpu.appearance.apply(Change::PanelAlpha(0.4));

    let line = cpu.appearance.log_line("change");
    assert!(line.starts_with("[appearance] change "), "{line}");
    assert!(line.contains("window_alpha=0.500"), "{line}");
    assert!(line.contains("panel_alpha=1.000"), "CPU 档生效值：{line}");
    assert!(line.contains("(requested 0.400)"), "请求值也要在：{line}");
    assert!(
        line.contains("byte=128"),
        "0.5×255=127.5 → 向上取整 128：{line}"
    );

    // 不透明时不给字节（而不是给 255）。
    let opaque = headless(Options::default(), SchemeFlavor::Light);
    assert!(opaque.appearance.log_line("boot").contains("byte=none"));
}

/// 抓得住：`State::new` 与 `apply_env` 之外，`State` 的构造不会偷偷读环境（测试之间不串味）。
/// 抓不住：`ULL_APPEARANCE_*` 在真机上的解析（`apply_env` 由截图矩阵覆盖，见报告）。
#[test]
fn a_fresh_state_is_the_factory_appearance() {
    let state = State::new(
        &unlucky_presenter::prefs::AppearancePrefs::factory(),
        &unlucky_presenter::prefs::AppearanceBase::factory(),
        Backend::Gpu,
        SchemeFlavor::Light,
    );
    assert_eq!(state.window_opacity, 1.0);
    assert_eq!(state.effective_panel_alpha(), 1.0);
    assert!(!state.background_active());
    assert!(state.diagnostics().is_empty());
    assert!(state.image().is_none());
}
