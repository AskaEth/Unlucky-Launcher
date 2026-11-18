//! M116 / M182：窗口外框圆角的**判据**（纯函数，零 `unsafe`、零 FFI、零环境读取）。
//!
//! ## 判据为什么单独拎出来
//!
//! 「能不能圆」= 用户策略 × 宿主平台 × 渲染档，这几件事**可以被断言证明**；而合成器的**视觉**
//! 结果在本仓库的验证环境（RDP 断连、抓屏失效、无 Mac 真机）上没有参考价值。把可测的那一半独立
//! 出来，才有「有测试的判据 + 没测试的观感」这条诚实的分界（与 [`super::super::effective_window_alpha`]
//! 同一套分法）。
//!
//! 真正下发给窗口的动作在 [`super::transparency`]（`unsafe` 落点之一，另一个是 [`super::console`]）。
//!
//! ## 平台矩阵（M182 修订：非 Windows 不再是「未知」一格）
//!
//! | 平台 | `source` | 谁画圆角 | 依据级别 |
//! |---|---|---|---|
//! | Win11 / Server 2025（build ≥ 22000） | `dwm` | DWM（我们只下发偏好） | 写入被接受 = **本机 API 实测**；「圆没圆」= **未验**（Server 2025 上实测无圆角，见下） |
//! | Win10 及更早（build < 22000） | `square` + `os-build` / `cpu-tier` | 没人 | 「该属性不存在」= **外部文档证据**；本机没有 Win10 |
//! | Windows 版本读不到 | `square` + `unknown-os` | 没人 | 探测失败，不猜 |
//! | macOS | `compositor` + `borderless` | 本该窗口服务器，但**它只裁 titled 窗**，我们是 borderless ⇒ 没人 | **源码级证据**（见 [`crate::platform::macos`]） |
//! | Linux X11 / Wayland | `compositor` + `client-side` | 本该合成器，但协议里没有圆角这回事，CSD 窗没人裁 | **规范级证据**（见 [`crate::platform::linux`]） |
//! | 其余（FreeBSD / Android / wasm…） | `square` + `unsupported` | 没人 | 不承诺 |
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
//!
//! ④ **整窗 α 会关掉圆角**（tower 裁定 A / D47：整窗透明度优先于 DWM 圆角）：α < 1 时我们补
//! `WS_EX_LAYERED`，此时系统会忽略圆角请求——请求照样发出（日志里 `pref=` 有值），
//! 只是不生效；α 回到 1.0 就恢复。日志用 `suppressed_by=window_alpha` 记这一条**观测事实**。
//!
//! 「圆角长什么样」归真机验收，证据目录见 `DevDocs/artifacts/m116/`。

use unlucky_presenter::prefs::{WindowCorner, WindowsVersion, WIN11_MIN_BUILD};

use crate::platform::host::HostOs;
use crate::Backend;

/// 一次「圆角该从哪来」的判定结果（M116 / M182）。
///
/// 三档**互斥**，回答的是同一个问题：「这个窗口的四角由谁负责」。
///
/// | 来源 | 谁负责 | 我们做什么 |
/// |---|---|---|
/// | [`CornerSource::Dwm`] | Windows 的 DWM（半径不由我们定，也读不到） | 只下发 `DWMWA_WINDOW_CORNER_PREFERENCE` |
/// | [`CornerSource::Compositor`] | 平台的窗口合成器 / 窗口服务器**按它自己的规则**（M182 新增） | **什么都不做**：不下发偏好、不自绘裁切（D46） |
/// | [`CornerSource::Square`] | 没人 | Windows 上显式下发 `DWMWCP_DONOTROUND`，把「方」也变成可观测的选择 |
///
/// **「合成器」这一档不等于「一定有圆角」**：它只说「圆角的决定权不在我们手里」。落到这一档
/// 之后到底圆不圆，由 `excluded` 那一栏如实交代（macOS = `borderless`、Linux = `client-side`）。
/// 把它写成「跟随合成器 ⇒ 圆角」正是 M182 要修的误读。
///
/// **没有 `Self`（自绘圆角）这一档**，这是刻意的（用户定案 2026-11-03 + tower 裁定，D46 第 4 条）：
/// 窗口是 `decorations=false` + 自绘标题栏 + 整窗 `LWA_ALPHA`，自绘圆角意味着把内容层裁成圆角，
/// 那会把标题栏三键与边缘缩放热区一起裁掉；系统也不暴露圆角半径（无 API 可读），
/// 「固定 8/12/16」只是凭空维护一个数字。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CornerSource {
    /// DWM 系统圆角（Win11 / Server 2025 内核，build ≥ [`WIN11_MIN_BUILD`]）。
    Dwm,
    /// 平台跟随合成器（M182）：圆角归属交给窗口合成器 / 窗口服务器，我们既不下发也不自绘。
    Compositor,
    /// 直角（用户选择、系统不支持、或该平台不承诺）。
    Square,
}

impl CornerSource {
    /// 日志取值（`corner=… source=dwm|compositor|square`）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dwm => "dwm",
            Self::Compositor => "compositor",
            Self::Square => "square",
        }
    }

    /// 是否要求 DWM 真去画圆角（唯一需要下发 FFI 的那一档）。
    #[must_use]
    pub const fn is_dwm(self) -> bool {
        matches!(self, Self::Dwm)
    }

    /// 是否把圆角归属交给合成器（M182：只在非 Windows 上可能为真）。
    #[must_use]
    pub const fn is_compositor(self) -> bool {
        matches!(self, Self::Compositor)
    }
}

/// 为什么**没有**圆角（`None` = 就是 DWM 圆角）。日志里逐字打出来。
///
/// 为什么要这个枚举而不是一个 `bool`：用户报的现象是「别的窗口都有圆角，只有我们的没有」，
/// 于是「没圆角」至少有六种完全不同的原因，混成一个 `square` 以后没人能分辨
/// 「没生效」与「生效了但看不见」（M116 任务书原话）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CornerExclusion {
    /// 用户在设置里选了「直角」。
    UserSquare,
    /// 系统太老：Win10 及更早（build < [`WIN11_MIN_BUILD`]）——DWM 没有这个偏好属性。
    OsBuild,
    /// Windows 版本探测不出来（注册表读不到 / 有东西在骗版本号）。**只在 Windows 上出现**。
    UnknownOs,
    /// 非 DWM 平台上的软件渲染档：圆角只能靠自绘/`SetWindowRgn` 毛边路，D36 的
    /// 「CPU 档不碰合成技巧」优先，所以退回直角。
    CpuTier,
    /// macOS（M182）：窗口是 `decorations=false` ⇒ winit 给 `NSWindowStyleMask::Borderless`
    /// （**没有 `Titled`**）⇒ 窗口服务器不给这类窗口做圆角裁剪。不是「系统不支持圆角」，
    /// 是「我们交上去的形状不在会被裁的那一类里」。证据见 [`crate::platform::macos`]。
    BorderlessFrame,
    /// Linux（M182）：`xdg-shell` / X11 协议里都没有「圆角半径」这种东西，CSD 无边框窗
    /// 合成器不会替我们裁（谁装饰谁负责，而我们自己声明了 CSD）。D46 又排除自绘 ⇒ 直角。
    /// 证据与各合成器差异见 [`crate::platform::linux`]。
    ClientSideFrame,
    /// 不认识的平台（FreeBSD / Android / wasm …）：不承诺。
    Unsupported,
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
            Self::BorderlessFrame => "borderless",
            Self::ClientSideFrame => "client-side",
            Self::Unsupported => "unsupported",
        }
    }
}

/// 判定结果：来源 + 排除原因 + 宿主平台 + Windows 版本（日志与测试共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CornerPlan {
    /// 圆角从哪来。
    pub source: CornerSource,
    /// `None` = 走合成器/DWM；`Some(..)` = 这一档没有圆角，并说明为什么。
    pub excluded: Option<CornerExclusion>,
    /// 宿主平台族（M182：判据的输入，日志里也打出来）。
    pub host: HostOs,
    /// 探测到的 Windows 版本（`None` = 非 Windows / 读不到）。
    pub os: Option<WindowsVersion>,
}

impl CornerPlan {
    /// 日志行：`corner=policy=system host=macos source=compositor os=n/a excluded_reason=borderless radius=system`。
    ///
    /// **`radius=system` 是刻意的**：系统不暴露圆角半径，我们也不自己定一个数字
    /// （用户定案取消了 8/12/16 档），所以这一栏永远只说「系统决定」。
    ///
    /// **`host=` 是 M182 加的**：用户看到的那行 `source=square os=unknown` 之所以难判断，
    /// 就是因为「未知」同时被 Windows 探测失败和「这台根本不是 Windows」共用。有了 `host=`
    /// 与 `excluded_reason=borderless|client-side`，同一行就能自己解释完。
    ///
    /// **非 Windows 的 `os=n/a`**：那一栏装的是 Windows 版本，在 macOS / Linux 上不是「没读到」，
    /// 而是「跟本机无关」——写 `unknown` 会让人去找一个根本不存在的注册表项。
    #[must_use]
    pub fn log_line(&self, policy: WindowCorner) -> String {
        let os = match (self.host, self.os) {
            (HostOs::Windows, Some(os)) => os.version_string(),
            (HostOs::Windows, None) => "unknown".to_owned(),
            _ => "n/a".to_owned(),
        };
        format!(
            "corner=policy={} host={} source={} os={os} excluded_reason={} radius=system",
            policy.as_str(),
            self.host.as_str(),
            self.source.as_str(),
            self.excluded.map_or("none", CornerExclusion::as_str),
        )
    }
}

/// 判定「这一次窗口该不该有系统圆角」（纯函数，不调 FFI、不读环境）。
///
/// 生产入口：平台取 [`HostOs::current`]（编译期常量），Windows 版本由调用方探测后传进来。
/// 想验另外两行真值表时用 [`plan_corner_for`]——那是同一个判据，只是平台可以由测试喂。
#[must_use]
pub fn plan_corner(
    policy: WindowCorner,
    os: Option<WindowsVersion>,
    backend: Backend,
) -> CornerPlan {
    plan_corner_for(policy, HostOs::current(), os, backend)
}

/// 真值表（`os` / `backend` 只对 Windows 行有意义）：
///
/// | `policy` | `host` | `os.build` | backend | `source` | `excluded` |
/// |---|---|---|---|---|---|
/// | `Square` | 任意 | 任意 | 任意 | `Square` | `UserSquare` |
/// | `System` | Windows | `None` | 任意 | `Square` | `UnknownOs` |
/// | `System` | Windows | `< 22000` | GPU/Auto | `Square` | `OsBuild` |
/// | `System` | Windows | `< 22000` | CPU | `Square` | `CpuTier` |
/// | `System` | Windows | `>= 22000` | 任意 | `Dwm` | `None` |
/// | `System` | MacOs | 忽略 | 任意 | `Compositor` | `BorderlessFrame` |
/// | `System` | Linux | 忽略 | 任意 | `Compositor` | `ClientSideFrame` |
/// | `System` | Other | 忽略 | 任意 | `Square` | `Unsupported` |
///
/// 为什么 CPU 档在 Win11 上**照样**用 DWM 圆角（**tower 2026-11-03 裁定，按此执行**）：
/// D36 的 CPU 档约束针对的是**我们自己的渲染路径**（面板 α / 玻璃 / 模糊要关、不许拿透明叠加
/// 去换效果），而 DWM 圆角是**合成器侧**的效果，不增加我们的 GPU/CPU 绘制负担。所以
/// 「CPU 档关掉 Win11 圆角」不是 D36 的要求，而是凭空造一个没人要的降级。
///
/// 反过来在非 DWM 平台上，圆角只剩「自绘裁切 / `SetWindowRgn` 毛边」这两条**我们自己的**
/// 合成技巧，那才是 D36 要拦的东西 ⇒ 那一格落 `cpu-tier` 而不是 `os-build`
/// （同一个直角，**原因不同**，日志里可分辨）。
///
/// macOS / Linux 两行**不看 `backend`**（M182）：我们根本不下发任何东西，也就没有「渲染档
/// 要不要让步」这回事——CPU 档与 GPU 档在这两个平台上得到同一份判定，这是事实而不是遗漏。
#[must_use]
pub fn plan_corner_for(
    policy: WindowCorner,
    host: HostOs,
    os: Option<WindowsVersion>,
    backend: Backend,
) -> CornerPlan {
    let square = |excluded| CornerPlan {
        source: CornerSource::Square,
        excluded: Some(excluded),
        host,
        os,
    };

    if policy == WindowCorner::Square {
        return square(CornerExclusion::UserSquare);
    }
    match host {
        // Windows：能不能圆是 DWM 的属性与版本问题，得问。
        HostOs::Windows => match os {
            None => square(CornerExclusion::UnknownOs),
            Some(os) if os.build < WIN11_MIN_BUILD => match backend {
                Backend::Cpu => square(CornerExclusion::CpuTier),
                Backend::Gpu | Backend::Auto => square(CornerExclusion::OsBuild),
            },
            Some(_) => CornerPlan {
                source: CornerSource::Dwm,
                excluded: None,
                host,
                os,
            },
        },
        // macOS：圆角归窗口服务器，但我们交的是 borderless 窗（`decorations: false`）⇒ 它不裁。
        // 迁移成原生标题栏是另一件需要真机验收的事，配方见 `platform::macos`。
        HostOs::MacOs => CornerPlan {
            source: CornerSource::Compositor,
            excluded: Some(CornerExclusion::BorderlessFrame),
            host,
            os,
        },
        // Linux：协议里没有圆角，CSD 窗由客户端装饰 ⇒ 没人替我们裁，自绘被 D46 排除。
        HostOs::Linux => CornerPlan {
            source: CornerSource::Compositor,
            excluded: Some(CornerExclusion::ClientSideFrame),
            host,
            os,
        },
        HostOs::Other => square(CornerExclusion::Unsupported),
    }
}

/// 产品日志行的**唯一格式化落点**（纯函数：把四段文本拼成一个固定形状）。
///
/// ```text
/// [corner] <context> corner=policy=system host=windows source=dwm os=10.0.26100 excluded_reason=none \
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
