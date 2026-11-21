//! 子进程构造器：本 crate 里**唯一**允许直接 `std::process::Command::new` 的地方（M213）。
//!
//! ## 为什么需要它（这不是风格问题，是窗口问题）
//!
//! Windows 上「某个子进程要不要一块控制台」由**创建者的可执行文件类型**决定：一个**自己没有
//! 控制台**的进程（GUI 程序的 PE Subsystem = 2；本仓库的 `unlucky-launcher.exe` 就是，M196 已改）
//! 去 spawn 一个**控制台程序**（`java.exe` / `reg.exe` / `tzutil.exe` / `cmd.exe` /
//! `powershell.exe` …）时，Windows 会为这个子进程**新建一块控制台**——桌面上看到的就是
//! 「黑框闪一下」。所以探 N 个 JDK 就闪 N 次 `java -version`，用户的原话是「疯狂闪」。
//!
//! 抑制手段只有一个：创建时带 `CREATE_NO_WINDOW`（`0x0800_0000`）。
//! **`Stdio::null()` 不管这件事**——它只把子进程的 stdout/stderr 指到空设备，控制台窗口照样会被
//! 创建。所以到处补 `.stdout(Stdio::null())` 是无效修法。
//!
//! ## 谁**不许**走这里
//!
//! **游戏进程**（[`crate::launch::process::RunningProcess::spawn`]）保留自己的
//! `CREATE_NEW_PROCESS_GROUP`（`0x0000_0200`），以便 `taskkill /T` 递归回收整棵进程树。
//!
//! 这两个 flag 在 **Win32 层是可以任意组合的**：[Process Creation Flags][flags] 原文
//! 「They can be specified in any combination, except as noted」，而两条各自 not 掉的只有
//! `CREATE_NEW_CONSOLE` / `DETACHED_PROCESS`，与这一组无关。真正的坑在 **Rust std 这一层**：
//! `CommandExt::creation_flags` 是 **setter 不是累加器**——「先 `quiet()` 再
//! `.creation_flags(CREATE_NEW_PROCESS_GROUP)`」会把 `CREATE_NO_WINDOW` **静默覆盖掉**。
//!
//! 而「游戏要不要带自己的控制台窗口」是**产品决定**（玩家要看游戏自己那段输出），不是顺手
//! 抑制的事——所以这里不复用 `quiet`，显式只设 group flag，M213 一个字没动它。
//!
//! 那边是唯一另一处合法的直接构造点，两处都由 `crates/core/tests/child_quiet.rs` 的守卫钉住。
//!
//! [flags]: https://learn.microsoft.com/windows/win32/procthread/process-creation-flags

use std::ffi::OsStr;
use std::process::Command;

/// `CREATE_NO_WINDOW`：子进程不获得控制台窗口。
///
/// 见 <https://learn.microsoft.com/windows/win32/procthread/process-creation-flags>。
/// 别和 `CREATE_NEW_CONSOLE`（`0x0000_0010`，意思正好相反）或
/// `DETACHED_PROCESS`（`0x0000_0008`，子进程不继承父控制台、也不新建；标准句柄由
/// `STARTUPINFO` 给，给了照样可用）搞混。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 起一个**不该在桌面上留下窗口**的子进程。
///
/// - Windows：`.creation_flags(CREATE_NO_WINDOW)`；
/// - 其它平台：原样返回，一个字不改（POSIX 没有「spawn 就新开一个终端」这回事，子进程的
///   窗口从来不由我们造）。
///
/// 返回值就是普通 [`std::process::Command`]，调用方照旧 `.arg(..)` / `.env(..)` /
/// `.output()` / `.status()`，或者整个交给 `game::java::proc::run_with_timeout` 去带超时跑。
/// 本 crate 是 `#![forbid(unsafe_code)]`，而 `CommandExt::creation_flags` 是 std 的安全 API，
/// 所以这里不需要任何 `unsafe`。
#[must_use]
pub fn quiet<S: AsRef<OsStr>>(program: S) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
