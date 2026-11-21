//! M453 会话选择器的守门测试（单独成文件：`tests.rs` 只剩 5 行余量）。
//!
//! 三层判据：①「点哪一行 ⇒ 哪条消息」的可判函数 [`sessions::row_message`] ②卡片**真的**走这条
//! 函数、且三条分支都在（源码结构）③三档（0 / 1 / 多）在模型与几何上的判据。
//!
//! 这里**没有截图断言**：三档的像素证据在 `DevDocs/artifacts/m453/`（浅/深 × 空/单/多），
//! 单测只钉「图上看不出来的那部分」——消息载荷、行文案的三个字段、定高档的高度来源。

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_i18n::Lang;

use crate::theme::{Families, Ui};

use super::super::layout::source;
use super::{demo, sessions, Message, Outcome};

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

/// **点某一行 ⇒ `LogsOpenSession(该行的 id)`**（不是「渲染了一个按钮」）。
///
/// 抓得住：id 传成空串 / 写死成第一场 / 把行接成 `LogsRefreshSessions`（`matches!` 的模式与
/// 载荷一起判，换消息类型必红）。
/// 抓不住：`card()` 有没有真的调用它——那是下面 [`the_card_routes_every_row_through_row_message`]
/// 与 `app/update/page_messages/logs_sessions_tests.rs`（断言 `AppCommand` 队列）两段合起来钉的。
#[test]
fn clicking_a_row_names_that_session() {
    let crash = sessions::Session::new("s-7", "开发测试", "11-02 20:05", Outcome::Crashed);
    let ok = sessions::Session::new("s-3", "1.20.1 原版", "10-31 19:30", Outcome::Ok);

    assert!(
        matches!(sessions::row_message(&crash), Message::LogsOpenSession(id) if id == "s-7"),
        "点崩溃那一场必须带它自己的 id"
    );
    assert!(
        matches!(sessions::row_message(&ok), Message::LogsOpenSession(id) if id == "s-3"),
        "id 必须跟着行走，不是写死的第一场"
    );
    assert!(
        !matches!(sessions::row_message(&ok), Message::LogsRefreshSessions),
        "点一行不是刷新（两条意图的落点命令不同）"
    );
}

/// **源码结构**：卡片里的每一行都经 [`sessions::row_message`]，刷新按钮走 `LogsRefreshSessions`，
/// 三档高度分支都在。
///
/// 为什么只能扫源码：`iced::Element` 没有「取回调消息」的内省接口（`tests.rs` 的
/// `the_page_wires_the_soft_breaks_into_both_rendering_paths` 是同一个理由）。
///
/// 抓得住：行改成「分支里现算一个消息」、刷新按钮被摘掉、「多场」不再定高滚动（会话一多就把
/// 下面的诊断卡挤出去）。
/// 抓不住：把 `row_message` 内联成同样的表达式——那是等价重构，这条会**假红**（真红的是上面
/// 那条：载荷换了它就红）。
#[test]
fn the_card_routes_every_row_through_row_message() {
    let card = source::function_region(include_str!("sessions.rs"), "fn card");

    assert!(
        card.contains("row_message("),
        "卡片没有经 `row_message` 造行的消息（点击的 id 就没有唯一出处了）：\n{card}"
    );
    assert!(
        card.contains("Message::LogsRefreshSessions"),
        "刷新按钮没接上（`LogsRefreshSessions` 是唯一手动路径）：\n{card}"
    );
    assert!(
        card.contains("panel::scroll("),
        "多场没有独立滚动区（会话一多就会把下面几块顶出去）：\n{card}"
    );
    assert!(
        card.contains("Length::Fixed(visible_height"),
        "多场没有定高（高度必须由 `visible_height` 从令牌算）：\n{card}"
    );
    assert!(
        card.contains("Length::Shrink"),
        "单场 / 空态也定高了（一场的时候不该有滚动条这种冗余 UI）：\n{card}"
    );
}

/// **三档在模型与几何上的判据**：空 / 单 / 多各自成立，定高档的余量小于一行行高。
///
/// 抓得住：`VISIBLE_ROWS` 被改成 0（定高档看不到任何完整行）、`visible_height` 里漏了行距或
/// 卡内距（余量会小于标题那一行 ⇒ 最后一行被切）、demo 的三种状态不再对应三个档位
/// （截图就会「拍了三张同一档」）。
/// 抓不住：真机上被裁掉几个像素（那要截图，见 `DevDocs/artifacts/m453/`）。
#[test]
fn the_three_states_hold_on_the_model_and_on_the_height() {
    let ui = ui();
    let row = ui.metrics().row_height;
    let rows = row * sessions::VISIBLE_ROWS as f32;
    let fixed = sessions::visible_height(&ui);

    assert!(
        rows >= row,
        "定高档至少要放得下一整行（VISIBLE_ROWS = {}）",
        sessions::VISIBLE_ROWS
    );
    assert!(
        fixed > rows,
        "定高档还必须容得下标题行与卡内距（实测 {fixed} vs 行 {rows}）"
    );
    // 标题行里有一颗标准控件（`Refresh` 按钮）⇒ 补白至少是 `control_height`，不是文字行高。
    // 这一条是本轮实拍的回归：按文字行高算 ⇒ 滚动视口只剩 ~60px，第二行被从中间切掉。
    assert!(
        fixed - rows >= ui.metrics().control_height,
        "补白（{}）连标题行里的那颗按钮（{}）都容不下：行的视口会被切掉半行",
        fixed - rows,
        ui.metrics().control_height
    );
    assert!(
        fixed - rows < row * 3.0,
        "补白（标题行 + 行距 + 卡内距）不该超过三行行高（实测 {}，一行 {row}）",
        fixed - rows
    );

    // 三个档位各有一个 demo 状态（三张截图的依据）。
    let empty = demo::State::Empty.build(Lang::En);
    assert!(empty.sessions.is_empty(), "空态必须真的没有会话");

    let running = demo::State::Running.build(Lang::En);
    assert!(
        running.sessions.is_empty() && !running.lines.is_empty(),
        "「正在跑」是**选择器空态 + 有日志**那一档：索引只在结束时追加"
    );

    let single = demo::State::Crash.build(Lang::En);
    assert_eq!(single.sessions.len(), 1, "单场档只有一行");
    assert_eq!(single.selected.as_deref(), Some("s-3"), "单场档要选中它");

    let many = demo::State::History.build(Lang::En);
    assert!(
        many.sessions.len() > sessions::VISIBLE_ROWS,
        "多场档必须超过可见行数（否则滚动分支根本没被画到）"
    );
    assert!(
        many.sessions.iter().any(|s| s.outcome == Outcome::Crashed),
        "多场档里必须有一场崩溃（任务要求的对照）"
    );
    assert_eq!(
        demo::State::of_env_value(Some("history")),
        demo::State::History
    );
}

/// **行文案要人认得出**：实例名、时间、结局三个都在，且随语言变。
///
/// 抓得住：`fill_labels` 没被 `with_status` 调到（行文案停在时间那一串）、结局那一截丢了
/// （只剩「实例 · 时间」）、语言换了文案不换（`LaunchOutcome*` 三 key 没走目录）。
/// 抓不住：行宽够不够放着三个字段（那要看截图）。
#[test]
fn a_row_names_the_instance_the_time_and_the_outcome() {
    let en = demo::State::History.build(Lang::En);
    let first = &en.sessions[0];
    let catalog = Catalog::of(Lang::En);

    assert!(first.label.contains("开发测试"), "{}", first.label);
    assert!(first.label.contains("11-02 19:40"), "{}", first.label);
    assert!(
        first.label.contains(catalog.text(Key::LaunchOutcomeCrash)),
        "结局没进文案：{}",
        first.label
    );

    let zh = demo::State::History.build(Lang::ZhCn);
    assert!(
        zh.sessions[0].label.contains("崩溃"),
        "{}",
        zh.sessions[0].label
    );
    assert_ne!(
        zh.sessions[0].label, first.label,
        "换了界面语言，行文案必须跟着换"
    );

    // 没拼过的行退回时间串（`Session::new` 的默认），不会是一串空
    let raw = sessions::Session::new("s-1", "开发测试", "10-31 19:30", Outcome::Ok);
    assert_eq!(raw.label, "10-31 19:30");
}

/// 选择器用到的三条新文案**两门语言都有**（缺译时目录会返回 `⟪missing⟫`）。
///
/// 抓得住：key 只进了一套目录 / 改名后页面还在用旧的。
/// 抓不住：译文的语气与长度（人读的事，交给截图）。
#[test]
fn the_picker_texts_exist_in_both_languages() {
    for lang in [Lang::En, Lang::ZhCn] {
        let catalog = Catalog::of(lang);
        for key in [
            Key::LogsSessionsTitle,
            Key::LogsSessionsEmpty,
            Key::LogsSessionsRefresh,
            Key::LogsBackToLive,
        ] {
            assert!(
                catalog.get(key).is_some(),
                "{} 缺 {}",
                lang.setting_value(),
                key.name()
            );
        }
    }
}

/// 三档 + 有/无诊断都能把卡片建出来（`Element` 构造失败 = 点了就崩，编译期看不出来）。
///
/// 抓得住：某条分支里的 `Length` 组合让版面建不起来、或空列表分支被写成 `unwrap` / 索引越界。
/// 抓不住：建出来的树长什么样（iced 的 `Element` 没有几何内省，像素归截图）。
#[test]
fn the_card_builds_in_every_state() {
    let ui = ui();
    for state in [
        demo::State::Empty,
        demo::State::Running,
        demo::State::Live,
        demo::State::Crash,
        demo::State::History,
        demo::State::HistoryLive,
    ] {
        for lang in [Lang::En, Lang::ZhCn] {
            let logs = state.build(lang);
            let _built = sessions::card(&ui, &logs, lang);
        }
    }
}

/// **M462 判据①（消息那一半）**：入口 → `LogsOpenSession(<被挡住那一场>)`。
///
/// 抓得住：载荷写成空串 / 写死成第一场历史 / 接成 `LogsRefreshSessions`（`matches!` 把模式与
/// 载荷一起判）。这一条只钉「点了发什么」；**点了之后真的退出历史视图、实时行真的续上**由
/// presenter 的 `host/logs_open_tests.rs` 钉（真 seam）。
/// 抓不住：卡片画不画这颗按钮（下面那条源码扫描 + 两张截图）。
#[test]
fn the_way_back_to_live_names_the_blocked_session() {
    assert!(
        matches!(
            sessions::back_to_live_message("s-1029-2001"),
            Message::LogsOpenSession(id) if id == "s-1029-2001"
        ),
        "入口必须带上被历史视图挡住那一场的 id（`AppCommand::OpenLogSession` 的载荷）"
    );
    assert!(
        !matches!(
            sessions::back_to_live_message("s-1029-2001"),
            Message::LogsRefreshSessions
        ),
        "点入口不是刷新"
    );
}

/// **M462 判据③（渲染那一半）**：`back_to_live` 为 `None` 时卡片里**没有**这颗按钮。
///
/// 为什么扫源码：`iced::Element` 没有「取回调消息」的内省（同上）。
///
/// 抓得住：把 `logs.back_to_live` 那层 `match` 拆掉、无条件画一颗（一个点了没用的控件）；
/// 也抓得住入口绕开 [`sessions::back_to_live_message`] 自己拼一个 id。
/// 抓不住：位置与留白（截图）。
#[test]
fn the_card_only_draws_the_way_back_when_there_is_one() {
    let card = source::function_region(include_str!("sessions.rs"), "fn card");
    let entry = source::function_region(include_str!("sessions.rs"), "fn back_to_live<");

    assert!(
        card.contains("logs.back_to_live.as_deref()"),
        "卡片没有按 `back_to_live` 决定要不要画入口：\n{card}"
    );
    assert!(
        card.contains("Some(id) => head.push(back_to_live("),
        "入口不是从 `logs.back_to_live` 那个 `Some` 分支画出来的（另一种写法会让判据③失效）：\n{card}"
    );
    assert!(
        entry.contains("back_to_live_message("),
        "入口没有经 `back_to_live_message` 造消息（载荷就没有唯一出处了）：\n{entry}"
    );

    // 模型两档（与 `history-live` / `history` 两个出图状态同源）。
    assert_eq!(
        demo::State::HistoryLive
            .build(Lang::En)
            .back_to_live
            .as_deref(),
        Some(demo::LIVE_SESSION),
        "`history-live` 夹具必须带上「回到实时」的落点（图 A 的依据）"
    );
    assert_eq!(
        demo::State::History.build(Lang::En).back_to_live,
        None,
        "`history` 夹具必须**没有**它（图 B 的依据）"
    );
}
