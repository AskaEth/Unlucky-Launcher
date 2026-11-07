//! 启动阶段的**验收诊断编排**：把 `app/boot.rs` 的 `run()` 收成「探测 → 字体 → 打印 → 起 iced」。
//!
//! ## 为什么单独一个文件（M98 r1 p2-1 的根因）
//!
//! 第一版把 `[window] requested … resizable=true decorations=false transparent=false` 直接写在
//! 格式化串里，而 `boot.rs` 给窗口的 `settings` 是另一处常量 —— 两者**没有任何绑定**。定向变异
//! 把 `settings.decorations` 改成 `true`，371 个测试全绿，而验收包里 `[caps] decorations=true` 与
//! `[window] decorations=false` 会互相矛盾（踩坑记录 U1：常量冒充观测结果）。
//!
//! 现在：窗口设置由 [`window_settings`] **一处**产生，日志由 [`report_window`] 从**同一个
//! `&window::Settings`** 取字段。钉死它的守门测试在 `boot/tests.rs`（`window_settings_*`）。
//!
//! 另一件事：所有「跑没跑」都要能如实说。降级重开那一轮没计时就写 `probe=skipped`，
//! 不许拿 `0ms` 冒充一次实测（M98 r1 p3-1）。

use std::time::Duration;

use iced::window;

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform as FontPlatform;

use crate::cli::Options;
use crate::probe::Probe;
use crate::renderer::{acceptance, Decision};

use super::MIN_SIZE;
use crate::app::APP_TITLE;

/// 产品窗口设置（**唯一产地**：`launch()` 用它开窗，`[window]` / `[caps]` 从它取字段）。
///
/// `ULL_WINDOW_SIZE` 的覆盖在 `boot.rs` 解析成 `size` 后从这里进来 —— 日志与真窗口永远同源。
pub(super) fn window_settings(size: (f32, f32)) -> window::Settings {
    window::Settings {
        size: iced::Size::new(size.0, size.1),
        min_size: Some(MIN_SIZE),
        // 自绘三键（M93）：窗口**不许**带系统装饰，否则 macOS 上会出现原生标题栏 + 自绘三键两套。
        decorations: false,
        resizable: true,
        // M93 / D36：整窗 α 需要 `WS_EX_LAYERED`，但这里给不了——`window::Settings` 没有扩展样式
        // 字段，写它只有 `Get/SetWindowLongPtrW`（`unsafe`，全仓只允许出现在
        // `platform/win32/transparency.rs`）。所以 α 下发在拿到句柄之后（`Message::WindowHandle`）。
        transparent: false,
        ..window::Settings::default()
    }
}

/// `[window] requested` + `[caps]` 两行，**都从同一个 `settings` 取**（不给手抄字面量的机会）。
pub(super) fn report_window(settings: &window::Settings, scale: Option<f32>, size_source: &str) {
    let facts = acceptance::WindowFacts::from_settings(settings);
    acceptance::report_window(&facts, scale, size_source, Some(APP_TITLE));
    acceptance::report_caps(
        std::env::current_exe().ok().as_deref(),
        facts.transparent,
        facts.resizable,
        facts.decorations,
    );
}

/// 第一段：`[acceptance] on` + `[backend] plan` +（验收模式）`[env]` / `[i18n]`。
pub(super) fn report_boot(options: &Options, probe: &Probe, decision: &Decision) {
    if acceptance::enabled() {
        acceptance::announce();
    }
    acceptance::report_backend_plan(decision, options.backend);
    if acceptance::enabled() {
        acceptance::report_startup(probe, options.scale_factor, options.screenshot.is_some());
    }
}

/// 第二段：`[font] chain` + `[perf] boot`（都要 `ULL_ACCEPTANCE=1`）。
///
/// `probe = None` = 这一轮**没有探测**（既不是 `auto` 也没开验收）⇒ 打 `probe=skipped`。
pub(super) fn report_font(
    platform: FontPlatform,
    probe: Option<Duration>,
    font: Duration,
    to_run: Duration,
) {
    if acceptance::enabled() {
        acceptance::report_font_chain(platform);
        acceptance::report_perf_boot(probe, font, to_run);
    }
}

/// 第三段：`[theme]`（恒开：它解释的是「这次主题凭什么是这个」）。
pub(super) fn report_theme(options: &Options, flavor: SchemeFlavor) {
    let (source, follows) = acceptance::theme_source(
        options.screenshot_theme.is_some(),
        options.theme.is_some(),
        options.screenshot.is_some(),
    );
    let explicit = (!follows).then_some(if flavor.is_dark() { "dark" } else { "light" });
    acceptance::report_theme(source, explicit, follows);
}

/// 第四段：`[backend] final`（恒开）+ `[perf] session`（验收模式）。
///
/// `error` = 具名失败原因（`None` = 起来了）；`via` 见 `renderer::acceptance::failure_via`。
/// `refuse` / `none` 两条路**什么都没跑起来** ⇒ `final=none`；只有 `respawn` 才有「换到哪个
/// 后端」可言（那一格是兜底名，子进程自己还会再打一遍自己的 plan/final）。
pub(super) fn report_outcome(
    decision: &Decision,
    error: Option<String>,
    via: &str,
    mode: &str,
    elapsed: Duration,
) {
    acceptance::report_outcome(decision, error.as_deref(), via, mode, elapsed);
}
