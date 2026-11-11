//! 宿主平台族（M182）：窗口外框判据的**输入值**，而不是散落各处的 `#[cfg]`。
//!
//! ## 为什么平台要是一个值
//!
//! 圆角判据（[`super::win32::corner::plan_corner_for`]）必须能表达「Windows / macOS / Linux
//! 各自怎么办」三行。写成 `#[cfg(target_os = …)]` 的话，在 Windows 上编译时另外两行**根本不存在**，
//! 判据就永远只能在一台机器上验三分之一——而 M182 要的恰恰是那两行。
//!
//! 把平台变成值以后：真值表可以在**本机**跑满三行，`cfg` 只留在真正需要 FFI 的地方
//! （那边本来就有，且只影响「能不能调」，不影响「判据是什么」）。这与 AGENTS.md §7
//! 「平台差异判断收敛到底层，不许在各模块里散写 `cfg`」是同一条纪律。
//!
//! ## 取值来源
//!
//! [`HostOs::current`] 读 `std::env::consts::OS`（编译期常量，运行期不会变）。
//! 它**不**做任何环境探测：本仓库里「探测」是 [`super::win32::transparency`] 与
//! `probe_windows_version` 的事，这里只回答「我编在哪个平台上」。

/// 宿主平台族（判据输入）。
///
/// 只分到判据真正会用到的粒度：Windows 要问 DWM、macOS 要交给窗口服务器、
/// Linux 没有可问的对象、其余平台（FreeBSD / Android / wasm …）一律不承诺。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    /// Windows（10 / 11 / Server 同族）。
    Windows,
    /// macOS。
    MacOs,
    /// Linux（X11 与 Wayland 都是它：判据不看后端，看的是有没有可问的对象）。
    Linux,
    /// 其余目标（FreeBSD / Android / wasm32 …）：不承诺圆角。
    Other,
}

impl HostOs {
    /// 当前编译目标所属的平台族。
    #[must_use]
    pub fn current() -> Self {
        Self::from_os_str(std::env::consts::OS)
    }

    /// [`std::env::consts::OS`] 的字符串 → 平台族（纯函数，测试直接钉它）。
    ///
    /// 只认 `"windows"` / `"macos"` / `"linux"` 三个字面量，其余（含未来新平台）落 [`Self::Other`]：
    /// 猜错的代价是「多给它一个圆角承诺」，而不认识的平台本来就没有承诺可言。
    #[must_use]
    pub fn from_os_str(os: &str) -> Self {
        match os {
            "windows" => Self::Windows,
            "macos" => Self::MacOs,
            "linux" => Self::Linux,
            _ => Self::Other,
        }
    }

    /// 日志取值（`host=windows|macos|linux|other`）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::MacOs => "macos",
            Self::Linux => "linux",
            Self::Other => "other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HostOs;

    /// **抓得住**：三个字面量任一个拼错（`"macos"` 写成 `"mac-os"` / `"darwin"`）——
    /// 那会让 macOS 静默掉进 `Other` 从而「不承诺圆角」，正是 M182 要修的那类静默。
    /// **抓不住**：`std::env::consts::OS` 在真机上到底是哪个值（那是编译器给的常量，本测试只钉映射）。
    #[test]
    fn the_three_supported_families_are_recognized_verbatim() {
        assert_eq!(HostOs::from_os_str("windows"), HostOs::Windows);
        assert_eq!(HostOs::from_os_str("macos"), HostOs::MacOs);
        assert_eq!(HostOs::from_os_str("linux"), HostOs::Linux);
        assert_eq!(HostOs::Windows.as_str(), "windows");
        assert_eq!(HostOs::MacOs.as_str(), "macos");
        assert_eq!(HostOs::Linux.as_str(), "linux");
    }

    /// 不认识的目标一律 `Other`（不承诺），且**不许**退化成 Windows/macOS 的承诺。
    #[test]
    fn anything_else_falls_to_other_instead_of_guessing() {
        for os in ["freebsd", "netbsd", "android", "ios", "emscripten", ""] {
            assert_eq!(HostOs::from_os_str(os), HostOs::Other, "{os}");
        }
        assert_eq!(HostOs::Other.as_str(), "other");
    }

    /// `current()` 必须与编译目标的 `cfg` 一致——这是**本机唯一**能自查「值与 cfg 没跑偏」的地方。
    #[test]
    fn current_matches_the_compilation_target() {
        #[cfg(target_os = "windows")]
        assert_eq!(HostOs::current(), HostOs::Windows);
        #[cfg(target_os = "macos")]
        assert_eq!(HostOs::current(), HostOs::MacOs);
        #[cfg(target_os = "linux")]
        assert_eq!(HostOs::current(), HostOs::Linux);
    }
}
