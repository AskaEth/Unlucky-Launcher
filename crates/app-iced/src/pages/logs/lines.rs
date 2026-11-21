//! 日志行列表：**等宽 + 折行 + 独立滚动**。
//!
//! ## 为什么折行而不是横向滚动（tower 要求二选一，这里选折行）
//!
//! 一行崩溃堆栈动辄 200+ 字符（样例里最长的一行 214 字符）。M102 起卡片吃满页宽
//! （1180 窗宽 → 卡内约 1004px），但 214 字符 × 等宽 12px ≈ 1412px 仍要折一次。两条路：
//!
//! | 方案 | 代价 |
//! |---|---|
//! | 横向滚动 | 一条日志要横甩两屏；外壳已经是竖向滚动，横竖两向嵌套后滚轮归属会打架 |
//! | **折行（本页选）** | 一条日志可能占 2~3 行，行号/等级栏必须能标明「这是同一条」 |
//!
//! 选折行的决定性理由：**折行让等宽族只承担「视觉密度」，不承担「列对齐」**。等宽栈在
//! Windows 上是 `Cascadia Mono` / `Consolas`，它们没有 CJK 字形——中文日志要靠 cosmic-text
//! 逐字回退到系统 CJK 族（本机实测回退到 `Microsoft YaHei UI`），回退字形的 advance 不是 1ch。
//! 横向滚动方案里「第 40 列对齐」是有意义的（堆栈缩进、`at a.b.C(File.java:12)` 的括号列），
//! 一旦 CJK 回退把列宽带偏，对齐就废了；折行版面不承诺任何列对齐，回退只影响同一行内的观感。
//! ## 悬挂缩进（M95 r2 落地，视觉复审 P2-1）
//!
//! r1 的 `entry()` 只有 `row![等级条, 单个 Text]`：折行的续行回到文本自己的左边距，
//! 于是 `at com.mojang…(GLX.java:64)` 被拆成「`at` 独占一行 + 帧名一行」，和「下一条日志」
//! 视觉上无法区分（一屏白花 ~5 行）。**现在的落地方式**是把一行拆成**前缀 + 正文两个文本控件**：
//!
//! ```text
//! │ [19:40:14] [Render thread/ERROR] 显卡驱动不支持 OpenGL 3.2，初始化窗口失败
//! │        ↑ 前缀（第一个空格及其之前，含空格）   ↑ 正文从它右边开始，带着 Fill 宽度自己折行
//! │ java.lang.IllegalStateException: GLFW error before init: [0x10008] Failed to create window…
//! │ ↑ 无前缀（第一个「词」超过 20 字符）⇒ 整行一个文本块
//! ```
//!
//! 为什么不是「NBSP 绑定前缀 + 单个 Text」：NBSP 只阻止断行，**改不了续行的起点**——续行依旧
//! 回到文本左边距，`at` 还是独占一行（除非整行按字形硬断，那会把时间戳也断掉）。两段布局是
//! iced 里唯一能同时做到「`at` 与帧名同行」+「续行比行首缩进」的写法。
//!
//! 代价与边界：续行缩进量 = **该行前缀宽度**（时间戳行 ≈ 10 字符、栈帧 `at` 行 ≈ 3 字符），
//! 所以不同行的续行缩进不一样——这是悬挂缩进的正常形态（按行首标签缩进），不是 bug。
//!
//! ## 软断点（M102，视觉复审 ③）
//!
//! 没有空格的长串（`graphics.gl_operation_failure:medium(70)`）在原版里只能按字形硬断，
//! 图里出现 `gl_o` / `peration_failure`。现在正文先过
//! [`super::super::layout::wrap::breakable`]：在 `.` `_` `:` `/` `\` 后插零宽空格，折行落回
//! 词边界；悬挂缩进不受影响（前缀仍是独立文本控件），`Line::text` 仍是原文。
//!
//! ## 贴底（M95 r2，视觉复审 P2-4）
//!
//! 崩溃排查看的是**最后几行**，所以行列表用 `panel::scroll_tail`（`Anchor::End`）：偏移按
//! 「距底多远」记，内容增长时视图保持距底同一位置 ⇒ 天然跟随尾部。用户上滚后的行为与
//! 「是否冻结」的边界写在 [`panel::scroll_tail`] 的文档里（结论：**没有**冻结，需要 app 层
//! 回灌滚动状态才做得到）。
//!
//! ## 等宽族的实际落点
//!
//! `ui.families().mono` 是启动时用系统字体库核对过的族名（[`crate::fonts`] 求值，启动日志里
//! 有一行 `mono=…`），本页只通过 [`mono_style`] 用它，不写死族名。字号/行高走
//! `TypeRole::BodySmall`（12/16），与报告里的数值表对得上（见 `logs/tests.rs`）。

use iced::widget::text::LineHeight;
use iced::widget::{column, container, row, space};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Pixels};

use unlucky_design::color::Role;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::{FontFamily, TypeRole, TypeStyle};
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_i18n::Lang;

use crate::theme::Ui;

use super::super::frame;
use super::super::layout::wrap;
use super::super::Message;
use super::model::{Level, Line, Logs};
use super::panel;

/// 前缀上限（字符）：超过就不拆（见 [`split_prefix`]）。
const PREFIX_MAX_CHARS: usize = 20;

/// 等级条宽度（2px：够看见，又不抢日志正文的起始列）。
const BAR_WIDTH: f32 = 2.0;

/// 日志卡片：标题行（`OUTPUT` + 行数）+ 独立滚动的行列表。高度由调用方定（见 [`panel`]）。
pub fn card<'a>(ui: &'a Ui, logs: &'a Logs, lang: Lang, height: Length) -> Element<'a, Message> {
    let catalog = Catalog::of(lang);

    let head = row![
        container(frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            catalog.text(Key::LogsOutputTitle),
            ui.color(Role::OnSurfaceVariant),
        ))
        .width(Length::Fill)
        .align_y(Alignment::Center),
        frame::text_role_owned(
            ui,
            TypeRole::LabelMedium,
            false,
            logs.lines.len().to_string(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .padding(Padding::default().bottom(ui.space(Space::X2)));

    let rows = column(
        logs.lines
            .iter()
            .map(|line| entry(ui, line))
            .collect::<Vec<_>>(),
    )
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    panel::card(ui, height, column![head, panel::scroll_tail(ui, rows)])
}

/// 一行：等级条（跨行）+ **前缀 / 正文两段**（正文折行后落在前缀右边 = 悬挂缩进）。
fn entry<'a>(ui: &'a Ui, line: &'a Line) -> Element<'a, Message> {
    let color = color_of(ui, line.level);
    let (prefix, body) = split_prefix(&line.text);

    let bar = container(space::horizontal())
        .width(BAR_WIDTH)
        .height(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                radius: ui.radius(ShapeScale::Full, BAR_WIDTH),
                ..Border::default()
            },
            ..container::Style::default()
        });

    // 「前缀 + 正文」拆两个文本控件是悬挂缩进的**唯一**落地方式（iced 的单个 `Text` 折行后
    // 续行一定回到自己的左边距，没有 `text-indent` 之类的排版钩子）：
    // 前缀占第一行开头，正文从它右边开始、带着 `Fill` 宽度自己折行 ⇒ 续行自然落在
    // `前缀宽度` 之后，与下一条日志的起点（x = 等级条右边）分开。
    //
    // M102：正文先过 `wrap::breakable`——`graphics.gl_operation_failure:medium(70)` 这种没有空格的
    // 长串在原版里只能按字形硬断（`gl_o` / `peration`），补上软断点后才断在 `.` `_` `:` 后面。
    let text = row![
        mono(ui, prefix, color),
        mono_owned(ui, wrap::breakable(body), color)
            .width(Length::Fill)
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
    ]
    .spacing(0.0)
    .width(Length::Fill)
    .align_y(Alignment::Start);

    row![bar, text]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill)
        .align_y(Alignment::Start)
        .into()
}

/// 拆「前缀」与「正文」：前缀 = 第一个空格及其之前的部分（**含**那个空格，所以接缝处不用再加
/// 间距），正文 = 其余。
///
/// 拆的条件：空格在 1..=[`PREFIX_MAX_CHARS`] 个字符之内。超长（一整行没有空格、或第一个
/// 「词」本身就比 20 字符长，例如以 URL 开头的行）就**不拆**，整行当一个文本块折行——否则
/// 前缀会把正文挤成一条 1 字符宽的缝。
pub(super) fn split_prefix(text: &str) -> (&str, &str) {
    match text.find(' ') {
        Some(index) if index > 0 && index < PREFIX_MAX_CHARS => text.split_at(index + 1),
        _ => ("", text),
    }
}

/// 等级 → 颜色（错误用 `Error`，警告用 `Tertiary`：与主题的 warning 槽位同源）。
fn color_of(ui: &Ui, level: Level) -> Color {
    match level {
        Level::Error => ui.color(Role::Error),
        Level::Warn => ui.color(Role::Tertiary),
        Level::Info => ui.color(Role::OnSurface),
        Level::Debug => ui.color(Role::OnSurfaceVariant),
    }
}

/// 等宽排版档：**同一个 `BodySmall` 档，只把字体族换成 mono 栈**。
///
/// 不新造字号（12/16 与辅助说明同档），所以「等宽」这件事只改字体族这一个维度，
/// 行高仍是令牌值——CJK 回退只换字形，行高不会跳（这也是折行方案的前提之一）。
#[must_use]
pub fn mono_style(ui: &Ui) -> TypeStyle {
    TypeStyle {
        family: FontFamily::Mono,
        ..ui.style(TypeRole::BodySmall)
    }
}

/// 等宽文本（字号 / 行高 / 字重 / 族全来自 [`mono_style`]）。
#[must_use]
pub fn mono<'a>(ui: &Ui, content: &'a str, color: Color) -> iced::widget::Text<'a> {
    mono_styled(ui, content, color)
}

/// 同 [`mono`]，但文案是**拥有的**字符串（摘要行这类运行时拼出来的短串）。
#[must_use]
pub fn mono_owned<'a>(ui: &Ui, content: String, color: Color) -> iced::widget::Text<'a> {
    mono_styled(ui, content, color)
}

fn mono_styled<'a>(
    ui: &Ui,
    content: impl iced::widget::text::IntoFragment<'a>,
    color: Color,
) -> iced::widget::Text<'a> {
    let style = mono_style(ui);

    iced::widget::text(content)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
        .font(ui.font_of(style))
        .color(color)
}
