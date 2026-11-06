//! 语言与系统 locale 探测（D28 前半）。
//!
//! ## 判据只有两条
//!
//! 1. **中文环境**（`zh*` 任意变体）→ [`Lang::ZhCn`]；
//! 2. **其它一切**（`en*`、`ja*`、`fr*`、`C`、`POSIX`、变量缺失、解析不了）→ [`Lang::En`]。
//!
//! 「其它一切 → 英文」不是偷懒：D28 的原话是「纯英文环境不用管」，而把 ja/fr/de 猜成中文才是真错
//! （会连带触发中文字体下载）。宁可给一个英文界面，也不给一个语种错误的中文界面。
//!
//! ## 为什么 locale 源要能注入
//!
//! [`detect`] 的形状是 `detect(&dyn LocaleSource)`，[`EnvLocale`] 只是其中一个实现。这样
//! **单测不依赖跑测试那台机器的 locale**（真值表测试在 [`lang`] 自己的 `tests` 里跑固定输入），
//! 后续 mission 加「Win32 `GetUserDefaultLocaleName`」或「读配置里的 `ui.language`」也只是再实现一遍
//! 这个 trait。
//!
//! ## 优先级（[`resolve`]）
//!
//! **`ui.language` 显式值 > 平台源 > POSIX 环境变量 > 英文**。本 crate 只有 [`EnvLocale`]
//! 一个具体实现——平台真值源（Windows 注册表那一路）在 UI 壳层，因为读注册表要开子进程，
//! 而本 crate 的纪律是不碰外部世界。
//!
//! ## 诚实边界（抓得住什么 / 抓不住什么）
//!
//! - **抓得住**：环境变量字符串 → 语言的映射（含 `zh_CN.UTF-8` / `zh-Hans-CN` / 大小写 / 修饰符）；
//!   变量优先级（`LC_ALL` > `LC_MESSAGES` > `LANG`）；[`resolve`] 的四级优先级；
//! - **抓不住**：**Windows 上这三个变量通常都不存在**——那条路的真值源是
//!   `crates/app-iced/src/platform/locale.rs`（`reg.exe` 读注册表，零 FFI），
//!   它不在本 crate 里（本 crate 零依赖、不 spawn 进程）。所以「Windows 上拿不到语言」
//!   这件事只在**没有接线平台源**时成立，接线后由 `resolve` 的二级兜住。
//!   配置里的 `ui.language` 读取属于设置层接线（本 crate 只提供 [`Lang::from_setting`] 与
//!   [`resolve`] 的形状）。

use std::env;

use crate::catalog::Catalog;

/// 界面语言。目前只有中英两套（D28 的「至少 zh-CN / en-US」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Lang {
    /// 英文（默认；未知 locale 也落在这里）。
    En,
    /// 简体中文。
    ZhCn,
}

impl Lang {
    /// 全部语言（顺序即设置页下拉框的顺序）。
    pub const ALL: &'static [Lang] = &[Lang::En, Lang::ZhCn];

    /// BCP-47 标签（日志 / 字体请求里用；`zh-CN` 是本 crate 唯一的中文标识）。
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::ZhCn => "zh-CN",
        }
    }

    /// 配置值（`core::setting::keys::LANGUAGES` 的同一份词表）。
    #[must_use]
    pub const fn setting_value(self) -> &'static str {
        match self {
            Lang::En => "en_us",
            Lang::ZhCn => "zh_cn",
        }
    }

    /// 解析**配置值**（严格）：只认 [`Lang::setting_value`] 那两个字符串。
    ///
    /// 与宽松的 [`Lang::from_locale`] 分工明确：配置是我们自己写进去的，写错就该暴露（`None` 让设置层
    /// 报错），而不是悄悄换成别的语言；系统 locale 是别人给的，只能尽量理解。
    #[must_use]
    pub fn from_setting(value: &str) -> Option<Lang> {
        Lang::ALL
            .iter()
            .copied()
            .find(|lang| lang.setting_value() == value)
    }

    /// 解析 **locale / 语言标签**（宽松）：`zh`、`zh_CN.UTF-8`、`zh-Hans-CN`、`ZH_cn` 都是中文。
    ///
    /// 只看**主语言子标签**：`zh-TW` / `zh-Hant` 也归 [`Lang::ZhCn`]——仓内只有一套中文目录，
    /// 繁体用户看简体比看英文更有用；真要繁体得加第三套目录（见 `bridge` 的上游约定）。
    #[must_use]
    pub fn from_locale(tag: &str) -> Lang {
        let trimmed = tag.trim();
        // `zh_CN.UTF-8@euro`：codeset 与修饰符与语种无关，先切掉。
        let without_codeset = trimmed.split('.').next().unwrap_or("");
        let without_modifier = without_codeset.split('@').next().unwrap_or("");
        let primary = without_modifier
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();

        if primary == "zh" {
            Lang::ZhCn
        } else {
            Lang::En
        }
    }

    /// 本语言的消息目录。
    #[must_use]
    pub const fn catalog(self) -> Catalog {
        Catalog::of(self)
    }
}

/// 没有任何 locale 信息时的落点（D28：纯英文环境不用管）。
impl Default for Lang {
    fn default() -> Self {
        Lang::En
    }
}

/// locale 的来源。实现方负责回答「这台机器说哪种语言」。
pub trait LocaleSource {
    /// 原始 locale 标签（`None` = 取不到，不是「英文」的断言）。
    fn locale(&self) -> Option<String>;

    /// 这个源在启动日志里的名字（`registry` / `env` / …）。默认 `injected`（测试用）。
    fn label(&self) -> &'static str {
        "injected"
    }
}

/// [`Detection::origin`]：结论来自 `ui.language` 的显式值。
pub const ORIGIN_SETTING: &str = "setting";
/// [`Detection::origin`]：结论来自环境变量（[`EnvLocale`]）。
pub const ORIGIN_ENV: &str = "env";
/// [`Detection::origin`]：一个源都没取到，落到默认英文。
pub const ORIGIN_DEFAULT: &str = "default";

/// 进程环境变量式 locale 源（POSIX 语义；Windows 上基本取不到，见模块文档）。
#[derive(Debug, Clone, Copy, Default)]
pub struct EnvLocale;

impl LocaleSource for EnvLocale {
    fn locale(&self) -> Option<String> {
        // POSIX 优先级：LC_ALL > LC_MESSAGES > LANG。空串按「没设置」处理。
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|name| env::var(name).ok().filter(|value| !value.trim().is_empty()))
    }

    fn label(&self) -> &'static str {
        ORIGIN_ENV
    }
}

/// 探测结论（`raw` 与 `origin` 一起带出来，是为了日志能解释「为什么是英文」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    /// 结论。
    pub lang: Lang,
    /// 原始 locale 标签（`None` = 一个源都没取到）。
    pub raw: Option<String>,
    /// 结论的来源（`setting` / 平台源自己的 [`LocaleSource::label`] / `env` / `default`）。
    pub origin: &'static str,
}

impl Detection {
    /// 一行日志（`lang=zh-CN locale=zh-CN source=registry` / `lang=en locale=<未设置> source=default`）。
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "lang={} locale={} source={}",
            self.lang.tag(),
            self.raw.as_deref().unwrap_or("<未设置>"),
            self.origin
        )
    }
}

/// 用给定来源探测语言。
#[must_use]
pub fn detect(source: &dyn LocaleSource) -> Detection {
    match source.locale() {
        Some(raw) => Detection {
            lang: Lang::from_locale(&raw),
            raw: Some(raw),
            origin: source.label(),
        },
        None => Detection {
            lang: Lang::En,
            raw: None,
            origin: ORIGIN_DEFAULT,
        },
    }
}

/// 用进程环境变量探测语言。
#[must_use]
pub fn detect_env() -> Detection {
    detect(&EnvLocale)
}

/// 全量优先级：**`ui.language` 显式值 > 平台源 > POSIX 环境变量 > 英文**。
///
/// - `setting`：配置里 `ui.language` 的原文（`zh_cn` / `en_us`）。给了但不认识 → **跳过**
///   （当作「没配」继续往下走，而不是把它猜成某种语言）；
/// - `platform`：平台真值源（Windows 上是 `app-iced::platform::locale` 读注册表；其它平台没有）；
/// - `env`：`LC_ALL > LC_MESSAGES > LANG`（[`EnvLocale`]）；
/// - 都不给 → [`Lang::En`]，`origin = default`。
///
/// 这正是 D28 要的顺序：**用户手选的**压过系统，系统压过环境变量，都没有就英文。
#[must_use]
pub fn resolve(
    setting: Option<&str>,
    platform: &dyn LocaleSource,
    env: &dyn LocaleSource,
) -> Detection {
    if let Some(value) = setting {
        if let Some(lang) = Lang::from_setting(value) {
            return Detection {
                lang,
                raw: Some(value.to_owned()),
                origin: ORIGIN_SETTING,
            };
        }
    }

    let from_platform = detect(platform);
    if from_platform.raw.is_some() {
        return from_platform;
    }

    detect(env)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(Option<&'static str>);

    impl LocaleSource for Fixed {
        fn locale(&self) -> Option<String> {
            self.0.map(str::to_owned)
        }
    }

    #[test]
    fn locale_truth_table() {
        // 抓得住：主语言子标签的解析（分隔符 / 大小写 / codeset / 修饰符 / 变体）。
        // 抓不住：这些字符串是否真的是这台机器的 locale——真值来自 LocaleSource 实现。
        let zh = [
            "zh",
            "zh_CN",
            "zh_CN.UTF-8",
            "zh-CN",
            "zh-Hans-CN",
            "zh_hans_cn",
            "ZH_cn",
            "  zh_TW  ",
            "zh_CN.GB18030@euro",
        ];
        for tag in zh {
            assert_eq!(Lang::from_locale(tag), Lang::ZhCn, "{tag}");
        }

        let en = [
            "",
            "C",
            "POSIX",
            "en",
            "en_US.UTF-8",
            "en-GB",
            "ja_JP.UTF-8",
            "fr_FR@euro",
            "de",
            // 不是中文但含 zh 之外的字：必须落英文，不许猜成中文
            "zhuang",
        ];
        for tag in en {
            assert_eq!(Lang::from_locale(tag), Lang::En, "{tag}");
        }
    }

    #[test]
    fn detect_falls_back_to_english_without_a_locale() {
        // 抓得住：取不到 locale 时**不 panic、不猜中文**（D28 的「纯英文环境不用管」）。
        // 抓不住：Windows 上「环境变量取不到」是否等于用户真的说英文——那是平台源的活
        // （`app-iced::platform::locale`），见下面 resolve 的优先级测试。
        let detection = detect(&Fixed(None));
        assert_eq!(detection.lang, Lang::En);
        assert_eq!(detection.raw, None);
        assert_eq!(detection.origin, ORIGIN_DEFAULT);
        assert_eq!(
            detection.describe(),
            "lang=en locale=<未设置> source=default"
        );
        assert_eq!(Lang::default(), Lang::En);
    }

    #[test]
    fn detect_uses_the_injected_source_verbatim() {
        // 抓得住：探测结论确实来自注入源（不会被本机环境变量污染），且 origin 取自源自报的名字。
        // 抓不住：EnvLocale 到底读了哪几个变量（那条路径只能靠人工/集成测试看日志）。
        let detection = detect(&Fixed(Some("zh_CN.UTF-8")));
        assert_eq!(detection.lang, Lang::ZhCn);
        assert_eq!(detection.raw.as_deref(), Some("zh_CN.UTF-8"));
        assert_eq!(detection.origin, "injected");
        assert_eq!(
            detection.describe(),
            "lang=zh-CN locale=zh_CN.UTF-8 source=injected"
        );
        assert_eq!(EnvLocale.label(), "env");
    }

    #[test]
    fn resolve_priority_is_setting_then_platform_then_env_then_english() {
        // 抓得住：四级优先级的顺序本身（用户手选压过系统，系统压过环境变量，都没有才英文）；
        //         以及「配置值不认识时不当成任何一种语言，继续往下走」。
        // 抓不住：这些源在真机上到底读到什么（平台源的真机行为在 app-iced 侧测）。
        struct Named(&'static str, Option<&'static str>);

        impl LocaleSource for Named {
            fn locale(&self) -> Option<String> {
                self.1.map(str::to_owned)
            }
            fn label(&self) -> &'static str {
                self.0
            }
        }

        let registry_zh = Named("registry", Some("zh-CN"));
        let env_en = Named("env", Some("en_US.UTF-8"));
        let nothing = Named("registry", None);

        // ① 配置显式值压过一切
        let detection = resolve(Some("en_us"), &registry_zh, &env_en);
        assert_eq!(
            (detection.lang, detection.origin),
            (Lang::En, ORIGIN_SETTING)
        );
        // ② 没配置（或配置不认识）→ 平台源
        for setting in [None, Some("zh-CN"), Some("")] {
            let detection = resolve(setting, &registry_zh, &env_en);
            assert_eq!(
                (detection.lang, detection.raw.as_deref(), detection.origin),
                (Lang::ZhCn, Some("zh-CN"), "registry"),
                "setting={setting:?}"
            );
        }
        // ③ 平台源没有 → 环境变量
        let detection = resolve(None, &nothing, &env_en);
        assert_eq!(
            (detection.lang, detection.origin),
            (Lang::En, "env"),
            "环境变量说是英文，但来源必须是 env"
        );
        // ④ 全没有 → 英文 + default
        let detection = resolve(None, &nothing, &Named("env", None));
        assert_eq!(
            (detection.lang, detection.origin),
            (Lang::En, ORIGIN_DEFAULT)
        );
    }

    #[test]
    fn setting_values_match_the_config_vocabulary() {
        // 抓得住：本 crate 的配置词表被改动（改名会让已落盘的 ui.language 读不出来）。
        // 抓不住：`core::setting::keys::LANGUAGES` 那边先改了——本 crate 不依赖 core（零依赖纪律），
        // 所以这条只能靠接线 mission 写一条跨 crate 的 pin 测试，或让 core 反向引用这份词表。
        let values = Lang::ALL
            .iter()
            .map(|lang| lang.setting_value())
            .collect::<Vec<_>>();
        assert_eq!(values, vec!["en_us", "zh_cn"]);
        assert_eq!(Lang::from_setting("zh_cn"), Some(Lang::ZhCn));
        assert_eq!(Lang::from_setting("en_us"), Some(Lang::En));
    }

    #[test]
    fn setting_parsing_is_strict_where_locale_parsing_is_lenient() {
        // 抓得住：宽松解析被误用到配置值上（`zh-CN` 是合法 locale 但不是合法配置值）。
        // 抓不住：有人把非法值直接塞进配置（那是设置层的 schema 校验与该键的 enumeration 约束）。
        for bogus in ["zh-CN", "ZH_CN", "zh", "", "en"] {
            assert_eq!(Lang::from_setting(bogus), None, "{bogus}");
        }
        assert_eq!(Lang::from_locale("zh-CN"), Lang::ZhCn);
    }
}
