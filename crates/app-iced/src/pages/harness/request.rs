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
    /// 截图窗口开**指针穿透**（`window::enable_mouse_passthrough` ⇒ winit `set_cursor_hittest(false)`）。
    ///
    /// 默认：给了 `--screenshot` 就开；**交互模式（没有截图）一律关**——交互窗口收不到鼠标是产品事故，
    /// 所以 `--passthrough` 在交互模式下直接报错，不留「配一下就行」的口子。
    ///
    /// `--passthrough=off` 只服务一件事：对照实验——证明「不开穿透时，同一指针位置会把 hover 漏进图」。
    /// 没有这个反向证据的话，「穿透开着时 hash 不变」既可能是真密闭，也可能只是指针压根没进窗口。
    pub passthrough: bool,
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
            passthrough: false,
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
                "--passthrough" => {
                    passthrough = Some(match take(&mut args, inline, "--passthrough")?.as_str() {
                        "on" => true,
                        "off" => false,
                        other => return Err(format!("--passthrough 只认 on|off，收到 {other:?}")),
                    });
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

    /// 出图文件名片段（`<screen>[-real]-<class>-<flavor>-w<宽>`，归档用）。
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
