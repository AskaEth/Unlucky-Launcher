//! 中文环境的 CJK 字体探测（D28 后半）：把 iced 系统字体库里的族名喂给 `unlucky_i18n::font::decide`。
//!
//! ## 分工
//!
//! - **判据**（要不要中文字体、哪些族算候选、缺了要不要下载）全在 `unlucky_i18n::font`，是纯函数，
//!   零 UI 依赖、脱机可测；
//! - **语言**从 [`unlucky_i18n::lang::resolve`] 拿：`ui.language` 显式值 > [`SystemLocale`]
//!   （Windows 读注册表，零 FFI）> 环境变量 > 英文；
//! - **本模块只做适配**：把 [`super::Face`] 列表变成一个「某族装没装」的查询，并处理
//!   **读不到字体库**这种情况。
//!
//! ## 为什么「读不到」不等于「需下载」
//!
//! `system_faces()` 拿不到字体库时返回 `None`。把它当成「没有中文字体」会直接触发一次 CDN 下载
//! （D28 的自动下载路径），而真相是**我们不知道**。所以这一路走 [`Strategy::Unprobeable`]：
//! 日志里留痕，不下载、不弹提示。
//!
//! ## 诚实边界（抓得住什么 / 抓不住什么）
//!
//! - **抓得住**：语言 + 字体库族名 → 策略（含 `Unprobeable` 这一支）；优先级顺序（注入的假源可测）；
//! - **抓不住**：①族名存在 ≠ 字形覆盖（`SimSun` 也可能是个残缺子集）。真判据要开 `ttf-parser`
//!   查 cmap，本 crate 不引依赖（M60 Q3 spike 做过）；
//!   ②`Strategy::NeedsDownload` 之后的下载与注册（M168 已实装，见 [`super::cdn`] /
//!   [`super::install`]：URL 与 sha256 钉在 `unlucky_i18n::font::NOTO_SANS_SC`，本模块只到判据为止）。
//!
//! ## `ui.language` 的接线（M106）
//!
//! 启动编排（`app/boot.rs`）那一拍只按系统 locale 判（`fonts::resolve` → `probe_with(None, ..)`）；真正的
//! `ui.language` 在 `App` 构造/切语言时读到，随后走 `fonts::cjk_for(setting)` 复算一次并打日志。
//! 所以「英文系统上手动切中文」也会得到 `needs-download` 而不是 `not-needed`——白块那条路被堵住了。

use unlucky_i18n::font::{self, Strategy};
use unlucky_i18n::lang::{self, Detection, EnvLocale};

use super::Face;
use crate::platform::locale::SystemLocale;

/// 一次中文探测的结论（启动日志的第二段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CjkReport {
    /// 语言探测（含原始 locale 与来源，便于解释「为什么是英文」）。
    pub detection: Detection,
    /// 字体策略结论。
    pub strategy: Strategy,
}

impl CjkReport {
    /// 一行日志片段（`lang=zh-CN locale=zh-CN source=registry cjk=system:"Microsoft YaHei"`）。
    pub fn describe(&self) -> String {
        format!("{} {}", self.detection.describe(), self.strategy.describe())
    }
}

/// 用给定语言 + 字体库内容做判定。
///
/// `faces` 为 `None` = 字体库读不出来（→ [`Strategy::Unprobeable`]，**不是**「需下载」）。
pub fn report(detection: Detection, faces: Option<&[Face]>) -> CjkReport {
    let strategy = faces.map_or(Strategy::Unprobeable, |faces| {
        let is_installed = |family: &str| faces.iter().any(|face| face.family == family);
        font::decide(detection.lang, &is_installed)
    });

    CjkReport {
        detection,
        strategy,
    }
}

/// 探测本机：优先级见 [`unlucky_i18n::lang::resolve`]。
///
/// `setting` 是配置里 `ui.language` 的原文（`zh_cn` / `en_us`）；`None` = **配置里没写过这个键**
/// （= 跟随系统，不是「没读」）。注意 `None` 与 `Some("不认识的值")` 效果相同：都不当显式选择，
/// 继续往下走。
pub fn probe_with(setting: Option<&str>, faces: Option<&[Face]>) -> CjkReport {
    report(lang::resolve(setting, &SystemLocale, &EnvLocale), faces)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn faces(families: &[&str]) -> Vec<Face> {
        families
            .iter()
            .map(|family| Face {
                family: (*family).to_owned(),
                weight: 400,
            })
            .collect()
    }

    fn zh() -> Detection {
        Detection {
            lang: unlucky_i18n::Lang::ZhCn,
            raw: Some("zh-CN".to_owned()),
            origin: "injected",
        }
    }

    fn en() -> Detection {
        Detection {
            lang: unlucky_i18n::Lang::En,
            raw: Some("en-US".to_owned()),
            origin: "injected",
        }
    }

    #[test]
    fn chinese_host_with_a_cjk_font_uses_the_system_one() {
        // 抓得住：适配层把字体库查反/查漏（例如只按第一个族判断、或把 `Segoe UI` 当候选）。
        // 抓不住：这台机器实测有没有这个族（测试用的是注入的假字面表）。
        let installed = faces(&["Segoe UI", "Microsoft YaHei", "Cascadia Mono"]);
        let report = report(zh(), Some(&installed));
        assert_eq!(
            report.strategy,
            Strategy::System {
                family: "Microsoft YaHei"
            }
        );
        assert_eq!(
            report.describe(),
            "lang=zh-CN locale=zh-CN source=injected cjk=system:\"Microsoft YaHei\""
        );
    }

    #[test]
    fn chinese_host_without_cjk_fonts_gets_the_download_signal() {
        // 抓得住：D28 的「中文环境扫不到 → 要下载」这条路（信号必须能被日志看出来）。
        // 抓不住：信号之后的下载与提示（本 mission 不联网，页面也没接）。
        let latin_only = faces(&["Segoe UI", "Roboto"]);
        let report = report(zh(), Some(&latin_only));
        assert_eq!(
            report.strategy,
            Strategy::NeedsDownload(&unlucky_i18n::font::NOTO_SANS_SC)
        );
        assert!(report
            .describe()
            .contains("cjk=needs-download(Noto Sans SC)"));
    }

    #[test]
    fn english_host_never_asks_for_a_chinese_font() {
        // 抓得住：英文环境的「零字体负担」（D28）——系统里有没有中文字体都不影响结论。
        // 抓不住：程序真的没去读那些字体文件（那是 iced/cosmic-text 的加载行为）。
        assert_eq!(report(en(), Some(&[])).strategy, Strategy::NotNeeded);
        assert_eq!(
            report(en(), Some(&faces(&["Microsoft YaHei"]))).strategy,
            Strategy::NotNeeded
        );
    }

    #[test]
    fn unreadable_font_database_is_unknown_not_missing() {
        // 抓得住：把「读不到字体库」错判成「缺字体」——那会在探测失败时触发一次多余的 CDN 下载。
        // 抓不住：真的读不到字体库时的运行行为（那条路径要 poisoned lock，测试造不出来）。
        let report = report(zh(), None);
        assert_eq!(report.strategy, Strategy::Unprobeable);
        assert!(
            report.describe().ends_with("cjk=unprobeable"),
            "{}",
            report.describe()
        );
    }

    #[test]
    fn explicit_setting_overrides_the_platform_source() {
        // 抓得住：`ui.language` 显式值被平台源盖掉（用户手选中文/英文必须赢过系统语言）；
        // 也让「平台源真的接在链路上」有个不依赖本机语言的证据。
        // 抓不住：`ui.language` 在真机上读到什么（那一级由 `App` 读配置后经 `fonts::cjk_for` 喂进来）。
        let installed = faces(&["Microsoft YaHei"]);

        let forced_english = probe_with(Some("en_us"), Some(&installed));
        assert_eq!(forced_english.detection.lang, unlucky_i18n::Lang::En);
        assert_eq!(
            forced_english.detection.origin,
            unlucky_i18n::lang::ORIGIN_SETTING
        );
        assert_eq!(forced_english.strategy, Strategy::NotNeeded);

        let forced_chinese = probe_with(Some("zh_cn"), Some(&installed));
        assert_eq!(
            forced_chinese.strategy,
            Strategy::System {
                family: "Microsoft YaHei"
            }
        );
        assert!(forced_chinese.describe().contains("source=setting"));
    }
}
