//! DWM 圆角（M116）：把 [`CornerPlan`](super::super::corner::CornerPlan) 落成一次属性下发。
//!
//! ## 本文件零 `unsafe`、零 `extern`
//!
//! `DwmSetWindowAttribute` / `DwmGetWindowAttribute` 的声明与调用点在 [`super`]
//! （`unsafe` 落点之一；另一个是 `super::super::console`）；这里只做「哪一档 → 哪个属性值」的映射、`HRESULT` 解释与回读。
//!
//! ## 为什么「直角」也要真的下发一次
//!
//! `DWMWCP_DONOTROUND` 是**显式**的直角请求。不下发也能得到直角（Win10 就没有这个属性），
//! 但那样日志里「方」与「没生效」长得一模一样。下发 + 回读之后，
//! `corner=… source=square excluded_reason=user` 与 `excluded_reason=os-build` 是两条
//! 可区分的事实，而不是一句猜测。
//!
//! ## 回读为什么是证据而不是自证
//!
//! `DwmSetWindowAttribute` 返回 `S_OK` 只说明「参数被接受了」。属性值由 DWM 保存，
//! 回读拿到的是 **DWM 侧的当前值**——它能抓住「设了 A 属性、读回还是旧值」这类事
//! （比如把 `DWMWA_WINDOW_CORNER_PREFERENCE` 写成别的号，回读会给出 `None` 或旧值）。

// M147：本模块七项（三个属性号 + 四个读函数）只在 Windows 上有使用者——唯一调用方 `probe::run_one`
// 的 Windows 版被门着，Linux + `-D warnings` 下就是 dead_code 错。用模块级 `cfg_attr` 而不是
// 逐项 `cfg`：`cfg` 会把这些项连同本文件那几条与平台无关的单测一起门掉（逐项 `cfg_attr` 多 7 行，顶破红线）。
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]
/// `DWMWA_WINDOW_CORNER_PREFERENCE`：窗口圆角偏好的属性号（Win11 / Server 2025，build ≥ 22000）。
pub const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 34;
/// `DWMWCP_DONOTROUND`：显式直角。
pub const DWMWCP_DONOTROUND: u32 = 1;
/// `DWMWCP_ROUND`：显式圆角（半径由系统决定，**不暴露**给应用）。
pub const DWMWCP_ROUND: u32 = 2;
/// `DWMWA_VISIBLE_FRAME_BORDER_THICKNESS`：可见边框厚度（像素，Win11 起）。
///
/// ⚠️ **本机实测它不能当「圆不圆」的判据**（我最初以为能，被探针否掉了）：
/// 三种窗口样式 × 三种状态（默认 / `DWMWCP_ROUND` / `DWMWCP_DONOTROUND`）读出来**全是 `1`**，
/// 没有任何区分度。保留它只作诊断读数（万一将来在别的机器上有变化），不参与判定。
/// 证据：`DevDocs/artifacts/m116/probe-table.md` 的 `bdef` / `brnd` / `bsq` 三列。
pub const DWMWA_VISIBLE_FRAME_BORDER_THICKNESS: u32 = 37;
/// `DWMWA_EXTENDED_FRAME_BOUNDS`：DWM 认为的窗口边界（**含**不可见的调整带）。
pub const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;

/// 一次圆角下发的结局（日志与测试共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CornerOutcome {
    /// 非 Windows：空实现，什么都没做。
    UnsupportedPlatform,
    /// 句柄为 0：还没拿到窗口，或窗口已销毁。
    NoWindow,
    /// 已下发且回读一致。`readback == None` 表示**这一版 DWM 不接受回读**
    /// （属性不被认识时 `DwmGetWindowAttribute` 返回 `E_INVALIDARG`），此时只有 `S_OK` 作证。
    Applied {
        /// 下发下去的偏好值（[`DWMWCP_ROUND`] / [`DWMWCP_DONOTROUND`]）。
        preference: u32,
        /// DWM 侧回读到的值（`None` = 读不回来）。
        readback: Option<u32>,
        /// 回读那一次的 `HRESULT`（`0` = `S_OK`）。
        read_hresult: u32,
    },
    /// 属性不被这个 Windows 版本认识（`E_INVALIDARG`）：Win10 就是这个结局。
    Unsupported {
        /// `DwmSetWindowAttribute` 的原始 `HRESULT`。
        hresult: u32,
    },
    /// 其他失败（句柄无效等），`hresult` 来自 DWM。
    Failed {
        /// `DwmSetWindowAttribute` 的原始 `HRESULT`。
        hresult: u32,
    },
}

impl CornerOutcome {
    /// 一行日志尾巴：`corner=source=dwm pref=2 readback=2`。
    ///
    /// `readback=?` 与 `pref=none`（平台不支持）是不同的坏消息：前者是「设了但读不回来」，
    /// 后者是「这里根本没有这个能力」。
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::UnsupportedPlatform => "pref=none readback=n/a platform=unsupported".to_owned(),
            Self::NoWindow => "pref=none readback=n/a window=none".to_owned(),
            Self::Applied {
                preference,
                readback,
                read_hresult,
            } => format!(
                "pref={preference} readback={} read_hr=0x{read_hresult:08X}",
                readback.map_or_else(|| "?".to_owned(), |value| value.to_string())
            ),
            Self::Unsupported { hresult } => {
                format!("pref=none readback=n/a hresult=0x{hresult:08X} (attribute unsupported)")
            }
            Self::Failed { hresult } => format!("pref=none readback=n/a hresult=0x{hresult:08X}"),
        }
    }
}

/// `HRESULT` 是不是 `E_INVALIDARG`（`0x8007_0057`）：Win10 上设这个属性就是这个返回值。
#[cfg(target_os = "windows")]
const E_INVALIDARG: u32 = 0x8007_0057;

/// 把「来源」映射成要下发的偏好值（纯函数，测试直接钉它）。
///
/// `Dwm` → [`DWMWCP_ROUND`]；`Square` → [`DWMWCP_DONOTROUND`]。
/// 刻意**不用**默认值（`0`）：那在 Win11 上等于「请 DWM 自己决定」，而无边框窗口
/// 到底算不算「有框」由 DWM 的启发式决定——那正是我们要么拿到、要么明确放弃的东西。
///
/// M182 的 `Compositor` 档（跟随合成器）在 Windows 上**不可达**（`plan_corner_for` 不产出它，
/// 单测钉着这一点）；真要出现，正确的落点是默认值 `0`（= 让系统自己决定），
/// 而不是被当成「强制直角」——否则 `source=compositor` 那行日志会与下发值自相矛盾。
#[must_use]
pub const fn preference_for(source: super::super::corner::CornerSource) -> u32 {
    use super::super::corner::CornerSource;
    match source {
        CornerSource::Dwm => DWMWCP_ROUND,
        CornerSource::Compositor => 0,
        CornerSource::Square => DWMWCP_DONOTROUND,
    }
}

/// 下发一次圆角偏好（幂等：重复调用结果相同）。
pub fn apply_corner(hwnd: isize, plan: &super::super::corner::CornerPlan) -> CornerOutcome {
    if hwnd == 0 {
        return CornerOutcome::NoWindow;
    }
    set_preference(hwnd, preference_for(plan.source))
}

/// 下发一个具体的偏好值并回读（拆开是为了让探针能逐格遍历）。
pub fn set_preference(hwnd: isize, preference: u32) -> CornerOutcome {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (hwnd, preference);
        CornerOutcome::UnsupportedPlatform
    }
    #[cfg(target_os = "windows")]
    {
        if hwnd == 0 {
            return CornerOutcome::NoWindow;
        }
        let hresult = super::dwm_set_u32(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, preference);
        if !super::hresult_ok(hresult) {
            // 原始位，不是 `unsigned_abs()`（见 `dwm_get_u32_hr` 的注释：后者印不出 HRESULT）。
            let code = hresult as u32;
            return if code == E_INVALIDARG {
                CornerOutcome::Unsupported { hresult: code }
            } else {
                CornerOutcome::Failed { hresult: code }
            };
        }
        let (readback, read_hresult) = corner_preference_hr(hwnd);
        CornerOutcome::Applied {
            preference,
            readback,
            read_hresult,
        }
    }
}

/// 读圆角偏好**以及它的 `HRESULT`**（`(None, code)` = 读不回来，`code` 是 DWM 给的原因）。
///
/// 为什么要暴露 `HRESULT`：M116 本机实测——**写成功、读回 `E_INVALIDARG`**。只报 `?`
/// 会让「设上了吗」变成猜谜；有了码就能说清是「属性不支持读」而不是「没设上」。
#[must_use]
pub fn corner_preference_hr(hwnd: isize) -> (Option<u32>, u32) {
    #[cfg(target_os = "windows")]
    {
        super::dwm_get_u32_hr(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = hwnd;
        (None, 0)
    }
}

/// 读可见边框厚度（像素）。**诊断用，不可作判据**（本机三样式三态实测全是 1，见常量文档）。
#[must_use]
pub fn visible_border_thickness(hwnd: isize) -> Option<u32> {
    #[cfg(target_os = "windows")]
    {
        super::dwm_get_u32(hwnd, DWMWA_VISIBLE_FRAME_BORDER_THICKNESS)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = hwnd;
        None
    }
}

/// 读 DWM 的窗口边界（`left, top, right, bottom`，屏幕坐标）。
#[must_use]
pub fn extended_frame_bounds(hwnd: isize) -> Option<(i32, i32, i32, i32)> {
    #[cfg(target_os = "windows")]
    {
        super::dwm_get_rect(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS)
            .map(|rect| (rect.left, rect.top, rect.right, rect.bottom))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = hwnd;
        None
    }
}

/// 读 `GetWindowRect`（含 DWM 加出来的不可见边框），用于和 [`extended_frame_bounds`] 求差。
#[must_use]
pub fn window_rect(hwnd: isize) -> Option<(i32, i32, i32, i32)> {
    #[cfg(target_os = "windows")]
    {
        super::window_rect(hwnd).map(|rect| (rect.left, rect.top, rect.right, rect.bottom))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = hwnd;
        None
    }
}

/// 读窗口样式位（`GWL_STYLE` = `-16`）：日志里 `frame=` 那一栏的取值来源。
///
/// ⚠️ 产品窗口**不是** `WS_POPUP`（M116 r4 复审订正）：winit 在 `decorations = false` 下给的是
/// `WS_CAPTION|WS_BORDER|...`（见 `probe::PRODUCT_STYLE`），真进程的 `frame=` 应为 `overlapped`
/// （关掉 min/max 则 `other`）。「无边框」是 `WM_NCCALCSIZE` 的效果，不是样式位。
#[must_use]
pub fn window_style(hwnd: isize) -> Option<isize> {
    #[cfg(target_os = "windows")]
    {
        (hwnd != 0).then(|| super::window_style(hwnd))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = hwnd;
        None
    }
}

/// 读窗口扩展样式位（`GWL_EXSTYLE`）：配合 [`window_style`] 回答「我们是不是 `WS_EX_LAYERED`」。
#[must_use]
pub fn window_ex_style(hwnd: isize) -> Option<isize> {
    #[cfg(target_os = "windows")]
    {
        (hwnd != 0).then(|| super::raw_ex_style(hwnd))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = hwnd;
        None
    }
}

/// `WS_POPUP` 位（`0x8000_0000`）：**带 owner 的弹出窗**才置它；产品主窗口不用这一位。
pub const WS_POPUP: isize = 0x8000_0000_u32 as isize;

/// 帧样式的一句话标签（纯函数，日志用）：`popup` / `overlapped` / `other`。
///
/// 为什么要它：用户的现象是「别的窗口有圆角，我们的没有」。把它打进日志，才能一眼看出
/// 窗口形状这一项**不是**变量（产品恒为 `overlapped`）——真正让系统忽略圆角请求的是整窗 α
/// 引入的 `WS_EX_LAYERED`（见 `layered=` 与 `suppressed_by=window_alpha`），不是 `popup` 形状。
#[must_use]
pub const fn frame_label(style: isize) -> &'static str {
    const WS_OVERLAPPEDWINDOW_MASK: isize = 0x00CF_0000;
    if style & WS_POPUP != 0 {
        "popup"
    } else if style & WS_OVERLAPPEDWINDOW_MASK == WS_OVERLAPPEDWINDOW_MASK {
        "overlapped"
    } else {
        "other"
    }
}

/// 分层位的一句话标签（纯函数，日志用）：`on` / `off`。
#[must_use]
pub const fn layered_label(ex_style: isize) -> &'static str {
    if ex_style & super::WS_EX_LAYERED != 0 {
        "on"
    } else {
        "off"
    }
}

/// 一个窗口的样式摘要（`frame=overlapped layered=off`，产品真进程实测值）；读不到时如实写
/// `frame=? layered=?`。「`frame=popup`」只会出现在**带 owner 的弹出窗**上，产品不会。
#[must_use]
pub fn style_summary(hwnd: isize) -> String {
    let frame = window_style(hwnd).map_or("?", frame_label);
    let layered = window_ex_style(hwnd).map_or("?", layered_label);
    format!("frame={frame} layered={layered}")
}

#[cfg(test)]
mod tests;

/// **抓得住**：`frame_label` 把 `WS_POPUP` 与 `WS_OVERLAPPEDWINDOW` 判反（那会让日志把
/// 「我们自己的窗口」写成「普通窗口」，这行日志的全部价值就是分辨这个）。
/// **抓不住**：真窗口上到底置了哪一位（那属于运行时读数）。
#[test]
fn the_frame_label_separates_popup_from_a_decorated_window() {
    assert_eq!(WS_POPUP, 0x8000_0000_u32 as isize);
    assert_eq!(frame_label(WS_POPUP | 0x1000_0000), "popup");
    assert_eq!(frame_label(0x00CF_0000), "overlapped");
    assert_eq!(frame_label(0), "other");
}

/// 分层位标签：只认 `WS_EX_LAYERED`，别把 `WS_EX_TRANSPARENT` 之类的邻居混进来。
#[test]
fn the_layered_label_reads_the_right_bit() {
    let layered = crate::platform::win32::transparency::WS_EX_LAYERED;
    assert_eq!(layered_label(layered), "on");
    assert_eq!(layered_label(WS_POPUP), "off");
    assert_eq!(layered_label(0), "off");
}

/// 读不到句柄时摘要不许编：`frame=? layered=?`（「不知道」与「没有」是两件事）。
#[test]
fn the_style_summary_never_guesses() {
    assert_eq!(style_summary(0), "frame=? layered=?");
}
