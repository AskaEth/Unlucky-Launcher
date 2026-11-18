//! 控制台附着（M196）：GUI 子系统进程为什么「参数看得见、输出看不见」，以及唯一的补救。
//!
//! ## 起因
//!
//! `crates/app-iced/src/main.rs` 顶上的 `#![cfg_attr(windows, windows_subsystem = "windows")]`
//! 把产物 PE 头的 `Subsystem` 从 CUI(3) 改成 WINDOWS_GUI(2)——双击不再多一个控制台宿主窗口
//! （群友 2026-11-08 实测「发布构建底下带一个控制台窗口」）。代价是：同一条命令行也仍然要能用。
//! `unlucky-launcher-iced` 一个二进制两种用法（M82），CLI 子命令与 `--diff` / `--screenshot`
//! 的日志全靠 std 句柄，而 GUI 子系统进程**不继承**父进程的控制台。
//!
//! ## 四情形实测（2026-11-08，本机 Windows；GUI 子系统探针与最终产物同属性）
//!
//! | 启动方式 | 启动瞬间 `GetStdHandle`(out/err) | 输出去哪了 | 要 attach 吗 |
//! |---|---|---|---|
//! | ① `> file` 重定向 | 磁盘句柄（`GetFileType`=1） | 文件 | 不要 |
//! | ② `\| pipe` 管道 | 管道句柄（=3） | 管道 | 不要 |
//! | ③ cmd 里直接跑 | **0 / 0**（`FILE_TYPE_UNKNOWN`） | **丢了** | **要** |
//! | ④ 双击 | 0 / 0 | 丢了（本来也没人看） | 要（但会失败，见下） |
//!
//! ③ 的补救实测：`AttachConsole(ATTACH_PARENT_PROCESS)` 返回 1 之后 `GetStdHandle` 立刻给出
//! 控制台句柄（release 产物在 cmd 模型下实测 `out 0x0→0x1F0` / `err 0x0→0x1F4`），随后
//! `WriteFile` 与 Rust 的 `eprintln!` 都真的落进了屏幕缓冲区（同一模型宿主读回的文本见 M196
//! 报告）。具体数字随进程与控制台实例变化（调试构建里是 `0x208`/`0x20C`），**能跨实例复现的
//! 是「从 0 变成非 0」这件事本身**。所以**不需要**再自己 `CreateFileW("CONOUT$")` +
//! `SetStdHandle`：那一层是多余的，而且它会覆盖用户自己重定向的句柄——①②两种情形会被打坏。
//!
//! ④ 的边界：双击时祖先进程（explorer）没有控制台，`AttachConsole` 返回 0
//! （`GetLastError` = 6 `ERROR_INVALID_HANDLE`），`GetConsoleWindow` 仍是 0、`CONOUT$` 也打不开。
//! **`AttachConsole` 只会附到一个已经存在的控制台，永远不会新建窗口**——这正是它比
//! `AllocConsole` 适合这里的原因（后者凭空多一个黑框，正是本次要修的那个 bug）。
//!
//! ## 什么时候调、为什么无条件调
//!
//! `main()` 的**第一件事**（早于 CLI 分发、早于 iced 初始化）：一条路径写 CLI 输出，另一条路径
//! 在 `--diff` / `--screenshot` / 参数报错时写 stderr，两条都要句柄。判据只有一条——
//! 「out 与 err 至少一个是不可用的就尝试 attach」，可用就绝不碰（那是 ①②）。
//!
//! 无条件（而不是只在 CLI 分支）的取舍：今天发的产物是 CUI(3)，被控制台拉起时**本来就是**
//! 附着状态（收得到 Ctrl+C / 控制台关闭事件）；在同一条路径上保持附着语义，等于「除了双击不再
//! 多黑框」之外**行为不变**。双击那条路 attach 必失败，不会多出任何窗口。
//!
//! ## 非 Windows
//!
//! 同签名函数返回 [`ConsoleOutcome::NotWindows`]：调用点不需要 `#[cfg]`，而「这个平台没有
//! Windows 控制台这回事」本身可以被断言。

// 本文件是 D36 那条「unsafe 落点」的第二个成员（详见 `transparency.rs` 顶部与
// `.github/workflows/guards.yml` 的 `unsafe-scope`）：控制台附着只有 Win32 一条路，
// 没有安全替代。放行范围仅限本文件。
#![allow(unsafe_code)]

/// `GetStdHandle` 的两个 `(DWORD)-n` 参数（同一个值在 u32 与 i32 两种记法下都要能写）。
#[cfg(target_os = "windows")]
const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5; // -11
#[cfg(target_os = "windows")]
const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4; // -12

/// `AttachConsole` 的 `ATTACH_PARENT_PROCESS`（`(DWORD)-1`）。
#[cfg(target_os = "windows")]
const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;

/// `FALSE`/`TRUE` 之外唯一需要认的无效句柄：`(HANDLE)-1`。
///
/// **必须带下面那行 `cfg`**：唯一使用点 [`usable`] 只活在 Windows 分支里，而本模块在
/// Linux/macOS 上也编译（`platform/mod.rs` 的 `pub mod win32;` 没有 cfg）。漏了它，
/// `ci.yml` 的 ubuntu clippy（`-- -D warnings`）会把它升级成 `dead_code` error——而本机只有
/// windows-gnu 工具链，非 Windows 的 cfg 一次都编不到，本地抓不住（M196 r1 复审 P1）。
/// 同目录 `transparency/dwm.rs` 用的是文件级 `#![cfg_attr(not(windows), allow(dead_code))]`；
/// 这里刻意**不用**那种写法：本文件的 Windows-only 面很小，每一项各自带 cfg 才能让编译器
/// 继续在非 Windows 上替我们盯着「又漏了一个」。
///
/// 自证办法（本机只有 Windows triple、交叉编 iced 会卡在 x11/fontconfig 时）：把本文件的
/// `target_os = "windows"` 全换成不存在的目标名，再
/// `rustc --edition 2021 --crate-type lib -D warnings --crate-name console_probe <那份拷贝>`
/// ——漏了上面那行 cfg 时它是 exit 1 + `constant INVALID_HANDLE_VALUE is never used`，
/// 补齐后 exit 0 且零告警（**其他**任何非 Windows 死项也会一起报出来，所以这一个 probe 就是
/// 「本文件在 Linux 上能不能过 -D warnings」的整份答案）。
#[cfg(target_os = "windows")]
const INVALID_HANDLE_VALUE: isize = -1;

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn GetStdHandle(which: u32) -> isize;
    fn GetFileType(handle: isize) -> u32;
    fn AttachConsole(process_id: u32) -> i32;
    fn GetLastError() -> u32;
}

/// 一次附着的结论。带着前后两对句柄值走，报告里贴的就是运行时真读数，不是「应该没问题」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleOutcome {
    /// 非 Windows：没有这回事。
    NotWindows,
    /// `out` 与 `err` 都已经是可用句柄（①②）：**一个字节都没动**。
    AlreadyUsable {
        /// `(stdout, stderr)` 的 `GetStdHandle` 读数。
        handles: (isize, isize),
    },
    /// 附着成功：`after` 是附着后的读数（控制台句柄）。
    Attached {
        /// 附着前 `(stdout, stderr)`。
        before: (isize, isize),
        /// 附着后 `(stdout, stderr)`。
        after: (isize, isize),
    },
    /// 没有可写的控制台（④ 双击，以及其他没有控制台的宿主）：附着失败**或**附着成功但
    /// 句柄仍不可用。服务 / 计划任务只是**近似**形态、未亲测，边界见 `packaging/README.md` §15。
    ///
    /// 两种情况在调用点上是同一件事（都没有输出通道），所以合并成一个结论；`last_error` 与
    /// 两对句柄一起进诊断文本，读的人能自己分清是哪一种。**没有新建任何窗口**。
    NoConsole {
        /// 附着前 `(stdout, stderr)`。
        before: (isize, isize),
        /// 附着后 `(stdout, stderr)`。
        after: (isize, isize),
        /// `AttachConsole` 之后的 `GetLastError`（双击时实测 6 = `ERROR_INVALID_HANDLE`）。
        last_error: u32,
    },
}

impl ConsoleOutcome {
    /// 单行可 grep 的验收诊断（`ULL_ACCEPTANCE=1` 时由 `main` 打到 stderr，前缀 `[console]`）。
    ///
    /// 句柄按 `0x` 十六进制打印：`0x0` / `0xFFFFFFFFFFFFFFFF` 一眼就是「没有」，
    /// 而「附着了但等于没附」这种半吊子状态（attach 成功、句柄仍是 0）也就藏不住。
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Self::NotWindows => "非 Windows：无控制台可附着（不适用）".to_owned(),
            Self::AlreadyUsable { handles } => format!(
                "std 句柄本来就可用（out=0x{:X} err=0x{:X}），未 attach",
                handles.0, handles.1
            ),
            Self::Attached { before, after } => format!(
                "已附着父进程控制台：out 0x{:X}→0x{:X}，err 0x{:X}→0x{:X}",
                before.0, after.0, before.1, after.1
            ),
            Self::NoConsole {
                before,
                after,
                last_error,
            } => format!(
                "没有可用的控制台（attach 失败或句柄仍不可用，GetLastError={}）：out 0x{:X}→0x{:X}，err 0x{:X}→0x{:X}；未新建窗口",
                last_error, before.0, after.0, before.1, after.1
            ),
        }
    }

    /// 是否真的（或本来就已经）有可用的输出通道。仅供诊断/日志判断，不影响控制流。
    #[must_use]
    pub fn output_visible(self) -> bool {
        match self {
            Self::NotWindows | Self::NoConsole { .. } => false,
            Self::AlreadyUsable { .. } | Self::Attached { .. } => true,
        }
    }
}

/// 判据（纯函数）：两个句柄都可用就**不**碰，否则尝试 attach。
///
/// 单拎出来的理由与 `platform/mod.rs` 里那两条判据一样：这一半可以不碰 FFI 就被断言证明，
/// 而 FFI 的实际效果（窗口有没有多出来）在本仓库的验证环境里没有参考价值。
#[must_use]
pub fn should_attach(stdout_usable: bool, stderr_usable: bool) -> bool {
    !(stdout_usable && stderr_usable)
}

/// 句柄是否指向一个能写的东西。
///
/// `GetFileType` 的返回值：1 磁盘 / 2 控制台 / 3 管道 / 4 remote，**0 = `FILE_TYPE_UNKNOWN`**
/// ——GUI 子系统进程从控制台启动时 `GetStdHandle` 给的就是 0（实测，见文件头表格 ③）。
#[cfg(target_os = "windows")]
fn usable(handle: isize) -> bool {
    handle != 0 && handle != INVALID_HANDLE_VALUE && {
        // SAFETY: `GetFileType` 只按值收一个句柄、不持有指针、无所有权转移；无效句柄返回 0。
        (unsafe { GetFileType(handle) }) != 0
    }
}

#[cfg(target_os = "windows")]
fn std_handles() -> (isize, isize) {
    // SAFETY: `GetStdHandle` 只按值收一个索引常量，不持有指针、不改任何状态。
    (unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }, unsafe {
        GetStdHandle(STD_ERROR_HANDLE)
    })
}

/// 让 GUI 子系统进程的输出接回父进程控制台（详见模块文档的四情形表）。
///
/// 幂等、不可失败（失败也是一种结论）：`out`/`err` 都可用时**一个字节都不动**，
/// 否则调一次 `AttachConsole(ATTACH_PARENT_PROCESS)` 再回读。
#[must_use]
pub fn attach_parent_console() -> ConsoleOutcome {
    #[cfg(not(target_os = "windows"))]
    {
        ConsoleOutcome::NotWindows
    }
    #[cfg(target_os = "windows")]
    {
        let before = std_handles();
        if !should_attach(usable(before.0), usable(before.1)) {
            return ConsoleOutcome::AlreadyUsable { handles: before };
        }
        // SAFETY: `AttachConsole` 只按值收一个 pid 常量（-1 = 父进程）；它不返回指针、
        // 不转移所有权，失败就是失败（父子关系不变、也没有窗口被创建）。
        let attached = (unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }) != 0;
        // 失败原因只进诊断文本（不改控制流）：返回值已经说明成败，这个码解释「为什么」。
        let last_error = unsafe { GetLastError() };
        let after = std_handles();
        if attached && usable(after.0) && usable(after.1) {
            ConsoleOutcome::Attached { before, after }
        } else {
            // 附着成功但句柄仍不可用，与「父进程根本没有控制台」在调用点上是同一件事：
            // 都没有可写的输出通道，所以合并成一个结论（`describe` 里照样带着读数）。
            ConsoleOutcome::NoConsole {
                before,
                after,
                last_error,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{should_attach, ConsoleOutcome};

    #[test]
    fn truth_table_attach_only_when_a_stream_is_missing() {
        // ①②：两个都在 → 绝不 attach（否则用户的重定向会被打坏）。
        assert!(!should_attach(true, true));
        // ③④：任意一个缺失 → 尝试 attach。三条都写出来，防止谁把它简化成 `!stdout_usable`。
        assert!(should_attach(false, true));
        assert!(should_attach(true, false));
        assert!(should_attach(false, false));
    }

    #[test]
    fn describe_carries_the_runtime_readings() {
        // 报告里贴的是这些行：句柄值必须真的出现在文本里，不然「已附着」就只是一句话。
        // 下面的 0xE4/0xE8 是**示例值**（真实读数随进程与控制台实例变化，见模块文档 ③）。
        let attached = ConsoleOutcome::Attached {
            before: (0, 0),
            after: (0xE4, 0xE8),
        };
        let text = attached.describe();
        assert!(text.contains("0x0→0xE4"), "{text}");
        assert!(text.contains("0xE8"), "{text}");
        assert!(attached.output_visible());

        let missing = ConsoleOutcome::NoConsole {
            before: (0, 0),
            after: (0, 0),
            last_error: 6,
        };
        assert!(missing.describe().contains("GetLastError=6"));
        assert!(!missing.output_visible());
    }

    #[test]
    fn already_usable_says_nothing_was_touched() {
        let outcome = ConsoleOutcome::AlreadyUsable {
            handles: (0x20C, 0x1F0),
        };
        assert!(outcome.describe().contains("未 attach"));
        assert!(outcome.output_visible());
        assert!(!ConsoleOutcome::NotWindows.output_visible());
    }
}
