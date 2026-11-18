//! 显示器**工作区**几何（M371）：Windows 上取 `GetMonitorInfoW` 的 `rcWork`，而不是显示器全尺寸。
//!
//! ## 为什么需要它（M362 留下的那 38 逻辑 px）
//!
//! iced 0.14 只有 [`iced::window::monitor_size`]，而它的物理源是 winit 的 `monitor.size()`
//! （= `rcMonitor` 系，`winit-0.30.13/src/monitor.rs:112-166`）。M362 把窗口收进了这个值，于是
//! 1366×768@125% 上「溢出 182 物理 px」降到「仍被任务栏压住约 38 逻辑 px」：严格更好，但任务栏
//! 没被扣。工作区 = `rcMonitor` − 任务栏（本机实测 2632×1272 → 2632×1224）。
//!
//! ## 为什么是**第四处** `unsafe` 落点（三条路逐条评过，都不走）
//!
//! 1. **寄居到已有三处**？`transparency.rs` = 窗口 α / DWM、`console.rs` = `AttachConsole`、
//!    `motion.rs` = 「减少动效」偏好。第三条最有诱惑力（`SPI_GETWORKAREA` 确实是 `SystemParametersInfoW`
//!    家族），但那个文件的题目是「系统偏好三态探针」：把显示器几何塞进去，「落点 = 能力」当场失效，
//!    而 `SPI_GETWORKAREA` 只给**主**显示器、多显示器下是错的（见下节）。
//! 2. **换成 `windows-sys` / `windows` crate**？调用它的函数仍然是 `unsafe {}` 块，`unsafe_code = "deny"`
//!    （`Cargo.toml` 的 `[workspace.lints.rust]`）一样会报，白名单一样要加：它只把**声明**藏进依赖里，
//!    反而失去「这一处放了哪几个符号」的可审计性，还多一个第三方依赖要过 `deny.toml`。仍手写 `extern`。
//! 3. **走子进程**（`powershell` 读 `GetMonitorInfo`）？可行，代价是 100~250 ms 启动 + 输出解析 + 自己兜
//!    超时，而且窗口缩放被推到子进程返回**之后**（M362 已登记过一次「可能跳变」，不能再加一拍）。
//!
//! ## 为什么选 `GetMonitorInfoW` 而不是 `SystemParametersInfoW(SPI_GETWORKAREA)`
//!
//! | 判据 | `GetMonitorInfoW(MonitorFromWindow(hwnd, …))` | `SPI_GETWORKAREA` |
//! |---|---|---|
//! | 多显示器 | 按**窗口所在**显示器（`MONITOR_DEFAULTTONEAREST`）⇒ 对 | 恒为**主**显示器 ⇒ 窗口在副屏就被主屏工作区收窄，错 |
//! | `rcMonitor` | 同一个 `MONITORINFO` 里就有（折算基准，见下） | 拿不到 |
//! | 句柄 | 需要窗口（`window::run` 给） | 不需要，但省下这一步换来上面两条错误 |
//!
//! ## 物理 → 逻辑：为什么基准是 `monitor_logical`，不是 `scale_factor`
//!
//! `rcWork` 是**物理** px，[`super::super::super::renderer::acceptance::fit_to_work_area`] 吃**逻辑** px，
//! 所以本文件只取物理矩形，折算在纯判据 [`MonitorRects::logical_work_area`] 里做：
//!
//! ```text
//! work_logical = monitor_logical × (rcWork ÷ rcMonitor)      ← 逐维算
//! ```
//!
//! 不用 `iced::window::scale_factor` 的理由：它给的是 winit 的**原始** scale
//! （`iced_winit-0.14.1/src/lib.rs:1546-1552`），而 `Resize` / `GetMonitorSize` 用的是**状态**里的
//! scale（同文件 1447-1470 / 1665-1677，可被 `application.scale_factor(...)` 覆盖，`--scale-factor`
//! 就靠它）。两者在强制 scale 下分叉，「工作区按 1.0 折算、窗口按 2.0 建」正是这类错位。
//! 拿 `monitor_logical` 当基准，折算因子与 `resize` 用的是同一个。
//!
//! ## 平台降级（显式，不编值）
//!
//! macOS / Linux：`probe` 恒 `None` ⇒ 调用方回退到 iced 的显示器尺寸（= M362 今天的行为），日志
//! `work_area_source=monitor` 照旧。这两个平台**没有实现**就不许编数：对应物是 macOS 的
//! `visibleFrame` / X11 的 `_NET_WORKAREA`，都还没做（见报告「没验的部分」）。
//!
//! ## 守门
//!
//! 本文件带 `#![allow(unsafe_code)]`，是 `unsafe-scope` 期望集合里的**第四个**路径（判据仍是「集合相等」，不是黑名单）。
//! 放行边界：只有 `MonitorFromWindow` / `GetMonitorInfoW` 两个 user32 调用，都在 [`monitor_rects`] 里，都只读、按值传参。

// M371：显示器工作区几何的 unsafe 落点（第 4 处，见模块文档与 `unsafe-scope` 的注释）。
#![allow(unsafe_code)]

use iced::window;

#[cfg(target_os = "windows")]
use super::transparency::WinRect;

/// 屏幕坐标下的物理像素矩形（字段顺序照 Win32 的 `RECT`：左、上、右、下）。
///
/// 只装 `i32`、只用减法：不持有任何指针，因此可以被自由比较、跨线程传。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhysicalRect {
    /// 左边界（含）。
    pub left: i32,
    /// 上边界（含）。
    pub top: i32,
    /// 右边界（**不含**，Win32 的 RECT 语义）。
    pub right: i32,
    /// 下边界（**不含**，Win32 的 RECT 语义）。
    pub bottom: i32,
}

impl PhysicalRect {
    /// 物理尺寸 `(宽, 高)`。故意**不**在这里判合法性（零 / 负值原样返回）：
    /// 合法性由 [`ratio`] 一个地方判，免得两处各判一半。
    #[must_use]
    pub const fn size(self) -> (i32, i32) {
        (self.right - self.left, self.bottom - self.top)
    }
}

/// 一次 `GetMonitorInfoW` 的两块矩形（都是物理 px）。
///
/// `work` = `rcWork`（扣掉任务栏/停靠栏的那块），`monitor` = `rcMonitor`（显示器全尺寸）。
/// 两块都留着不是为了好看：物理 → 逻辑的折算需要 `rcMonitor` 当基准（见模块文档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorRects {
    /// `rcWork`：可用工作区。
    pub work: PhysicalRect,
    /// `rcMonitor`：显示器全尺寸。
    pub monitor: PhysicalRect,
}

impl MonitorRects {
    /// 纯判据：工作区 = 显示器**逻辑**尺寸 × (`rcWork` ÷ `rcMonitor`)，逐维算。
    ///
    /// `monitor_logical` 是 iced 给的显示器全逻辑尺寸（[`iced::window::monitor_size`] 的回值）。
    /// 任一维的物理值非正、或折算结果不是有限正数 ⇒ `None`（**不编值**：宁可回退到显示器尺寸，
    /// 也不把 0×0 当成「可用区为零」）。
    ///
    /// ⚠️ 下面 `self.work.size()` 那一行就是本 mission 的**全部**语义：换成 `self.monitor.size()`
    /// 等于退回 M362（可用区 = 显示器尺寸，任务栏不被扣）。`tests` 里有一条断言盯着它。
    #[must_use]
    pub fn logical_work_area(self, monitor_logical: (f32, f32)) -> Option<(f32, f32)> {
        let (work_w, work_h) = self.work.size();
        let (monitor_w, monitor_h) = self.monitor.size();
        let area = (
            monitor_logical.0 * ratio(work_w, monitor_w)?,
            monitor_logical.1 * ratio(work_h, monitor_h)?,
        );
        (usable(area.0) && usable(area.1)).then_some(area)
    }
}

/// 窗口所在显示器的工作区（Windows）；其它平台 / 读不到 ⇒ `None`。
///
/// 调用方式是 `window::run(id, probe)`：唯一能拿到原生句柄的公开入口
/// （`iced::window::Window` 只实现 `HasWindowHandle` / `HasDisplayHandle`，没有 scale）。
///
/// 非 Windows 的实现**恒 `None`**：调用方据此回退到 iced 的显示器尺寸（M362 的行为），
/// 日志写 `work_area_source=monitor`。这里不许返回一个「猜的」可用区。
#[cfg(target_os = "windows")]
#[must_use]
pub fn probe(handle: &dyn window::Window) -> Option<MonitorRects> {
    monitor_rects(hwnd_of(handle)?)
}

/// 非 Windows：**恒 `None`**（本平台未实现，不编值；见模块文档「平台降级」）。
#[cfg(not(target_os = "windows"))]
#[must_use]
pub fn probe(_handle: &dyn window::Window) -> Option<MonitorRects> {
    None
}

/// 把 iced 的窗口句柄翻成 Win32 的 `HWND`（`isize`）。
///
/// ⚠️ 与 `app::window_alpha::probe_hwnd` 是同一段三行转换，**故意各留一份**：
/// 那是 `pub(super)` 的私有工具（只给 α 那条链），而要把它提成公共 API 得改 `app/window_alpha.rs`，
/// 不在 M371 的改动面里。两处都是「raw-window-handle → HWND」的标准写法，没有第二份语义。
#[cfg(target_os = "windows")]
fn hwnd_of(handle: &dyn window::Window) -> Option<isize> {
    use window::raw_window_handle::RawWindowHandle;

    let handle = handle.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
        _ => None,
    }
}

/// `MONITOR_DEFAULTTONEAREST`：窗口不在任何显示器上时给最近的（**不是**主显示器）。
const MONITOR_DEFAULTTONEAREST: u32 = 2;

/// `MONITORINFO`（`GetMonitorInfoW` 的 out 参数）。字段顺序与 `cbSize` 照 Win32 头文件，
/// `repr(C)` 保证布局——**`cb_size` 写错会让调用直接返回 FALSE**，所以
/// `tests::monitor_info_layout_is_accepted_by_windows` 在真机上验一次。
#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct MonitorInfoW {
    cb_size: u32,
    rc_monitor: WinRect,
    rc_work: WinRect,
    dw_flags: u32,
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    /// `MonitorFromWindow`：窗口所在（或最近）的显示器句柄。
    fn MonitorFromWindow(hwnd: isize, flags: u32) -> isize;

    /// `GetMonitorInfoW`：填 `MONITORINFO`（`rcMonitor` + `rcWork` + 标志位）。
    fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfoW) -> i32;

    /// `GetDesktopWindow`：**只给测试用**的常量句柄（`HWND_DESKTOP`）。
    ///
    /// 为什么需要它：`GetMonitorInfoW` 对布局错误的 `cbSize` 只会返回 FALSE，函数本身不会报错，
    /// 于是「结构体写错」在实践中表现为「静默回退到显示器尺寸」——正是本 mission 要消灭的那种静默。
    /// 拿桌面窗口当锚点，就能在真机上把这条 FFI 走通一次（不碰任何真实窗口）。
    #[cfg(test)]
    fn GetDesktopWindow() -> isize;
}

/// 读一块显示器信息（Windows 的真实落点）。句柄无效 / 调用失败 ⇒ `None`。
#[cfg(target_os = "windows")]
fn monitor_rects(hwnd: isize) -> Option<MonitorRects> {
    // 布局与 `cbSize` 不一致时 Win32 会拒答，所以先按 `size_of` 算，不写死 40。
    let cb_size = u32::try_from(std::mem::size_of::<MonitorInfoW>()).ok()?;
    // SAFETY: 两个调用都只按值接收句柄/枚举值；`info` 是本栈帧里的 POD，`cb_size` 与它逐字节一致，
    // 指针只在调用期间有效。失败路径上 Win32 不写 `info`，返回值 0 在这里被判掉。
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if monitor == 0 {
            return None;
        }
        let mut info = MonitorInfoW {
            cb_size,
            ..MonitorInfoW::default()
        };
        if GetMonitorInfoW(monitor, &raw mut info) == 0 {
            return None;
        }
        Some(MonitorRects {
            work: rect_of(info.rc_work),
            monitor: rect_of(info.rc_monitor),
        })
    }
}

/// `RECT` → [`PhysicalRect`]（同一份字段顺序，只做类型搬移）。
#[cfg(target_os = "windows")]
const fn rect_of(rect: WinRect) -> PhysicalRect {
    PhysicalRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

/// `part ÷ whole`，两级都必须是正的；结果还要是有限正数。
///
/// 单独一个函数是为了让「除零 / 负尺寸」只有一处判据：虚拟显示器适配器上 `to_logical` 给出 0
/// 的历史（`acceptance::fit` 的 `usable`）就是这么来的。
fn ratio(part: i32, whole: i32) -> Option<f32> {
    if part <= 0 || whole <= 0 {
        return None;
    }
    let value = part as f32 / whole as f32;
    usable(value).then_some(value)
}

/// 逻辑尺寸的合法性：有限且 > 0（与 `acceptance::fit` 的同名判据同义）。
fn usable(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

#[cfg(test)]
mod tests {
    use super::{ratio, MonitorRects, PhysicalRect};

    /// 一块 1920×1080 的屏 + 底部 40 px 任务栏（Win32 的 RECT 右下边界**不含**）。
    const MONITOR: PhysicalRect = PhysicalRect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };
    const WORK: PhysicalRect = PhysicalRect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    /// **本 mission 的核心断言**：可用区必须**严格小于**显示器尺寸（任务栏真的被扣了）。
    ///
    /// 抓得住：把 `logical_work_area` 里的 `self.work.size()` 写成 `self.monitor.size()`（= 退回
    /// M362 的行为）——`<` 与下面的差 40 px 两条都红。抓不住：FFI 是否真读到这一对矩形（那要真机）。
    #[test]
    fn work_area_is_strictly_smaller_than_the_monitor_when_a_taskbar_is_reserved() {
        let rects = MonitorRects {
            work: WORK,
            monitor: MONITOR,
        };
        let monitor_logical = (1920.0, 1080.0);
        let work = rects
            .logical_work_area(monitor_logical)
            .expect("100% DPI 上必须算得出可用区");

        assert_eq!(work, (1920.0, 1040.0), "可用区就是 rcWork，不是显示器尺寸");
        assert!(
            work.1 < monitor_logical.1,
            "扣掉任务栏后高度必须**严格**变小：work={work:?} monitor={monitor_logical:?}"
        );
        assert_eq!(
            monitor_logical.1 - work.1,
            40.0,
            "差值必须恰好是任务栏那 40 px（多扣或少扣都是错的）"
        );
    }

    /// 125% DPI：折算必须走**显示器逻辑尺寸**这条基准，任务栏在逻辑 px 下是 `40 / 1.25 = 32`。
    ///
    /// 抓得住：拿「请求尺寸」或写死的 scale 当基准（数值会差几 px 且随屏变）。
    #[test]
    fn logical_conversion_follows_the_monitor_grid() {
        let rects = MonitorRects {
            work: PhysicalRect {
                right: 1366,
                bottom: 728,
                ..PhysicalRect::default()
            },
            monitor: PhysicalRect {
                right: 1366,
                bottom: 768,
                ..PhysicalRect::default()
            },
        };
        let monitor_logical = (1366.0 / 1.25, 768.0 / 1.25);
        let work = rects.logical_work_area(monitor_logical).expect("125% DPI");

        assert!((work.0 - 1092.8).abs() < 0.01, "{work:?}");
        assert!((work.1 - 582.4).abs() < 0.01, "{work:?}");
        assert!(
            (monitor_logical.1 - work.1 - 32.0).abs() < 0.01,
            "125% 下任务栏是 40 / 1.25 = 32 逻辑 px：{work:?} vs {monitor_logical:?}"
        );
    }

    /// 退化输入一律 `None`（**不编值**）：零/负矩形、零/负基准、非有限的逻辑尺寸。
    #[test]
    fn degenerate_rects_yield_none_instead_of_a_made_up_area() {
        let cases = [
            // rcWork 宽为 0（虚拟显示器适配器的经典产物）
            MonitorRects {
                work: PhysicalRect::default(),
                monitor: MONITOR,
            },
            // rcMonitor 高为 0 ⇒ 没有折算基准
            MonitorRects {
                work: WORK,
                monitor: PhysicalRect {
                    right: 1920,
                    ..PhysicalRect::default()
                },
            },
            // 负尺寸（RECT 左右写反）
            MonitorRects {
                work: PhysicalRect {
                    left: 1920,
                    right: 0,
                    top: 0,
                    bottom: 1040,
                },
                monitor: MONITOR,
            },
        ];
        for rects in cases {
            assert_eq!(
                rects.logical_work_area((1920.0, 1080.0)),
                None,
                "{rects:?} 不许算出一个假的可用区"
            );
        }

        // 逻辑尺寸本身非法（iced 在某些虚拟屏上会给 0）⇒ 也是 None，不许掉成 0×0
        for monitor_logical in [(0.0, 1080.0), (1920.0, f32::NAN)] {
            let rects = MonitorRects {
                work: WORK,
                monitor: MONITOR,
            };
            assert_eq!(rects.logical_work_area(monitor_logical), None);
        }
    }

    /// `ratio` 的两个边界（除零、负值）各一格，防止「几何判据」被一次 refactor 悄悄放宽。
    #[test]
    fn ratio_refuses_non_positive_inputs() {
        assert_eq!(ratio(1040, 1080), Some(1040.0 / 1080.0));
        assert_eq!(ratio(0, 1080), None, "零高等价于「没有可用区」");
        assert_eq!(ratio(1040, 0), None, "零基准等价于「算不出比例」");
        assert_eq!(ratio(-5, 1080), None);
    }

    /// **真机上验一次 FFI 的线是不是通的**（只在 Windows 编译）。
    ///
    /// 抓得住：`MONITORINFO` 的字段顺序 / `cbSize` 写错 —— 那时 `GetMonitorInfoW` 返回 FALSE，
    /// `probe` 会**静默**回退到显示器尺寸（本 mission 最坏的失败形态）。抓不住：任务栏是否真被扣
    /// （没有任务栏的机器上两块矩形相等，所以这里只断言 `≤`；「严格小于」在纯判据那条测试里）。
    #[cfg(target_os = "windows")]
    #[test]
    fn monitor_info_layout_is_accepted_by_windows() {
        // SAFETY: `GetDesktopWindow` 不接受参数、只返回一个常量句柄，不转移所有权。
        let desktop = unsafe { super::GetDesktopWindow() };
        assert_ne!(desktop, 0, "桌面窗口句柄不该是 0");

        let rects = super::monitor_rects(desktop).expect("GetMonitorInfoW 拒绝了这个布局");
        let (monitor_w, monitor_h) = rects.monitor.size();
        let (work_w, work_h) = rects.work.size();
        assert!(
            monitor_w > 0 && monitor_h > 0,
            "显示器全尺寸必须为正：{rects:?}"
        );
        assert!(
            work_w > 0 && work_h > 0 && work_w <= monitor_w && work_h <= monitor_h,
            "工作区必须落在显示器内：{rects:?}"
        );
    }
}
