//! `[env]` / `[i18n]` 的钉死测试（从 `tests.rs` 拆出来：那边撞到 400 行上限）。
//!
//! 这两类都靠「把真实输入喂进纯函数再逐字断言」，抓得住的是本项目的头号事故类型：
//! **在需要结果的地方打印请求或常量**。

use super::*;
use crate::probe::AdapterSummary;
use crate::Probe;
use unlucky_i18n::lang::LocaleSource;

fn probe_of(adapters: &[(&str, &'static str)]) -> Probe {
    let adapters: Vec<AdapterSummary> = adapters
        .iter()
        .map(|(name, device_type)| AdapterSummary {
            name: (*name).to_owned(),
            device_type,
        })
        .collect();
    let hardware = adapters
        .iter()
        .any(|adapter| !matches!(adapter.device_type, "Cpu" | "Other"));
    Probe { adapters, hardware }
}

/// 抓得住：`[env]` 的 `gpu=` 取的是**硬件**适配器而不是枚举里的第一个（WARP 排在第一是本机实测
/// 的形状：`Microsoft Basic Render Driver [Cpu]`）。改回 `adapters.first()` 这条就红。
#[test]
fn env_line_prefers_the_hardware_adapter() {
    let probe = probe_of(&[
        ("Microsoft Basic Render Driver", "Cpu"),
        ("AMD Radeon RX 6600 XT", "DiscreteGpu"),
    ]);
    let line = env_line(&probe, "zh-CN", Some(1.0), Some("Asia/Shanghai"));
    assert!(
        line.contains("gpu=\"AMD Radeon RX 6600 XT [DiscreteGpu]\""),
        "{line}"
    );
    assert!(line.contains("adapters=2"), "{line}");
    assert!(line.contains("hardware=true"), "{line}");
    assert!(line.contains("language=zh-CN"), "{line}");
    assert!(line.contains("tz=Asia/Shanghai"), "{line}");
    assert!(line.contains("scale_requested=1"), "{line}");
    // 显示器只有开窗后才知道：这里必须留白并**自证**留白（写个猜的值比留白更坏）
    assert!(line.contains("display=needs-window"), "{line}");
    assert!(line.starts_with("[env] os="), "{line}");
}

/// 抓得住：「一个适配器都没枚举到」不许写成 `gpu=""` 或者省略（那会让人以为枚举到了）。
#[test]
fn env_line_says_so_when_there_is_no_adapter() {
    let line = env_line(&Probe::default(), "en", None, None);
    assert!(line.contains("gpu=\"<无适配器>\""), "{line}");
    assert!(line.contains("adapters=0 hardware=false"), "{line}");
    assert!(line.contains("scale_requested=system"), "{line}");
    assert!(line.contains("tz=<未设置>"), "{line}");
}

/// 抓得住：`[i18n]` 必须把**每一级的原始值**打出来，且结论跟着真优先级走。
/// 抓不住：这台机器的 `defaults` / `reg.exe` 真的返回什么（那是真机验收）。
#[test]
fn i18n_line_shows_every_rung_and_the_resolved_language() {
    struct Fixed(Option<&'static str>, Option<&'static str>);
    impl LocaleSource for Fixed {
        fn locale(&self) -> Option<String> {
            self.0.map(str::to_owned)
        }
        fn label(&self) -> &'static str {
            self.1.unwrap_or("injected")
        }
    }

    let chain = Chain::read(
        None,
        None,
        false,
        &Fixed(Some("zh-Hans-CN"), Some("applelanguages")),
        &Fixed(None, Some("env")),
    );
    assert_eq!(chain.language(), "zh-CN");
    let line = chain.line();
    assert!(
        line.contains("platform=applelanguages:zh-Hans-CN"),
        "平台源原始值必须打出来：{line}"
    );
    assert!(line.contains("env=env:<未设置>"), "{line}");
    assert!(line.contains("forced=<未设置>"), "{line}");
    assert!(line.contains("setting=<未设置=跟随系统>"), "{line}");
    assert!(line.contains("shot=false"), "{line}");
    assert!(
        line.contains("-> lang=zh-CN locale=zh-Hans-CN source=applelanguages"),
        "{line}"
    );
    // **回归绊线（M106 r1 p2-1）**：这行曾经钉着 `ui_text=pinned-en`，在文案真的接了
    // `model.lang` 之后变成一句假话（「界面钉在英文」）。它不许回来。
    assert!(
        !line.contains("pinned-en"),
        "`ui_text=pinned-en` 是 M106 之前的旧事实，现在会与 App 的 [i18n] 行自相矛盾：{line}"
    );
}

/// 配置的手选必须**同时**体现在结论与 `[env]` 的分工上（M106 r1 p2-1 的修法证据）。
///
/// 抓得住：①`ui.language` 这一级被丢掉（那行 `->` 的结论会退回系统语言，与 `App` 的
/// `[i18n] choice=…` 矛盾）；②把「机器语言」也改成手选值（`[env] language=` 答的是「本机是什么」，
/// 用户手选英文不该改写机器的事实）。
#[test]
fn i18n_line_honours_the_explicit_setting_without_rewriting_the_machine_language() {
    struct Fixed(Option<&'static str>);
    impl LocaleSource for Fixed {
        fn locale(&self) -> Option<String> {
            self.0.map(str::to_owned)
        }
        fn label(&self) -> &'static str {
            "registry"
        }
    }

    let chain = Chain::read(
        None,
        Some("en_us"),
        false,
        &Fixed(Some("zh-CN")),
        &Fixed(None),
    );
    assert_eq!(chain.language(), "zh-CN", "[env] 报的是机器语言");
    assert_eq!(chain.detection.lang, unlucky_i18n::Lang::En, "手选压过系统");
    let line = chain.line();
    assert!(line.contains("setting=en_us"), "{line}");
    assert!(line.contains("source=setting"), "{line}");
    assert!(line.contains("lang=en"), "{line}");
}

/// **两个 `[i18n]` 行不许打架**（r4 复审 p2-1 的回归绊线）。
///
/// 复审实测的两组复现，现在必须是同一个结论：
/// - ① 只设 `ULL_LANG=en_us`、配置干净 ⇒ 结论 = en（`forced` 压过系统中文）；
/// - ② 配置 `ui.language=en_us` + **截图通道** ⇒ 结论 = zh-CN（D30：截图不读用户配置）；
/// - ③ 配置 `ui.language=en_us` + 交互通道 ⇒ 结论 = en（配置这一级只在非截图时参与）。
///
/// 抓得住：①`ULL_LANG` 这一级被丢掉；②截图通道又去读配置（「一行说 zh-CN、下一行说 effective=en」
/// 就是这么来的）；③acceptance 侧自己复制了一份优先级表 —— 每条都用
/// `crate::ui::lang::choose_choice`（产品路径那张表）算出**期望结论**再与 `Chain` 比，谁复制谁红。
/// 抓不住：真机上 `reg.exe` / `defaults` 读到什么（那是真机验收），以及 `App` 那行日志本身
/// （它由 `ui/lang.rs` 的单测守）。
#[test]
fn i18n_line_and_the_app_agree_on_every_channel_and_override() {
    use unlucky_i18n::Lang;

    struct Fixed(Option<&'static str>);
    impl LocaleSource for Fixed {
        fn locale(&self) -> Option<String> {
            self.0.map(str::to_owned)
        }
        fn label(&self) -> &'static str {
            "registry"
        }
    }

    let registry_zh = Fixed(Some("zh-CN"));
    let no_env = Fixed(None);

    /// 一行真值表：`(ULL_LANG, 配置, 截图通道) -> (期望语言, 期望来源)`。
    type Case<'a> = (Option<&'a str>, Option<&'a str>, bool, Lang, &'a str);
    let table: &[Case<'_>] = &[
        (Some("en_us"), None, false, Lang::En, "setting"),
        // 复审组合 ①：`ULL_LANG` 在截图通道里同样压过系统中文
        (Some("en_us"), None, true, Lang::En, "setting"),
        // 复审组合 ②：截图通道**忽略配置**，于是退回系统语言
        (None, Some("en_us"), true, Lang::ZhCn, "registry"),
        // 组合 ③：非截图通道，配置这一级生效
        (None, Some("en_us"), false, Lang::En, "setting"),
        (None, None, false, Lang::ZhCn, "registry"),
    ];

    for (forced, setting, screenshot, lang, origin) in table {
        let chain = Chain::read(*forced, *setting, *screenshot, &registry_zh, &no_env);
        let line = chain.line();
        assert_eq!(
            (chain.detection.lang, chain.detection.origin),
            (*lang, *origin),
            "forced={forced:?} setting={setting:?} shot={screenshot} 结论不对：{line}"
        );
        // `[env] language=` 只报机器语言，任何手选/覆盖都不该改写它。
        assert_eq!(chain.language(), "zh-CN", "{line}");

        // **同一张表**的结构性钉子：期望结论取自产品路径那个函数，谁在 acceptance 侧另写一份就红。
        let expected = crate::ui::lang::choose_choice(*forced, *setting, *screenshot)
            .resolve(&registry_zh, &no_env);
        assert_eq!(chain.detection, expected, "两侧的判据表漂了：{line}");
        assert!(
            line.contains(&format!("shot={screenshot}")),
            "截图通道必须在行里写出来：{line}"
        );
    }
}

/// 抓得住 macOS 最典型的失败形态：**一个源都没有**（Finder 启动的 GUI 进程既没有 `LANG`
/// 也没有注册表）。这时必须是 `lang=en source=default`，而不是「假装读到了英文」。
#[test]
fn i18n_line_distinguishes_no_source_from_english_source() {
    struct Nothing;
    impl LocaleSource for Nothing {
        fn locale(&self) -> Option<String> {
            None
        }
        fn label(&self) -> &'static str {
            "applelanguages"
        }
    }

    let empty = Chain::read(None, None, false, &Nothing, &Nothing);
    assert_eq!(empty.language(), "en");
    assert!(empty.line().contains("source=default"), "{}", empty.line());

    struct EnvEnglish;
    impl LocaleSource for EnvEnglish {
        fn locale(&self) -> Option<String> {
            Some("en_US.UTF-8".to_owned())
        }
        fn label(&self) -> &'static str {
            "env"
        }
    }

    let env = Chain::read(None, None, false, &Nothing, &EnvEnglish);
    assert!(env.line().contains("source=env"), "{}", env.line());
    assert!(
        env.line().contains("locale=en_US.UTF-8"),
        "两种「英文」必须能从日志上分开：{}",
        env.line()
    );
}

/// 真机 `defaults read -g AppleLanguages` 的输出形状（多行、带引号、逗号）。
#[test]
fn apple_languages_takes_the_first_entry() {
    let real = "(\n    \"zh-Hans-CN\",\n    \"en-US\"\n)\n";
    assert_eq!(parse_apple_languages(real).as_deref(), Some("zh-Hans-CN"));
    assert_eq!(
        parse_apple_languages("(\n    \"en-US\"\n)\n").as_deref(),
        Some("en-US")
    );
    assert_eq!(parse_apple_languages("()"), None);
    assert_eq!(parse_apple_languages(""), None);
    // 空串（某些机器上 `defaults` 会回 `(\n)`）不许被当成语言
    assert_eq!(parse_apple_languages("(\n    \"\"\n)"), None);
}
