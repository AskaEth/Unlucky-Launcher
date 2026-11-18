//! 主题三档 / `Ui` 重算 / 主题交叉淡入的用例（从 `theme.rs` 拆出守 400 行硬线；M313）。
//!
//! 判据分两层：**令牌层**（`rebuild_ui` 真的换了色，M250）与**动效层**（换主题走中间帧、
//! 落到最终板、`NONE` 瞬时到位且一帧都不烧，M313）。每条都写清「抓得住什么 / 抓不住什么」。

use super::super::App;

use crate::app::components::feedback::progress;
use crate::cli::Options;
use crate::pages::appearance::Change;
use crate::theme::Families;
use unlucky_design::color::{Color, SchemeFlavor};
use unlucky_presenter::mock::MockBackend;
use unlucky_presenter::settings_bridge::SettingsBridge;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 强调色接线（M250 ACCENTWIRE）的 App 级判据：`rebuild_ui` 真的换了令牌。
///
/// **这条是判据一的变异探针**：把 [`App::rebuild_ui`] 改回「恒走 `Ui::new` 出厂板」
/// （或在它前面短路掉 `accent.color()` 分支），下面 `assert_ne!` 当场红——`pages/appearance`
/// 那条用例只证明「`Ui::with_accent` 能给出不同的色」，抓不到「谁去调它」。
///
/// 抓得住：接线不存在 / 被短路 / 强调色解析结果没进 `Ui`。
/// 抓不住：像素（样式闭包被别处覆盖）——那是出图的事。
#[test]
fn rebuild_ui_follows_the_configured_accent() {
    let mut app = App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(MockBackend::new()),
        SettingsBridge::memory_only(),
    );

    // 出厂态：同一组件（进度指示条 = `Role::Primary`）取到的值
    let (_, factory_primary) = progress::linear_colors(app.ui());
    assert_eq!(app.ui().name(), "ull-silver");

    // 设置页填色 → `on_appearance` 会 rebuild 一次（与真机路径同一个函数）
    app.on_appearance(Change::Accent(Color::rgb(0x00, 0x6A, 0x5C).to_hex()));
    let (_, accent_primary) = progress::linear_colors(app.ui());

    assert_ne!(
        factory_primary, accent_primary,
        "填了自定义强调色，`rebuild_ui` 却还给出出厂板的 Primary ⇒ 接线没落地"
    );
    assert_eq!(
        app.ui().flavor(),
        SchemeFlavor::Light,
        "强调色只换颜色来源，不改明暗"
    );
}

/// 换主题的**驱动源**夹具：标准档、还没画过帧的 App。
fn app_for_theme(scale: unlucky_design::motion::MotionScale) -> App {
    crate::app::components::motion::reset();
    crate::app::components::motion::configure_for_test(scale, None);
    let mut app = App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(MockBackend::new()),
        SettingsBridge::memory_only(),
    );
    // 运行期换主题的前提：屏幕上**已经画过**内容（首帧之前不许淡入，见 `rebuild_ui` 的 ③）。
    app.frames = 1;
    app
}

/// **运行期换主题：走中间帧、落到最终板、落位后不再烧帧**（M313 的 App 级判据）。
///
/// 抓得住：①`rebuild_ui` 里不起跑（`motion::theme::observe` 被摘掉）——「必须存在中间帧」
/// 那句红（切完就是终点色）；②`crossfade` 改回硬切——同上一句；③落位之后还在续帧
/// （`theme_frame()` 恒 `Some` 却不落位）——`!needs_frames()` / `!view_dirty` 那两句红。
/// 抓不住：真机像素（同一份判据的像素版在 `components/motion/tests/theme.rs` 与
/// `DevDocs/artifacts/m313/`）。
#[test]
fn a_runtime_switch_crossfades_and_lands_on_the_final_board() {
    use crate::app::components::motion;
    use unlucky_design::color::Role;

    let mut app = app_for_theme(unlucky_design::motion::MotionScale::STANDARD);
    let light = app.ui().color(Role::Surface);
    let dark = crate::theme::Ui::new(SchemeFlavor::Dark, FAMILIES).color(Role::Surface);
    assert_ne!(light, dark, "前提：两块板的 Surface 不同");

    app.set_flavor(SchemeFlavor::Dark);
    assert_eq!(
        app.ui().color(Role::Surface),
        light,
        "换主题**那一拍**必须还是旧颜色：闪切就是在这里发生的"
    );
    assert!(motion::needs_frames(), "淡入期间要订阅帧");
    assert!(app.view_dirty, "淡入期间每个采样点都要重建视图");

    // 连拍：每一帧采一次 Surface，直到交付判据说「不用再续」。
    let mut trace = vec![app.ui().color(Role::Surface)];
    for _ in 0..60 {
        motion::advance_by(motion::FRAME_MS);
        app.refresh_view();
        trace.push(app.ui().color(Role::Surface));
        if !app.view_dirty {
            break;
        }
    }

    let between = |color: iced::Color| {
        let span = |a: f32, b: f32| (a.min(b), a.max(b));
        let (r0, r1) = span(light.r, dark.r);
        let (g0, g1) = span(light.g, dark.g);
        let (b0, b1) = span(light.b, dark.b);
        color != light
            && color != dark
            && (r0..=r1).contains(&color.r)
            && (g0..=g1).contains(&color.g)
            && (b0..=b1).contains(&color.b)
    };
    assert!(
        trace.iter().any(|color| between(*color)),
        "连拍里必须有**与两端都不同**的中间帧（这一句红 = 改回硬切 / 没起跑）：{trace:?}"
    );

    assert_eq!(
        app.ui().color(Role::Surface),
        dark,
        "末帧必须严格等于目标板（不许停在中途）"
    );
    assert!(!motion::needs_frames(), "落位之后不许再订阅帧");
    assert!(!app.view_dirty, "落位之后不许把 view_dirty 永久置真");

    // 空闲不烧帧：再刷 5 次，一次推进都不该发生。
    let samples = motion::samples();
    for _ in 0..5 {
        app.refresh_view();
        assert!(!app.view_dirty, "空闲刷新之后 view_dirty 必须还是假");
    }
    assert_eq!(motion::samples(), samples, "空闲刷新不许推进帧泵");
}

/// **`MotionScale::NONE`（含「减少动效」）：瞬时切到最终值，一帧都不烧。**
///
/// 抓得住：把 `rebuild_ui` 的起跑改成「无条件起跑」或让 `Theme::observe` 忽略 `off`——
/// 下面 `samples` / `needs_frames` 两句红，且「切换当拍就是终点色」那句也红。
/// 抓不住：系统「减少动效」开关本身（`platform::motion` 的用例与真机）。
#[test]
fn no_scale_switches_instantly_and_never_burns_frames() {
    use crate::app::components::motion;
    use unlucky_design::color::Role;

    let mut app = app_for_theme(unlucky_design::motion::MotionScale::NONE);
    let dark = crate::theme::Ui::new(SchemeFlavor::Dark, FAMILIES).color(Role::Surface);

    app.set_flavor(SchemeFlavor::Dark);
    assert_eq!(
        app.ui().color(Role::Surface),
        dark,
        "关掉动效 = 当拍就是最终颜色（不是跑一遍 0 时长）"
    );
    assert!(!motion::needs_frames(), "关掉动效不许为换主题订阅帧");
    assert!(!app.view_dirty, "关掉动效不许把 view_dirty 置真");

    let samples = motion::samples();
    motion::advance_by(motion::FRAME_MS);
    app.refresh_view();
    assert_eq!(motion::samples(), samples, "NONE 档一帧都不许推进");
    assert_eq!(app.ui().color(Role::Surface), dark, "刷新之后仍是最终颜色");
}
