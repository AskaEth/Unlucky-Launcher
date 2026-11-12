//! `pages::harness` 的断言（M90 r2 从 `harness.rs` 外置：那边曾 402 行，guards 的 file-size 会判红）。
//!
//! 只测**参数解析 / slug / 采样闸门 / 三态标签**，不碰 iced 运行时。所以「穿透在真实窗口上真的
//! 关掉了命中测试」这条这里证明不了——它要看出图日志里的 `[caps] passthrough_ext=`（三态，
//! 来自 `GWL_EXSTYLE` 的 `WS_EX_TRANSPARENT`）；`指针事件=N` 只是辅助指标（它只证明消息到没到）。

use super::request::Request;
use super::{capture_stalled, should_request_capture, CAPTURE_AT_FRAME, CAPTURE_GIVE_UP_FRAMES};
use crate::cli::Backend;
use crate::pages::Screen;

use unlucky_design::color::SchemeFlavor;

fn parse(args: &[&str]) -> Result<Request, String> {
    Request::parse(args.iter().map(|arg| (*arg).to_owned()))
}

#[test]
fn defaults_are_the_acceptance_frame() {
    let request = parse(&[]).expect("空参数要能解析");
    assert_eq!(request.screen, Screen::Instances);
    assert_eq!((request.width, request.height), (1180.0, 760.0));
    assert_eq!(request.flavor, SchemeFlavor::Light);
    assert!(request.screenshot.is_none());
    assert!(!request.real_appearance, "默认画演示行");
}

#[test]
fn screen_and_width_parse_in_both_argv_forms() {
    let spaced = parse(&["--screen", "settings", "--width", "900"]).unwrap();
    let inline = parse(&["--screen=settings", "--width=900"]).unwrap();
    assert_eq!(spaced, inline);
    assert_eq!(spaced.screen, Screen::Settings);
    assert_eq!(spaced.width, 900.0);
    assert!(parse(&["--screen=gallery"]).is_err());
    assert!(parse(&["--width=abc"]).is_err());
}

#[test]
fn screenshot_theme_wins_over_theme() {
    let request = parse(&["--theme=dark", "--screenshot-theme=light"]).unwrap();
    assert_eq!(request.flavor, SchemeFlavor::Light);
    assert_eq!(parse(&["--theme=dark"]).unwrap().flavor, SchemeFlavor::Dark);
}

#[test]
fn software_renderer_is_an_alias_of_cpu() {
    assert_eq!(
        parse(&["--software-renderer"]).unwrap().backend,
        Backend::Cpu
    );
    assert_eq!(parse(&["--renderer=cpu"]).unwrap().backend, Backend::Cpu);
    assert_eq!(parse(&["--renderer=gpu"]).unwrap().backend, Backend::Gpu);
}

#[test]
fn slug_carries_screen_class_and_flavor() {
    let request = parse(&[
        "--screen=instances",
        "--width=700",
        "--screenshot-theme=dark",
    ])
    .unwrap();
    assert_eq!(request.slug(), "instances-medium-dark-w700");
}

#[test]
fn tiny_windows_are_rejected() {
    assert!(parse(&["--width=200"]).is_err());
    assert!(parse(&["--height=100"]).is_err());
}

/// **密闭不变量（M100）**：截图通道默认开指针穿透；交互模式（没有 `--screenshot`）一律关。
///
/// 抓得住：① 谁把取证默认值改成关（那会让取证图重新受系统指针影响）；
/// ② 谁让交互模式也开穿透（用户窗口收不到鼠标 = 产品事故）。
/// 抓不住：穿透**真的**生效了没有——那要看出图日志的 `指针事件=N`，测试替不了。
#[test]
fn capture_is_sealed_by_default_and_interactive_never_is() {
    assert!(
        parse(&["--screenshot", "o.png"]).unwrap().passthrough,
        "截图通道默认必须开指针穿透"
    );
    assert!(!parse(&[]).unwrap().passthrough, "交互模式不许开指针穿透");
    assert!(
        !parse(&["--screen=settings"]).unwrap().passthrough,
        "交互模式（带页名）同样不许开"
    );
    // 显式给值在交互模式下是**配置错误**，不是「忽略」：静默忽略会让人以为交互窗口也密闭了
    for flags in [
        &["--passthrough=on"][..],
        &["--passthrough", "off"],
        &["--screen=settings", "--passthrough=on"],
    ] {
        assert!(parse(flags).is_err(), "{flags:?} 应该报错");
    }
}

/// 对照实验的口子（`--passthrough=off`）：只服务「不开穿透时 hover 会不会漏进图」这一条。
#[test]
fn the_control_run_can_turn_the_passthrough_off() {
    let shot = parse(&["--screenshot", "o.png", "--passthrough=off"]).unwrap();
    assert!(!shot.passthrough, "显式 off 必须尊重");
    assert!(shot.screenshot.is_some(), "off 只对截图通道有意义");
}

/// **外观真状态那一屏（M100）**：`--appearance=real` 只在设置页成立，且 slug 要与演示行版分开。///
/// 抓得住：① 谁把 real 接到别的页（那会出一张「假装接了线」的图）；
/// ② 两张图落成同名文件（归档后无法区分哪张是真控件版）。
/// 抓不住：那张图里到底有没有控件——那是像素的事，由两张图的 sha256 不同 + tower 亲自看图判。
#[test]
fn the_real_control_screen_is_settings_only_and_says_so_in_the_slug() {
    let real = parse(&["--screen=settings", "--appearance=real"]).unwrap();
    assert!(real.real_appearance);
    assert_eq!(real.slug(), "settings-real-expanded-light-w1180");
    assert_eq!(
        parse(&["--screen=settings"]).unwrap().slug(),
        "settings-expanded-light-w1180",
        "演示行版的 slug 不许变（既有归档脚本按它找图）"
    );
    assert!(
        parse(&["--screen=instances", "--appearance=real"]).is_err(),
        "别的页没有外观分组，静默接受会出一张假图"
    );
    assert!(parse(&["--screen=settings", "--appearance=x"]).is_err());
}

/// **一次一采样（M100 r1 P2-1）**：采样请求最多发一次，且最早在 `CAPTURE_AT_FRAME`。
///
/// 抓得住：谁把闸门改回 `frames >= CAPTURE_AT_FRAME`（那样第 3 帧之后每帧都发一次采样，
/// `Captured` 每次都覆盖同一文件 ⇒ **最后写入者说了算**；复审实测 12 次里 7 次写了两次，
/// 并把窗口在两次采样之间改成 900×600 之后落盘的就是 900×600 那张）。
/// 抓不住：「落盘次数真的是 1」——那要看真实运行的日志 `落盘次数=1`（本测试只测闸门逻辑）。
#[test]
fn the_capture_gate_opens_once_and_never_again() {
    // 窗口到第 4 帧才就绪也要恰好请求一次（`window` 未就绪时不置位，保留重试，不会挂死）
    let mut dispatched = false;
    let mut requests = 0;
    for frames in 1..=10u32 {
        let window_ready = frames >= 4;
        if should_request_capture(frames, dispatched, true) && window_ready {
            dispatched = true;
            requests += 1;
        }
    }
    assert_eq!(requests, 1, "整个进程只许请求一次采样");

    assert!(!should_request_capture(1, false, true), "第 3 帧前不许采样");
    assert!(should_request_capture(3, false, true), "第 3 帧取第一张");
    assert!(
        !should_request_capture(99, true, true),
        "已派发过就不再请求（这条就是 P2-1 的修复本身）"
    );
    assert!(
        !should_request_capture(99, false, false),
        "交互模式（没有 --screenshot）不采样"
    );
}

/// **密闭日志三态（M100 r1 P2-2）**：拿不到句柄必须记 `unknown`，不许记 `off`。
///
/// 抓得住：谁把 `None` 掉成 `off`（那会把「没读到」说成「读到了、没开」——这台机器上从此永远
/// 看不出密闭到底生效没有）。
/// 抓不住：真实窗口上的位（那要看运行日志里的 `passthrough_ext=on`）。
#[test]
fn the_passthrough_capability_is_three_states() {
    assert_eq!(super::caps::passthrough_label(None), "unknown");
    assert_eq!(
        super::caps::passthrough_label(Some(0)),
        "unknown",
        "句柄 0 = 还没拿到窗口"
    );
}

/// **兜底不许在正常路径误触发**（M100 r2 p3-b）。
///
/// 抓得住：宽限帧数被改小到「采样还在飞行中就判死」（例如把它写成 0）——那时第 4 帧
/// （第 3 帧派发、第 5 帧才落盘）就会误触发，CI 上表现为「正常出图却红」。
/// 抓不住：真实 `window::screenshot` 卡死的时序（那要注入一个不回的采样，测试替不了）。
#[test]
fn the_stall_guard_never_fires_on_the_normal_path() {
    // 实测时序：第 3 帧派发，第 4~5 帧才收到 Captured（采样在飞行中 writes 还是 0）
    let mut first_bad: Option<(u32, bool, u32)> = None;
    for frames in 1..=200u32 {
        let dispatched = frames >= CAPTURE_AT_FRAME;
        let writes = u32::from(dispatched && frames >= CAPTURE_AT_FRAME + 2);
        if capture_stalled(frames, writes) {
            first_bad.get_or_insert((frames, dispatched, writes));
        }
    }
    assert!(
        first_bad.is_none(),
        "正常路径误触发兜底（帧, dispatched, writes）={first_bad:?}"
    );
}

/// **兜底只在「有帧但没采样」时触发**，且必须过了宽限窗口才触发。
///
/// 抓得住：① 判据写成 `frames >= CAPTURE_AT_FRAME`（宽限为 0）——正常路径会立刻误触发；
/// ② 判据漏掉 `writes == 0`（已经落过盘还报超时）；
/// ③ 宽限窗口被写成 `>=` 边界错位（差一帧）。
/// 抓不住：`exit(1)` 到底有没有被操作系统收到（那要跑真二进制，见报告里的变异实测）。
#[test]
fn the_stall_guard_fires_only_after_the_grace_window() {
    let threshold = CAPTURE_AT_FRAME + CAPTURE_GIVE_UP_FRAMES;
    assert!(
        !capture_stalled(threshold, 0),
        "刚好到阈值还不算超时（要给最后一帧留余量）"
    );
    assert!(
        capture_stalled(threshold + 1, 0),
        "越过阈值且一次都没落盘 ⇒ 必须放弃（红着退出，不许挂死）"
    );
    assert!(
        !capture_stalled(u32::MAX, 1),
        "已经落过盘就不算卡住（正常路径由 exit() 结束）"
    );
    assert!(!capture_stalled(1, 0), "前几帧本来就没有图，不算卡住");
    // 宽限必须明显大于「派发→落盘」的实测间隔（约 2 帧），否则正常路径会误触发。
    // 用局部绑定而不是直接比较常量：`assert!(CONST >= 10)` 会被 clippy 判成常量断言。
    let grace = CAPTURE_GIVE_UP_FRAMES;
    assert!(grace >= 10, "宽限帧数 {grace} 太小，正常路径会误触发");
}

/// **终态不变量：跑完了没有图就是失败 —— 但只在取证运行里这么判**（r3 P2-1 → r4 P2-1）。
///
/// 抓得住：① 判据被改成恒 false（终态检查成摆设）；② **判据丢掉 `capture_mode` 前提**
/// （r4 实测到的真缺陷：不给 `--screenshot` 的交互预览模式本来就不出图，却会被判失败 ⇒
/// exit 1 + 一句「取证结束但没有任何图落盘」的假消息，退出码因此失去区分力）。
/// 抓不住：`run()` 里那个**调用点**被删掉（判据还在、本测试照绿）——那一层只能靠变异实测
/// （M3：第 3 帧改关窗 ⇒ 期望 exit 1），测试替不了。
#[test]
fn finishing_without_a_single_write_is_a_failure_only_while_capturing() {
    use super::app::no_evidence;

    // 红线（r4 P2-1）：交互预览模式关窗**不是**失败
    assert!(
        !no_evidence(false, 0),
        "交互预览模式（没给 --screenshot）从未要求出图，关窗必须 exit 0"
    );
    assert!(
        no_evidence(true, 0),
        "取证运行一张图都没落盘 ⇒ 必须判失败（不许 exit 0）"
    );
    assert!(!no_evidence(true, 1), "落过盘就算有证据");
    assert!(!no_evidence(false, 1), "预览模式更不该报错");
}
