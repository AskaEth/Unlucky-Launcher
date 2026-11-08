//! 验收诊断（M98）：把「借朋友一台 Mac、反复问 7 个问题」压成「一次运行、日志自己回答」。
//!
//! ## 八类日志的分工（M98 任务表）
//!
//! | 类 | 谁打 | 是不是真实结果 |
//! |---|---|---|
//! | `[env]` | [`locale`] | 是（`os/arch/cores` 来自编译目标；`gpu` 来自真适配器枚举；`language` 来自下面的语言链） |
//! | `[backend]` | 本模块（`app::boot` / `pages::harness` 调用） | 是（单项 `ICED_BACKEND` ⇒ 起步成功就是它；失败带具名错误） |
//! | `[window]` | 本模块 | **请求值**（`requested=`）。实际值由脚本从 `[shot] …scale=` 与 PNG 像素尺寸换算 |
//! | `[font]` | `fonts`（已有）+ [`visual`]（`chain`） | 是：`platform=` 是实际落点，`chain=` 是令牌声明的回退链 |
//! | `[theme]` | 本模块 | 是（来源 = 哪个参数/系统决定；系统真值更晚，由已有 `[theme] 系统主题 = …` 打） |
//! | `[perf]` | 本模块 | 是（`Instant` 实测） |
//! | `[i18n]` | [`locale`] | 是（四级链的**每一级原始值**都打出来，结论由 `unlucky_i18n::lang::resolve` 给） |
//! | `[caps]` | [`visual`] | 半观测（`bundle` / `menu_bar_name` 从可执行文件路径读）、半**请求值**（`transparent` / `decorations`） |
//!
//! **反模式警告（本项目反复踩的那一类）**：在需要「结果」的地方打「请求」。所以本模块凡是请求值
//! 都给字段加后缀（`requested` / `…_requested`），凡是推断出来的都给 `_source`，拿不到就写
//! `<未设置>` / `pending` / `none`，**不猜**。
//!
//! ## 开关
//!
//! [`ACCEPTANCE_ENV`]（`ULL_ACCEPTANCE=1`）打开「贵的那部分」：`[env]` 里的适配器枚举（本机
//! 0.65~1.2 s）在 `--renderer=gpu|cpu` 时本来会被跳过，验收要它 ⇒ 这个开关强制枚举一次。
//! 关掉它对产品行为没有任何影响：本模块只打日志，不改决策。
//!
//! 便宜的行（`[backend]` / `[window] requested` / `[theme]`）**没有开关**：报 bug 的人随手截的
//! 日志里就该有「实际用哪个后端」这件事。

use std::time::Duration;

use crate::renderer::{Decision, Kind};
use crate::{Backend, Probe};

pub mod locale;
pub mod visual;
pub mod window;

pub use locale::{env_line, tz, Chain};
pub use visual::{caps_line, font_chain_line, report_caps, report_font_chain};
pub use window::{
    parse_window_size, report_window, resolve_window_size, window_size, window_text, WindowFacts,
    WINDOW_SIZE_ENV,
};

/// 打开「贵的诊断」的开关（见模块文档 §开关）。
pub const ACCEPTANCE_ENV: &str = "ULL_ACCEPTANCE";

/// `ULL_ACCEPTANCE` 是否打开（缺省关；`0` / `off` / `false` / `no` 也算关）。
#[must_use]
pub fn enabled() -> bool {
    match std::env::var(ACCEPTANCE_ENV) {
        Ok(value) => !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        ),
        Err(_) => false,
    }
}

/// 第一行：告诉读日志的人「后面这些类是刻意打开的，不是噪声」。
pub fn announce() {
    eprintln!(
        "[acceptance] on（{ACCEPTANCE_ENV}）八类诊断已开：[env] [backend] [window] [font] [theme] \
         [perf] [i18n] [caps]。关掉：{ACCEPTANCE_ENV}=0"
    );
}

/// 启动那一段（只在不贵的时候由调用方决定打不打）：`[env]` + `[i18n]`。
///
/// 两行都吃**同一次**适配器枚举与**同一次**语言链（`defaults` / `reg.exe` 是子进程，读两次就是
/// 两次开销，而且两次之间值可能变）。
pub fn report_startup(probe: &Probe, scale: Option<f32>, screenshot: bool) {
    // `screenshot` 一路递到 `Chain`：截图通道按 D30 忽略配置，判定必须与产品路径同源
    // （两行日志用的都是 `ui::lang::choose_choice` 那一张表，见 `locale::Chain::read`）。
    let chain = Chain::host(screenshot);
    eprintln!(
        "{}",
        env_line(probe, chain.language(), scale, tz().as_deref())
    );
    eprintln!("{}", chain.line());
}

// ---------------------------------------------------------------- [theme]

/// `[theme]`：本次主题**由谁决定**（来源）+ 语义值。
///
/// `follows_system=true` 时 `flavor=pending`：系统主题是异步到达的（`iced::system::theme()`），
/// 启动这一刻还没有值。真值随后由 `app/update.rs` 的 `[theme] 系统主题 = Dark` 给出——
/// **两行合起来才是完整答案**，缺一行就分不清「跟了系统」和「没跟」。
pub fn report_theme(source: &str, flavor: Option<&str>, follows_system: bool) {
    eprintln!("{}", theme_text(source, flavor, follows_system));
}

/// 主题**由谁决定**（纯判据，好测）：`(来源, 是否跟随系统)`。
///
/// 顺序与 `Options::capture_flavor` 一致：`--screenshot-theme` > `--theme` > 截图默认 Light >
/// 系统。**取值口只有一个**（`capture_flavor`），这里只回答「为什么是它」。
#[must_use]
pub fn theme_source(screenshot_theme: bool, theme: bool, screenshot: bool) -> (&'static str, bool) {
    match (screenshot_theme, theme, screenshot) {
        (true, _, _) => ("screenshot-theme", false),
        (_, true, _) => ("theme", false),
        (_, _, true) => ("default-light", false),
        _ => ("system", true),
    }
}

/// 见 [`report_theme`]（单独返回字符串是为了让测试能逐字断言）。
#[must_use]
pub fn theme_text(source: &str, flavor: Option<&str>, follows_system: bool) -> String {
    format!(
        "[theme] source={source} flavor={} follows_system={follows_system}",
        flavor.unwrap_or("pending")
    )
}

// ---------------------------------------------------------------- [backend]

/// `[backend] plan`：这一次准备试**哪些**后端（单项 + 本 crate 的兜底，不是 iced 的候选回退）。
pub fn report_backend_plan(decision: &Decision, request: Backend) {
    eprintln!("{}", backend_plan_text(decision, request));
}

/// 见 [`report_backend_plan`]。
#[must_use]
pub fn backend_plan_text(decision: &Decision, request: Backend) -> String {
    format!(
        "[backend] plan request={} primary={} fallback={} candidates=1 reason={}",
        request.name(),
        decision.primary.name(),
        decision.fallback.map_or("none", Kind::name),
        decision.reason,
    )
}

/// `[backend] final`：起步的**结果**。
///
/// 判据是 M97 的「单项 `ICED_BACKEND`」：候选只有一项 ⇒ `run()` 能正常起来就必定是它
/// （没有静默回退的空间）。`evidence=probe-single-item` 就是这条推理的名字，别把它读成
/// 「我们查了渲染器内部」——那件事只有 `[gpu] iced_wgpu: …Selected: AdapterInfo…` 那行能做。
pub fn report_backend_started(primary: Kind, elapsed: Duration) {
    eprintln!("{}", backend_started_text(primary, elapsed));
}

/// 见 [`report_backend_started`]。
#[must_use]
pub fn backend_started_text(primary: Kind, elapsed: Duration) -> String {
    format!(
        "[backend] final={} result=started elapsed={}ms evidence=probe-single-item",
        primary.name(),
        elapsed.as_millis()
    )
}

/// `[backend] final`：起步失败（具名错误原样带在 `error=` 里，一行可 grep）。
///
/// `via` = 失败之后去了哪：
/// - `respawn`：交互启动换了进程用兜底后端重开（子进程自己再打一遍 plan/final）；
/// - `refuse`：取证启动（`--screenshot`）拒绝降级出图，**没有图**；
/// - `none`：没有兜底（`--renderer=cpu` / 没编 wgpu），原样失败。
///
/// `final` 是**终态**：`refuse` / `none` 两条路什么都没跑起来 ⇒ `final=none`（不是兜底名）。
/// 第一版让 `final=` 与 `fallback=` 装同一个值，于是 `[backend] final=tiny-skia result=failed`
/// 会被 REPORT 的「后端实际选中」照抄成「这次跑的是 tiny-skia」——报告说谎（M98 r1 p3-4）。
pub fn report_backend_failed(
    primary: Kind,
    fallback: Option<Kind>,
    final_kind: Option<Kind>,
    via: &str,
    error: &str,
) {
    eprintln!(
        "{}",
        backend_failed_text(primary, fallback, final_kind, via, error)
    );
}

/// 见 [`report_backend_failed`]。
#[must_use]
pub fn backend_failed_text(
    primary: Kind,
    fallback: Option<Kind>,
    final_kind: Option<Kind>,
    via: &str,
    error: &str,
) -> String {
    let fallback_name = fallback.map_or("none", Kind::name);
    let final_name = final_kind.map_or("none", Kind::name);
    // `error="…"` 必须是**格式良好的单个字段**：具名错误里常自带双引号（wgpu 的 Debug 输出），
    // 原样塞进去会把这一行撕成两半，脚本按 `error="` 取值就只能取到前半截。
    let error = error.replace('"', "'").replace(['\n', '\r'], " ");
    format!(
        "[backend] final={final_name} result=failed primary={} fallback={fallback_name} \
         via={via} error=\"{error}\"",
        primary.name(),
    )
}

/// 起步失败后去了哪（日志字段 `via=`；与 `app::boot::fallback::OnFailure` 同一套取值）。
///
/// 放在这里而不是 `app/boot/fallback.rs`：`mod boot` 在 `app/mod.rs` 里是私有的（那份文件不在
/// M98 的 scope），`pages::harness` 够不到它，而两个调用点必须用**同一个**判据。
#[must_use]
pub fn failure_via(has_fallback: bool, evidence: bool) -> &'static str {
    match (has_fallback, evidence) {
        (false, _) => "none",
        (true, true) => "refuse",
        (true, false) => "respawn",
    }
}

/// 起步结果 → `[backend] final` 一行 +（验收模式）`[perf] session` 一行。
///
/// `final_kind` 的口径：`respawn` 时是**兜底**（进程即将带着它重开），`refuse` / `none` 时是
/// `None`（什么都没跑起来）——见 [`backend_failed_text`]。成功时恒为 `primary`。
pub fn report_outcome(
    decision: &Decision,
    error: Option<&str>,
    via: &str,
    mode: &str,
    elapsed: Duration,
) {
    let final_kind = match via {
        "respawn" => decision.fallback,
        _ => None,
    };
    match error {
        None => report_backend_started(decision.primary, elapsed),
        Some(message) => report_backend_failed(
            decision.primary,
            decision.fallback,
            final_kind,
            via,
            message,
        ),
    }
    if enabled() {
        report_perf_session(mode, elapsed, error.is_none());
    }
}

/// `[perf] boot`：启动三段（探测 / 字体 / 到 `run()` 之前）的实测毫秒数。
///
/// `probe = None` ⇒ 打 `probe=skipped`。**不许拿 `0ms` 冒充一次实测**（M98 r1 p3-1：降级重开那一轮
/// 没计时却报了 0，而报告说「[perf] 全是 Instant 实测」）。
pub fn report_perf_boot(probe: Option<Duration>, font: Duration, to_run: Duration) {
    eprintln!("{}", perf_boot_text(probe, font, to_run));
}

/// 见 [`report_perf_boot`]。
#[must_use]
pub fn perf_boot_text(probe: Option<Duration>, font: Duration, to_run: Duration) -> String {
    format!(
        "[perf] boot probe={} font={}ms to_run={}ms",
        probe.map_or_else(
            || "skipped".to_owned(),
            |value| format!("{}ms", value.as_millis())
        ),
        font.as_millis(),
        to_run.as_millis()
    )
}

/// `[perf] session`：`iced::application().run()` 从进到出（或到出图）的总时长。
///
/// 取证启动（`--screenshot`）的出图在第 3 帧，所以这条约等于「进程启动 → 出图」——验收里它就是
/// 「这个场景要多久」。交互启动那次是「窗口开了多久」，所以带 `mode=` 区分，别混着读。
pub fn report_perf_session(mode: &str, elapsed: Duration, ok: bool) {
    eprintln!("{}", perf_session_text(mode, elapsed, ok));
}

/// 见 [`report_perf_session`]。
#[must_use]
pub fn perf_session_text(mode: &str, elapsed: Duration, ok: bool) -> String {
    format!(
        "[perf] session mode={mode} elapsed={}ms result={}",
        elapsed.as_millis(),
        if ok { "ok" } else { "failed" }
    )
}

/// `[perf] scene`：**取证场景**（`pages_demo` 通道）的帧数据。
///
/// `fps` 由 `frames / elapsed` 算——注意它是**取证窗口**（第 1 帧到第 N 帧）的吞吐，不是长时间
/// 稳态帧率：产品路径的长跑帧率看 `[tick]` 那串（`ULL_TICK_TRACE=1`，每秒一行、带累计帧数）。
/// 两个来源刻意不合并：一个是「出这张图花了多久」，一个是「放着不动烧不烧风扇」。
pub fn report_perf_scene(
    scene: &str,
    frames: u32,
    first_frame: Option<Duration>,
    elapsed: Duration,
) {
    eprintln!("{}", perf_scene_text(scene, frames, first_frame, elapsed));
}

/// 见 [`report_perf_scene`]。
#[must_use]
pub fn perf_scene_text(
    scene: &str,
    frames: u32,
    first_frame: Option<Duration>,
    elapsed: Duration,
) -> String {
    // fps 只从**首帧之后**的帧间隔算：整段起点到首帧那段是启动成本（字体扫描、适配器枚举），
    // 把它算进帧率会让「8 fps」这种结论完全是启动慢的假象。少于 2 帧就没有间隔可测 ⇒ unknown。
    let fps = first_frame.and_then(|first| {
        let window = elapsed.checked_sub(first)?;
        (frames >= 2 && !window.is_zero()).then(|| f64::from(frames - 1) / window.as_secs_f64())
    });
    format!(
        "[perf] scene={scene} frames={frames} first_frame={}ms elapsed={}ms fps={}",
        first_frame.map_or_else(
            || "unknown".to_owned(),
            |value| value.as_millis().to_string()
        ),
        elapsed.as_millis(),
        fps.map_or_else(|| "unknown".to_owned(), |value| format!("{value:.1}")),
    )
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
