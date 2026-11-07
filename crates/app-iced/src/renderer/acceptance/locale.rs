//! `[env]` 与 `[i18n]`：**本机是什么**（系统 / arch / 核数 / GPU / 时区）与**为什么是这个语言**。
//!
//! ## 语言链为什么在这里而不是 `platform/`
//!
//! `platform::locale::SystemLocale` 是 **Windows 注册表**那一路（`reg.exe`，零 FFI）。macOS 的
//! 真值源是 `defaults read -g AppleLanguages`，而 M98 的 scope 没有 `platform/**`，所以
//! [`AppleLanguages`] 先落在这里。**它该搬去 `platform/locale.rs`**——两处并存会漂（见交接报告）。

use unlucky_i18n::lang::{self, Detection, LocaleSource};
use unlucky_presenter::settings_bridge::SettingsBridge;

use crate::Probe;

// ---------------------------------------------------------------- [env]

/// `[env]`：系统 / arch / 核数 / 语言 / 时区 / 缩放请求 / GPU。
///
/// `gpu` 与 `hardware` **来自真实适配器枚举**（[`Probe`]），不是请求值；枚举不到就写
/// `<无适配器>` 并把 `hardware=false` 一起打出来——「没有适配器」与「没枚举」是两件事。
///
/// `display=` 恒为 `needs-window`：显示器尺寸只有拿到窗口句柄才问得到，而这一行在开窗之前就打了。
/// 真值由验收脚本用 `system_profiler SPDisplaysDataType` 采进 `logs/00-env.txt`——**写个猜的值
/// 比留白更坏**（那会让「Monitor 4K / 无外接」这种结论变成猜测）。
#[must_use]
pub fn env_line(probe: &Probe, language: &str, scale: Option<f32>, tz: Option<&str>) -> String {
    format!(
        "[env] os={} arch={} cores={} language={language} tz={} scale_requested={} display=needs-window \
         adapters={} hardware={} gpu=\"{}\"",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map_or(0, std::num::NonZeroUsize::get),
        tz.unwrap_or("<未设置>"),
        scale.map_or_else(|| "system".to_owned(), |value| value.to_string()),
        probe.adapters.len(),
        probe.hardware,
        gpu_summary(probe),
    )
}

/// 枚举结果里**最该被当成 GPU** 的那个适配器的摘要：`DiscreteGpu > IntegratedGpu > 其它`。
///
/// 为什么不是「第一个非 Cpu/Other」（M98 r1 p3-2）：本机枚举顺序是核显在前，于是 `[env] gpu=`
/// 报的是 `AMD Radeon(TM) Graphics [IntegratedGpu]`，而 iced 实际选的是
/// `AMD Radeon RX 6600 XT [DiscreteGpu]`。一行「机器有什么显卡」的日志报错型号，比不报更坏。
/// 判据仍然只管**型号名称**：真正的「iced 选了谁」只有 `[gpu] …Selected: AdapterInfo` 那行有。
#[must_use]
pub fn gpu_summary(probe: &Probe) -> String {
    probe
        .adapters
        .iter()
        .max_by_key(|adapter| gpu_rank(adapter.device_type))
        .map_or_else(
            || "<无适配器>".to_owned(),
            |adapter| format!("{} [{}]", adapter.name, adapter.device_type),
        )
}

/// 适配器的「该不该被当成这台机器的显卡」排序权重（越大越该）。
fn gpu_rank(device_type: &str) -> u8 {
    match device_type {
        "DiscreteGpu" => 3,
        "IntegratedGpu" => 2,
        "VirtualGpu" => 1,
        "Cpu" | "Other" => 0,
        _ => 1,
    }
}

/// 时区（`TZ` 环境变量）。`std` 拿不到系统时区，所以这里**只报环境变量**，没有就写未设置。
///
/// macOS 上的真值时区由脚本用 `date +%Z%z` 采（那份是观测值，这个只是进程视角）。
#[must_use]
pub fn tz() -> Option<String> {
    std::env::var("TZ")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

// ---------------------------------------------------------------- [i18n]

/// 语言决策链的四级（顺序即优先级：显式值 > 平台源 > 环境变量 > 英文）。
///
/// 每一级的**原始值**都留着，日志才能解释「为什么是英文」——只打结论的话，macOS 上「英文」
/// 到底是「用户就是英文」还是「一个源都没取到」根本分不出来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    /// `ULL_LANG` 的显式覆盖（截图矩阵的注入值）；`None` = 没设。
    ///
    /// 它与 [`Chain::setting`] 是**两个不同的源**：一个是我们为了出图钉的值，一个是用户配置里存的。
    /// 判定顺序里前者压后者（见 [`Chain::read`]），日志里分开打，读的人不用猜。
    pub forced: Option<String>,
    /// `ui.language` 的显式值（`None` = 文件里**没写过** = 跟随系统）。
    ///
    /// 与 `App` 启动判定同一个读法（`SettingsBridge::load_ui_language`：只认显式值，不走 schema
    /// 默认值），所以「没写过」在这里与 `ui::lang` 里是同一个意思。
    pub setting: Option<String>,
    /// (源自称的名字, 原始 locale)。
    pub platform: (&'static str, Option<String>),
    /// (源自称的名字, 原始 locale)。
    pub env: (&'static str, Option<String>),
    /// 只按**平台源 + 环境变量**算出来的「这台机器说什么语言」（**不看** `ui.language`）。
    ///
    /// `[env] language=` 用的是它：那一行回答的是「本机是什么」，用户手选英文不该改写机器的事实。
    pub machine: Detection,
    /// 最终结论：**与 `App` 启动日志的 `[i18n] choice=…` 用同一个判据函数**
    /// （`ui::lang::choose_choice` + `LangChoice::resolve`），所以两行不可能说的是两种语言。
    pub detection: Detection,
    /// 这一次是不是**截图通道**（D30）：`true` 时配置那一级按设计被忽略，日志里打出来，
    /// 否则「配置写了英文、结论是中文」看起来像 bug。
    pub screenshot: bool,
}

impl Chain {
    /// 用全部四个源探测一次。**每个源只读一次**（`defaults` / `reg.exe` 都是子进程）。
    ///
    /// 优先级**不在这里重写**：档位由 [`crate::ui::lang::choose_choice`] 出（与 `App` 启动判定
    /// 同一个函数），结论由 `LangChoice::resolve` 出（与 `App` 同一个 `resolve`）。
    /// 顺序即 **`ULL_LANG` > 配置（`screenshot == false` 才算） > 平台源 > 环境变量 > 英文**。
    #[must_use]
    pub fn read(
        forced: Option<&str>,
        setting: Option<&str>,
        screenshot: bool,
        platform: &dyn LocaleSource,
        env: &dyn LocaleSource,
    ) -> Self {
        let platform = Captured {
            label: platform.label(),
            raw: platform.locale(),
        };
        let env = Captured {
            label: env.label(),
            raw: env.locale(),
        };
        // 「机器说什么语言」：与 `ui.language` 无关，`[env] language=` 用它。
        let machine = lang::resolve(None, &platform, &env);
        // 「这一次界面用什么语言」：**调产品路径那个函数**，不在这里复制一遍优先级表。
        let detection =
            crate::ui::lang::choose_choice(forced, setting, screenshot).resolve(&platform, &env);
        Self {
            forced: forced.map(str::to_owned),
            setting: setting.map(str::to_owned),
            platform: (platform.label, platform.raw),
            env: (env.label, env.raw),
            machine,
            detection,
            screenshot,
        }
    }

    /// 本机链路（`ULL_LANG` + 配置 + 平台源 + 环境变量）。
    ///
    /// `screenshot` 由调用方从 `Options` 递进来（`report_boot` 手上有）：**截图通道按 D30 忽略配置**，
    /// 这一级判断必须与产品路径同源，否则又会出现「一行说 zh-CN、下一行说 effective=en」。
    ///
    /// 诚实边界：这里 `SettingsBridge::open()` 走**默认数据根**；同一次运行若带了 `--data-dir`，
    /// 配置可能来自另一份文件（此时以 `App` 那行 `[i18n] choice=…` 为准，它用的是 App 自己的桥）。
    #[must_use]
    pub fn host(screenshot: bool) -> Self {
        let forced = std::env::var(crate::ui::lang::LANG_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty());
        let setting = SettingsBridge::open().load_ui_language();
        Self::read(
            forced.as_deref(),
            setting.as_deref(),
            screenshot,
            platform_locale_source(),
            &lang::EnvLocale,
        )
    }

    /// **机器的**语言标签（`zh-CN` / `en`）——不含 `ui.language` 的手选。
    #[must_use]
    pub fn language(&self) -> &'static str {
        self.machine.lang.tag()
    }

    /// `[i18n]` 一行：四个源 + 结论。
    ///
    /// 顺序即 `ULL_LANG` > 配置（`shot=false` 才参与） > 平台源 > 环境变量 > 英文；`->` 后面那一段
    /// 就是**最终生效语言 + 它来自哪一级**，与 `App` 的 `[i18n] choice=…` **用同一个判据函数**算出来。
    ///
    /// 为什么把 `forced=` 与 `shot=` 也打出来：`shot=true` 时配置按 D30 被忽略，不写这一格的话
    /// 「配置里明明写了 en_us，结论却是 zh-CN」看起来像 bug（r4 复审 p2-1 的两组复现就是这两种）。
    ///
    /// 曾经这里钉着 `ui_text=pinned-en`（那会儿 `pages/mod.rs::render` 真的把下载页写死成英文）。
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "[i18n] forced={} setting={} platform={}:{} env={}:{} shot={} -> {}",
            self.forced.as_deref().unwrap_or("<未设置>"),
            self.setting.as_deref().unwrap_or("<未设置=跟随系统>"),
            self.platform.0,
            show(self.platform.1.as_deref()),
            self.env.0,
            show(self.env.1.as_deref()),
            self.screenshot,
            self.detection.describe()
        )
    }
}

fn show(value: Option<&str>) -> String {
    value.map_or_else(|| "<未设置>".to_owned(), |raw| raw.to_owned())
}

/// 把值钉住的源（为了「读一次、按真优先级算」）。
struct Captured {
    label: &'static str,
    raw: Option<String>,
}

impl LocaleSource for Captured {
    fn locale(&self) -> Option<String> {
        self.raw.clone()
    }

    fn label(&self) -> &'static str {
        self.label
    }
}

/// 本平台的「系统语言」真值源。
///
/// - Windows：`reg.exe` 读注册表（[`crate::platform::locale::SystemLocale`]）；
/// - macOS：[`AppleLanguages`]（`defaults read -g AppleLanguages`）；
/// - 其它：没有平台源（`resolve` 会继续往环境变量走）。
///
/// 为什么 macOS 不复用 `SystemLocale`：它 `cfg!(windows)` 之外恒返回 `None`，在 Mac 上等于
/// 「没有平台源」——于是语言只剩环境变量一级，而 GUI 程序从 Finder 启动时 `LANG` 通常是空的。
#[must_use]
pub fn platform_locale_source() -> &'static dyn LocaleSource {
    #[cfg(target_os = "macos")]
    {
        &AppleLanguages
    }
    #[cfg(not(target_os = "macos"))]
    {
        &crate::platform::locale::SystemLocale
    }
}

/// macOS 的系统语言源：`defaults read -g AppleLanguages` 的第一项。
#[derive(Debug, Clone, Copy, Default)]
pub struct AppleLanguages;

impl LocaleSource for AppleLanguages {
    fn locale(&self) -> Option<String> {
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("defaults")
                .args(["read", "-g", "AppleLanguages"])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            parse_apple_languages(&String::from_utf8_lossy(&output.stdout))
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }

    fn label(&self) -> &'static str {
        "applelanguages"
    }
}

/// 解析 `defaults` 的输出，取**第一条**语言（真机格式见测试夹具）。
///
/// ```text
/// (
///     "zh-Hans-CN",
///     "en-US"
/// )
/// ```
///
/// 首项是用户的**首选语言**；后面几项是「语言与地区」里的备选，与本 crate「只看主语言子标签」
/// 的口径无关。
#[must_use]
pub fn parse_apple_languages(text: &str) -> Option<String> {
    let start = text.find('"')?;
    let rest = &text[start + 1..];
    let end = rest.find('"')?;
    let value = rest[..end].trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
#[path = "locale_tests.rs"]
mod tests;
