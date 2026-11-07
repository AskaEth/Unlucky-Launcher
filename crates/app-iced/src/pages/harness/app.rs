//! 截图 harness 的**运行部分**（M100 r1：`harness.rs` 撞 400 行后从那里搬出来）。
//!
//! 这里只有 [`run`]：探后端 → 造外观真状态 → 起 iced → 出图 → 按验收口径打日志。
//! 参数解析在 [`super::request`]，窗口状态机与采样闸门在 [`super`]，OS 侧密闭指标在
//! [`super::caps`]。搬出来没有改行为（同一 argv 的 PNG sha256 不变）。

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use iced::{window, Point, Task};

use unlucky_presenter::prefs::{AppearanceBase, AppearancePrefs};

use crate::app::components::interaction::Interaction;
use crate::cli::Backend;
use crate::fonts;
use crate::pages::appearance;
use crate::probe::{self, Probe};
use crate::renderer;
use crate::theme::Ui;

use super::{demo_model, Harness, Message, Request, CAPTURE_POSITION};

/// **终态判据**：取证运行跑完了却一张图都没落盘 ⇒ 失败。
///
/// 单独拎成纯函数是为了让它能被断言（`run()` 要真窗口才能跑）。它守的是 M100 r3 P2-1 那个
/// 真实缺陷：窗口在第 3 帧消失时，`window::frames()` 不再产消息 ⇒ 帧超时兜底**求值不到**，
/// 而 iced_winit 对「最后一个窗口 Destroyed」直接退出 ⇒ 曾经是 **exit 0 + 没有 PNG**。
///
/// ## 为什么必须有 `capture_mode` 前提（M100 r4 P2-1）
///
/// 不给 `--screenshot` 是文档明确支持的**交互预览模式**（「没给 `--screenshot` 就只是开窗看」）：
/// 该模式 `subscription()` 直接 `Subscription::none()`，**永远不会有 `Captured`** ⇒ `written` 恒 0。
/// 没有这个前提，用户**正常关掉预览窗**会拿到 exit 1 和一句「取证结束但没有任何图落盘」的
/// 假消息——那是把"从未要求出图"判成"取证失败"，通道的退出码就此失去区分力
/// （真崩溃 / 真取证失败 / 正常关个预览窗全是 1）。
///
/// ## 为什么用计数器而不是 `path.exists()`
///
/// `written` 是**本进程**成功落盘的次数，不会被执行前就存在的旧 PNG 骗过；换成「目标文件存在」
/// 会在脏路径下给出「exit 0 + 上一次的旧图」。这一点写在这里，免得后来人「顺手改成 exists」。
pub(super) fn no_evidence(capture_mode: bool, written: u32) -> bool {
    capture_mode && written == 0
}

/// 启动（阻塞到出图或关窗）。
///
/// **取证通道的硬规则（M97）**：宁可红着退出，也不能悄悄换一种后端继续出图（以前 wgpu 挂掉时
/// 会静默降级 tiny-skia 照样落 PNG，而报告上写着「GPU 渲染截图」）。所以这里 ① 决策被
/// [`renderer::strict`] 剥掉兜底、② 起不来带**具名**错误返回、③ 每张图日志都带 `backend=`。
///
/// # Errors
/// 后端探测/字体求值失败、**渲染后端起不来（含 iced 的具名图形错误）**、截图落盘失败时返回人话。
pub fn run(request: Request) -> Result<(), String> {
    let started = Instant::now();

    let needs_probe = request.backend == Backend::Auto || renderer::acceptance::enabled();
    let probe = if needs_probe {
        Probe::run()
    } else {
        Probe::default()
    };
    // 剥掉兜底：gallery 没有「偷偷换 CPU」这个选项。
    let decision = renderer::strict(probe::apply(request.backend, &probe));
    eprintln!(
        "[shot] 出图后端 = {} reason={}（取证通道无回退：起不来就红着退出，绝不换 CPU 出图）",
        decision.primary.name(),
        decision.reason
    );
    renderer::acceptance::report_backend_plan(&decision, request.backend);

    let resolved = fonts::resolve();
    eprintln!("{}", resolved.describe());
    let families = resolved.families();

    // 这一份就是喂给 iced 的真值：`[window]` 那行从它取字段（M98 r1 p2-1 的同一个教训）
    let settings = window::Settings {
        size: iced::Size::new(request.width, request.height),
        decorations: false,
        resizable: true,
        // 取证窗口钉在固定落点：这样「把指针放进窗口某个位置」是可复算的（真实矩形由
        // `Harness` 用 `window::position` / `window::size` 的返回值打日志报出，见 `Message::Sized`）
        position: match request.screenshot {
            Some(_) => {
                window::Position::Specific(Point::new(CAPTURE_POSITION.0, CAPTURE_POSITION.1))
            }
            None => window::Position::default(),
        },
        ..window::Settings::default()
    };
    renderer::acceptance::report_window(
        &renderer::acceptance::WindowFacts::from_settings(&settings),
        request.scale_factor,
        "pages_demo",
        None,
    );

    // 外观**真状态**（`--appearance=real`）：工厂态 + `ULL_APPEARANCE_*` 覆盖。
    // 这不是给设置页塞常量：状态从 `State::new` / `apply_env` 来，打日志的是它算出来的生效值。
    let appearance = request.real_appearance.then(|| {
        let mut state = appearance::State::new(
            &AppearancePrefs::factory(),
            &AppearanceBase::factory(),
            request.backend,
            request.flavor,
        );
        state.apply_env();
        eprintln!(
            "[harness] 外观真状态：window={:.2} panel={:.2}（生效 {:.2}）bg={} fill={:?} dim={:.2} \
             配色来源={:?} 选中分组={}",
            state.window_opacity,
            state.panel_alpha,
            state.effective_panel_alpha(),
            state.bg_enabled,
            state.bg_fill,
            state.bg_dim,
            state.palette_source,
            state.selected_group,
        );
        state
    });

    // 落盘计数器（终态不变量 + 日志共用）：只在 `shot::save` 成功后自增。
    let written = Arc::new(AtomicU32::new(0));

    let application = |backend: renderer::Kind| {
        let boot = request.clone();
        let appearance = appearance.clone();
        // 落盘次数要能被 `run()` 读到（终态不变量用它），所以走共享计数器而不是 `Harness` 的私有字段：
        // 值只在 `shot::save` **成功**之后自增，是「真的写出去了」而不是「我们打算写」（U1）。
        let written = Arc::clone(&written);
        iced::application(
            move || {
                let mut model = demo_model(boot.screen, boot.width);
                // 截图通道把**消费 forced 的那批控件**（实例页）钉成常态，与产品 `--screenshot`
                // 的 `ForcedState::Rest` 同一条纪律。它管不到设置页 / 外观区（那边不读 forced），
                // 那两屏的密闭靠窗口级 passthrough——别把这条当成万能的。
                if boot.screenshot.is_some() {
                    model.forced = Some(Interaction::Enabled);
                }
                (
                    Harness {
                        ui: Ui::new(boot.flavor, families),
                        model,
                        appearance: appearance.clone(),
                        request: boot.clone(),
                        window: None,
                        hwnd: None,
                        placed: None,
                        frames: 0,
                        dispatched: false,
                        written: Arc::clone(&written),
                        pointer: 0,
                        started,
                        first_frame: None,
                        backend: backend.name(),
                    },
                    Task::done(Message::Boot),
                )
            },
            Harness::update,
            Harness::view,
        )
        .theme(Harness::theme)
        .subscription(Harness::subscription)
        .default_font(iced::Font {
            family: iced::font::Family::Name(families.text),
            ..iced::Font::DEFAULT
        })
        .window(settings)
    };

    eprintln!(
        "[harness] screen={} width={} height={} slug={}（布局档位 {}）",
        request.screen.slug(),
        request.width,
        request.height,
        request.slug(),
        crate::pages::layout::WidthClass::of(request.width).name()
    );

    // 兜底已被 `strict` 剥掉，所以这条最多跑一次；具名错误的口径与 App 启动一致，
    // 但**取证通道不许降级**：出不了图就红着退出，不换后端（那是另一处 iced 行为）。
    let session_started = Instant::now();
    let result = renderer::run_once(decision, |backend| {
        std::env::set_var("ICED_BACKEND", backend.env_value());
        let application = application(backend);
        match request.scale_factor {
            Some(scale) => application.scale_factor(move |_| scale).run(),
            None => application.run(),
        }
    });
    let session = session_started.elapsed();

    let outcome = result.map_err(|error| match renderer::named_graphics_error(&error) {
        Some(named) => format!(
            "渲染后端 {} 初始化失败（取证通道无回退，未出图）：{named}",
            decision.primary.name()
        ),
        None => format!("iced 启动失败：{error}"),
    });

    // 取证通道恒为 evidence 启动 ⇒ 有兜底时 via=refuse、没兜底时 none（不再写死，M98 r1 p3-4）
    renderer::acceptance::report_outcome(
        &decision,
        outcome.as_ref().err().map(String::as_str),
        renderer::acceptance::failure_via(decision.fallback.is_some(), true),
        "screenshot",
        session,
    );

    outcome?;

    // **终态不变量（M100 r3 P2-1）**：取证跑完了却一张图都没落盘 = 失败。
    //
    // 为什么要有这一条（复审变异实测）：兜底原来只有「帧数超时」，而 `window::frames()` 只在
    // `Event::Window(RedrawRequested)` 时产消息 —— 窗口一旦消失就不再 redraw ⇒ `Message::Frame`
    // 不再产生 ⇒ `capture_stalled` **永远求值不到**；同时 iced_winit 对「最后一个窗口 Destroyed」
    // 直接 `Control::Exit`，`run()` 返回 `Ok(())`。表现是 **exit 0 + 没有 PNG**，与成功跑同一形状
    // 只少了出图那几行 —— 只看退出码的脚本会读到「成功 + 零证据」。
    //
    // 所以判据从「超时」升级成「终态」：**跑完了没有图就是失败**。它比超时强，因为覆盖
    // 「帧不再产生」这一类；帧超时那条保留，管「有帧但采样回不来」。
    //
    // **前提是这次真的是取证运行**（M100 r4 P2-1）：不给 `--screenshot` 的交互预览模式本来
    // 就不出图，判成失败会打出假消息、并让退出码失去区分力。前提写在判据里，不写在这里。
    if no_evidence(
        request.screenshot.is_some(),
        written.load(Ordering::Relaxed),
    ) {
        eprintln!("[harness] 取证结束但没有任何图落盘（窗口提前消失 / 落盘失败），按失败退出");
        std::process::exit(1);
    }

    eprintln!(
        "[harness] 用时 {:.0} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}
