//! M116：窗口外框圆角的**判据**（纯函数，零 `unsafe`、零 FFI、零环境读取）。
//!
//! ## 判据为什么单独拎出来
//!
//! 「能不能圆」= 用户策略 × 系统版本 × 渲染档，这三件事**可以被断言证明**；而 DWM 的**视觉**
//! 结果在本仓库的验证环境（RDP 断连、抓屏失效）上没有参考价值。把可测的那一半独立出来，
//! 才有「有测试的判据 + 没测试的观感」这条诚实的分界（与 [`super::super::effective_window_alpha`]
//! 同一套分法）。
//!
//! 真正下发给窗口的动作在 [`super::transparency`]（全仓唯一 `unsafe` 落点）。
//!
//! ## 平台矩阵（哪些是实测、哪些是推断）
//!
//! | 平台 | 我们做什么 | 依据级别 |
//! |---|---|---|
//! | Win11 / Server 2025（build ≥ 22000） | 下发 `DWMWCP_ROUND`（**请求**系统圆角） | 写入被接受 = **本机 API 实测**；「圆没圆 / 边缘什么样」= **未验**（无像素、回读不可用） |
//! | Win10 及更早（build < 22000） | 直角；不做 `SetWindowRgn` 毛边默认 | 「该属性不存在」= **外部文档证据**；本机没有 Win10 |
//! | macOS | 不主动加，交给系统 | **推断**（未真机验） |
//! | Linux X11 / Wayland | 不承诺，直角即如实结果 | **推断**（未真机验） |
//!
//! ⚠️ **「Win11 上我们的窗口一定有圆角、边缘平滑」这句话本分支的证据不支持**，别写回去。
//! 本机（Server 2025 build 26100、RDP 会话断开）测到的是：
//! ① `DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE)` 被接受（三种窗口样式都接受）；
//! ② `DwmGetWindowAttribute` 同一属性返回 `E_INVALIDARG`（本会话不支持回读，与窗口形状无关）；
//! ③ DWM 的调整带只给**有 `WS_CAPTION|WS_THICKFRAME` 的窗口**：`EXTENDED_FRAME_BOUNDS −
//!    GetWindowRect` 在有边框窗口上是 `+0,−7`，在**裸 `WS_POPUP` 假形状**上是 `+0,+0`。
//!
//! ⚠️ **第 ③ 条不能拿裸 popup 的读数去解释产品窗口**（M116 r4 复审订正）：产品窗口
//! **不是** `WS_POPUP`（winit 只在带 owner 的弹出窗才加它；无边框观感来自 `WM_NCCALCSIZE`），
//! 它带 `WS_CAPTION|WS_THICKFRAME` ⇒ 按同一张表应得 `+0,−7`，与 `decorated` 同族。
//! 探针里 `product` / `product+alpha` 两行量的就是这个真实形状。
//!
//! ④ **整窗 α 会关掉圆角**（tower 裁定 A / D47：整窗透明度优先于 DWM 圆角）：α < 1 时我们补
//! `WS_EX_LAYERED`，此时系统会忽略圆角请求——请求照样发出（日志里 `pref=` 有值），
//! 只是不生效；α 回到 1.0 就恢复。日志用 `suppressed_by=window_alpha` 记这一条**观测事实**。
//!
//! 「圆角长什么样」归真机验收，证据目录见 `DevDocs/artifacts/m116/`。
//!
//! 后两行在本 crate 里落到同一格：探测不到 Windows 版本 ⇒ `UnknownOs` ⇒ 直角。
//! 这是「不承诺」，不是「做不到」——两者的区别写在日志的原因码里。

use unlucky_presenter::prefs::{WindowCorner, WindowsVersion, WIN11_MIN_BUILD};

use crate::Backend;

/// 一次「圆角该从哪来」的判定结果（M116）。
///
/// 只有两种来源，且**互相排斥**：
///
/// | 来源 | 谁画圆角 | 我们做什么 |
/// |---|---|---|
/// | [`CornerSource::Dwm`] | 系统合成器（是否真画、半径多少都不由我们定，也读不到） | 只下发 `DWMWA_WINDOW_CORNER_PREFERENCE` |
/// | [`CornerSource::Square`] | 没人 | 明确下发 `DWMWCP_DONOTROUND`，把「方」也变成可观测的选择 |
///
/// **没有 `Self`（自绘圆角）这一档**，这是刻意的（用户定案 2026-11-03 + tower 裁定）：
/// 窗口是 `decorations=false` + 自绘标题栏 + 整窗 `LWA_ALPHA`，自绘圆角意味着把内容层裁成圆角，
/// 那会把标题栏三键与边缘缩放热区一起裁掉；系统也不暴露圆角半径（无 API 可读），
/// 「固定 8/12/16」只是凭空维护一个数字。所以非 DWM 平台**就是直角**，并且日志里说明原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CornerSource {
    /// DWM 系统圆角（Win11 / Server 2025 内核，build ≥ [`WIN11_MIN_BUILD`]）。
    Dwm,
    /// 直角（用户选择、系统不支持、或该平台不承诺）。
    Square,
}

impl CornerSource {
    /// 日志取值（`corner=… source=dwm|square`）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dwm => "dwm",
            Self::Square => "square",
        }
    }

    /// 是否要求 DWM 真去画圆角。
    #[must_use]
    pub const fn is_dwm(self) -> bool {
        matches!(self, Self::Dwm)
    }
}

/// 为什么**不是** DWM 圆角（`None` = 就是 DWM 圆角）。日志里逐字打出来。
///
/// 为什么要这个枚举而不是一个 `bool`：用户报的现象是「别的窗口都有圆角，只有我们的没有」，
/// 于是「没圆角」至少有四种完全不同的原因，混成一个 `square` 以后没人能分辨
/// 「没生效」与「生效了但看不见」（M116 任务书原话）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CornerExclusion {
    /// 用户在设置里选了「直角」。
    UserSquare,
    /// 系统太老：Win10 及更早（build < [`WIN11_MIN_BUILD`]）——DWM 没有这个偏好属性。
    OsBuild,
    /// 版本探测不出来（非 Windows / 注册表读不到 / 有东西在骗版本号）。
    UnknownOs,
    /// 非 DWM 平台上的软件渲染档：圆角只能靠自绘/`SetWindowRgn` 毛边路，D36 的
    /// 「CPU 档不碰合成技巧」优先，所以退回直角。
    CpuTier,
}

impl CornerExclusion {
    /// 日志取值（`excluded_reason=…`）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserSquare => "user",
            Self::OsBuild => "os-build",
            Self::UnknownOs => "unknown-os",
            Self::CpuTier => "cpu-tier",
        }
    }
}

/// 判定结果：来源 + 排除原因 + 平台版本（日志与测试共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CornerPlan {
    /// 圆角从哪来。
    pub source: CornerSource,
    /// `None` = 走 DWM；`Some(..)` = 走直角，并说明为什么。
    pub excluded: Option<CornerExclusion>,
    /// 探测到的 Windows 版本（`None` = 不是 Windows / 读不到）。
    pub os: Option<WindowsVersion>,
}

impl CornerPlan {
    /// 日志行：`corner=policy=system source=dwm os=10.0.26100 excluded_reason=none radius=system`。
    ///
    /// **`radius=system` 是刻意的**：系统不暴露 DWM 的圆角半径，我们也不自己定一个数字
    /// （用户定案取消了 8/12/16 档），所以这一栏永远只说「系统决定」。
    #[must_use]
    pub fn log_line(&self, policy: WindowCorner) -> String {
        let os = self
            .os
            .map_or_else(|| "unknown".to_owned(), |os| os.version_string());
        format!(
            "corner=policy={} source={} os={os} excluded_reason={} radius=system",
            policy.as_str(),
            self.source.as_str(),
            self.excluded.map_or("none", CornerExclusion::as_str),
        )
    }
}

/// 判定「这一次窗口该不该有系统圆角」（纯函数，不调 FFI、不读环境）。
///
/// 真值表（`backend` 只影响**非 DWM** 平台，见下）：
///
/// | `policy` | `os.build` | backend | `source` | `excluded` |
/// |---|---|---|---|---|
/// | `Square` | 任意 | 任意 | `Square` | `UserSquare` |
/// | `System` | `None` | 任意 | `Square` | `UnknownOs` |
/// | `System` | `< 22000` | GPU/Auto | `Square` | `OsBuild` |
/// | `System` | `< 22000` | CPU | `Square` | `CpuTier` |
/// | `System` | `>= 22000` | 任意 | `Dwm` | `None` |
///
/// 为什么 CPU 档在 Win11 上**照样**用 DWM 圆角（**tower 2026-11-03 裁定，按此执行**）：
/// D36 的 CPU 档约束针对的是**我们自己的渲染路径**（面板 α / 玻璃 / 模糊要关、不许拿透明叠加
/// 去换效果），而 DWM 圆角是**合成器侧**的效果，不增加我们的 GPU/CPU 绘制负担。所以
/// 「CPU 档关掉 Win11 圆角」不是 D36 的要求，而是凭空造一个没人要的降级。
///
/// 反过来在非 DWM 平台上，圆角只剩「自绘裁切 / `SetWindowRgn` 毛边」这两条**我们自己的**
/// 合成技巧，那才是 D36 要拦的东西 ⇒ 那一格落 `cpu-tier` 而不是 `os-build`
/// （同一个直角，**原因不同**，日志里可分辨）。
#[must_use]
pub fn plan_corner(
    policy: WindowCorner,
    os: Option<WindowsVersion>,
    backend: Backend,
) -> CornerPlan {
    let square = |excluded| CornerPlan {
        source: CornerSource::Square,
        excluded: Some(excluded),
        os,
    };

    if policy == WindowCorner::Square {
        return square(CornerExclusion::UserSquare);
    }
    match os {
        None => square(CornerExclusion::UnknownOs),
        Some(os) if os.build < WIN11_MIN_BUILD => match backend {
            Backend::Cpu => square(CornerExclusion::CpuTier),
            Backend::Gpu | Backend::Auto => square(CornerExclusion::OsBuild),
        },
        Some(_) => CornerPlan {
            source: CornerSource::Dwm,
            excluded: None,
            os,
        },
    }
}

/// 产品日志行的**唯一格式化落点**（纯函数：把四段文本拼成一个固定形状）。
///
/// ```text
/// [corner] <context> corner=policy=system source=dwm os=10.0.26100 excluded_reason=none \
///          radius=system pref=2 readback=? read_hr=0x80070057 frame=overlapped layered=on \
///          suppressed_by=window_alpha
/// ```
///
/// 为什么单独拎出来：`App::apply_window_corner` 要真句柄、真 DWM 才能跑，而**这一行的形状**
/// 恰恰是「以后分辨没生效 / 生效了看不见」的唯一依据（M116 任务书原话）。形状必须在没有窗口的
/// 环境里也能被断言——tower 2026-11-03 明确要求：不为了这行日志去构建 bin。
///
/// 参数里 `outcome` / `style` 由调用方给（`CornerOutcome::describe()` / `style_summary(hwnd)`），
/// 本文件不依赖 FFI 层，因此可以在非 Windows 目标上编、也能被单测覆盖。
///
/// `suppressed_by` 是**观测事实**（tower 裁定 A / D47 要求的那一栏）：整窗 α < 1 时我们补了
/// `WS_EX_LAYERED`，此时系统忽略圆角请求。配合同一行里的 `pref=`（请求确实发出去了）与
/// `layered=`（分层位确实在），读者能自己推出「不是我们没发请求」——本函数不写结论性措辞。
#[must_use]
pub fn format_line(
    context: &str,
    plan: &CornerPlan,
    policy: WindowCorner,
    outcome: &str,
    style: &str,
    suppressed_by: Option<&'static str>,
) -> String {
    let mut line = format!(
        "[corner] {context} {} {outcome} {style}",
        plan.log_line(policy)
    );
    if let Some(reason) = suppressed_by {
        line.push(' ');
        line.push_str("suppressed_by=");
        line.push_str(reason);
    }
    line
}

/// 一次「已经下发过」的**指纹**：只由判定本身与「当时有没有窗口」组成，不含读数。
///
/// 为什么需要它：设置页的滑条（整窗 α / 面板 α / 暗化）每动一下就触发一次外部外观变更，
/// 而圆角与那些键**无关**。没有去重时，拖一次滑条就会刷出一串
/// `[corner] change …`，把「分辨有没有生效」的那条日志淹掉（M116 r1 复审 P2-4）。
///
/// 指纹**刻意不含 `pref=` / `readback=` 这类读数**：那些只有真下发才知道，拿它们参与比较
/// 会变成「自己比较自己」。判定没变就不重下、也不重记——这也是「同一策略 N 次只记 1 行」的判据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CornerApplied {
    /// 当时的策略（`system` / `square`）。
    pub policy: WindowCorner,
    /// 判定出来的来源。
    pub source: CornerSource,
    /// 直角时的原因码（`None` = 走 DWM）。
    pub excluded: Option<CornerExclusion>,
    /// 下发时**有没有窗口句柄**：`false → true` 必须重下（首次拿到 HWND 时才真能设属性）。
    pub has_window: bool,
    /// 整窗 α 是否 < 1（= 我们补了 `WS_EX_LAYERED`，系统会忽略圆角请求）。
    ///
    /// 进指纹是为了让「α 从 1.0 拉低 / 拉回 1.0」各触发一次重下与一行日志——那两次的
    /// 系统行为确实不同，不重记就看不出来。
    pub suppressed_by_window_alpha: bool,
}

impl CornerApplied {
    /// 由判定 + 「有没有句柄」+ 「整窗 α 是否补了分层」构造指纹。
    #[must_use]
    pub const fn new(
        policy: WindowCorner,
        plan: &CornerPlan,
        has_window: bool,
        suppressed_by_window_alpha: bool,
    ) -> Self {
        Self {
            policy,
            source: plan.source,
            excluded: plan.excluded,
            has_window,
            suppressed_by_window_alpha,
        }
    }
}

/// 这一次要不要真的下发 / 记日志（纯函数）。
///
/// `false` = 判定与上次完全一致（策略、来源、原因码、有没有句柄都没变）⇒ **不动窗口、不打日志**。
/// `true` 的四种情形：首次、换档、系统版本变化导致判定变化、首次拿到句柄。
///
/// 注意：判定不变而上次**下发失败**时这里也返回 `false`——失败原因（`pref=none hresult=…`）
/// 已经在那一行日志里了，重复重试只会刷屏；真要重试应该由「换档 / 重启」触发，不是每帧。
#[must_use]
pub fn needs_redispatch(previous: Option<&CornerApplied>, next: &CornerApplied) -> bool {
    match previous {
        None => true,
        Some(previous) => previous != next,
    }
}

#[cfg(test)]
mod tests;
