//! 命令行参数（骨架期 + 组件库 harness 需要的那些；业务开关等页面 mission 再加）。
//!
//! 取值语法与现役 Slint 版一致：`--key value` 与 `--key=value` 两种都认
//! （`crates/app/src/screenshot.rs` 的 `value_of` 是同一口径），这样将来 CI 换 bin 不用改脚本。
//!
//! ## 参数总表
//!
//! | 参数 | 取值 | 语义 |
//! |---|---|---|
//! | `--software-renderer` | — | 等价 `--renderer=cpu`（D27 的 CPU 档） |
//! | `--renderer` | `auto`/`gpu`/`cpu` | 渲染后端请求 |
//! | `--theme` | `light`/`dark` | 主题；不给则跟随系统（仅交互模式） |
//! | `--screenshot` / `--screenshot-theme` | 路径 / `light`·`dark` | 截图通道（出图后退出）与截图专用主题 |
//! | `--screen` | `shell`/`gallery` | 启动画面；**`gallery` = 组件库视觉验收 harness**（D39 §5） |
//! | `--scale-factor` | `>0` | 强制缩放因子（验收要 100%，即 `1`） |
//! | `--diff` / `--diff-scan` | 两个路径 / 非负整数 | 逐像素对比（不开窗）与 ±n px 平移扫描 |
//! | `--force-state` | `rest`/`hover`/`pressed` | 把整屏可交互控件强制成某态（取证）；**给了 `--screenshot` 就默认 `rest`** |
//! | `--data-dir` | 路径 | 翻牌兼容：**接受并忽略**（CI 冒烟在用） |
//!
//! **截图主题默认 Light（门禁依赖，不许改回 None）**，理由与不变量见
//! [`Options::capture_flavor`] 与 `app.rs` 的截图路径。

use std::path::PathBuf;

use unlucky_design::color::SchemeFlavor;

/// 渲染后端请求（D27）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backend {
    /// 探测后决定：有硬件 GPU 就让 iced 自己走 wgpu→tiny-skia 回退；没有就强制 CPU 起步。
    #[default]
    Auto,
    /// 优先 GPU（写 `ICED_BACKEND=wgpu,tiny-skia`；拿不到适配器仍会降级，不硬失败）。
    Gpu,
    /// 强制 CPU（`--software-renderer` 的等价物，写 `ICED_BACKEND=tiny-skia`）。
    Cpu,
}

impl Backend {
    /// 稳定名字（日志用）。
    pub const fn name(self) -> &'static str {
        match self {
            Backend::Auto => "auto",
            Backend::Gpu => "gpu",
            Backend::Cpu => "cpu",
        }
    }

    /// 截图日志用的后端说明（`cfg!(feature = "gpu")` 决定有没有 wgpu 这一档）。
    pub fn note(self) -> &'static str {
        if !cfg!(feature = "gpu") {
            return "cpu(build without wgpu)";
        }
        match self {
            Backend::Cpu => "cpu(forced)",
            Backend::Gpu => "gpu(preferred)",
            Backend::Auto => "auto",
        }
    }
}

/// 启动画面（D39 §5：`gallery` 是视觉验收 harness，不是产品页面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// 产品骨架：自绘标题栏 + 导航轨 + 内容区。
    #[default]
    Shell,
    /// 组件库 harness：整窗只放状态矩阵。
    Gallery,
}

impl Screen {
    /// 稳定名字（日志用）。
    pub const fn name(self) -> &'static str {
        match self {
            Screen::Shell => "shell",
            Screen::Gallery => "gallery",
        }
    }
}

/// 解析后的启动参数。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Options {
    /// 渲染后端请求。
    pub backend: Backend,
    /// 主题：`None` = 探测系统主题。
    pub theme: Option<SchemeFlavor>,
    /// 截图通道：给了路径就出图后退出。
    pub screenshot: Option<PathBuf>,
    /// `--screenshot-theme`（优先级高于 `--theme`）。
    pub screenshot_theme: Option<SchemeFlavor>,
    /// 强制窗口缩放因子（验收要 100% DPI，即 `1`；`None` = 用系统 DPI）。
    pub scale_factor: Option<f32>,
    /// `--diff <left> <right>` 与 `--diff-scan <n>`：逐像素对比（不开窗）与 ±n px 平移扫描。
    pub diff: Option<(PathBuf, PathBuf)>,
    /// 见上（默认 0 = 不扫）。
    pub diff_scan: u32,
    /// `--force-state=hover|pressed`：把按钮强制成某个状态便于截图取证（D30 的状态图）。
    pub force_state: Option<ForcedState>,
    /// `--screen=shell|gallery`：启动画面（`gallery` = 组件库视觉验收 harness）。
    pub screen: Screen,
    /// `--data-dir <dir>`：当前**接受并忽略**（现役 Slint 版 CI 冒烟在用；
    /// 真正的消费由后续页面 mission 接 `host`/`state` 时做）。
    pub data_dir: Option<PathBuf>,
}

/// 截图取证用的强制交互状态（只影响观感，不改变任何状态机）。
///
/// 五态完整矩阵由 `--screen=gallery` 的 `Button::state(..)` 出，本开关只给整屏取证。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForcedState {
    /// **常态**（静息）：压掉系统光标带来的 hover/pressed。
    ///
    /// 截图通道（给了 `--screenshot`）**默认就是它**，理由是实测：M99 复审在同一个 exe 上连跑 8 次，
    /// 第 6 次给出第三个 hash，差异区是某个按钮被画成 `Hovered`——因为 harness/产品都开真窗口，
    /// `button::Status` 由系统光标位置决定。取证图不能因为「指针恰好压在某个按钮上」而变色。
    /// 显式 `--force-state=hover|pressed` 仍然优先（那是**故意**要画那个态）。
    ///
    /// ⚠️ **语义边界**：`Rest` 落成 `Interaction::Enabled`，而 `components::buttons::assemble()`
    /// 把「没有 `on_press`」的按钮默认画成 **Disabled** —— 于是 `Rest` 会把一个本应禁用的按钮
    /// 画成可用。当前零影响（所有 `forced_state(` 调用点与 shell 的 caption / nav 按钮都带消息），
    /// 但给「无消息按钮」出取证图时别套 `Rest`。
    Rest,
    /// 所有可交互控件按 hover 画。
    Hover,
    /// 所有可交互控件按 pressed 画。
    Pressed,
}

impl Options {
    /// 从进程参数解析（错误信息是人话，不 panic、不 `exit`）。
    ///
    /// # Errors
    /// 取值不合法（未知 `--renderer`/`--theme`/`--screen`、非法 `--scale-factor`）时返回人话。
    pub fn from_env() -> Result<Self, String> {
        Self::parse(std::env::args().skip(1))
    }

    /// 从任意参数序列解析（`argv[1..]`，测试直接喂数组）。
    ///
    /// # Errors
    /// 同 [`Options::from_env`]。
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            let (key, inline) = match arg.split_once('=') {
                Some((key, value)) => (key.to_owned(), Some(value.to_owned())),
                None => (arg.clone(), None),
            };

            match key.as_str() {
                "--software-renderer" => options.backend = Backend::Cpu,
                "--renderer" => {
                    let value = take(&mut args, inline, "--renderer")?;
                    options.backend = match value.as_str() {
                        "auto" => Backend::Auto,
                        "gpu" => Backend::Gpu,
                        "cpu" => Backend::Cpu,
                        other => {
                            return Err(format!("--renderer 只认 auto|gpu|cpu，收到 {other:?}"));
                        }
                    };
                }
                "--theme" => {
                    options.theme = Some(flavor(&take(&mut args, inline, "--theme")?)?);
                }
                "--screenshot-theme" => {
                    options.screenshot_theme =
                        Some(flavor(&take(&mut args, inline, "--screenshot-theme")?)?);
                }
                "--screenshot" => {
                    options.screenshot =
                        Some(PathBuf::from(take(&mut args, inline, "--screenshot")?));
                }
                "--scale-factor" => {
                    let raw = take(&mut args, inline, "--scale-factor")?;
                    let value: f32 = raw
                        .parse()
                        .map_err(|_| format!("--scale-factor 需要数字，收到 {raw:?}"))?;
                    if !(value.is_finite() && value > 0.0) {
                        return Err(format!("--scale-factor 必须 > 0，收到 {raw:?}"));
                    }
                    options.scale_factor = Some(value);
                }
                "--diff" => {
                    let left = take(&mut args, inline, "--diff")?;
                    let right = args
                        .next()
                        .ok_or_else(|| "--diff 需要两张图的路径".to_owned())?;
                    options.diff = Some((PathBuf::from(left), PathBuf::from(right)));
                }
                "--diff-scan" => {
                    let raw = take(&mut args, inline, "--diff-scan")?;
                    options.diff_scan = raw
                        .parse()
                        .map_err(|_| format!("--diff-scan 需要非负整数，收到 {raw:?}"))?;
                }
                "--force-state" => {
                    let raw = take(&mut args, inline, "--force-state")?;
                    options.force_state = Some(match raw.as_str() {
                        "rest" => ForcedState::Rest,
                        "hover" => ForcedState::Hover,
                        "pressed" => ForcedState::Pressed,
                        other => {
                            return Err(format!(
                                "--force-state 只认 rest|hover|pressed，收到 {other:?}"
                            ));
                        }
                    });
                }
                "--screen" => {
                    let raw = take(&mut args, inline, "--screen")?;
                    options.screen = match raw.as_str() {
                        "shell" => Screen::Shell,
                        "gallery" => Screen::Gallery,
                        other => {
                            return Err(format!("--screen 只认 shell|gallery，收到 {other:?}"));
                        }
                    };
                }
                // 翻牌前的兼容参数：接受并忽略（见字段文档）
                "--data-dir" => {
                    options.data_dir = Some(PathBuf::from(take(&mut args, inline, "--data-dir")?));
                }
                other => return Err(format!("未知参数 {other:?}")),
            }
        }

        // **截图通道默认强制 `Rest`**（M100）：取证图不许跟着系统指针的位置变。
        // 交互模式保持 `None`——那是用户的真窗口，不许替用户把状态钉死（钉死了就永远看不到 hover）。
        if options.screenshot.is_some() && options.force_state.is_none() {
            options.force_state = Some(ForcedState::Rest);
        }

        Ok(options)
    }

    /// 截图时实际使用的主题（**只表达用户显式给了什么**；默认值见 [`Options::capture_flavor`]）。
    pub fn shot_flavor(&self) -> Option<SchemeFlavor> {
        self.screenshot_theme.or(self.theme)
    }

    /// 本次进程真正要画的主题（**唯一取值口**）。
    ///
    /// 截图模式（给了 `--screenshot`）**永远返回 `Some`**：`--screenshot-theme` > `--theme` > Light。
    /// 交互模式只有显式指定才有值（`None` = 等系统主题）。
    ///
    /// ⚠️ **门禁依赖，不许把截图分支改回 `None`**：CI 的 runtime-smoke 打
    /// `--software-renderer --screenshot <png> --data-dir <dir>`（不带主题），xvfb 里没有桌面主题
    /// 会话，M68 的「等系统主题到达再出图」闸门在这条路径上会把 runner 挂到超时（不是红，更难查）。
    pub fn capture_flavor(&self) -> Option<SchemeFlavor> {
        match self.screenshot {
            Some(_) => Some(self.shot_flavor().unwrap_or(SchemeFlavor::Light)),
            None => self.shot_flavor(),
        }
    }

    /// 是否要问系统主题。截图模式**永远不问**——这就是「无主题环境不会卡住」的不变量。
    pub fn wants_system_theme(&self) -> bool {
        self.capture_flavor().is_none()
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

fn flavor(raw: &str) -> Result<SchemeFlavor, String> {
    match raw {
        "light" => Ok(SchemeFlavor::Light),
        "dark" => Ok(SchemeFlavor::Dark),
        other => Err(format!("--theme 只认 light|dark，收到 {other:?}")),
    }
}

#[cfg(test)]
#[path = "cli/tests.rs"]
mod tests;
