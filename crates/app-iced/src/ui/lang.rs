//! 界面语言的**入口层**（M106 `I18NFOLLOW`）：`Ui` 缺的那条语言入口就是这里。
//!
//! ## 它解决的是什么
//!
//! M92 把探测链做对了（启动日志里 `lang=zh-CN locale=zh-CN source=registry` 一直是对的），但
//! **没人消费它**：`app/view.rs` 的 `model()` 里 `Lang::En` 是写死的，于是中文系统上开出来的是
//! 全英文界面，设置页还写着 `English (System)`。本模块把「用户档位 + 探测结论」变成一个**值**
//! （[`LangState`]），`App` 持有它、`view::model` 只认它，页面侧 `pages::Model.lang` 决定目录。
//!
//! ## 判定的四级优先级（[`LangState::boot`]）
//!
//! | 级 | 来源 | 什么时候用 |
//! |---|---|---|
//! | 1 | `ULL_LANG`（`zh_cn` / `en_us`） | 截图矩阵要**可复现地**换语言（D30） |
//! | 2 | 配置里的 `ui.language` | 用户手选（设置页写入） |
//! | 3 | 系统 locale（注册表 / 环境变量） | 跟随系统 |
//! | 4 | 英文 | 一个源都没有 |
//!
//! **截图通道不读用户配置**（与 `appearance_prefs` 同一条 D30 纪律）：同一份代码在两台机器上出
//! 不了同一张图，除非语言也由 `ULL_LANG` 显式给。
//!
//! ## 日志（两个下一个人要问的问题）
//!
//! 「探测错了吗」与「探测对了但没用上吗」在日志里必须分得开：
//!
//! ```text
//! [i18n] choice=system lang=zh-CN locale=zh-CN source=registry effective=zh-CN
//! [i18n] lang=zh-CN locale=zh-CN source=registry cjk=system:"Microsoft YaHei UI"
//! ```
//!
//! 第一行是**档位 + 探测结论 + 最终生效语言**（[`LangState::log`]），第二行是中文策略（D28：
//! 缺字体是信号，不许静默；[`LangState::log_cjk`]）。
//!
//! ## 诚实边界
//!
//! - **抓得住**：档位的选择顺序、坏值不猜、截图通道忽略配置、切档后生效语言跟着换；
//! - **抓不住**：系统 locale 的真机行为（那在 [`crate::platform::locale::SystemLocale`]，Windows 上
//!   真的去跑 `reg.exe`）；中文字体到底装没装（`fonts::cjk_for` 查 iced 的字体库）。

use unlucky_i18n::lang::{Detection, EnvLocale};
use unlucky_i18n::{Lang, LangChoice};
use unlucky_presenter::settings_bridge::SettingsBridge;

use crate::pages;
use crate::platform::locale::SystemLocale;

/// 截图通道的语言覆盖（D30 可复现）：`ULL_LANG=zh_cn` / `ULL_LANG=en_us`。
///
/// 为什么走环境变量而不是 `--lang`：`cli.rs` 不在本 mission 的 scope 里，而截图矩阵要能**只**改
/// 一个环境变量就换语言（与 `ULL_APPEARANCE_*` 那批同一条做法）。
pub(crate) const LANG_ENV: &str = "ULL_LANG";

/// 设置页左侧分组的取证开关：`ULL_SETTINGS_GROUP=0` 直达 General（语言行在那里）。
///
/// 只影响 `--screenshot` 那一次进程（设置页出厂停在「外观」，而用户抱怨的语言行在 General 组；
/// 出图必须能直达）。交互模式一律按用户点的那一组，所以这里拿 `screenshot` 当闸门。
/// 为什么住在 `ui/lang.rs`：它与 [`LANG_ENV`] 是同一族「出图取证开关」，一起放在语言入口层，
/// `App` 那边只留一行调用（`app/state.rs` 贴着 400 行硬线）。
pub(crate) fn screenshot_settings_group(screenshot: bool) -> Option<usize> {
    if !screenshot {
        return None;
    }
    std::env::var("ULL_SETTINGS_GROUP")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
}

/// 这一次进程的界面语言：用户选的档 + 探测出来的结论。
///
/// 不是 `Copy`：结论里带着探测到的原始 locale 字符串（`Detection.raw`），而它只在切档/启动时
/// 构造一次——每帧问的是 [`LangState::lang`]（一个 `Copy` 的枚举），不是整份状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LangState {
    /// 用户意图（决定设置页画哪个按钮、落盘写不写 `ui.language`）。
    choice: LangChoice,
    /// 生效结论（决定用哪套目录；`describe()` 就是日志那一段）。
    detection: Detection,
}

impl LangState {
    /// 启动判定：`ULL_LANG` > 配置 `ui.language` > 系统 locale > 英文。
    pub(crate) fn boot(bridge: &SettingsBridge, screenshot: bool) -> Self {
        let forced = std::env::var(LANG_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty());
        let setting = bridge.load_ui_language();
        Self::of(choose_choice(
            forced.as_deref(),
            setting.as_deref(),
            screenshot,
        ))
    }

    /// 换了档位之后的状态（重新走一遍 `resolve`，不保留旧结论）。
    pub(crate) fn of(choice: LangChoice) -> Self {
        Self {
            choice,
            detection: choice.resolve(&SystemLocale, &EnvLocale),
        }
    }

    /// 当前档位。
    pub(crate) fn choice(&self) -> LangChoice {
        self.choice
    }

    /// 当前生效的语言（目录选择）。
    pub(crate) fn lang(&self) -> Lang {
        self.detection.lang
    }

    /// 设置页语言行要的两件（档位 + 生效语言）。
    pub(crate) fn for_settings(&self) -> pages::Language {
        pages::Language {
            choice: self.choice,
            effective: self.lang(),
        }
    }

    /// 主日志行：档位 + 探测结论 + 最终生效语言。
    pub(crate) fn log(&self) {
        eprintln!(
            "[i18n] choice={} {} effective={}",
            self.choice.setting_value().unwrap_or("system"),
            self.detection.describe(),
            self.lang().tag()
        );
    }

    /// 中文策略日志（D28）：缺字体是**信号**，不许静默。
    ///
    /// 为什么要单独一次（启动编排里已经打过一行 `[font]`）：那一拍按**系统 locale** 判，不知道用户
    /// 手选了什么。英文系统上手动切中文时，只有这一行会变成 `cjk=needs-download(...)`——白块那条路
    /// 就是靠它被看见的。
    pub(crate) fn log_cjk(&self) {
        let report = crate::fonts::cjk_for(self.choice.setting_value());
        eprintln!("[i18n] {}", report.describe());
        if matches!(
            report.strategy,
            unlucky_i18n::font::Strategy::NeedsDownload(_)
        ) {
            // M168：这里**不再**说「通道还没做」——下载在这一拍之后就已经发起（同一份报告交给
            // `fonts::install::after_probe`），结果与状态在 `[font] cjk-download …` 那几行里，
            // 也在设置页 General 组的状态行上。上一版这句 stderr 在下载发起后说「还是 PendingPin」，
            // 等于当面否定刚做的事（复审 r1 P2-3）。
            eprintln!(
                "[i18n] 缺中文字体：已按 D28 发起下载，进度与结果见 [font] cjk-download … 与设置页「中文字体」那一行；失败会降级用系统字体"
            );
        }
    }
}

/// 档位选择的**纯判据**（单测直接打这张表）。
///
/// **产品路径与验收路径共用它**：`LangState::boot` 用它决定目录，`renderer::acceptance::locale`
/// 的 `[i18n]` 行也用它算结论——两行日志因此不可能再说出不同的语言（r4 复审 p2-1 就是
/// 「两处各算一遍」造成的）。
///
/// | 输入 | 结果 |
/// |---|---|
/// | `ULL_LANG` 给了合法值 | 那一档（压过配置） |
/// | 没给 + 配置写了 `zh_cn` / `en_us` | 那一档 |
/// | 没给 + 配置没写（或写了坏值） | 跟随系统 |
/// | **截图通道** + 配置写了值 | 跟随系统（除非 `ULL_LANG` 另给） |
pub(crate) fn choose_choice(
    forced: Option<&str>,
    setting: Option<&str>,
    screenshot: bool,
) -> LangChoice {
    let configured = if screenshot { None } else { setting };
    LangChoice::from_setting(forced.or(configured))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 档位判据的真值表（D28 第一级 + D30 的截图可复现）。
    ///
    /// 抓得住：①`ULL_LANG` 没压过配置（截图矩阵就换不了语言）；②截图通道读了用户配置（同一份代码
    /// 在两台机器上出不同的图）；③坏值被猜成某种语言。
    /// 抓不住：系统 locale 真探测成什么（那是 `SystemLocale` + `i18n::lang::resolve` 的事）。
    #[test]
    fn the_language_choice_is_ull_lang_then_config_then_system() {
        let en = Some("en_us");
        let zh = Some("zh_cn");

        assert_eq!(choose_choice(None, None, false), LangChoice::System);
        assert_eq!(
            choose_choice(None, zh, false),
            LangChoice::Fixed(Lang::ZhCn),
            "配置里的手选要生效"
        );
        assert_eq!(
            choose_choice(en, zh, false),
            LangChoice::Fixed(Lang::En),
            "ULL_LANG 压过配置（截图矩阵靠它换语言）"
        );
        assert_eq!(
            choose_choice(zh, None, false),
            LangChoice::Fixed(Lang::ZhCn)
        );

        assert_eq!(
            choose_choice(None, zh, true),
            LangChoice::System,
            "截图通道必须忽略配置文件里的语言，否则同一份代码出不了同一张图"
        );
        assert_eq!(choose_choice(en, zh, true), LangChoice::Fixed(Lang::En));

        for bogus in ["zh-CN", "system", "", "fr_fr"] {
            assert_eq!(
                choose_choice(Some(bogus), None, false),
                LangChoice::System,
                "{bogus}"
            );
        }
    }

    /// 「重启后保持所选语言」的**闭环证据**：真的落一个临时 `launcher.json`，再用**新开的** bridge
    /// 读回来（等价于重启）。
    ///
    /// 抓得住：①「跟随系统」被读成 schema 默认值 `zh_cn`（英文机器一启动就是中文）；
    /// ②「跟随系统」写了哨兵值（下次启动撒谎）；③删键时把别的键一起删了。
    /// 抓不住：数据根解析（`SettingsBridge::open` 那条路）——这里用的是显式路径。
    #[test]
    fn the_choice_round_trips_through_the_config_file() {
        use unlucky_presenter::settings_bridge::SettingsBridge;

        let dir = std::env::temp_dir().join(format!("ull-m106-lang-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("config")).expect("临时目录");
        let path = dir.join("config/launcher.json");
        std::fs::write(
            &path,
            r#"{"appearance":{"theme_mode":"dark"},"custom":{"keep":true}}"#,
        )
        .expect("写配置");

        // ① 没写过 = 跟随系统（不是默认值 zh_cn）
        let bridge = SettingsBridge::with_path(&path);
        assert_eq!(
            bridge.load_ui_language(),
            None,
            "没写过这个键 = 跟随系统，不许回落成 schema 默认值"
        );

        // ② 手选落盘 → 新开的 bridge（= 重启）读回来
        let mut bridge = SettingsBridge::with_path(&path);
        assert!(bridge.persist_ui_language(Some("zh_cn")));
        assert_eq!(
            SettingsBridge::with_path(&path)
                .load_ui_language()
                .as_deref(),
            Some("zh_cn"),
            "重启后要保持所选语言"
        );

        // ③ 切回跟随系统 = 删键，且不许顺手删别的键
        let mut bridge = SettingsBridge::with_path(&path);
        assert!(bridge.persist_ui_language(None));
        assert_eq!(SettingsBridge::with_path(&path).load_ui_language(), None);
        let text = std::fs::read_to_string(&path).expect("读配置");
        assert!(text.contains("theme_mode"), "外观键被删了：{text}");
        assert!(text.contains("custom"), "用户手写的未知键被删了：{text}");

        // ④ 已经是跟随系统时再点一次是幂等的
        let mut bridge = SettingsBridge::with_path(&path);
        assert!(!bridge.persist_ui_language(None), "没有键可删 = 没有落盘");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 手选档会赢过系统源，且 `raw`/`origin` 看得出来是手选的（日志可分诊）。
    ///
    /// 抓得住：`of()` 忘了重算（切档后结论还停在旧语言）、手选被系统源盖掉。
    /// 抓不住：真机上系统源读到什么（`SystemLocale` 跑 `reg.exe`）。
    #[test]
    fn a_fixed_choice_wins_over_the_system_locale() {
        let zh = LangState::of(LangChoice::Fixed(Lang::ZhCn));
        assert_eq!(zh.lang(), Lang::ZhCn);
        assert_eq!(zh.for_settings().effective, Lang::ZhCn);
        assert_eq!(zh.for_settings().choice, LangChoice::Fixed(Lang::ZhCn));

        let en = LangState::of(LangChoice::Fixed(Lang::En));
        assert_eq!(en.lang(), Lang::En, "手选英文压过任何中文系统源");
        assert_eq!(en.detection.origin, "setting");
    }
}
