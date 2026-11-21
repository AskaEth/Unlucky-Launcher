//! 截图 harness 的**运行部分**（M100 r1：`harness.rs` 撞 400 行后从那里搬出来）。
//!
//! 这里只有 [`run`]：探后端 → 造外观真状态 → 起 iced → 出图 → 按验收口径打日志。
//! 参数解析在 [`super::request`]，窗口状态机在 [`super::state`]、采样闸门在 [`super`]，
//! OS 侧密闭指标在 [`super::caps`]。搬出来没有改行为（同一 argv 的 PNG sha256 不变）。
//! M416：状态机主体（`Harness` 的 `update` / `theme` / `view`）随 `harness.rs` 贴线再搬了一站，
//! 落到 `super::state`；`Harness` 本体、采样闸门、以及 `subscription` 仍在 [`super`]。
//!
//! M450（#25）：`app/subscription.rs` 那条守门不再按**文件名**钉 `pages/harness.rs`，改按「落在
//! 哪个模块」数（`pages/harness.rs` 与 `pages/harness/**` 同属 harness 那棵子树）——所以
//! `subscription` 现在**可以**搬到任何 harness 子模块而不红，只是还没搬（`../harness.rs` 那份注释
//! 里的话是改判据之前的理由）。

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

/// 造外观真状态（`--appearance=real` / 强调色出图）：第三个实参是**本次出图的后端**。
///
/// ## M450（#55）：这里以前收的是**请求值**
///
/// 旧代码把 `request.backend` 传进 [`appearance::State::new`]，而它的默认值 `Backend::Auto`
/// 是「探测之前的请求」：在「auto + 探针只认到软光栅」的机器上进程实际跑 `tiny-skia`，
/// `State` 却记着 `Auto` ⇒ 它的分档规则（`panel_is_forced_opaque` / `effective_glass` /
/// `window_alpha_byte`）全部按「非 CPU 档」求值，日志里那句「生效 α」也就与进程真在跑的档无关。
///
/// **今天这些值在 harness 路径上只进那行日志**：`pages/**` 里没有第二个消费者（产品侧的消费者是
/// `app/state/theme.rs`），所以这次改的是「State 看到哪个档」，**像素没变**——M450 实测同一个
/// argv 在改前/改后出的 PNG sha256 相同（`f32caa1c…8a5`，7 次跑全等）。但「日志说的档 ≠ 进程真
/// 在跑的档」就是 #55 本身：视觉验收正是读这行日志，而任何一个将来接上分档值的控件也会跟着错。
///
/// 现在取的是 [`renderer::Decision::primary`] 的档位（[`renderer::Kind::tier`]，`renderer/decision.rs`
/// 里 `Kind` 与 `Backend` 之间**唯一**的换算）。它就是 `run()` 交给
/// [`renderer::run_once`] 的那一项，也就是 iced 真会用的后端：`strict` 已经剥掉兜底
/// （判据表第 9 行），不存在「primary 失败后偷偷换一个」。
///
/// 为什么不搬进 `application` 闭包「按真后端现造」：`State::new` 会读背景图、解析配色，
/// 而闭包对每个尝试过的后端各跑一次；搬进去会多读一次盘、多打一套日志——那是另一件事。
pub(super) fn appearance_state(
    request: &Request,
    decision: renderer::Decision,
) -> appearance::State {
    let mut state = appearance::State::new(
        &AppearancePrefs::factory(),
        &AppearanceBase::factory(),
        decision.primary.tier(),
        request.flavor,
    );
    state.apply_env();
    // M159：`ULL_SETTINGS_GROUP=<下标>` 在 harness 里也生效（产品路径走
    // `app/state/construct.rs` 的同一条判据）。没有它就只能拍默认的「General」组，
    // 而设置页有五组——「更新」组的验收图正是靠它拍出来的。
    if let Some(index) = crate::ui::lang::screenshot_settings_group(request.screenshot.is_some()) {
        state.selected_group = index;
    }
    state
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
    //
    // M250：**gallery 也要这一份** —— 强调色出图矩阵拍的是组件库（buttons + progress），
    // 而 `--appearance=real` 按设计只对设置页生效（`request.rs` 的守卫，别绕过它）。所以判据加
    // 一条：设了 `ULL_APPEARANCE_ACCENT` 就造状态，让 `Ui` 按强调色求值（设置页的语义不变）。
    let wants_accent = std::env::var_os(appearance::accent::ENV_ACCENT).is_some();
    let appearance = (request.real_appearance || wants_accent).then(|| {
        let state = appearance_state(&request, decision);
        eprintln!(
            "[harness] 外观真状态：window={:.2} panel={:.2}（生效 {:.2}）bg={} fill={:?} dim={:.2} \
             配色来源={:?} accent={} 选中分组={}",
            state.window_opacity,
            state.panel_alpha,
            state.effective_panel_alpha(),
            state.bg_enabled,
            state.bg_fill,
            state.bg_dim,
            state.palette_source,
            state.accent.log_token(),
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
                let mut model = demo_model(boot.screen, boot.width, boot.height);
                // 截图通道把消费 forced 的控件钉成常态，与产品 `--screenshot` 的 `ForcedState::Rest`
                // 同一条纪律；显式的 `--force-state=hover|pressed` **优先**（那是故意要画那个态）。
                //
                // M285：这条线现在真的到得了设置页 / 外观区——`pages::Model.forced` 经
                // `settings::view(.., forced)` → `Group.body` → 语言档 / 主题档 / 日志按钮 / 外观 choice，
                // 由 `settings::resolve_state` 落到样式闭包。此前那四处的状态层只有单测证据。
                model.forced = boot
                    .force_state
                    .or_else(|| boot.screenshot.is_some().then_some(Interaction::Enabled));
                // M250：gallery 出图也要能看见强调色。**规则不在这里抄** —— 调产品用的同一个
                // `theme::tokens`（优先级只定义一次）。如实补一句：harness 不建 `App`，所以这里
                // 没有 D38 取色方案（第三个实参恒 `None`），出图**不覆盖**那条分支。
                let (ui, _) = crate::theme::tokens(
                    boot.flavor,
                    appearance.as_ref().and_then(|state| state.accent.color()),
                    None,
                    families,
                );
                // 渲染档（D27）的真值：本闭包的参数就是这次出图的**实际**后端（判据表第 1~7 行
                // 的产物），不是请求值。三处必须同改，见 `theme::tier` 的模块文档；这里是③。
                let ui = ui.with_render_tier(crate::theme::tier::of_kind(Some(backend)));
                (
                    Harness {
                        ui,
                        model,
                        appearance: appearance.clone(),
                        form: crate::pages::demo_launch_form(),
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
                        after_fired: false,
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

/// `--after` 的那一次交互动作（M361 r3）。「只派一次」由调用方（`Harness::update`）管。
///
/// 词表只有一项 [`super::request::After::ContentSelect`] = `pages::content::snap_to_detail()` —— 与
/// `pages/content/message.rs::Select` 派的是**同一个函数**。所以这张图证明的是「派了之后长这样」，
/// 「点一下真的会派」由 `app/update/content/tests.rs` 的 Task 计数用例钉；两条合起来才是完整证据链。
pub(super) fn after_task(action: super::request::After) -> Task<super::Message> {
    use super::request::After;
    match action {
        After::ContentSelect => crate::pages::content::snap_to_detail().map(super::Message::Page),
    }
}

#[cfg(test)]
mod tests {
    //! [`super::appearance_state`] 的守卫（M450 / #55）：`run()` 要真窗口，这条只能测到值的层面。

    use super::*;

    /// `State` 拿到的必须是**实际后端**，不是请求值（#55）。
    ///
    /// 判据：`--renderer` 默认 `Auto` + 探针认不到硬件适配器 ⇒ 判据表第 4/5 行都判 `tiny-skia`
    /// 起步，于是 `State` 必须以为自己在 CPU 档——「CPU 档面板 α 强制 1.0」这条分档逻辑才生效。
    ///
    /// 抓得住：第三实参换回 `request.backend` ⇒ `state.backend` 变成 `Auto`，前两条断言当场红；
    /// 换算写成 `Kind::tier` 之外的（凭空造名）⇒ 同上；调用点不再传 `decision` ⇒ 末尾两条文本断言红。
    /// 抓不住：**真机**上 `Auto` + 真有独显那条（要真 GPU，CI 只用本用例替）。
    #[test]
    fn the_appearance_state_sees_the_backend_the_harness_actually_uses() {
        let request =
            Request::parse(["--screenshot", "out.png"].map(str::to_owned)).expect("要能解析");
        assert_eq!(request.backend, Backend::Auto, "前提：默认请求值是 Auto");
        let probe = Probe::default();
        assert!(!probe.hardware, "前提：`Probe::default()` 没有硬件适配器");

        // 与 `run()` 同一条链（只差 `probe::apply` 里写 `ICED_BACKEND` 那个副作用）。
        let decision = renderer::strict(renderer::decide(
            request.backend,
            &probe,
            cfg!(feature = "gpu"),
        ));
        // harness **实际使用**的后端 = 交给 `renderer::run_once` 的那一项。
        let used = decision.primary;
        assert_eq!(used, renderer::Kind::TinySkia, "前提：CPU 起步");

        let mut state = appearance_state(&request, decision);
        assert_eq!(
            state.backend,
            used.tier(),
            "State 看到的必须是实际后端（`Kind::tier`）"
        );
        assert_eq!(state.backend, Backend::Cpu, "请求值 Auto 不是实际后端");
        // 分档逻辑真的生效：面板被要成 0.5 的半透明，CPU 档必须强制回 1.0。
        state.panel_alpha = 0.5;
        assert_eq!(
            state.effective_panel_alpha(),
            1.0,
            "CPU 档必须强制 α=1.0（这正是 #55 里失效的那条规则）"
        );

        // 调用点也钉住：`run()` 里这两处必须是**同一份** `decision`。
        let src = include_str!("app.rs");
        assert!(
            src.contains("renderer::run_once(decision,"),
            "iced 收到的就是 decision"
        );
        assert!(
            src.contains("appearance_state(&request, decision)"),
            "State 必须由同一份 decision 造（不许换回 request.backend）"
        );
    }
}
