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

/// 窗口图标的**边长（像素）**。改尺寸只改这一个常量：字节长度断言、`from_rgba` 的两个参数、
/// 守门测试都从它取，别在别处再写一份 `64` / `128`（M376 r2 P2-5）。
///
/// **M379 把它从 64 换成 128**：任务栏与 Alt-Tab 在 150%/200% 缩放下取到的档位是 96/128，64 那张
/// 只能被放大（发虚）。128×128×4 = 65,536 字节，比 64 那档多 49 KB —— 仍在「二进制增量 ≤ 400 KB」
/// 的预算内（实测数见 M379 报告）。64 那档**保留**：标题栏标记用它（见 `shell/title_icon.rs`），
/// 一个 20px 的标记不需要 65 KB 的源。
pub(super) const ICON_SIZE: u32 = 128;

/// 窗口图标的**裸 RGBA 字节**：`assets/branding/ull-128.rgba`，行优先、每像素 R,G,B,A、无文件头。
pub(super) const ICON_PIXELS: &[u8] = include_bytes!("../../../../../assets/branding/ull-128.rgba");

/// 资产被换成别的尺寸时**编不过**，而不是等运行时 `.expect()` 炸（M376 r2 P2-5）。
const _: () = assert!(ICON_PIXELS.len() == (ICON_SIZE * ICON_SIZE * 4) as usize);

/// 产品窗口设置（**唯一产地**：`launch()` 用它开窗，`[window]` / `[caps]` 从它取字段）。
///
/// `ULL_WINDOW_SIZE` 的覆盖在 `boot.rs` 解析成 `size` 后从这里进来 —— 日志与真窗口永远同源。
pub(super) fn window_settings(size: (f32, f32)) -> window::Settings {
    window::Settings {
        size: iced::Size::new(size.0, size.1),
        min_size: Some(MIN_SIZE),
        // M376：窗口/任务栏/Alt-Tab 的图标，吃的是 `assets/branding/ull-128.rgba`（见上面的常量）。
        // 选裸字节而不是 PNG，理由是**启动路径不做解码**：`include_bytes!` 把「给窗口一张图标」
        // 降级成一个常量 + 一次内存拷贝，不必给 `iced` 打开 `image` feature，也不在启动时多一条
        // 图像解码路径。（本 crate 自己确实有 `image` 依赖，用于截图 / 背景图，那是 M93 的既有依赖；
        // D40 管的是「设计层零依赖、解码单独一层」，不是「app-iced 不许有 image」——别把两者混起来。）
        // 尺寸与字节数在**编译期**钉死（上面那条 `const _`），CI 侧另由
        // `packaging/check_branding_assets.py` 守一遍。
        icon: Some(
            window::icon::from_rgba(ICON_PIXELS.to_vec(), ICON_SIZE, ICON_SIZE)
                .expect("ICON_PIXELS 的尺寸已由 const 断言钉死；走到这里说明那条断言被绕过"),
        ),
        // 自绘三键（M93）：窗口**不许**带系统装饰，否则 macOS 上会出现原生标题栏 + 自绘三键两套。
        //
        // M182：这一行**同时**是「macOS 四角是直角」的原因——winit 见到 `decorations: false`
        // 会给出 `NSWindowStyleMask::Borderless`（没有 `Titled`），而窗口服务器只给 titled 窗
        // 做圆角裁剪。链条与「要不要迁移原生标题栏」的口径写在 `platform::macos` 的模块文档里，
        // 改动这一行之前先读那一页（迁移必须连同撤掉自绘三键一起做，否则两套控件打架）。
        decorations: false,
        resizable: true,
        // M93 / D36：整窗 α 需要 `WS_EX_LAYERED`，但这里给不了——`window::Settings` 没有扩展样式
        // 字段，写它只有 `Get/SetWindowLongPtrW`（`unsafe`，只住在 `platform` 下的少数落点文件里，
        // 清单以 `guards.yml` 的 `unsafe-scope` 作业的 `EXPECTED` 为准）。所以 α 下发在拿到句柄之后（`Message::WindowHandle`）。
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
