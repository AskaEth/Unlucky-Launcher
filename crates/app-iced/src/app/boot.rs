//! 启动编排：**探测 → 字体 → 打印诊断 → 起 iced**（四步，别把它撑回一坨）。
//!
//! ## 三条纪律（每条都对应一次返工）
//!
//! 1. **后端起不来时的两条路策略不同**（判据表第 8/9 行）：交互启动换进程重开、取证启动
//!    （`--screenshot`）拒绝出图 —— 实现与测试都在 [`fallback`]。
//! 2. **窗口设置只有一个产地**（[`diagnostics::window_settings`]），`[window]` / `[caps]` 两行日志
//!    从**同一个** `&window::Settings` 取字段。M98 r1 p2-1 的教训：把 `decorations=false` 写进
//!    格式化串，等于让日志与真实行为可以各走各的（踩坑记录 U1）。
//! 3. **没测过的就写 `skipped`**，不许拿 `0ms` 冒充实测（[`diagnostics::report_font`] 的 `probe`）。
//!
//! 为什么重开必须是**换进程**：Windows 实测本进程里再 `run()` 一次直接 panic
//! （`iced_winit-0.14.1/src/lib.rs:81` → `Create event loop: RecreationAttempt`）。

use crate::app::diag::diag;
use std::time::{Duration, Instant};

use iced::Task;

use unlucky_design::color::SchemeFlavor;

use crate::cli::Options;
use crate::fonts;
use crate::probe;
use crate::renderer::{self, acceptance, Decision};
use crate::theme::Families;

use super::message::Message;
use super::state::{App, STARTED};
use super::{MIN_SIZE, WINDOW_SIZE};

mod diagnostics;
mod fallback;
mod termux_gpu;
mod termux_window;

use fallback::{on_failure, respawn, OnFailure};

/// 启动 iced（阻塞到退出）。`--diff` 不走这里，见 `main.rs`。
pub(crate) fn run(options: Options) -> iced::Result {
    let _ = STARTED.set(Instant::now());
    let boot_started = Instant::now();

    // ① 探测（判据取在哪、日志在 `report_boot` 里统一打）
    // Termux + glibc 构建：装了我们自编的 turnip ICD 就先生效，auto 也能选到 GPU。
    termux_gpu::enable();
    let boot = boot_probe(&options);
    let decision = boot.decision;
    diagnostics::report_boot(&options, &boot.probe, &decision);

    // ② 字体（`fonts` 自己打两行既有日志；新加的那行在 `report_font` 里）
    let font_started = Instant::now();
    let resolved = fonts::resolve();
    let font_ms = font_started.elapsed();
    diag!("[font] 求值耗时 {:.0} ms", font_ms.as_secs_f64() * 1000.0);
    diag!("{}", resolved.describe());
    if resolved.has_synthetic() {
        diag!(
            "[font] 警告：有字重落在「合成加粗」档（观感降级），D28 后半（CDN 字体/按档换字族）要处理"
        );
    }
    let families = resolved.families();

    // ③ 打印（主题来源 + 字体回退链 + 启动耗时）
    // 截图模式：`--screenshot-theme` > `--theme` > **Light**（唯一取值口 `Options::capture_flavor`）。
    // **不再有**「等系统主题到达」的闸门——CI 的 xvfb 没有主题会话，等它就是挂死（见 cli 文档）。
    if options.screenshot.is_some() && options.shot_flavor().is_none() {
        diag!("[shot] 未指定主题，按 Light 出图（D30 可复现；CI 冒烟依赖这条默认值）");
    }
    let flavor = options.capture_flavor().unwrap_or(SchemeFlavor::Light);
    diagnostics::report_theme(&options, flavor);
    diagnostics::report_font(
        resolved.platform,
        boot.probe_ms,
        font_ms,
        boot_started.elapsed(),
    );

    // ④ 起 iced（失败按判据表第 8/9 行分流；具名原因在分流之前就打到同一个 stderr 上了）
    let launch_started = Instant::now();
    let result = match launch(&options, families, flavor, decision.primary) {
        Err(iced::Error::GraphicsCreationFailed(error)) => {
            renderer::report_failure(decision.primary, &error);
            match on_failure(decision, options.screenshot.as_ref()) {
                Some(OnFailure::Respawn(fallback)) => {
                    respawn(decision.primary, fallback, &error.to_string())
                }
                Some(OnFailure::Refuse) => {
                    // 取证启动：**拒绝降级出图**（换了后端出的图会让报告撒谎）。
                    diag!(
                        "[gpu] 拒绝降级出图：本次是取证启动（--screenshot {}），换后端出的图会撒谎。",
                        options
                            .screenshot
                            .as_ref()
                            .map_or_else(String::new, |path| path.display().to_string())
                    );
                    diag!(
                        "[gpu] 想用 CPU 出图请显式加 --renderer=cpu / --software-renderer（CI 就是这么跑的）。"
                    );
                    Err(iced::Error::GraphicsCreationFailed(error))
                }
                None => Err(iced::Error::GraphicsCreationFailed(error)),
            }
        }
        other => other,
    };
    let launch_elapsed = launch_started.elapsed();

    diagnostics::report_outcome(
        &decision,
        result.as_ref().err().map(|error| {
            renderer::named_graphics_error(error).unwrap_or_else(|| error.to_string())
        }),
        acceptance::failure_via(decision.fallback.is_some(), options.screenshot.is_some()),
        if options.screenshot.is_some() {
            "screenshot"
        } else {
            "interactive"
        },
        launch_elapsed,
    );

    result
}

/// 按一次后端启动 iced。`backend` 决定这次写进 `ICED_BACKEND` 的唯一一项。
fn launch(
    options: &Options,
    families: Families,
    flavor: SchemeFlavor,
    backend: renderer::Kind,
) -> iced::Result {
    std::env::set_var("ICED_BACKEND", backend.env_value());

    let boot_options = options.clone();

    // M98：`ULL_WINDOW_SIZE=1520x900` 覆盖尺寸（矩阵的「最小 / 宽屏」两档靠它，且必须走**真外壳**
    // ——`pages_demo` example 的标题栏是留白带，出不了「三键位置」那张图）。
    // Termux：没有环境覆盖时，默认尺寸按 X 屏幕等比自适应（横屏优先），
    // 失败就退回 1180×760 的验收尺寸。
    let (default_size, default_source) = termux_window::default_size(
        (WINDOW_SIZE.width, WINDOW_SIZE.height),
        (MIN_SIZE.width, MIN_SIZE.height),
    )
    .unwrap_or(((WINDOW_SIZE.width, WINDOW_SIZE.height), "default"));
    let (size, size_source) =
        acceptance::window_size(default_size, (MIN_SIZE.width, MIN_SIZE.height));
    let size_source = if size_source == "default" {
        default_source
    } else {
        size_source
    };
    let settings = diagnostics::window_settings(size);
    // 两行日志与开窗用的是**同一个** settings（见模块文档纪律 2）
    diagnostics::report_window(&settings, options.scale_factor, size_source);

    let application = iced::application(
        move || {
            (
                App::new(boot_options.clone(), families, flavor),
                Task::done(Message::Boot),
            )
        },
        App::update,
        App::view,
    )
    .title(App::title)
    .theme(App::theme)
    .subscription(App::subscription)
    .default_font(default_font(families))
    .window(settings);

    match options.scale_factor {
        Some(scale) => application.scale_factor(move |_| scale).run(),
        None => application.run(),
    }
}

/// [`boot_probe`] 的产物：判据 + 探测到的适配器 + **探测耗时（没测就是 `None`）**。
///
/// 为什么把 `Probe` 带出来：`[env]` 的 `gpu=` / `hardware=` 必须是**这一次真枚举到的**结果
/// （再枚举一遍多花 0.65~1.2 s，还可能给出不同状态）。判据与诊断看同一份数据。
struct Boot {
    decision: Decision,
    probe: probe::Probe,
    probe_ms: Option<Duration>,
}

/// 渲染后端探测 + 写 `ICED_BACKEND`，返回判据产物（供 `run` 决定要不要换进程重开）。
fn boot_probe(options: &Options) -> Boot {
    // 「上一步失败后重开」的那一次：别再探（会得到同一个结论，然后无限重开）。兜底必须是 None
    // ——由 `decision::fallback_round` 单点保证。
    if let Some(kind) = renderer::forced_fallback() {
        let decision = renderer::fallback_round(kind);
        std::env::set_var("ICED_BACKEND", kind.env_value());
        diag!(
            "[probe] 本次是降级重开（{}={}）：跳过探测 -> ICED_BACKEND={}",
            renderer::FALLBACK_ENV,
            kind.env_value(),
            kind.env_value()
        );
        diag!("[probe] {}", decision.describe());
        // 验收要 `[env]` 里的真实适配器：这一轮**也枚举一次，并如实计时**（没计时就 `None`
        // ⇒ 日志写 `probe=skipped`，不写 0ms）。
        let (probe, probe_ms) = if acceptance::enabled() {
            let started = Instant::now();
            let probe = probe::Probe::run();
            (probe, Some(started.elapsed()))
        } else {
            (probe::Probe::default(), None)
        };
        return Boot {
            decision,
            probe,
            probe_ms,
        };
    }

    // 只有 `auto` 需要探测（`cpu`/`gpu` 的决策与硬件无关，枚举 13 个适配器本机要 0.65~1.2 s）。
    // 验收模式例外：`[env]` 那行要 GPU 名，不管请求是哪个后端。
    let needs_probe = options.backend == crate::Backend::Auto || acceptance::enabled();
    let probe_started = Instant::now();
    let probe = if needs_probe {
        probe::Probe::run()
    } else {
        probe::Probe::default()
    };
    let probe_ms = needs_probe.then(|| probe_started.elapsed());

    // `ULL_PREFLIGHT=1` 时先自己验一遍 wgpu（判据表第 7 行）；默认不跑，见 preflight 模块文档。
    let preflight = renderer::preflight::run_if_requested();
    let decision = probe::apply_with_preflight(options.backend, &probe, preflight.as_ref());

    if cfg!(feature = "gpu") {
        match probe_ms {
            Some(elapsed) => diag!(
                "[probe] 探测耗时 {:.0} ms（wgpu 实例 + 全后端适配器枚举）",
                elapsed.as_secs_f64() * 1000.0
            ),
            None => diag!(
                "[probe] 探测耗时 跳过（--renderer={} 与硬件无关，未枚举）",
                options.backend.name()
            ),
        }
    }
    Boot {
        decision,
        probe,
        probe_ms,
    }
}

/// 默认字体（正文档解析出来的族）。
fn default_font(families: Families) -> iced::Font {
    iced::Font {
        family: iced::font::Family::Name(families.text),
        ..iced::Font::DEFAULT
    }
}

#[cfg(test)]
#[path = "boot/tests.rs"]
mod tests;
