//! 日志页（M95）：**单列**版面的第二个用例，也是本项目卖点（崩溃日志诊断）的落地页。
//!
//! ## 一屏由三块组成
//!
//! ```text
//! ┌ 页头（两行 84：标题 + 运行状态；右侧 Open folder / Clear）──────────┐
//! ├ 诊断卡片（只在有结论时出现，**按置信度档分组**：HIGH / MEDIUM / LOW）──┤
//! │   DIAGNOSIS 3        CRITICAL 1 / WARNING 2                        │
//! │   integrity.unsatisfied_link（规则 id，等宽）                       │
//! │   结论（规则表原文） / 说明 / Suggested fix / Confidence 85%        │
//! │   ─ One-line summary ─ (等宽可复制的一行) [Copy summary]            │
//! ├ 日志行列表（等宽 + 折行 + **独立滚动**，吃满剩余高度）──────────────┤
//! └──────────────────────────────────────────────────────────────────┘
//! ```
//!
//! 数据源只有两个（页面**不读盘**，路径从不进过页面）：
//!
//! | 来源 | 谁造 | 用途 |
//! |---|---|---|
//! | presenter 快照（`log_lines` / `diagnosis` / `log_sessions`） | `app/view.rs::logs_model` | 产品运行时 |
//! | [`demo`] 内嵌的崩溃日志片段 | `pages::demo_model`（`ULL_LOGS_FIXTURE=empty\|live\|crash`） | 出图 |
//!
//! ## M95 的取舍（逐条）
//!
//! | # | 取舍 | 为什么 |
//! |---|---|---|
//! | 1 | **折行**而不是横向滚动 | 见 [`lines`] 模块文档：一行 200+ 字符的现实下，横滚与外壳竖向滚动打架，且等宽族的 CJK 回退会毁掉「按列对齐」这个横滚唯一的好处 |
//! | 2 | 诊断卡片在日志列表**之上** | 崩溃时用户先要结论；把结论放在 14 行日志下面等于让人先翻页 |
//! | 3 | 规则原文（中文）当**数据**直接画 | 界面文案全走 i18n key；规则表的 `cause.summary` 是诊断结论本身，翻译要等 `rules.d` 带 `cause.key` 进 DTO（已报 finding） |
//! | 4 | 分组用**置信度档**（High/Medium/Low），不借 core 的严重度词汇 | `dto::Diagnosis` 没带 core 的 `severity`；拿置信度凑严重度会让 64 条规则里 12 条进错组（两个方向都有，复审实测）。档位由 `Confidence::of` 定，DTO 补齐后应换回真严重度（已报 finding） |
//! | 5 | 页头保持**两行 84** | M84 定的版式（运行状态在副标题上）；计数留在两张卡片自己的标题行，避免为了拼计数把 i18n 字符串漏进页头生命期 |
//!
//! ## 指令（只有真业务命令才 dispatch）
//!
//! | 控件 | 消息 | 落到哪 |
//! |---|---|---|
//! | Copy summary | [`Message::CopySummary`] | `app/update.rs` → `iced::clipboard::write` |
//! | Open folder | `Action::OpenLogs` | **还没有 `AppCommand`**（报 finding，现在是占位分支） |
//! | Clear | `Action::ClearLogs` | `app/update.rs` → 清空 presenter 的 `log_lines` |
//! | 空态的 CTA | [`Message::Navigate`] | 压一层路由栈回实例页（真导航，不是占位动作） |
//!
//! 截图的强弱边界写在 `logs/tests.rs` 每条断言下面。
//!
//! ## M102：页面块吃满页宽 + 单词不再被劈开
//!
//! - 正文块走 [`frame::body`]（左缘 = 页头标题左缘 = 112，宽 = 可用宽度 1044 @1180），
//!   旧版把整页块压成 424 居中，视觉复审 ① 就是它；
//! - 日志行与一行摘要先过 [`super::layout::wrap::breakable`]：在 `.` `_` `:` `/` `\` 后面插零宽空格，
//!   于是 `graphics.gl_operation_failure` 断在 `_` 后面，而不是被按字形劈成 `gl_o` / `peration`。
//!   零宽空格只进显示串，`Line::text` 与 `Logs::summary()`（剪贴板）仍是原文。

mod demo;
mod diagnostic;
mod lines;
mod model;
mod panel;

#[cfg(test)]
mod tests;

/// 诊断卡在正文里的高度份额（见 [`panel`] 的模块文档：两块必须都是定高 + 内部滚动）。
///
/// r2 是 3:2 —— 1180×760 下诊断卡的滚动视口只剩 141px，而一条完整结论（分组标题 → 建议动作 →
/// 置信度）要 177px，于是**首屏看不到任何一条完整结论**（视觉复审 r2 的 P2-A：任务要求的
/// 「建议动作」在验收尺寸下不可见）。改成 4:2：视口 ~184px，一条结论整块可见。
const DIAGNOSIS_SHARE: u16 = 4;
/// 日志卡的高度份额。
const LOGS_SHARE: u16 = 2;

pub use demo::demo;
pub use model::{Confidence, Finding, Level, Line, Logs, Outcome};

use iced::widget::column;
use iced::{Element, Length};

use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_i18n::Lang;
use unlucky_presenter::router::Route;

use crate::app::components::buttons::{self, Button};
use crate::app::components::feedback::empty_state::{empty_state, EmptyState};
use crate::theme::Ui;

use super::frame::{self, Header};
use super::layout::FrameMetrics;
use super::{Action, Message};

/// 整页。
pub fn view<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    logs: &'a Logs,
    lang: Lang,
) -> Element<'a, Message> {
    let catalog = Catalog::of(lang);

    let header = Header::new(catalog.text(Key::NavLogs))
        .subtitle(subtitle(&catalog, logs))
        .action(buttons::text(
            ui,
            Button::new(
                catalog.text(Key::LogsOpenFolder),
                Message::Action(Action::OpenLogs),
            ),
        ))
        .action(buttons::text(
            ui,
            Button::new(
                catalog.text(Key::LogsClear),
                Message::Action(Action::ClearLogs),
            ),
        ));

    let body = if logs.is_empty() {
        frame::body(metrics, empty(ui, &catalog))
    } else if logs.findings.is_empty() {
        // 没有结论：日志卡片独吞整页（这样「正常运行的一场」不会留一块空卡片）。
        frame::body(metrics, lines::card(ui, logs, lang, Length::Fill))
    } else {
        // 有结论：两块**都定高**、各自独立滚动（诊断 4 : 日志 2）。
        // 定高不是偷懒：第一版让诊断卡按内容撑开，三条结论就把日志卡压成 0 高度（截图见报告）。
        let stack = column![
            diagnostic::card(ui, logs, lang, Length::FillPortion(DIAGNOSIS_SHARE)),
            lines::card(ui, logs, lang, Length::FillPortion(LOGS_SHARE)),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .spacing(ui.space(Space::X4));

        frame::body(metrics, stack)
    };

    frame::page(ui, metrics, header, body)
}

/// 页头副标题：有会话身份（实例 · 时间 · 结局）就显示它，否则退回静态文案。
fn subtitle<'a>(catalog: &Catalog, logs: &'a Logs) -> &'a str {
    logs.status
        .as_deref()
        .unwrap_or_else(|| catalog.text(Key::LogsSubtitleIdle))
}

/// 空态（`feedback::EmptyState` 真组件：图标块 + 标题 + 说明 + 一个 CTA）。
fn empty<'a>(ui: &'a Ui, catalog: &Catalog) -> Element<'a, Message> {
    empty_state(
        ui,
        EmptyState::new(
            IconKind::Logs,
            catalog.text(Key::LogsEmptyTitle),
            catalog.text(Key::LogsEmptyBody),
        )
        .action(
            catalog.text(Key::LogsEmptyAction),
            Message::Navigate(Route::Instances),
        ),
    )
}
