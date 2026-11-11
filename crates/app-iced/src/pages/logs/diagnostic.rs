//! 诊断结果区：**按置信度档分组**的结论卡片 + 可直接粘进 issue 的一行摘要。
//!
//! ## 一条结论要能看出什么（tower 的三问）
//!
//! | 问题 | 落点 | 数据来源 |
//! |---|---|---|
//! | 哪条规则命中 | 规则 id（等宽、`LabelMedium`） | `Finding::rule` |
//! | 置信度（不是严重度） | 分组标题 + 分组顺序（`HIGH` → `MEDIUM` → `LOW`） | `Finding::confidence_band` |
//! | 建议动作 | `Suggested fix` 一行（规则表的 `fix_action`，技术标识原样） | `Finding::action` |
//!
//! ## 用户真的要拿去用的两样东西
//!
//! 1. **结论原文**（`title` / `hint`）：规则表 `rules.d/*.yaml` 写的 `cause.summary` / `detail`。
//!    它是**数据**（诊断结论），页面不翻译也不改写——改写会让「规则命中」与「用户读到的」
//!    变成两套说法。界面文案（分组名 / 标签 / 按钮）仍然全部走 i18n key。
//! 2. **一行摘要**（[`Logs::summary`]）：等宽显示 + 「Copy summary」按钮；按钮把字符串塞进
//!    剪贴板（`Message::CopySummary` → `iced::app/update.rs` → `iced::clipboard::write`）。
//!    崩溃日志换 issue 时粘这一行就够：实例 / 时间 / 结局 / 行数 / 命中的规则与置信度都在里面。

use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{column, row, text};
use iced::{Alignment, Color, Element, Length, Pixels};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::{TypeRole, TypeStyle};
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_i18n::Lang;

use crate::app::components::buttons::{self, Button};
use crate::theme::Ui;

use super::super::frame;
use super::super::layout::wrap;
use super::super::Message;
use super::lines::{mono, mono_owned};
use super::model::{Confidence, Finding, Logs, GROUPS};
use super::panel;

/// 诊断卡片（只在有结论时画）。高度由调用方定（见 [`panel`]）。
///
/// 结构 = 头（`DIAGNOSIS` + 条数）/ **可滚动的分组区** / **固定的摘要页脚**。
/// 摘要钉在页脚而不是跟着滚（视觉复审 P2-3）：三条结论就足以把它顶到折叠线以下，
/// 而「Copy」复制的是用户必须看得见的东西——看不见的字符串不该能被复制。
pub fn card<'a>(ui: &'a Ui, logs: &'a Logs, lang: Lang, height: Length) -> Element<'a, Message> {
    let catalog = Catalog::of(lang);

    let mut group = column![].spacing(ui.space(Space::X2)).width(Length::Fill);

    for band in GROUPS {
        let findings = logs.group(band);
        if findings.is_empty() {
            continue;
        }

        group = group.push(frame::divider(ui));
        group = group.push(title_row(ui, catalog.text(key_of(band)), findings.len()));
        for finding in findings {
            group = group.push(finding_block(ui, &catalog, finding));
        }
    }

    panel::card(
        ui,
        height,
        column![
            title_row(
                ui,
                catalog.text(Key::LogsDiagnosisTitle),
                logs.findings.len(),
            ),
            panel::scroll(ui, group),
            frame::divider(ui),
            summary_block(ui, &catalog, logs),
        ]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill),
    )
}

/// 标题行：左边标签（大写字距靠令牌档），右边条数。
fn title_row<'a>(ui: &'a Ui, label: &'a str, count: usize) -> Element<'a, Message> {
    row![
        frame::text_role(
            ui,
            TypeRole::LabelMedium,
            true,
            label,
            ui.color(Role::OnSurfaceVariant),
        ),
        iced::widget::Space::new().width(Length::Fill),
        frame::text_role_owned(
            ui,
            TypeRole::LabelMedium,
            false,
            count.to_string(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .into()
}

/// 一条结论：规则 id / 结论 / 说明 / 建议动作 / 置信度。
fn finding_block<'a>(ui: &'a Ui, catalog: &Catalog, finding: &'a Finding) -> Element<'a, Message> {
    let mut block = column![
        mono(ui, &finding.rule, ui.color(Role::OnSurfaceVariant)),
        wrapped(
            ui,
            TypeRole::BodyMedium,
            &finding.title,
            ui.color(Role::OnSurface)
        ),
        wrapped(
            ui,
            TypeRole::BodySmall,
            &finding.hint,
            ui.color(Role::OnSurfaceVariant)
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    if let Some(action) = &finding.action {
        block = block.push(
            column![
                frame::text_role(
                    ui,
                    TypeRole::LabelMedium,
                    false,
                    catalog.text(Key::LogsSuggestedFix),
                    ui.color(Role::OnSurfaceVariant),
                ),
                mono(ui, action, ui.color(Role::OnSurface)),
            ]
            .spacing(0.0)
            .width(Length::Fill),
        );
    }

    block = block.push(frame::text_role_owned(
        ui,
        TypeRole::LabelMedium,
        false,
        catalog.render(
            Key::LogsConfidence,
            &[("percent", &finding.confidence.to_string())],
        ),
        ui.color(Role::OnSurfaceVariant),
    ));

    block.into()
}

/// **固定的摘要页脚**：标签 +「Copy summary」+ 等宽摘要（可整段带走）。
///
/// 页脚不滚动（见 [`card`] 的文档）：按钮紧邻它要复制的那段文字，用户复制的一定是看得见的串。
fn summary_block<'a>(ui: &'a Ui, catalog: &Catalog, logs: &Logs) -> Element<'a, Message> {
    column![
        row![
            frame::text_role(
                ui,
                TypeRole::LabelMedium,
                false,
                catalog.text(Key::LogsSummaryLabel),
                ui.color(Role::OnSurfaceVariant),
            ),
            iced::widget::Space::new().width(Length::Fill),
            buttons::text(
                ui,
                // M237：页脚按钮在诊断卡（`panel::card`）里 ⇒ 状态层叠卡底，不是画布底。
                Button::new(
                    catalog.text(Key::LogsCopySummary),
                    Message::CopySummary(logs.summary()),
                )
                .base(panel::BASE),
            ),
        ]
        .width(Length::Fill)
        .align_y(Alignment::Center),
        mono_owned(
            ui,
            wrap::breakable(&logs.summary()),
            ui.color(Role::OnSurface)
        )
        .width(Length::Fill)
        .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}

/// 置信度档 → 分组标题的 key。
///
/// 刻意**不叫严重度**（`CRITICAL` / `WARNING` / `NOTICE`）：DTO 没带 core 的 severity，
/// 拿置信度去凑严重度会让 64 条规则里的 12 条进错组（`mod.mixin_failure` critical/0.70 沉底、
/// `other.debug_crash` info/0.95 冒顶）。标题只说它对「这条结论有多确定」。
fn key_of(band: Confidence) -> Key {
    match band {
        Confidence::High => Key::LogsConfidenceHigh,
        Confidence::Medium => Key::LogsConfidenceMedium,
        Confidence::Low => Key::LogsConfidenceLow,
    }
}

/// 令牌排版档 + **按词或字形折行**（长堆栈里的点分标识符没有空格，只按词会溢出）。
fn wrapped<'a>(ui: &'a Ui, role: TypeRole, content: &'a str, color: Color) -> Element<'a, Message> {
    let style: TypeStyle = ui.style(role);

    text(content)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
        .font(ui.font_of(style))
        .color(color)
        .width(Length::Fill)
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}
