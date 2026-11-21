//! 设置页的**界面语言档位**（M106 `I18NFOLLOW`）：跟随系统 / 手选某一门。
//!
//! ## 为什么单独一个文件
//!
//! `lang.rs` 讲的是「系统说哪种语言」（locale → [`Lang`] 的探测，M92），本文件讲的是「用户要哪种
//! 语言」——两者的判据不一样：探测只能尽量理解别人给的字符串，档位是我们自己写进配置的，写错就该
//! 暴露。放在一起会让 `lang.rs` 顶破 400 行（任务书那条硬线要求按功能拆文件）。
//!
//! ## D28 的判定顺序（整条挂在 [`LangChoice::resolve`] 上）
//!
//! **手选 > 系统语言 > 环境变量 > 英文**。最后一级「英文」不是偷懒：纯英文环境不扫、不下载中文
//! 字体（零额外体积 / 零网络），这条由 [`crate::font::decide`] 兜住，真值表见下面
//! `english_never_needs_a_chinese_font_and_chinese_always_resolves_one`。
//!
//! ## 「跟随系统」怎么表达
//!
//! **不写 `ui.language` 这个键**（[`LangChoice::setting_value`] 返回 `None`）。不写哨兵值是因为
//! 两件事：① schema 的允许值只有 `zh_cn` / `en_us`（`core::setting::keys::LANGUAGES`），写别的
//! 会被枚举校验拒掉；② 读的那一头（`SettingBridge::load_ui_language`）只认显式值，所以「键不存在」
//! 与「用户选了跟随系统」本来就是同一件事。

use crate::catalog::Key;
use crate::lang::{resolve, Detection, Lang, LocaleSource, ORIGIN_SETTING};

/// 设置页「界面语言」的三档选择。
///
/// 为什么不是 `Option<Lang>`：`None` 与「用户明确勾了跟随系统」在类型上分不开，而设置页要把那一档
/// **画成选中**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LangChoice {
    /// 跟随系统（= 配置里没有 `ui.language`）。
    System,
    /// 用户手选的固定语言。
    Fixed(Lang),
}

impl LangChoice {
    /// 全部档位（顺序即设置页语言行的按钮顺序）。
    pub const ALL: &'static [LangChoice] = &[
        LangChoice::System,
        LangChoice::Fixed(Lang::ZhCn),
        LangChoice::Fixed(Lang::En),
    ];

    /// 配置里 `ui.language` 的原文 → 档位。
    ///
    /// `None`（键不存在）与**不认识的值**都算「跟随系统」——与 [`resolve`] 跳过坏值的行为同源：
    /// 配置被 AI / CLI 写脏时退回系统语言，而不是猜成某一种。
    #[must_use]
    pub fn from_setting(value: Option<&str>) -> LangChoice {
        value
            .and_then(Lang::from_setting)
            .map_or(LangChoice::System, LangChoice::Fixed)
    }

    /// 落盘用：跟随系统 = `None`（**删掉那个键**，不写哨兵值——哨兵会让「跟随系统」永远失效）。
    #[must_use]
    pub const fn setting_value(self) -> Option<&'static str> {
        match self {
            LangChoice::System => None,
            LangChoice::Fixed(lang) => Some(lang.setting_value()),
        }
    }

    /// 这一档在设置页上的名字（key 在 `catalog/general.rs` 域）。
    #[must_use]
    pub const fn label_key(self) -> Key {
        match self {
            LangChoice::System => Key::SettingsLanguageFollowSystem,
            LangChoice::Fixed(Lang::ZhCn) => Key::SettingsLanguageZhCn,
            LangChoice::Fixed(Lang::En) => Key::SettingsLanguageEn,
        }
    }

    /// 这一档最终生效的语言 + 来源（日志与设置页回显都用它）。
    ///
    /// 跟随系统 → 走 [`resolve`] 的三级链；手选 → 直接赢，`origin = setting`
    /// （`raw` 写配置值，日志里能看出「这是用户手选的」）。
    #[must_use]
    pub fn resolve(self, platform: &dyn LocaleSource, env: &dyn LocaleSource) -> Detection {
        match self {
            LangChoice::System => resolve(None, platform, env),
            LangChoice::Fixed(lang) => Detection {
                lang,
                raw: Some(lang.setting_value().to_owned()),
                origin: ORIGIN_SETTING,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::ORIGIN_DEFAULT;

    #[test]
    fn choice_round_trips_through_the_config_value() {
        // 抓得住：①「跟随系统」被写成哨兵值——那会让 `Lang::from_setting` 认不出、下次启动退回
        //        系统语言，「手选」这件事静默丢失；②档位顺序被调换（设置页按钮顺序）。
        // 抓不住：配置文件里到底有没有这个键（那是 `SettingsBridge` 的读写，见 presenter 侧测试）。
        assert_eq!(LangChoice::from_setting(None), LangChoice::System);
        assert_eq!(
            LangChoice::from_setting(Some("zh_cn")),
            LangChoice::Fixed(Lang::ZhCn)
        );
        assert_eq!(
            LangChoice::from_setting(Some("en_us")),
            LangChoice::Fixed(Lang::En)
        );
        for bogus in ["zh-CN", "system", "", "fr_fr"] {
            assert_eq!(
                LangChoice::from_setting(Some(bogus)),
                LangChoice::System,
                "{bogus}"
            );
        }

        assert_eq!(LangChoice::System.setting_value(), None, "跟随系统不写键");
        for choice in LangChoice::ALL {
            assert_eq!(
                LangChoice::from_setting(choice.setting_value()),
                *choice,
                "{} 落盘后读不回来",
                choice.setting_value().unwrap_or("<跟随系统>")
            );
        }
        assert_eq!(
            LangChoice::ALL,
            &[
                LangChoice::System,
                LangChoice::Fixed(Lang::ZhCn),
                LangChoice::Fixed(Lang::En)
            ][..]
        );
    }

    #[test]
    fn choice_resolution_truth_table() {
        // D28 的四级优先级，**逐档逐源**列出来：手选 > 系统 > 环境变量 > 英文。
        // 抓得住：优先级顺序被改（例如手选的英文被中文系统盖掉）、或「跟随系统」直接落英文。
        // 抓不住：这些源在真机上读到什么（`SystemLocale` 的真机行为在 `app-iced` 侧）。
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
        let registry_en = Named("registry", Some("en-US"));
        let nothing = Named("registry", None);
        let env_zh = Named("env", Some("zh_CN.UTF-8"));
        let env_en = Named("env", Some("en_US.UTF-8"));
        let env_none = Named("env", None);

        let follow = LangChoice::System;
        let en_choice = LangChoice::Fixed(Lang::En);
        let zh = LangChoice::Fixed(Lang::ZhCn);

        // (档位, 平台源, 环境变量源) -> (生效语言, 来源)
        let table: &[(LangChoice, &dyn LocaleSource, &dyn LocaleSource, Lang, &str)] = &[
            // ① 手选压过一切（系统是中文、环境是中文，手选英文就画英文）
            (en_choice, &registry_zh, &env_zh, Lang::En, ORIGIN_SETTING),
            (zh, &registry_en, &env_en, Lang::ZhCn, ORIGIN_SETTING),
            // ② 跟随系统 → 系统源
            (follow, &registry_zh, &env_en, Lang::ZhCn, "registry"),
            (follow, &registry_en, &env_zh, Lang::En, "registry"),
            // ③ 系统源取不到 → 环境变量
            (follow, &nothing, &env_zh, Lang::ZhCn, "env"),
            (follow, &nothing, &env_en, Lang::En, "env"),
            // ④ 都没有 → 英文 + default（D28：纯英文环境不用管）
            (follow, &nothing, &env_none, Lang::En, ORIGIN_DEFAULT),
        ];

        for (choice, platform, env, lang, origin) in table {
            let detection = choice.resolve(*platform, *env);
            assert_eq!(
                (detection.lang, detection.origin),
                (*lang, *origin),
                "{choice:?} platform={:?} env={:?}",
                platform.locale(),
                env.locale()
            );
        }

        // 手选档的 `raw` 是配置值（日志一眼看出「用户手选的」），跟随系统档的 `raw` 是系统原文。
        assert_eq!(
            zh.resolve(&registry_en, &env_en).describe(),
            "lang=zh-CN locale=zh_cn source=setting"
        );
        assert_eq!(
            follow.resolve(&registry_zh, &env_en).describe(),
            "lang=zh-CN locale=zh-CN source=registry"
        );
    }

    #[test]
    fn english_never_needs_a_chinese_font_and_chinese_always_resolves_one() {
        // D28 字体那一级的真值表（`font::decide` 是纯函数，这里把语言 × 系统字体库全组合扫一遍）：
        // 抓得住：①英文环境被接上中文字体/下载（「零额外体积/零网络」的直接违约）；②中文环境扫不到
        //        字体却不给下载信号（会白块）；③扫到了却不用系统字体（会白下 20 MB）。
        // 抓不住：本机到底装没装这些族（`app-iced` 的 `fonts/cjk.rs` 把真字体库喂进来）。
        use crate::font::{self, Strategy, CJK_FAMILIES, NOTO_SANS_SC};

        let has_font =
            |families: &'static [&'static str]| move |family: &str| families.contains(&family);

        for installed in [
            has_font(&[]),
            has_font(&["Segoe UI"]),
            has_font(CJK_FAMILIES),
        ] {
            assert_eq!(
                font::decide(Lang::En, &installed),
                Strategy::NotNeeded,
                "英文环境不许要中文字体"
            );
        }

        assert_eq!(
            font::decide(Lang::ZhCn, &has_font(&["Segoe UI"])),
            Strategy::NeedsDownload(&NOTO_SANS_SC)
        );
        let first = CJK_FAMILIES[0];
        let installed = has_font(&["Segoe UI", "Microsoft YaHei UI"]);
        assert_eq!(first, "Microsoft YaHei UI");
        assert_eq!(
            font::decide(Lang::ZhCn, &installed),
            Strategy::System { family: first }
        );
    }

    #[test]
    fn every_choice_has_a_label_in_both_catalogs() {
        // 抓得住：设置页语言行接了一个 catalog 里不存在的 key（画出来是 `⟪missing:…⟫`）。
        // 抓不住：译文翻得对不对（金表 + `chinese_catalog_is_actually_chinese` 管那一层）。
        let mut keys = Vec::new();
        for choice in LangChoice::ALL {
            let key = choice.label_key();
            for lang in Lang::ALL {
                assert!(
                    lang.catalog().get(key).is_some(),
                    "{key:?} 在 {} 目录里缺词",
                    lang.tag()
                );
            }
            keys.push(key);
        }
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), LangChoice::ALL.len(), "两档用了同一个 key");
    }
}
