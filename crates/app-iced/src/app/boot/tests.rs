//! `app/boot.rs` 的单元测试 + **变异验证说明**（M97 r1）。
//!
//! 放独立文件（同 `pages/harness/tests.rs`）是为了让 `boot.rs` 留在 400 行内。

//! ## 变异验证怎么做（M97 r1 的教训，改测试前先读这段）
//!
//! 这批测试的价值不是「跑绿了」，而是「把实现改坏会红」。做变异时**只在目标函数体内替换**，
//! 并且把测试里的同一字面量排除在外：
//!
//! ```text
//! # // 示意（真实脚本是一次性的，没入库）
//! def in_body(path, marker, changes):
//!     """按函数体范围替换。**不能**全局 sed：那会连测试期望里的同一字面量一起改掉，
//!     于是实现和断言被换成同一个错值 ⇒ 测试照样绿 ⇒ 验证的是变异脚本自己。"""
//! ```
//!
//! M97 r1 第一次跑变异时，M1/M2/M3 三个变异「照样绿」——不是测试弱，是脚本用全局 `sed`
//! 把断言一起改了。这类假绿最阴：变异验证本来是我们判断「测试够不够」的终极手段，
//! 手段本身假绿时，它还会附赠「已验证」的强信心。

use super::fallback::*;
use super::*;

use crate::renderer::acceptance::WindowFacts;
use crate::renderer::decision;
use crate::renderer::Kind;
use std::ffi::{OsStr, OsString};
use std::path::Path;

fn probe(device_type: &'static str) -> crate::Probe {
    crate::Probe {
        adapters: vec![crate::probe::AdapterSummary {
            name: "adapter".to_owned(),
            device_type,
        }],
        hardware: !matches!(device_type, "Cpu" | "Other"),
    }
}

/// **降级重开到底传了什么**（M97 r1 p2-1 的守门）。
///
/// 抓得住：`fallback_env` 少写/写错任一条——两条都是静默失效型：
/// 漏 `ICED_BACKEND` ⇒ 子进程又去试 wgpu（白开一次窗）；漏 `ULL_GPU_FALLBACK` ⇒ **无限重开**。
/// 抓不住：`Command` 本身没被建起来（那要真起进程，见 §「Windows 实测」）。
#[test]
fn fallback_env_carries_the_backend_and_the_gate() {
    for fallback in [Kind::TinySkia, Kind::Wgpu] {
        let env = fallback_env(fallback);
        assert_eq!(
            env.len(),
            2,
            "就这两条，多一条少一条都要在这里改文档：{env:?}"
        );
        assert!(
            env.contains(&("ICED_BACKEND", fallback.env_value())),
            "子进程必须直接用 {fallback:?} 跑，实际 {env:?}"
        );
        assert!(
            env.contains(&(renderer::FALLBACK_ENV, fallback.env_value())),
            "缺防无限重开的闸门 {FALLBACK_ENV:?}，实际 {env:?}",
            FALLBACK_ENV = renderer::FALLBACK_ENV
        );
    }
}

/// 闸门的另一半：子进程那一轮的决策**不能再有兜底**，否则失败后继续 respawn ⇒ 无限递归。
///
/// 抓得住：`fallback_round` 的 `fallback` 被写成 `Some(..)`（自杀式循环）。
/// 抓不住：`forced_fallback()` 没被 `boot_probe` 调用（那是环境变量的读，见下一条的说明）。
#[test]
fn fallback_round_never_degrades_again() {
    for kind in [Kind::TinySkia, Kind::Wgpu] {
        let decision = decision::fallback_round(kind);
        assert_eq!(decision.primary, kind);
        assert_eq!(
            decision.fallback, None,
            "降级重开那一次不许再有兜底，否则会无限递归重开"
        );
        assert_eq!(decision.reason, "fallback-after-failure");
    }
}

/// 判据表第 8 行 vs 第 9 行：**交互降级 / 取证拒绝**，两条路的策略不同。
///
/// 抓得住：`--screenshot` 分支被改回「也降级」（那正是 M97 要杀的「取证工具自己换后端」）。
/// 抓不住：`screenshot` 传进来的时机（那是 `run()` 的接线，见 Windows 实测记录）。
#[test]
fn evidence_start_refuses_instead_of_degrading() {
    let auto = decision::decide(crate::Backend::Auto, &probe("IntegratedGpu"), true);
    let evidence = std::path::PathBuf::from("out.png");

    assert_eq!(
        on_failure(auto, Some(&evidence)),
        Some(OnFailure::Refuse),
        "取证启动 + wgpu 起不来 ⇒ 拒绝出图"
    );
    assert_eq!(
        on_failure(auto, None),
        Some(OnFailure::Respawn(Kind::TinySkia)),
        "交互启动 + wgpu 起不来 ⇒ 换进程降级"
    );
}

/// **构造侧**：argv 原样透传、两组 env 都在、program 就是传进来的那个。
///
/// 抓得住：重开时丢掉用户参数（`--screenshot` / `--screen` / `--data-dir` 全没了，
/// 子进程会按默认参数跑，那是最难查的一种「重开后行为变了」）、env 少传（M97 r1 的 p2-1）。
/// 抓不住：stdio 继承（`Command` 没有 stdio getter，只能真跑一次，见 `fallback_command` 的注释）。
#[test]
fn fallback_command_passes_program_argv_and_envs() {
    let args = ["--renderer=auto", "--screenshot", "out.png"];
    let command = fallback_command(
        Path::new("unlucky-launcher-iced"),
        args.iter().map(OsString::from),
        Kind::TinySkia,
    );

    assert_eq!(
        command.get_program(),
        OsStr::new("unlucky-launcher-iced"),
        "program 必须是传进来的那个（生产里是 current_exe）"
    );
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        args.iter().map(OsStr::new).collect::<Vec<_>>(),
        "argv 必须原样透传，一条不漏、顺序不变"
    );
    let envs: Vec<_> = command.get_envs().collect();
    for (key, value) in fallback_env(Kind::TinySkia) {
        assert!(
            envs.contains(&(OsStr::new(key), Some(OsStr::new(value)))),
            "重开命令缺 {key}={value}，实际 {envs:?}"
        );
    }
}

/// **执行侧**：子进程退出 7 → 父进程拿到 7（不是 0、不是自己造的值）。
///
/// 抓得住：`exit_code` 被改成恒 0 / 恒 1 / 写成 `status.success().into()`。
/// 抓不住：生产里 `current_exe()` 那个 program（测试用假命令，见 `fallback_command` 注释）。
#[test]
fn fallback_command_really_passes_the_child_exit_code() {
    #[cfg(windows)]
    let (program, args) = ("cmd", vec!["/c", "exit", "7"]);
    #[cfg(unix)]
    let (program, args) = ("sh", vec!["-c", "exit 7"]);

    let status = fallback_command(
        Path::new(program),
        args.iter().map(OsString::from),
        Kind::TinySkia,
    )
    .status()
    .expect("假子进程必须能起来");
    assert_eq!(
        exit_code(status.code()),
        7,
        "子进程退出 7，父进程必须透传 7；{status:?}"
    );
}

/// 子进程**被信号杀死**（`code()` 为 `None`）：走兜底码，不 panic、不当成功。
///
/// 这是纯函数测试，所以在 Windows 上也能跑（真造一个信号死亡的 `ExitStatus`
/// 需要 `ExitStatusExt`，只有 Unix 有；而这里测的正是「拿不到码」那一刻的行为）。
#[test]
fn signal_death_falls_back_to_a_nonzero_code() {
    assert_eq!(exit_code(Some(0)), 0, "成功必须透传 0");
    assert_eq!(exit_code(Some(7)), 7);
    let fallback = exit_code(None);
    assert_ne!(fallback, 0, "拿不到退出码绝不能当成功");
    assert_eq!(fallback, 2, "兜底码与 main 的「参数或运行错误」一致");
}

/// 没有兜底可用的两行（强制 CPU / 未编 wgpu）：失败就原样把错误交回去，不许 respawn。
#[test]
fn no_fallback_means_no_respawn() {
    let cpu = decision::decide(crate::Backend::Cpu, &probe("IntegratedGpu"), true);
    let evidence = std::path::PathBuf::from("out.png");
    assert_eq!(cpu.fallback, None);
    assert_eq!(on_failure(cpu, None), None);
    assert_eq!(
        on_failure(cpu, Some(&evidence)),
        None,
        "强制 CPU 也不该走到「拒绝降级」那条提示上（它压根没想上 GPU）"
    );

    let not_compiled = decision::decide(crate::Backend::Auto, &probe("IntegratedGpu"), false);
    assert_eq!(on_failure(not_compiled, None), None);
}

/// **窗口设置就是验收日志里那三个字段的来源**（M98 r1 p2-1 的守门）。
///
/// 抓得住：把 `decorations` 改回 `true`（自绘三键退回系统装饰）、把 `resizable` / `transparent`
/// 打开 —— 这三种改动都会让 `[window]` 与 `[caps]` 两行与代码自相矛盾，而在此之前**371 个测试
/// 全绿**（复审 M6 变异实证）。现在它们必须红在这里。
/// 抓不住：winit 是否真按这些设置建窗（那要真机看窗口，见判据表「谁能验证」列）。
#[test]
fn window_settings_match_what_the_acceptance_logs_claim() {
    let settings = diagnostics::window_settings((1180.0, 760.0));

    assert!(
        !settings.decorations,
        "M93 自绘三键：窗口不能带系统装饰（否则 macOS 上会出现两套标题栏）"
    );
    assert!(
        settings.resizable,
        "验收要验「能否缩放」，窗口必须声明可缩放"
    );
    assert!(
        !settings.transparent,
        "整窗透明只在 win32 拿到句柄后下发（D36），窗口创建侧不许打开它"
    );
    assert_eq!(
        settings.min_size,
        Some(MIN_SIZE),
        "最小尺寸档（880×560）是矩阵的一格，不能被改动"
    );

    // 日志确实从上面这个 settings 取值（而不是另抄一份字面量）
    let facts = WindowFacts::from_settings(&settings);
    let line = crate::renderer::acceptance::window_text(
        &facts,
        None,
        "default",
        Some(super::super::APP_TITLE),
    );
    assert!(line.contains("resizable=true"), "{line}");
    assert!(line.contains("decorations=false"), "{line}");
    assert!(line.contains("transparent=false"), "{line}");
    assert!(line.contains("size=1180x760"), "{line}");
    assert!(line.contains("min=880x560"), "{line}");

    // 抽字段这一步必须**忠实**：拿一份处处相反的 settings 进来，facts 必须跟着反
    // （抓的是「from_settings 里又写死一份常量」这条回退路 —— M98 r1 M7 变异实证它当年漏抓）
    let odd = iced::window::Settings {
        size: iced::Size::new(640.0, 480.0),
        min_size: None,
        decorations: true,
        resizable: false,
        transparent: true,
        ..iced::window::Settings::default()
    };
    assert_eq!(
        WindowFacts::from_settings(&odd),
        WindowFacts {
            size: (640.0, 480.0),
            min: None,
            resizable: false,
            decorations: true,
            transparent: true,
        },
        "from_settings 必须逐字段搬运，不能在内部再写死一份常量"
    );

    // 反向：把事实翻个身，日志必须跟着变（证明没有第二份真值）
    let flipped = WindowFacts {
        decorations: true,
        ..facts
    };
    let line = crate::renderer::acceptance::window_text(
        &flipped,
        None,
        "default",
        Some(super::super::APP_TITLE),
    );
    assert!(
        line.contains("decorations=true"),
        "日志必须跟着 settings 走：{line}"
    );
}
