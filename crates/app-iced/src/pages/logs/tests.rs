//! 日志页的守门测试。每条断言下面写清「抓得住什么 / 抓不住什么」（这个仓的硬要求）。
//!
//! 这里**没有截图断言**：像素级验收靠 `pages_demo --screen=logs` 的六张图（浅/深 × 空态/有日志/崩溃），
//! 单测只钉住「图上看不出来的那部分」——数值令牌、分组顺序、摘要格式、悬挂缩进的拆分规则、
//! 页头会话身份的拼法。

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::{FontFamily, Platform};
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_i18n::Lang;

use crate::theme::Families;

use super::super::layout::{frame_metrics, measure_cap_px, source, wrap, PaneKit};
use super::model::{self, GROUPS, SUMMARY_HEADER};
use super::{demo, lines, *};

fn ui() -> Ui {
    Ui::with_platform(
        SchemeFlavor::Dark,
        Platform::Windows,
        Families {
            text: "Segoe UI",
            display: "Segoe UI",
            mono: "Consolas",
        },
    )
}

#[test]
fn page_is_single_column_and_never_wider_than_the_measure_cap() {
    for width in [700.0, 1180.0, 1600.0] {
        let metrics = frame_metrics(PaneKit::Single, width);
        // 抓得住：谁把日志页改成两列（`secondary` 出现就会把正文列挤窄）；
        //          谁绕开 `layout` 自己算列宽（`column` 会超 424）。
        // 抓不住：列内文本是否真在这 424 里折行（那要量字形，单测量不到）。
        assert!(metrics.secondary.is_none(), "w={width}");
        assert!(
            metrics.column <= measure_cap_px() + f32::EPSILON,
            "w={width} 正文列 {} > 上限 {}",
            metrics.column,
            measure_cap_px()
        );
    }
}

/// **M102 ③**：`wrap::breakable` 这张表本身是好的，且**只改显示**（原文能还原）。
///
/// **抓得住**：断点表被掏空（`BREAK_AFTER` 少了一档 ⇒ 长标识符又没有断点）、
/// `breakable` 开始污染原文（`strip` 还不了原）；
/// **抓不住**：页面到底有没有把 `breakable` 接在渲染路径上——那条由
/// [`the_page_wires_the_soft_breaks_into_both_rendering_paths`] 扫调用点守
/// （r2 复审的变异 I：两处接线去掉后这条仍全绿，所以 docstring 不能替它吹牛）。
#[test]
fn log_text_carries_soft_breaks_but_keeps_the_original() {
    let logs = demo::crash();
    let summary = wrap::breakable(&logs.summary());

    assert!(
        !summary.contains("gl_operation_failure"),
        "摘要里没有软断点，长标识符会被按字形劈开：{summary}"
    );
    assert_eq!(
        wrap::strip(&summary),
        logs.summary(),
        "剪贴板拿到的仍是原文"
    );

    // 每条日志行的正文（拆前缀后的那一半）都要过同一件事：至少最长的那几行里有断点。
    let broken = logs
        .lines
        .iter()
        .filter(|line| {
            !wrap::breakable(lines::split_prefix(&line.text).1).contains(wrap::SOFT_BREAK)
        })
        .count();
    assert!(
        broken * 2 < logs.lines.len(),
        "{} / {} 行日志没有任何软断点（断点表可能被掏空了）",
        broken,
        logs.lines.len()
    );
}

/// **源码结构（调用点）**：日志页必须把 `wrap::breakable` 接在**两条渲染路径**上。
///
/// r2 复审的变异 I：把 `lines::entry()` 的 `mono_owned(ui, wrap::breakable(body), …)` 改回
/// `mono(ui, body, …)`、把 `diagnostic::summary_block()` 的 `wrap::breakable(&logs.summary())`
/// 改回原文 ⇒ 420 条全绿，而视觉复审 ③（`graphics.gl_o` / `peration_failure`）原样回来。
/// 断点表那侧已经 [`log_text_carries_soft_breaks_but_keeps_the_original`] 守着了，
/// 这里守的是**接线**：页面真把显示串过了一遍它。
///
/// **抓得住**：任一处接线被摘掉（日志行 / 一行摘要）；
/// **抓不住**：接线写得对但断点表被掏空（那是上一条的事）。
///
/// 用 [`source::function_region`] 而不是 `function_body`（r4 复审的变异 M3）：把
/// `wrap::breakable` 外面套一层同义包装（`fn soft(…) { wrap::breakable(…) }`）再调用，是
/// **等价重构**，不该假红；而「整条链路都没碰 `wrap::breakable`」照样红。
#[test]
fn the_page_wires_the_soft_breaks_into_both_rendering_paths() {
    let entry = source::function_region(include_str!("lines.rs"), "fn entry");
    assert!(
        entry.contains("wrap::breakable("),
        "日志行没接软断点（`graphics.gl_operation_failure` 会被按字形劈开）：\n{entry}"
    );

    let summary = source::function_region(include_str!("diagnostic.rs"), "fn summary_block");
    assert!(
        summary.contains("wrap::breakable("),
        "一行摘要没接软断点：\n{summary}"
    );
}

#[test]
fn mono_lines_use_the_mono_stack_with_the_body_small_metrics() {
    let ui = ui();
    let style = lines::mono_style(&ui);

    // 抓得住：把等宽行换成正文栈（`family` 会变 `Text`）——那是「等宽」失真的唯一入口；
    //          顺手改字号/行高（数值表要写 12/16，漂了就红）。
    // 抓不住：CJK 字形到底有没有被回退到别的族（要查字体 cmap，本 crate 不做）。
    assert_eq!(style.family, FontFamily::Mono);
    assert_eq!(style.size_px, 12.0);
    assert_eq!(style.line_height_px, 16.0);
    assert_eq!(
        ui.font_of(style).family,
        iced::font::Family::Name("Consolas")
    );
}

#[test]
fn findings_are_grouped_critical_first_and_empty_groups_do_not_exist() {
    let findings = demo::crash().findings;
    let logs = Logs {
        findings,
        ..Logs::default()
    };
    let counts: Vec<usize> = GROUPS.iter().map(|band| logs.group(*band).len()).collect();

    // 抓得住：分组顺序被改成低置信度在前（最像主因的沉底）；空组也被画出一个标题。
    // 抓不住：组内顺序（页面刻意不重排 presenter 给的顺序）。
    assert_eq!(GROUPS[0], Confidence::High);
    assert_eq!(
        counts.iter().sum::<usize>(),
        logs.findings.len(),
        "每条结论必须落进恰好一个分组"
    );
    assert!(counts[0] >= 1, "样例至少有高置信度结论");
    assert!(logs.group(Confidence::Low).is_empty());
}

#[test]
fn summary_is_a_single_deterministic_line_carrying_rule_and_confidence() {
    let logs = demo::crash();
    let summary = logs.summary();

    // 抓得住：摘要里丢了规则 id / 置信度档 / 置信度百分比（换 issue 时最关键的三个字段）；
    //          有人把换行塞进摘要（"一行"就不成立了）。
    // 抓不住：粘贴到 issue 后的可读性（那是人判断的），也不守文案语言（摘要刻意不含界面文案）。
    assert!(!summary.contains('\n'), "{summary}");
    assert!(summary.starts_with(SUMMARY_HEADER), "{summary}");
    assert!(
        summary.contains("integrity.unsatisfied_link:high(85)"),
        "{summary}"
    );
    assert!(summary.contains("lines="), "{summary}");
    assert_eq!(summary, logs.summary(), "同一份模型两次摘要必须一致");

    let empty = Logs::default();
    assert_eq!(empty.summary(), "ULL-LOGS v1 | lines=0");
}

#[test]
fn the_band_is_the_documented_confidence_band_not_a_severity_impostor() {
    // 抓得住：阈值边界写成 `>` / `>=` 的 off-by-one（80 / 50 是档位的定义点）；
    //          以及有人把档位名字改回 `critical/warning/notice` 那种**严重度词汇**
    //          （那会让 64 条规则里 12 条进错组，两个方向都有——视觉复审 r3 的 P2-2）。
    // 抓不住：档位划分本身对不对（那是产品口径；DTO 补上 `severity` 后应换回真严重度，已报 finding）。
    assert_eq!(Confidence::of(100), Confidence::High);
    assert_eq!(Confidence::of(80), Confidence::High);
    assert_eq!(Confidence::of(79), Confidence::Medium);
    assert_eq!(Confidence::of(50), Confidence::Medium);
    assert_eq!(Confidence::of(49), Confidence::Low);
    assert_eq!(Confidence::of(0), Confidence::Low);

    assert_eq!(Confidence::High.slug(), "high");
    assert_eq!(Confidence::Medium.slug(), "medium");
    assert_eq!(Confidence::Low.slug(), "low");
}

#[test]
fn the_longest_stack_line_reaches_the_view_model_untruncated() {
    let logs = demo::crash();
    let longest = logs
        .lines
        .iter()
        .map(|line| line.text.chars().count())
        .max()
        .unwrap_or(0);

    // 抓得住：为了排版把行截断 / 加省略号（用户就再也拿不回原始堆栈）。
    // 抓不住：折行后的视觉行数（渲染期的事）。
    assert!(
        longest >= 200,
        "样例里应有一条 ≥200 字符的堆栈行，实际 {longest}"
    );
    assert!(!logs.lines.iter().any(|line| line.text.ends_with('…')));
}

#[test]
fn a_line_is_split_into_prefix_and_body_for_the_hanging_indent() {
    // 抓得住：悬挂缩进被改回「整行一个文本块」（`at` 会重新独占一行、续行回到行首）。
    // 抓不住：缩进到底落在第几像素（那要量字形/截图，见报告的前后对比图）。
    assert_eq!(
        lines::split_prefix("[19:40:12] [main/INFO] hi"),
        ("[19:40:12] ", "[main/INFO] hi")
    );
    assert_eq!(
        lines::split_prefix("at com.mojang.blaze3d.platform.GLX._initGlfw(GLX.java:64)"),
        (
            "at ",
            "com.mojang.blaze3d.platform.GLX._initGlfw(GLX.java:64)"
        )
    );

    // 没有空格 / 第一个「词」过长 ⇒ 不拆（否则正文被挤成一条 1 字符宽的缝）。
    assert_eq!(
        lines::split_prefix("java.lang.RuntimeException"),
        ("", "java.lang.RuntimeException")
    );
    assert_eq!(
        lines::split_prefix("https://example.invalid/aaaaaaaaaaaaaaaaaaaaaaaa va"),
        ("", "https://example.invalid/aaaaaaaaaaaaaaaaaaaaaaaa va")
    );
    // CJK 行：第一个空格若落在 20 字符以内就正常拆（时间戳 + 中文正文）。
    assert_eq!(
        lines::split_prefix("[19:40:12] 正在启动 1.20.4"),
        ("[19:40:12] ", "正在启动 1.20.4")
    );
}

#[test]
fn status_line_names_the_session_with_the_launch_outcome_keys() {
    // 抓得住：页头副标题不再说「这是哪个实例、哪一场、什么结局」（视觉复审 P2-2），
    //          以及 `LaunchOutcome*` 三 key 又回到「定义了但零调用点」。
    // 抓不住：副标题在 1180 下会不会撞上右侧动作区（那要看截图）。
    let en = model::status_line(
        Some("开发测试"),
        Some("11-02 19:40"),
        Some(Outcome::Crashed),
        Lang::En,
    )
    .expect("有实例名与结局就该有副标题");
    assert_eq!(en, "开发测试 · 11-02 19:40 · The game crashed");

    let zh = model::status_line(
        Some("开发测试"),
        Some("11-02 19:40"),
        Some(Outcome::SilentFail),
        Lang::ZhCn,
    )
    .expect("静默失败也有副标题");
    assert!(zh.contains("疑似静默失败"), "{zh}");

    // 不知道是哪一场就别编：没有实例名 / 没有结局时退回静态副标题。
    assert!(model::status_line(None, Some("11-02 19:40"), Some(Outcome::Ok), Lang::En).is_none());
    assert!(model::status_line(Some("x"), None, None, Lang::En).is_none());

    // demo 的三种状态里，有会话身份的那两种必须带副标题（页面才有东西可显示）。
    assert!(demo::State::Crash.build(Lang::En).status.is_some());
    assert!(demo::State::Live.build(Lang::En).status.is_some());
}

#[test]
fn empty_state_is_driven_by_both_lists_being_empty() {
    let mut logs = Logs::default();
    assert!(logs.is_empty());

    logs.findings = demo::crash().findings;
    // 抓得住：把「没有日志行」当成空态（有诊断时那样会藏掉最该看的东西）。
    assert!(!logs.is_empty(), "只有诊断也不算空态");

    let live = demo::State::Live.build(Lang::En);
    assert!(!live.is_empty());
    assert!(live.findings.is_empty(), "live 状态没有诊断");
}

#[test]
fn the_fixture_parser_survives_a_non_ascii_first_char() {
    // 抓得住：`parse` 用 `split_at(1)` 切多字节序列会直接 panic（视觉复审 r2 的 p3 实测过）。
    // 抓不住：样例文件本身写坏（比如等级前缀写成小写 `e`）——那会被静默跳过，只体现为行数变少。
    assert!(!demo::parse("日本語の行")
        .iter()
        .any(|line| line.level == Level::Info));

    let parsed = demo::parse(
        "I 正常一行
W 警告一行
# 注释

E 错误一行",
    );
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0].text, "正常一行");
    assert_eq!(parsed[1].level, Level::Warn);
    assert_eq!(parsed[2].level, Level::Error);
}

#[test]
fn demo_state_comes_from_the_fixture_variable() {
    // 抓得住：环境变量取值拼错（截图会静默退成 crash，出图的人以为拍到了空态）。
    assert_eq!(demo::State::of_env_value(Some("empty")), demo::State::Empty);
    assert_eq!(demo::State::of_env_value(Some("live")), demo::State::Live);
    assert_eq!(demo::State::of_env_value(Some("crash")), demo::State::Crash);
    assert_eq!(demo::State::of_env_value(None), demo::State::Crash);
}

#[test]
fn every_key_the_page_uses_is_translated_in_both_languages() {
    let keys = [
        Key::NavLogs,
        Key::LogsSubtitleIdle,
        Key::LogsStatus,
        Key::LogsOpenFolder,
        Key::LogsClear,
        Key::LogsOutputTitle,
        Key::LogsDiagnosisTitle,
        Key::LogsConfidenceHigh,
        Key::LogsConfidenceMedium,
        Key::LogsConfidenceLow,
        Key::LogsSubtitleLive,
        Key::LogsConfidence,
        Key::LogsSuggestedFix,
        Key::LogsSummaryLabel,
        Key::LogsCopySummary,
        Key::LogsEmptyTitle,
        Key::LogsEmptyBody,
        Key::LogsEmptyAction,
        // 结局文案由页头副标题间接用到（见 `status_line`）
        Key::LaunchOutcomeNormal,
        Key::LaunchOutcomeCrash,
        Key::LaunchOutcomeSilent,
    ];

    // 抓得住：页面用到的 key 被改名/删掉，或只进了其中一套目录（缺译会返回 `⟪missing⟫`）。
    // 抓不住：译文的语气与长度（人读的事）。
    for lang in [Lang::En, Lang::ZhCn] {
        let catalog = Catalog::of(lang);
        for key in keys {
            assert!(
                catalog.get(key).is_some(),
                "{} 缺 {}",
                lang.setting_value(),
                key.name()
            );
        }
    }

    // 占位符名写错时 `render` 会把 `{percent}` 原样留在文案里——这条钉住它。
    let rendered = Catalog::of(Lang::En).render(Key::LogsConfidence, &[("percent", "85")]);
    assert_eq!(rendered, "Confidence 85%");
}

#[test]
fn header_stays_two_lines_because_of_the_status_subtitle() {
    let ui = ui();
    // 抓得住：有人把副标题去掉，页头从 84 掉回 56（版式台账上会露出来）。
    assert_eq!(ui.metrics().header_height(true), 84.0);
    assert!(ui.metrics().header_height(true) > ui.metrics().header_height(false));
}
