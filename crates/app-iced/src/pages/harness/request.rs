//! harness 的**参数解析**（M100 从 `harness.rs` 搬出来：那边正好 400 行、零余量）。
//!
//! 搬移**不改行为**：同一份 argv 方言、同一份 slug、同一句报错。等价性判据是「同一 argv 出的 PNG
//! sha256 与拆分前一致」（M100 报告里有实测）。
//!
//! 参数方言与 `crates/app-iced/src/cli.rs` 同源（`--key value` 与 `--key=value` 都认），但**刻意不共用
//! 那个 parser**：harness 是取证通道，默认值与产品启动不同（`--screenshot` 默认开**指针穿透**，见
//! [`Request::passthrough`]）。产品 bin 的密闭手段是另一套（`cli::ForcedState::Rest`）——两条通道的
//! 证据分开算，别互相冒充。

use std::path::PathBuf;

use unlucky_design::color::SchemeFlavor;

use crate::app::components::interaction::Interaction;
use crate::cli::Backend;
use crate::pages::{layout::WidthClass, Screen};

/// 验收窗口尺寸（D30：1180×760、100% DPI）。
const DEFAULT_SIZE: (f32, f32) = (1180.0, 760.0);

/// 取证窗口的落点请求值（`--screenshot` 时才用）。
///
/// 为什么要固定：密闭验证要把系统指针放进窗口里某个点（**故意压在控件上**），窗口位置每次都飘的话，
/// 「指针到底在不在窗口里」就变成了猜——那正是 U1 的形状。真实矩形由 harness 用
/// `window::position` / `window::size` 的**返回值**打日志报出，脚本按日志放指针，不以这个常量为准
/// （它只是请求值）。
pub const CAPTURE_POSITION: (f32, f32) = (80.0, 80.0);

/// 出图前派**一次**的交互动作（M361 r3）——本仓第一次能出「**交互后态**」的图。
///
/// ## 为什么需要它
///
/// 之前的每一张取证图都是**首帧**：能证明「静态版面长什么样」，证明不了「点了之后用户看到什么」。
/// M361 的 P2-2 正是这种断言（「点一条结果 ⇒ 详情与装入按钮进视野」）：算得出来、Task 计数也对，
/// 但**没有一张图**。这个开关就为补那一张。
///
/// ## 能表达什么 / 不能表达什么
///
/// * **能**：开机（首帧）之后派**一个**页面层动作，让滚动/焦点这类**运行期状态**变化在图上落下来。
///   现在的动作表只有一项 [`After::ContentSelect`]（= `pages::content::snap_to_detail()`，
///   与 `app/update/content.rs::Select` 派的是**同一个函数**）。
/// * **不能**：它**不是**事件回放框架——不支持点击坐标、键盘、多步序列、消息队列；
///   也不是「跑一遍真 `App`」：harness 没有 `App`（见 `harness.rs` 模块文档），`Select` 的状态副作用
///   （`request_detail` / `view_dirty`）在这里**不发生**。「`Select` 真的派这个动作」由
///   `app/update/content/tests.rs` 的 Task 计数用例钉住，两边合起来才是完整证据链。
/// * 只在 `--screenshot` 通道 + `--screen=content` 上有效：交互模式没有帧节拍（`subscription()` 在无
///   截图时是空的），「开机后」这个时刻不存在；打到别的页上是**静默空转**（图与首帧逐字节相同却
///   报「已派发」），两条都**直接报错**。
///
/// ## 为什么派在这一帧（与 `CAPTURE_AT_FRAME = 3` 的关系）
///
/// 动作必须排在**采样之前**，否则拍到的是首帧；而派发点就放在采样闸门第一次打开的那一帧（第 3 帧），
/// 那一帧 `return` 掉动作、不请求采样 ⇒ 采样顺延一帧。实测帧号：
///
/// ```text
/// 不带 --after：第 3 帧请求采样 → 第 4 帧落盘（[perf] frames=4）
/// 带   --after：第 3 帧派动作      → 第 4 帧请求采样 → 第 5 帧落盘（frames=5）
/// ```
///
/// 代价是「多一帧」，换来「动作与采样之间至少隔一次 layout」：`snap_to` 落的是**相对偏移**，由 layout
/// 按「视口 / 内容」解算，隔一帧必然已解算完 ⇒ 图里是稳定后的滚动位置（该通道确定性已验：同一 argv
/// 两次拍出的 PNG 逐字节相同）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum After {
    /// 派一次内容库的「滚到详情尾块」——取证「点一条结果之后详情在视野内」。
    ContentSelect,
}

impl After {
    /// `--after` 的词表。**只认这一项**：多一项就等于默许把这里做成回放框架。
    fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "content-select" => Ok(Self::ContentSelect),
            other => Err(format!(
                "--after 只认 content-select（派一次 pages::content::snap_to_detail），收到 {other:?}"
            )),
        }
    }

    /// 日志里的名字（`[harness] --after=...`）。
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ContentSelect => "content-select",
        }
    }
}

/// 一次渲染请求。
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// 渲染哪一页。
    pub screen: Screen,
    /// 窗口宽度（**含外壳**：布局档位就是按它判的）。
    pub width: f32,
    /// 窗口高度。
    pub height: f32,
    /// 主题（`--screenshot-theme` > `--theme` > 浅色）。
    pub flavor: SchemeFlavor,
    /// 出图路径（`None` = 只开窗）。
    pub screenshot: Option<PathBuf>,
    /// 渲染后端请求。
    pub backend: Backend,
    /// 强制缩放因子（截图要 100% DPI）。
    pub scale_factor: Option<f32>,
    /// 设置页正文画**真控件**（`appearance: Some(..)`）还是演示行（`--appearance=real|demo`）。
    ///
    /// 只对 `--screen=settings` 有意义：别的页没有外观分组，静默接受会造出「以为接了线」的假图（U1）。
    pub real_appearance: bool,
    /// 强制交互态（`--force-state=rest|hover|pressed`）：`Some` 时**控件按那一态画**（M285）。
    ///
    /// 为什么这个开关在本文件而不在产品 bin：产品 bin 的 `--screen` 只认 `shell|gallery`，设置页 /
    /// 外观区只能走本 harness 出图（`examples/pages_demo.rs`）。M282 / M284 各自撞过一次——
    /// `pages::Model.forced` 到不了设置页，于是「hover 与 pressed 必须不同」这条判据**做不出像素证据**，
    /// 只能降级成单测。接上之后两条态各自一张图。
    ///
    /// 词与语义与产品的 `cli::ForcedState` 一致（`rest|hover|pressed`），但**刻意不共用那个 enum**：
    /// 产品那条通道的默认值与守卫（`--screenshot` 恒钉 `Rest`）是另一套，把两者拧在一起会让
    /// 「取证通道的默认值」和「产品窗口的默认值」互相污染。这里存的是下游形状 [`Interaction`]，
    /// 落点是 `pages::Model.forced`（`harness/app.rs`）。
    pub force_state: Option<Interaction>,
    /// 截图窗口开**指针穿透**（`window::enable_mouse_passthrough` ⇒ winit `set_cursor_hittest(false)`）。
    ///
    /// 默认：给了 `--screenshot` 就开；**交互模式（没有截图）一律关**——交互窗口收不到鼠标是产品事故，
    /// 所以 `--passthrough` 在交互模式下直接报错，不留「配一下就行」的口子。
    ///
    /// `--passthrough=off` 只服务一件事：对照实验——证明「不开穿透时，同一指针位置会把 hover 漏进图」。
    /// 没有这个反向证据的话，「穿透开着时 hash 不变」既可能是真密闭，也可能只是指针压根没进窗口。
    pub passthrough: bool,
    /// 出图前派**一次**的交互动作（`--after=content-select`，M361 r3）。
    ///
    /// `None`（默认）= 原来那条通道：只出首帧，行为逐像素不变。见 [`After`] 的「能表达什么」。
    pub after: Option<After>,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            screen: Screen::Instances,
            width: DEFAULT_SIZE.0,
            height: DEFAULT_SIZE.1,
            flavor: SchemeFlavor::Light,
            screenshot: None,
            backend: Backend::Auto,
            scale_factor: None,
            real_appearance: false,
            force_state: None,
            passthrough: false,
            after: None,
        }
    }
}

impl Request {
    /// 从进程参数解析（`argv[1..]`）。未知参数 / 缺值 / 数字非法时返回人话。
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut request = Self::default();
        let mut theme: Option<SchemeFlavor> = None;
        let mut shot_theme: Option<SchemeFlavor> = None;
        let mut passthrough: Option<bool> = None;
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            let (key, inline) = match arg.split_once('=') {
                Some((key, value)) => (key.to_owned(), Some(value.to_owned())),
                None => (arg.clone(), None),
            };

            match key.as_str() {
                "--screen" => {
                    let raw = take(&mut args, inline, "--screen")?;
                    request.screen = Screen::from_slug(&raw).ok_or_else(|| {
                        format!("--screen 只认 instances|download|settings|logs，收到 {raw:?}")
                    })?;
                }
                "--width" => {
                    request.width = number(&take(&mut args, inline, "--width")?, "--width")?
                }
                "--height" => {
                    request.height = number(&take(&mut args, inline, "--height")?, "--height")?;
                }
                "--theme" => theme = Some(flavor(&take(&mut args, inline, "--theme")?)?),
                "--screenshot-theme" => {
                    shot_theme = Some(flavor(&take(&mut args, inline, "--screenshot-theme")?)?);
                }
                "--screenshot" => {
                    request.screenshot =
                        Some(PathBuf::from(take(&mut args, inline, "--screenshot")?));
                }
                "--software-renderer" => request.backend = Backend::Cpu,
                "--renderer" => {
                    request.backend = match take(&mut args, inline, "--renderer")?.as_str() {
                        "auto" => Backend::Auto,
                        "gpu" => Backend::Gpu,
                        "cpu" => Backend::Cpu,
                        other => {
                            return Err(format!("--renderer 只认 auto|gpu|cpu，收到 {other:?}"))
                        }
                    };
                }
                "--scale-factor" => {
                    let raw = take(&mut args, inline, "--scale-factor")?;
                    let value = number(&raw, "--scale-factor")?;
                    if value <= 0.0 {
                        return Err(format!("--scale-factor 必须 > 0，收到 {raw:?}"));
                    }
                    request.scale_factor = Some(value);
                }
                "--appearance" => {
                    request.real_appearance =
                        match take(&mut args, inline, "--appearance")?.as_str() {
                            "real" => true,
                            "demo" => false,
                            other => {
                                return Err(format!("--appearance 只认 real|demo，收到 {other:?}"))
                            }
                        };
                }
                "--force-state" => {
                    // 三档词与产品 bin 同一套（`cli.rs` 的 `--force-state`），报错文案也照抄。
                    request.force_state =
                        Some(match take(&mut args, inline, "--force-state")?.as_str() {
                            "rest" => Interaction::Enabled,
                            "hover" => Interaction::Hovered,
                            "pressed" => Interaction::Pressed,
                            other => {
                                return Err(format!(
                                    "--force-state 只认 rest|hover|pressed，收到 {other:?}"
                                ))
                            }
                        });
                }
                "--passthrough" => {
                    passthrough = Some(match take(&mut args, inline, "--passthrough")?.as_str() {
                        "on" => true,
                        "off" => false,
                        other => return Err(format!("--passthrough 只认 on|off，收到 {other:?}")),
                    });
                }
                // M361 r3：出图前派一次交互动作（现在的词表只有 content-select）
                "--after" => {
                    request.after = Some(After::parse(&take(&mut args, inline, "--after")?)?);
                }
                other => return Err(format!("未知参数 {other:?}")),
            }
        }

        request.flavor = shot_theme.or(theme).unwrap_or(SchemeFlavor::Light);

        // 指针穿透**只服务取证通道**：没有 `--screenshot` 时显式给值就是配置错误，不是「忽略」——
        // 静默忽略会让人以为交互窗口也「密闭」了，而它其实照常收鼠标。
        if passthrough.is_some() && request.screenshot.is_none() {
            return Err(
                "--passthrough 只服务 --screenshot 取证通道：交互模式的窗口必须收得到鼠标"
                    .to_owned(),
            );
        }
        request.passthrough = match request.screenshot {
            // 取证默认密闭：开窗口的那一刻就断开指针输入（见 `Harness::update` 的 `WindowReady`）
            Some(_) => passthrough.unwrap_or(true),
            None => false,
        };

        // 交互后态**只服务取证通道**：开机后这个时刻由帧节拍给（`window::frames()`），而没有
        // `--screenshot` 时 `subscription()` 是空的 ⇒ 那一刻根本不会到来。静默忽略会造出一张
        // 「看起来是首帧」的假图（U1 的同族），所以显式报错。
        if request.after.is_some() && request.screenshot.is_none() {
            return Err(
                "--after 只服务 --screenshot 取证通道：没有帧节拍就没有「开机后」这个时刻"
                    .to_owned(),
            );
        }

        // 动作是**页面层**的（现在只有 `content-select` = 内容库的「滚到详情」）：打到别的页上就是
        // **静默空转**——派发日志照打，图与首帧**逐字节相同**（复审 r2 实测 settings 那一对）。
        // 那正是这条通道要禁止的「假装接了线」，所以照 `--appearance=real` 的形状直接报错。
        // ⚠️ 注意与下面那条**不是**同一件事：`--screen=content` 的 `Wide` / `Narrow` 档仍是空操作
        // （详情不在列表滚动区里 ⇒ 没有那个 Id），那是刻意的设计（见 `parts.rs::list` 的注释），
        // 不报错。
        if request.after.is_some() && request.screen != Screen::Content {
            return Err(format!(
                "--after=content-select 只对 --screen=content 有意义（当前 --screen={}）：\
                 打到别的页上是静默空转，日志会说派了动作而图与首帧逐字节相同",
                request.screen.slug()
            ));
        }

        if request.real_appearance && request.screen != Screen::Settings {
            return Err(format!(
                "--appearance=real 只对 --screen=settings 生效（当前 --screen={}），别的页没有外观分组，\
                 静默接受会出一张假装接了线的图",
                request.screen.slug()
            ));
        }

        if request.width < 320.0 || request.height < 320.0 {
            return Err(format!(
                "窗口太小：{}×{}（至少 320×320）",
                request.width, request.height
            ));
        }

        Ok(request)
    }

    /// 出图文件名片段（`<screen>[-real][-hover|-pressed]-<class>-<flavor>-w<宽>`，归档用）。
    pub fn slug(&self) -> String {
        let class = WidthClass::of(self.width);
        let flavor = if self.flavor.is_dark() {
            "dark"
        } else {
            "light"
        };
        let width = self.width as u32;
        // 「演示行版」与「真控件版」必须能靠文件名分开，否则归档里两张图分不清谁是谁
        let screen = if self.real_appearance {
            format!("{}-real", self.screen.slug())
        } else {
            self.screen.slug().to_owned()
        };
        // 强制态也进文件名（`-hover` / `-pressed`）：同一屏在两个 forced 下是**两张不同的证据图**，
        // 归档里必须能分开（与 `-real` 同一条理由）。`rest` / 不给 = 不加后缀 ⇒ 既有文件名逐字不变。
        let screen = match self.force_state {
            Some(Interaction::Hovered) => format!("{screen}-hover"),
            Some(Interaction::Pressed) => format!("{screen}-pressed"),
            _ => screen,
        };
        format!("{screen}-{}-{flavor}-w{width}", class.name())
    }
}

fn take(
    args: &mut impl Iterator<Item = String>,
    inline: Option<String>,
    flag: &str,
) -> Result<String, String> {
    inline
        .or_else(|| args.next())
        .ok_or_else(|| format!("{flag} 需要一个值"))
}

fn number(raw: &str, flag: &str) -> Result<f32, String> {
    raw.parse::<f32>()
        .map_err(|_| format!("{flag} 需要数字，收到 {raw:?}"))
        .and_then(|value| {
            if value.is_finite() && value > 0.0 {
                Ok(value)
            } else {
                Err(format!("{flag} 必须是正数，收到 {raw:?}"))
            }
        })
}

fn flavor(raw: &str) -> Result<SchemeFlavor, String> {
    match raw {
        "light" => Ok(SchemeFlavor::Light),
        "dark" => Ok(SchemeFlavor::Dark),
        other => Err(format!("主题只认 light|dark，收到 {other:?}")),
    }
}
