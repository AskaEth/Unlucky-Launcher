//! 起步后端失败后的**兜底/重开**一簇：判据、进程环境交接、`respawn`、退出码透传。
//!
//! 从 `app/boot.rs` 拆出来的理由：这一簇**与启动编排零耦合**，但已有 5 条测试，而且它是
//! M97 判据表第 8/9 行的落点。`boot.rs` 那边只负责「什么时候调它」。
//!
//! ## 为什么必须换进程（别改回进程内重试）
//!
//! Windows 实测本进程里再 `run()` 一次直接 panic
//! （`iced_winit-0.14.1/src/lib.rs:81` → `Create event loop: RecreationAttempt`），winit 一个进程
//! 只能建一次事件循环。

use crate::app::diag::diag;
use crate::renderer::Decision;
use crate::renderer::{self, acceptance, Kind};

/// 起步后端失败后的去向（判据表第 8 行 vs 第 9 行——**两条路策略不同，别合成一条**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OnFailure {
    /// 交互启动：安全网优先，换进程用这个后端重开。
    Respawn(Kind),
    /// 取证启动：红着退出，**不出图**。
    Refuse,
}

/// 纯判据：起步后端失败时走哪条路。
///
/// `evidence` 非空 = 本次是取证启动（带 `--screenshot`）；返回 `None` = 没有兜底可用
/// （`--renderer=cpu` / 未编 wgpu），原样把错误交回去。抽成纯函数只为一个理由：这条分支以前
/// **没有任何测试**，`respawn` / `Refuse` 两条路可以被静默改反（M97 r1 p2-1 就是这么测出来的）。
pub(super) fn on_failure(
    decision: Decision,
    evidence: Option<&std::path::PathBuf>,
) -> Option<OnFailure> {
    let fallback = decision.fallback?;
    Some(match evidence {
        Some(_) => OnFailure::Refuse,
        None => OnFailure::Respawn(fallback),
    })
}

/// 降级重开要传给子进程的环境变量（**两条缺一不可**）。
///
/// ① `ICED_BACKEND`：让子进程直接用 `fallback` 跑。
/// ② [`renderer::FALLBACK_ENV`]：**防无限重开的闸门**——子进程见它就必须跳过探测
///    （`boot_probe` 的第一个分支），否则会算出同一个结论、再失败、再 respawn。
///
/// 抽成纯函数是为了让「漏传其中之一」变成**测试能抓到**的错误：两条都是静默失效型
/// （漏 ① 重开又跑一遍 wgpu、漏 ② 无限重开），线上极难看出来。
pub(super) fn fallback_env(fallback: Kind) -> [(&'static str, &'static str); 2] {
    [
        ("ICED_BACKEND", fallback.env_value()),
        (renderer::FALLBACK_ENV, fallback.env_value()),
    ]
}

/// 安全网：起一个**新进程**用 `fallback` 后端重开，然后带着它的退出码退出。
///
/// 只在**交互启动**时走到这里（取证启动宁可红着退出也不换后端出图）；子进程看到
/// [`renderer::FALLBACK_ENV`] 必须跳过探测，否则无限递归重开（闸门在 `boot_probe` 入口）。
pub(super) fn respawn(failed: Kind, fallback: Kind, error: &str) -> ! {
    let exe = std::env::current_exe().unwrap_or_else(|err| {
        diag!("[gpu] 降级重开失败：拿不到自身路径（{err}）");
        std::process::exit(exit_code(None));
    });
    diag!(
        "[gpu] 降级 → {}（{} 已失败，原因见上两行；换一个进程重开：winit 不允许同进程建两次事件循环）。\
         下一次的日志从 `[probe] 本次是降级重开` 开始。",
        fallback.name(),
        failed.name()
    );
    // `[backend] final` 一行把「哪个后端失败了、去了哪、为什么」收在一起（可 grep 的单一结论行）。
    // 这里 `final=` 装的是**兜底**（进程即将带着它重开）；`via=respawn` 说明它是换进程去的。
    acceptance::report_backend_failed(failed, Some(fallback), Some(fallback), "respawn", error);

    let status = fallback_command(&exe, std::env::args_os().skip(1), fallback).status();

    // 退出码**透传**子进程的：上层脚本/CI 靠它判断成败。写错的失效方式是「安静地给出一个错
    // 结论」——不崩、不静默降级、不无限重开，只是让 CI 把失败当成功。
    let code = match status {
        Ok(status) => exit_code(status.code()),
        Err(err) => {
            diag!("[gpu] 降级重开失败：{err}");
            exit_code(None)
        }
    };
    std::process::exit(code)
}

/// 子进程退出码 → 本进程退出码。
///
/// `None` = 子进程被信号杀死（只有 Unix）：走兜底码而**不是** panic、也不是 0——上层判据是
/// 「非零 = 失败」，「拿不到退出码」和「退出码是某个非零值」对脚本是一回事。
pub(super) fn exit_code(code: Option<i32>) -> i32 {
    /// 重开失败 / 拿不到退出码时的兜底码（与 `main` 的「参数或运行错误」一致）。
    const RESPAWN_FAILED: i32 = 2;
    code.unwrap_or(RESPAWN_FAILED)
}

/// 构造降级重开的子进程命令（**可测**：program / argv / env 都能在测试里断言）。
///
/// `program` / `args` 是参数而不是在函数里读 `std::env`，唯一目的是让测试能喂假命令
/// （`cmd /c exit 7` / `sh -c "exit 7"`）来验退出码透传——真用 `current_exe()` 就没法测。
///
/// 三路 stdio **显式**继承：子进程日志必须和父进程同一路输出，否则验收只抓到父进程那两行具名
/// 错误。`Command` 没有 stdio 的 getter（stable 没有），所以这条只能靠真跑一次证明。
pub(super) fn fallback_command(
    program: &std::path::Path,
    args: impl IntoIterator<Item = std::ffi::OsString>,
    fallback: Kind,
) -> std::process::Command {
    // 子进程就是本 GUI exe 自己（PE Subsystem=2，本来就不会分到控制台），所以这一处今天闪不了；
    // 走 `quiet` 是为了让「app-iced 里除 opener 外不留裸构造」这条规则整体可被守卫钉住——将来
    // 某天把降级重开换成控制台辅助程序时，不会从这里漏出一块黑框（M217）。
    let mut command = unlucky_core::platform::child::quiet(program);
    command.args(args).envs(fallback_env(fallback));
    command
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    command
}
