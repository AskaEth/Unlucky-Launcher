//! 诊断结果区：**按真严重度档分组**的结论卡片 + 可直接粘进 issue 的一行摘要。
//!
//! ## 一条结论要能看出什么（tower 的三问）
//!
//! | 问题 | 落点 | 数据来源 |
//! |---|---|---|
//! | 哪条规则命中 | 规则 id（等宽、`LabelMedium`） | `Finding::rule` |
//! | 有多严重（**分组依据**） | 分组标题 + 分组顺序（`CRITICAL` → `WARNING` → `NOTICE`） | `Finding::severity` |
//! | 引擎有多确定（**仍画**，只是不再分组） | 卡片末行 `Confidence 85%` | `Finding::confidence` / `confidence_band` |
//! | 建议动作 | `Suggested fix` 一行（规则表的 `fix_action`，技术标识原样） | `Finding::action` |
//!
//! M394 `SEVGROUP` 换的分组轴：从「高/中/低置信度」换成 core 的 `Severity`。**置信度没被删**——
//! 它从分组头降级成卡片上的一行标记（见 [`finding_block`] 的最后一段）。
//!
//! ## 用户真的要拿去用的两样东西
//!
//! 1. **结论标题与提示**：从 `unlucky_i18n` 目录按 `Finding::cause_key` 取，`{sourcemod}` 这类
//!    占位符用 `Finding::groups` 代入（M392）。页面里**没有一个中文字面量**：M392 之前这两个字段
//!    是 core 规则表 `rules.d/*.yaml` 的中文原文直出，英文界面会冒出整句中文。M395 之后连 10 条
//!    引擎内置结论（`signal.*` / `hs_err.*`）也走目录——它们的动态片段由 core 传进 `groups`——
//!    所以 `Finding::system_text` 只剩「模板参数缺了」时的一条退路（见 [`texts_of`]）。
//! 2. **一行摘要**（[`Logs::summary`]）：等宽显示 + 「Copy summary」按钮；按钮把字符串塞进
//!    剪贴板（`Message::CopySummary` → `iced::app/update.rs` → `iced::clipboard::write`）。
//!    崩溃日志换 issue 时粘这一行就够：实例 / 时间 / 结局 / 行数 / 命中的规则与置信度都在里面。

use std::collections::BTreeMap;

use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{column, row, text};
use iced::{Alignment, Color, Element, Length, Pixels};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::{TypeRole, TypeStyle};
use unlucky_i18n::catalog::{diagnose_keys_from_cause, placeholders, Catalog, Key};
use unlucky_i18n::Lang;

use crate::app::components::buttons::{self, Button};
use crate::theme::Ui;

use super::super::frame;
use super::super::layout::wrap;
use super::super::Message;
use super::lines::{mono, mono_owned};
use super::model::{Finding, Logs, Severity, GROUPS};
use super::panel;

/// 诊断卡片（只在有结论时画）。高度由调用方定（见 [`panel`]）。
///
/// 结构 = 头（`DIAGNOSIS` + 条数）/ **可滚动的分组区** / **固定的摘要页脚**。
/// 摘要钉在页脚而不是跟着滚（视觉复审 P2-3）：三条结论就足以把它顶到折叠线以下，
/// 而「Copy」复制的是用户必须看得见的东西——看不见的字符串不该能被复制。
pub fn card<'a>(ui: &'a Ui, logs: &'a Logs, lang: Lang, height: Length) -> Element<'a, Message> {
    let catalog = Catalog::of(lang);

    let mut group = column![].spacing(ui.space(Space::X2)).width(Length::Fill);

    for severity in GROUPS {
        let findings = logs.group(severity);
        // 空组**不画组头**（M394 对齐的三样之一，见 `model::GROUPS`）：没有结论的档位不占位置。
        if findings.is_empty() {
            continue;
        }

        group = group.push(frame::divider(ui));
        group = group.push(title_row(
            ui,
            catalog.text(key_of(severity)),
            findings.len(),
        ));
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
    // 文案是**目录渲染出来的 `String`**（模板 + 捕获组），所以走 `_owned` 那一档排版助手：
    // 借用的 `wrapped` 只能吃 `&'a str`，而这个 `String` 活不过本函数。
    let (title, hint) = texts_of(catalog, finding);

    let mut block = column![
        mono(ui, &finding.rule, ui.color(Role::OnSurfaceVariant)),
        wrapped_owned(ui, TypeRole::BodyMedium, title, ui.color(Role::OnSurface)),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    if let Some(hint) = hint {
        block = block.push(wrapped_owned(
            ui,
            TypeRole::BodySmall,
            hint,
            ui.color(Role::OnSurfaceVariant),
        ));
    }

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

    // **旧维度留在卡片上**（M394）：分组头说的是严重度，这一行说的是「引擎有多确定」。
    // 换分组不等于删掉置信度——同一档严重度里，85% 和 55% 对用户的下一步判断不一样。
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

/// 一条结论的（标题, 提示）**文案**——全部从目录取，页面里没有一个中文字面量。
///
/// ## 三条口径（M392，M395 修订第二条）
///
/// | 情形 | 中文界面 | 英文界面 |
/// |---|---|---|
/// | 目录有条目（64 条规则 + 10 条内置结论） | `diagnose.title/hint.<rule id>` 的 zh，`groups` 代入 | 同 key 的 en |
/// | 目录有条目、但模板参数**缺**（M395 之前写的会话 JSONL） | core 的原话 `system_text` | `[system text not translated]` |
/// | 目录、原话都没有 | 兜底 key | 兜底 key |
///
/// 内置结论（`signal.*` ×6 / `hs_err.*` ×4）在 M395 之前属于第二种：它们的句子是 core 用
/// `format!` 拼的（退出码摘要 / 错误行数 / hs_err 的模块名与失败帧），塞不进模板，所以中文直出
/// 原话、英文诚实承认没译。M395 把那些动态片段提成 core 的 `Finding::groups` 具名参数，
/// 于是这 10 条也进了目录 ⇒ **两种语言都走模板**，`system_text` 不再参与。
///
/// 为什么还留着「参数缺 ⇒ 退回原话」这一支：会话 JSONL 里落着**这次改动之前**写的结论
/// （`Candidate.groups` 那时对内置结论是空表）。照直渲染会画出 `{exit_code}` 字面量——
/// 那正是塔的硬门禁要防的「中文界面倒退」。缺参数时退回 core 的中文整句，行为与 M392 一致。
///
/// `groups` 必须代进去：目录里带占位符的那些条目（64 条规则里 16 条 + 10 条内置结论里 4 条）
/// 的模板带 `{sourcemod}` / `{error_lines}` 这类占位符，不代就画出字面的
/// `{sourcemod} 与 {destmod} 不能共存`——中文没了，也没变好。
///
/// `pub(super)`：`pages/logs/diagnostic_tests.rs` 要直接测它（拆出去是因为本文件撞 400 行红线）。
pub(super) fn texts_of(catalog: &Catalog, finding: &Finding) -> (String, Option<String>) {
    if let Some(keys) = diagnose_keys_from_cause(&finding.cause_key) {
        if is_builtin(&finding.rule) && !params_complete(catalog, keys.title, &finding.groups) {
            return core_text_of(catalog, finding);
        }
        let args = args_with_unknowns(catalog, finding);
        let title = catalog.render(keys.title, &args);
        let hint = keys.hint.map(|key| catalog.render(key, &args));
        return (title, hint);
    }

    // 目录没有条目：未来新加的、还没接线的结论。
    core_text_of(catalog, finding)
}

/// core 只给了中文原话时的兜底（口径见 [`texts_of`] 的表）。
///
/// 中文界面画原话（零倒退），英文界面画 `CoreTextUntranslated`（**诚实**：这句确实没译）。
fn core_text_of(catalog: &Catalog, finding: &Finding) -> (String, Option<String>) {
    let title = match catalog.lang() {
        Lang::ZhCn => finding
            .system_text
            .clone()
            .unwrap_or_else(|| catalog.text(Key::CoreTextUntranslated).to_owned()),
        Lang::En => catalog.text(Key::CoreTextUntranslated).to_owned(),
    };
    (title, None)
}

/// 引擎内置结论（`signal.*` / `hs_err.*`）：`core/src/diagnose/synthetic.rs` 产出的那 10 条。
///
/// 只有一处判据：它们的模板参数是 M395 才写进 `groups` 的，**缺了要退回原话**（老会话）。
/// 规则结论不设这一支（M392 的口径：老会话里没捕获组就画 `{sourcemod}` 字面量，登记在案）。
fn is_builtin(rule: &str) -> bool {
    rule.starts_with("signal.") || rule.starts_with("hs_err.")
}

/// core **可能不知道**的内置参数：空值 ⇒ 按语言补目录里的词。
///
/// 为什么不在 core 里直接填「未知模块」：那是中文。core 传空串 = 「我不知道」，
/// 词由界面按语言出（`unknown module` / `未知模块`）。
const UNKNOWN_ARGS: &[(&str, Key)] = &[
    ("module_name", Key::DiagnoseBuiltinUnknownModule),
    ("failed_frame", Key::DiagnoseBuiltinUnknownFrame),
];

/// 模板参数是否**齐**（只看在不在，不要求非空：`module_name` 空串是合法值 = 不知道）。
///
/// 只给内置结论用：它们的参数是 M395 才写进 `groups` 的，老会话缺这一层。
fn params_complete(catalog: &Catalog, key: Key, groups: &BTreeMap<String, String>) -> bool {
    catalog.get(key).is_some_and(|template| {
        placeholders(template)
            .iter()
            .all(|name| groups.contains_key(*name))
    })
}

/// `groups` → 目录模板的渲染参数，顺手把 [`UNKNOWN_ARGS`] 里的空值补成词。
fn args_with_unknowns<'a>(catalog: &Catalog, finding: &'a Finding) -> Vec<(&'a str, &'a str)> {
    finding
        .groups
        .iter()
        .map(|(name, value)| {
            let value = if value.is_empty() {
                UNKNOWN_ARGS
                    .iter()
                    .find(|(param, _)| param == name)
                    .map_or("", |(_, key)| catalog.text(*key))
            } else {
                value.as_str()
            };
            (name.as_str(), value)
        })
        .collect()
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

/// 严重度档 → 分组标题的 key。
///
/// M394 起分组依据是 core 的 `Severity`，标题就是严重度本身（`CRITICAL` / `WARNING` / `NOTICE`）。
/// 旧的 `LogsConfidence*` 三 key 随之退场——**分组头的名字跟着分组轴走**，留着一套按置信度命名的
/// 组头只会让下一个人以为分组还是置信度。
fn key_of(severity: Severity) -> Key {
    match severity {
        Severity::Critical => Key::LogsSeverityCritical,
        Severity::Warning => Key::LogsSeverityWarning,
        Severity::Info => Key::LogsSeverityInfo,
    }
}

/// 令牌排版档 + **按词或字形折行**（长堆栈里的点分标识符没有空格，只按词会溢出）。
///
/// 吃**自己拥有**的串：结论文案是目录渲染出来的模板（已经代入过捕获组），是新 `String`，
/// 借用的 `&'a str` 版本在这里活不过函数。
fn wrapped_owned<'a>(
    ui: &Ui,
    role: TypeRole,
    content: String,
    color: Color,
) -> Element<'a, Message> {
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
