//! 系统「减少动效」偏好（M256）：**三态**探针，只在启动时读一次。
//!
//! ## 为什么要它
//!
//! `DevDocs/PlanDoc/14-motion-and-animation.md` §6 的第 1 条未核项：`MotionScale` 到底有没有
//! 被系统偏好驱动。M256 核清的答案是**没有**——在此之前它只有三个来源（`ULL_MOTION` 环境变量、
//! 截图通道、渲染后端档），操作系统里那句「动画效果」从没人读过。本模块是那条缺失的入口。
//!
//! ## 三态，不是两态（这条是硬约束）
//!
//! | 值 | 含义 | 调用方该做什么 |
//! |---|---|---|
//! | [`ReduceMotion::Reduced`] | 系统明确要求减少动效 | 把档位降到 `MotionScale::NONE` |
//! | [`ReduceMotion::AnimationsOn`] | 系统里动效是开着的 | 用原来的档位 |
//! | [`ReduceMotion::Unknown`] | 读不到（命令缺失 / 无桌面 / 超时 / 平台未实现） | **用原来的档位**（什么都不改） |
//!
//! `Unknown` **不等于** `Reduced`：探针失败就静默关掉动效，会出现「某些机器上动效神秘消失」，
//! 那种问题比「少一个动效」难查得多。这条有守卫（`components::motion::tests`）。
//!
//! ## 廉价、一次、可超时
//!
//! - 结果缓存在 [`std::sync::OnceLock`] 里：**进程内只探一次**（`motion::configure` 是唯一调用点，
//!   它在 `on_boot` 跑一次；配置重载时读的是同一个缓存，不会重复 spawn）。
//! - 子进程有 [`COMMAND_TIMEOUT`] 上限：`gsettings` / `defaults` 在坏环境里会挂住，
//!   超时按 `Unknown` 处理并 `kill` 掉。
//! - 子进程经 `unlucky_core::platform::child::quiet` 起（M272）：仓规「spawn 控制台程序必须走
//!   `quiet`」是全树硬约束（`crates/core/tests/child_quiet_guard.rs` 钉着），探针不是例外——
//!   唯一调用点在这里。
//!
//! ## `quiet` 不影响「可杀」
//!
//! `child::quiet` 返回的就是普通 [`std::process::Command`]（Windows 上多一个
//! `CREATE_NO_WINDOW`，其它平台原样返回），`.spawn()` 拿到的 [`std::process::Child`] 仍归
//! `run` 自己，所以 **300 ms 超时 + 超时 `kill` + `wait` 一个字没丢**。
//!
//! | 平台 | 来源 | `unsafe` |
//! |---|---|---|
//! | Windows | `SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION)` | **有**（收在 [`system_animations_enabled`] 一个函数里，全仓第三处放行，门禁 `unsafe-scope` 已登记） |
//! | Linux | `gsettings get org.gnome.desktop.interface enable-animations` | 没有 |
//! | macOS | `defaults read com.apple.universalaccess reduceMotion` | 没有（**未在真机验证**：本仓无 Mac 实机，键名与权限都可能有差异） |
//! | 其它 | 恒 `Unknown` | —— |

use std::sync::OnceLock;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use unlucky_core::platform::child::quiet;

/// 系统偏好：「减少动效」是否要生效。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReduceMotion {
    /// 系统要求减少动效（装配层应降到 `MotionScale::NONE`）。
    Reduced,
    /// 系统里动效是开着的（装配层按原档位）。
    AnimationsOn,
    /// 读不到（**不等于** Reduced：装配层按原档位，见模块文档）。
    Unknown,
}

impl ReduceMotion {
    /// 进程内只探一次的入口（`motion::configure` 唯一调用点）。
    pub fn probe() -> Self {
        static CACHE: OnceLock<ReduceMotion> = OnceLock::new();
        *CACHE.get_or_init(detect)
    }

    /// 稳定名（启动日志与断言失败信息用）。
    pub const fn as_str(self) -> &'static str {
        match self {
            ReduceMotion::Reduced => "reduced",
            ReduceMotion::AnimationsOn => "animations-on",
            ReduceMotion::Unknown => "unknown",
        }
    }

    /// 是否是「系统要求减少动效」（唯一该改档位的取值）。
    pub const fn is_reduced(self) -> bool {
        matches!(self, ReduceMotion::Reduced)
    }
}

/// 子进程读偏好的超时上限（毫秒）。
///
/// 为什么是这个量级：`gsettings` / `defaults` 正常在 10~40 ms 内返回；300 ms 已经足够覆盖冷启动
/// 的 D-Bus / cfprefsd 唤醒，再长就等于让启动卡在这一步。
pub const COMMAND_TIMEOUT: u128 = 300;

#[cfg(target_os = "windows")]
fn detect() -> ReduceMotion {
    match system_animations_enabled() {
        Some(true) => ReduceMotion::AnimationsOn,
        Some(false) => ReduceMotion::Reduced,
        None => ReduceMotion::Unknown,
    }
}

#[cfg(target_os = "windows")]
const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;

/// 系统「动画效果」是不是开着（`None` = 调用失败）。
///
/// `SPI_GETCLIENTAREAANIMATION`（`winuser.h`，Win10+ 稳定可用）就是「设置 → 辅助功能 →
/// 视觉效果 → 动画效果」那个开关的真值源；`pvParam` 是 `BOOL*`，非 0 = 动画开着。
///
/// ## 为什么允许 unsafe（全仓第三处，已登记进 `unsafe-scope` 门禁）
///
/// 这个开关**没有**安全的 Rust API 可达（注册表里的对应键不稳定、且与"最佳性能"混在一起），
/// 只有 Win32 一条路。放行范围收在这一个函数里，别的判据照旧是纯函数。
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn system_animations_enabled() -> Option<bool> {
    use core::ffi::c_void;

    #[link(name = "user32")]
    extern "system" {
        /// `BOOL SystemParametersInfoW(UINT, UINT, PVOID, UINT)`。
        fn SystemParametersInfoW(action: u32, param: u32, data: *mut c_void, win_ini: u32) -> i32;
    }

    let mut enabled: i32 = 0;
    // SAFETY: ①`SPI_GETCLIENTAREAANIMATION` 的 `pvParam` 契约是「指向 BOOL 的可写指针」，
    // 这里给的是栈上 `i32` 的地址，类型与对齐都符合（Win32 `BOOL` = 32 位 int）；
    // ②`uiParam`/`fWinIni` 对这条 action 不使用，传 0；③本调用只写这 4 个字节，不持有指针、
    // 不转移所有权，返回后栈变量即可读；④结果为 0 表示失败，此时按 `None`（Unknown）处理，
    // 不去猜 `enabled` 的值。
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut enabled as *mut i32).cast(),
            0,
        )
    };
    (ok != 0).then_some(enabled != 0)
}

#[cfg(target_os = "linux")]
fn detect() -> ReduceMotion {
    // GNOME 的开关；Cinnamon / MATE 同键名。KDE 的 `kdeglobals` 走另一条路（未实现，记 Unknown）。
    match run(
        COMMAND,
        &["get", "org.gnome.desktop.interface", "enable-animations"],
    ) {
        Some(text) => match text.trim() {
            "false" => ReduceMotion::Reduced,
            "true" => ReduceMotion::AnimationsOn,
            _ => ReduceMotion::Unknown,
        },
        None => ReduceMotion::Unknown,
    }
}

#[cfg(target_os = "linux")]
const COMMAND: &str = "gsettings";

#[cfg(target_os = "macos")]
fn detect() -> ReduceMotion {
    match run(
        COMMAND,
        &["read", "com.apple.universalaccess", "reduceMotion"],
    ) {
        Some(text) => match text.trim() {
            "1" | "true" | "YES" => ReduceMotion::Reduced,
            "0" | "false" | "NO" => ReduceMotion::AnimationsOn,
            // 键不存在（这台机器没改过这个开关）时 `defaults` 会报错退出 ⇒ 走不到这里；
            // 其它输出按 Unknown，不猜。
            _ => ReduceMotion::Unknown,
        },
        None => ReduceMotion::Unknown,
    }
}

#[cfg(target_os = "macos")]
const COMMAND: &str = "defaults";

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn detect() -> ReduceMotion {
    ReduceMotion::Unknown
}

/// 跑一条短命令并取回 stdout（**带超时**，超时 `kill` 掉并返回 `None`）。
///
/// 为什么自己写而不是 `Command::output()`：`output()` 会一直等到子进程退出，坏环境里（没有
/// session bus 的 `gsettings`）就是无限期挂住——那会让**启动**卡死，是比"没有动效"严重得多的事故。
///
/// 子进程经 [`quiet`] 起（M272，仓规硬约束）。`quiet` 只改创建 flag、返回的仍是普通
/// `std::process::Command`，所以下面这套自己写的等待循环拿得到 `Child` 句柄，**超时 `kill`
/// 照旧**——「安静」与「可杀」在这里不冲突。
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run(program: &str, args: &[&str]) -> Option<String> {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let mut child = quiet(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + Duration::from_millis(COMMAND_TIMEOUT as u64);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => return None,
        }
    }

    // 输出只有几个字节（`true` / `false` / `1`），管道容量远大于它 ⇒ 先等进程退出再读不会死锁。
    let mut text = String::new();
    child.stdout.take()?.read_to_string(&mut text).ok()?;
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 三态必须**三**个值可区分：`Unknown` 不能被当成 `Reduced`（M256 的硬约束 C）。
    ///
    /// 抓得住：把 `is_reduced()` 写成「不是 AnimationsOn 就算 Reduced」（= 探针失败就静默关动效，
    /// 于是某些机器上动效神秘消失）——第一条断言立刻红。
    /// 抓不住：真机上这三个值读得对不对（那要改系统设置，见报告的未验清单）。
    #[test]
    fn unknown_is_not_reduced() {
        assert!(!ReduceMotion::Unknown.is_reduced());
        assert!(ReduceMotion::Reduced.is_reduced());
        assert!(!ReduceMotion::AnimationsOn.is_reduced());
        let names = [
            ReduceMotion::Reduced.as_str(),
            ReduceMotion::AnimationsOn.as_str(),
            ReduceMotion::Unknown.as_str(),
        ];
        let unique: std::collections::BTreeSet<&str> = names.iter().copied().collect();
        assert_eq!(unique.len(), 3, "三个取值的名字必须互不相同：{names:?}");
    }

    /// 探针在**本机**要么给出两个确定值之一，要么 `Unknown`——但它必须有返回值、不许 panic，
    /// 且**第二次调用走缓存**（同一进程内结果恒定）。
    ///
    /// 抓得住：把缓存去掉、第二次重新 spawn，在坏环境里就会看到不同结果（或让这条用例超时）。
    /// 抓不住：真机上"读到的是不是用户设的那一格"（那要改系统设置，见报告的未验清单）。
    #[test]
    fn probe_is_cached_and_total() {
        let first = ReduceMotion::probe();
        let second = ReduceMotion::probe();
        assert_eq!(first, second, "同一次进程内探针结果必须恒定（走缓存）");
        assert!(matches!(
            first,
            ReduceMotion::Reduced | ReduceMotion::AnimationsOn | ReduceMotion::Unknown
        ));
    }
}
