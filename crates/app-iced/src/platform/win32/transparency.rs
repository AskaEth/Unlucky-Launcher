//! 整窗透明度（D36）：**全仓唯一**允许 `unsafe` 的文件。
//!
//! ## 为什么只有这里有 unsafe
//!
//! 窗口级 α 没有任何「安全」的 Rust API 可达：`SetLayeredWindowAttributes` 是 Win32 的
//! 唯一路径（D36 逐路核实过 wgpu / iced / winit 都没有代理入口）。所以把**整个仓库**的
//! FFI 面收在这一处，其余一层（[`super::super::effective_window_alpha`]）保持纯函数。
//! 守门在 `.github/workflows/guards.yml` 的 `unsafe-scope` job：全仓带这个 allow 的文件集合
//! 必须**恰好等于**本文件的路径（集合相等，不是黑名单命中）。
//!
//! ## 只做 LWA_ALPHA
//!
//! `LWA_COLORKEY` / `DwmEnableBlurBehindWindow` 一律不用：色键会把某个颜色整块变透明（视觉
//! 事故），模糊是另一条路线（D36 已排除）。
//!
//! `WS_EX_LAYERED`（`SetLayeredWindowAttributes` 的调用前提）**也归本文件**：加扩展位只有
//! `Get/SetWindowLongPtrW` 能走，必须 `unsafe`，而全仓放行 unsafe 的文件集合恰好只有本文件。
//! 所以窗口创建侧（`app::boot`）不碰扩展样式，它只负责拿到句柄后先调 [`ensure_layered`]
//! 再调 [`apply_window_alpha`]——两件事都在同一个 unsafe 落点里，门禁仍是「集合相等」。
//!
//! ## 为什么它不影响命中测试与自绘标题栏拖动（D36 任务书要求写清）
//!
//! `LWA_ALPHA` 只改**合成**（DWM 把整窗乘一个常数），不动窗口的命中测试：
//!
//! | 机制 | 会不会让鼠标穿透 | 本项目用不用 |
//! |---|---|---|
//! | `WS_EX_TRANSPARENT`（分层窗口的「点击穿透」位） | **会**（鼠标事件直接发给下层的窗口） | 不用 |
//! | `LWA_COLORKEY`（色键：某种颜色整块变透明） | **会**（命中测试按颜色挖洞） | 不用 |
//! | `WS_EX_LAYERED` + `LWA_ALPHA` | **不会**（只是合成时乘 α，整个矩形仍然接收命中） | 用这一条 |
//!
//! 所以自绘标题栏的拖动照旧：`Message::DragWindow` → `window::drag(id)` 走 winit 的
//! `WM_NCLBUTTONDOWN(HTCAPTION)`，它依赖的是**窗口矩形内的鼠标事件**，与 α 无关。
//! 唯一会「点不动」的情形是 α = 0（整窗不可见，用户以为程序消失）——这就是
//! [`unlucky_presenter::prefs::WINDOW_ALPHA_MIN`]（0.08）作为硬底、且 `α ≤ 0` 直接拒绝的原因。
//!
//! D36 已在真机上实测「α ≥ 1 时分层样式不破坏命中测试与拖动」；本仓库的 RDP 环境无法复核
//! 观感，所以这条结论的**行为**部分归真机验收（见 M93 报告「没验的部分」）。
//!
//! ## 非 Windows
//!
//! 同签名的空实现返回 [`AlphaOutcome::Unsupported`]：调用点不需要 `#[cfg]`，
//! 而「这个平台不支持」这件事本身可以被断言。

// D36：这是全仓唯一的 unsafe 落点，CI 的 `unsafe-scope` job 会比对文件集合。
#![allow(unsafe_code)]

mod alpha;
mod dwm;
mod exstyle;
mod probe;

pub use dwm::{apply_corner, style_summary, CornerOutcome};
pub use exstyle::{click_through, is_click_through_style, WS_EX_TRANSPARENT};
pub use probe::{controlled_experiment, TABLE_HEADER};

pub use alpha::{
    alpha_byte, apply_window_alpha, ensure_layered, AlphaOutcome, AlphaRejection, GWL_EXSTYLE,
    LWA_ALPHA, WS_EX_LAYERED,
};

/// 读扩展样式（跨位宽的唯一落点：64 位走 `GetWindowLongPtrW`，32 位走 `GetWindowLongW`）。
#[cfg(all(target_os = "windows", target_pointer_width = "64"))]
pub(super) fn raw_ex_style(hwnd: isize) -> isize {
    // SAFETY: 只读一个窗口属性，按值传参、不持有指针、无所有权转移；无效句柄只会返回 0。
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) }
}

/// 32 位目标：符号是 `GetWindowLongW`（`link_name` 已映射），返回 `i32`，扩到 `isize`。
#[cfg(all(target_os = "windows", target_pointer_width = "32"))]
pub(super) fn raw_ex_style(hwnd: isize) -> isize {
    // SAFETY: 同 64 位分支。
    unsafe { isize::from(GetWindowLongPtrW(hwnd, GWL_EXSTYLE)) }
}

/// 非 Windows：没有扩展样式这个概念，读 0（`ensure_layered` 因此恒 `false`）。
#[cfg(not(target_os = "windows"))]
pub(super) fn raw_ex_style(_hwnd: isize) -> isize {
    0
}

/// 写扩展样式（位宽分档同上）。
#[cfg(all(target_os = "windows", target_pointer_width = "64"))]
pub(super) fn raw_set_ex_style(hwnd: isize, style: isize) {
    // SAFETY: 同 `raw_ex_style`：按值传参、不持有指针；返回值刻意不用（改完靠回读取证）。
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
    }
}

/// 32 位目标：符号是 `SetWindowLongW`；扩展样式本身就是 32 位量，收窄无损。
#[cfg(all(target_os = "windows", target_pointer_width = "32"))]
pub(super) fn raw_set_ex_style(hwnd: isize, style: isize) {
    let style = i32::try_from(style).unwrap_or(i32::MAX);
    // SAFETY: 同 64 位分支。
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
    }
}

/// 非 Windows：空实现。
#[cfg(not(target_os = "windows"))]
pub(super) fn raw_set_ex_style(_hwnd: isize, _style: isize) {}

/// 回读判据：这一位在不在扩展样式里。
pub(super) fn has_layered(hwnd: isize) -> bool {
    raw_ex_style(hwnd) & WS_EX_LAYERED != 0
}

/// 句柄是否指向一个**存在**的窗口（全仓唯一 `unsafe` 落点的一部分）。
///
/// 为什么在这里而不是在取证探针那个文件里：`transparency.rs` 是全仓**唯一**允许 `unsafe` 的
/// 文件（`unsafe-scope` 判据 = 「带 `#![allow(unsafe_code)]` 的文件集合恰好等于本文件」）。
/// 探针用到 FFI 时就只能**把 FFI 搬进来**，不能让 `unsafe` 溜到没有标记的文件里——
/// 那会让门禁照样绿、而「唯一落点」这个性质已经破了（M100 r1 复审实测抓到的形状）。
///
/// `0` / 无效句柄 → `false`（Win32 的 `IsWindow` 对无效句柄返回 0）。
#[cfg(target_os = "windows")]
pub(super) fn window_exists(hwnd: isize) -> bool {
    // SAFETY: `IsWindow` 只按值接受句柄，不持有指针、无所有权转移；无效句柄返回 0。
    (unsafe { IsWindow(hwnd) }) != 0
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    /// `IsWindow`：句柄是否指向一个存在的窗口。
    ///
    /// 为什么需要它（M100 r1）：`GetWindowLongPtrW` 对**无效句柄**返回 0，与「扩展样式全 0」
    /// 不可区分。取证探针要分「拿不到」与「读到了没有这一位」，所以先验一次句柄。
    fn IsWindow(hwnd: isize) -> i32;

    /// `SetLayeredWindowAttributes`：窗口必须已带 `WS_EX_LAYERED`，α 才生效。
    ///
    /// 签名照 Win32 头文件：`BOOL SetLayeredWindowAttributes(HWND, COLORREF, BYTE, DWORD)`。
    fn SetLayeredWindowAttributes(hwnd: isize, color_key: u32, alpha: u8, flags: u32) -> i32;

    /// `GetWindowLongPtrW`：读窗口属性（`GWL_EXSTYLE` = `-20`）。
    ///
    /// 头文件里 `GetWindowLongPtrW` 是宏：64 位映射到 `GetWindowLongPtrW`、32 位映射到
    /// `GetWindowLongW`。user32 在 32 位导出里**没有** `...PtrW`，所以这里按指针宽度分档声明，
    /// 否则 i686 目标会在链接期失败（本仓的 Win7 冒烟是 x64，但编译不该只在一个位宽上成立）。
    #[cfg(target_pointer_width = "64")]
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
    /// `SetWindowLongPtrW`：写窗口属性（同上，32 位映射到 `SetWindowLongW`）。
    #[cfg(target_pointer_width = "64")]
    fn SetWindowLongPtrW(hwnd: isize, index: i32, new_value: isize) -> isize;

    /// 32 位目标的实际符号（见上）。
    #[cfg(target_pointer_width = "32")]
    #[link_name = "GetWindowLongW"]
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> i32;
    /// 32 位目标的实际符号（见上）。
    #[cfg(target_pointer_width = "32")]
    #[link_name = "SetWindowLongW"]
    fn SetWindowLongPtrW(hwnd: isize, index: i32, new_value: i32) -> i32;

    /// `GetWindowRect`：窗口矩形的屏幕坐标（**含** DWM 加出来的不可见边框）。
    ///
    /// 与 `DWMWA_EXTENDED_FRAME_BOUNDS` 对照，是 M116 探针里「DWM 到底给这个窗口加了多宽的
    /// 边框」这条差值的来源：差值 ≈ 系统认为的边框带宽度。
    fn GetWindowRect(hwnd: isize, rect: *mut WinRect) -> i32;

    /// `CreateWindowExW`：M116 的**受控对照实验**要用它造三种样式的窗口（同进程）。
    ///
    /// 只用系统自带类 `STATIC`（不需要 `RegisterClassW`，也就没有窗口过程要写）。
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: isize,
        menu: isize,
        instance: isize,
        param: *mut core::ffi::c_void,
    ) -> isize;

    /// `DestroyWindow`：探针窗口用完即毁（不泄漏 HWND）。
    fn DestroyWindow(hwnd: isize) -> i32;

    /// `ShowWindow`：DWM 只管理**可见**窗口，所以探针窗口必须真显示一次才有读数。
    fn ShowWindow(hwnd: isize, command: i32) -> i32;
}

/// `RECT`（`GetWindowRect` 的 out 参数）。字段顺序照 Win32 头文件，`repr(C)` 保证布局。
#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct WinRect {
    /// 左
    pub left: i32,
    /// 上
    pub top: i32,
    /// 右
    pub right: i32,
    /// 下
    pub bottom: i32,
}

#[cfg(target_os = "windows")]
#[link(name = "dwmapi")]
extern "system" {
    /// `DwmSetWindowAttribute`：M116 圆角的唯一落点（`DWMWA_WINDOW_CORNER_PREFERENCE`）。
    ///
    /// 返回 `HRESULT`：`S_OK`(0) = 成功；属性不被这个 Windows 版本认识时是 `E_INVALIDARG`
    /// （`0x8007_0057`）——Win10 上就是这个返回值，**不是**「设了但没效果」。
    fn DwmSetWindowAttribute(
        hwnd: isize,
        attribute: u32,
        value: *const core::ffi::c_void,
        size: u32,
    ) -> i32;

    /// `DwmGetWindowAttribute`：读回 DWM 侧的属性（探针与日志的证据来源）。
    fn DwmGetWindowAttribute(
        hwnd: isize,
        attribute: u32,
        value: *mut core::ffi::c_void,
        size: u32,
    ) -> i32;
}

/// 写一个 `u32` 的 DWM 属性，返回 `HRESULT`（0 = `S_OK`）。
#[cfg(target_os = "windows")]
pub(super) fn dwm_set_u32(hwnd: isize, attribute: u32, value: u32) -> i32 {
    // SAFETY: `value` 是栈上的 POD，指针只在调用期间有效，`size` 与它逐字节一致；
    // DWM 只按值读取，不保留指针。
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            (&raw const value).cast::<core::ffi::c_void>(),
            u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4),
        )
    }
}

/// `HRESULT` 的成功判据：**看严重性位**（bit 31），不是「等于 0」。
///
/// Win32 里成功码不只有 `S_OK`(0)：`S_FALSE`(1) 与各种 `S_*` / 信息级码都是成功，所以判据必须
/// 是 `hr >= 0`。这条判据与 M116 探针的具体读数无关——它只是 `HRESULT` 的通用语义。
///
/// ⚠️ **别把 `0x7FF8FFA9` 当成 DWM 的返回值**（本分支一度这么写过，已订正）：本机
/// `DwmGetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE)` 三种窗口样式实测**全部**返回
/// `E_INVALIDARG`（`0x8007_0057`，负数）；`0x7FF8FFA9` 是我早期用 `i32::unsigned_abs()` 打印
/// HRESULT 时的产物（把负数的补码取了绝对值），它既不是 `S_OK` 也不是任何 DWM 信息码。
#[must_use]
pub const fn hresult_ok(hr: i32) -> bool {
    hr >= 0
}

/// 读一个 `u32` 的 DWM 属性**并保留 `HRESULT`**（M116 探针要它：读不回来时得知道为什么）。
#[cfg(target_os = "windows")]
pub(super) fn dwm_get_u32_hr(hwnd: isize, attribute: u32) -> (Option<u32>, u32) {
    let mut value: u32 = 0;
    // SAFETY: out 指针指向本栈帧里的 `u32`，`size` 与之一致；失败时 DWM 不写。
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            attribute,
            (&raw mut value).cast::<core::ffi::c_void>(),
            u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4),
        )
    };
    // 判据是**严重性位**（bit 31），不是「等于 0」：`S_FALSE` / 信息级码也是成功。
    // ⚠️ 报错时给的是 `hr as u32`（原始位），**不是** `unsigned_abs()`——后者会把
    // `E_INVALIDARG`（`0x8007_0057`）印成 `0x7FF8_FFA9`（M116 实测踩过：负数取绝对值
    // 得到的是「补码的绝对值」，不是 HRESULT）。`hresult_ok` 与这条打印规则一起被测试钉住。
    (hresult_ok(hr).then_some(value), hr as u32)
}

/// 读一个 `u32` 的 DWM 属性；`HRESULT` 非 0 返回 `None`（**不许**把失败读成 0）。
#[cfg(target_os = "windows")]
pub(super) fn dwm_get_u32(hwnd: isize, attribute: u32) -> Option<u32> {
    dwm_get_u32_hr(hwnd, attribute).0
}

/// 读 `DWMWA_EXTENDED_FRAME_BOUNDS` 这类 `RECT` 属性；失败返回 `None`。
#[cfg(target_os = "windows")]
pub(super) fn dwm_get_rect(hwnd: isize, attribute: u32) -> Option<WinRect> {
    let mut rect = WinRect::default();
    // SAFETY: 同 `dwm_get_u32`，只是 out 类型换成 `RECT`。
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            attribute,
            (&raw mut rect).cast::<core::ffi::c_void>(),
            u32::try_from(std::mem::size_of::<WinRect>()).unwrap_or(16),
        )
    };
    // 与 `dwm_get_u32_hr` 同一条判据：`HRESULT` 看严重性位，不是「等于 0」（judgment 只留一处）。
    hresult_ok(hr).then_some(rect)
}

/// `GetWindowRect`：窗口矩形（屏幕坐标）。失败返回 `None`。
#[cfg(target_os = "windows")]
pub(super) fn window_rect(hwnd: isize) -> Option<WinRect> {
    let mut rect = WinRect::default();
    // SAFETY: out 指针指向本栈帧的 `RECT`；`GetWindowRect` 只在成功时写它。
    let ok = unsafe { GetWindowRect(hwnd, &raw mut rect) };
    (ok != 0).then_some(rect)
}

/// 造一个探针窗口（受控对照实验用）。`0` = 创建失败。
///
/// 类名用系统自带的 `STATIC`：不需要 `RegisterClassW`，也就不必在这个文件里再写一个窗口过程。
#[cfg(target_os = "windows")]
pub(super) fn create_probe_window(style: u32, ex_style: u32) -> isize {
    let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
    let title: Vec<u16> = "ULL M116 corner probe\0".encode_utf16().collect();
    // SAFETY: 两个指针都指向本帧里以 NUL 结尾的 UTF-16 缓冲；其余参数全是按值 POD。
    // 失败返回 0，调用方按「拿不到句柄」处理，不会产生未定义行为。
    unsafe {
        CreateWindowExW(
            ex_style,
            class.as_ptr(),
            title.as_ptr(),
            style,
            40,
            40,
            420,
            260,
            0,
            0,
            0,
            core::ptr::null_mut(),
        )
    }
}

/// 让探针窗口可见（DWM 只管理可见窗口），`SW_SHOWNA` = 显示但不激活。
#[cfg(target_os = "windows")]
pub(super) fn show_probe_window(hwnd: isize) {
    const SW_SHOWNA: i32 = 8;
    // SAFETY: 按值传句柄；无效句柄只会让 Win32 返回 0。
    unsafe {
        ShowWindow(hwnd, SW_SHOWNA);
    }
}

/// 销毁探针窗口（成对使用，避免 HWND 泄漏）。
#[cfg(target_os = "windows")]
pub(super) fn destroy_probe_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    // SAFETY: 只对本模块 `create_probe_window` 造出来的句柄调用；销毁后不再使用它。
    unsafe {
        DestroyWindow(hwnd);
    }
}

/// 读窗口样式位（`GWL_STYLE` = `-16`）。探针用它取 `frame=` 那一栏（产品的真实样式见
/// `probe::PRODUCT_STYLE`，**不是** `WS_POPUP`）。
#[cfg(target_os = "windows")]
pub(super) fn window_style(hwnd: isize) -> isize {
    const GWL_STYLE: i32 = -16;
    // SAFETY: 同 `raw_ex_style`：只读一个窗口属性，按值传参、不持有指针。
    unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) }
}

#[cfg(target_os = "windows")]
pub(super) fn set_layered(hwnd: isize, byte: u8) -> AlphaOutcome {
    // SAFETY: 四个参数都是按值传入的 POD；`flags` 只带 LWA_ALPHA，于是系统忽略 `color_key`
    // （传 0 与之无关）。句柄由调用方给出：无效句柄只会让 Win32 返回 0（走下面的 Failed 分支），
    // 不会造成未定义行为——本调用不持有指针，也没有释放语义。
    let ok = unsafe { SetLayeredWindowAttributes(hwnd, 0, byte, LWA_ALPHA) };
    if ok == 0 {
        // ⚠️ **这里的 `unsigned_abs()` 是有意的，别跟 DWM 那几处「统一」**（tower 裁定 2026-11-03）：
        //
        // - 这里取的是 `GetLastError()` 的 **DWORD**（`raw_os_error()` 给的是被塞进 `i32` 的 DWORD，
        //   例如 `87` / `1400`），它不是 HRESULT，没有严重性位；`unsigned_abs` 只是把
        //   「高位被当符号」的那点噪声抹掉，语义仍是「错误号」。
        // - `DwmGet/SetWindowAttribute` 返回的是 **HRESULT**：判成功看 bit 31（`hresult_ok`），
        //   打印必须用 `hr as u32` 的原始位——用 `unsigned_abs()` 会把 `E_INVALIDARG`
        //   （`0x8007_0057`）印成 `0x7FF8_FFA9`（M116 实测踩过，见 `dwm_get_u32_hr` 的注释）。
        //
        // 两种「错误号」不同源，所以打印方式也不该相同。这条由
        // `tests::dword_error_codes_and_hresults_are_printed_differently` 钉住。
        let code = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(0)
            .unsigned_abs();
        return AlphaOutcome::Failed { code };
    }
    AlphaOutcome::Applied { byte }
}

#[cfg(not(target_os = "windows"))]
pub(super) fn set_layered(_hwnd: isize, _byte: u8) -> AlphaOutcome {
    AlphaOutcome::Unsupported
}

#[cfg(test)]
mod tests;
