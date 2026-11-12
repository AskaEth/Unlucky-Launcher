//! 向导卡的外壳与页脚（M223 从 `app/adopt.rs` 搬出，守 400 行硬线）。
//!
//! | 函数 | 职责 |
//! |---|---|
//! | [`card`] | 卡壳（标题 + 说明 + 中间区 + 页脚）、定高/收高两种几何、遮罩上的那一张卡 |
//! | [`title_text`] / [`subtitle`] | 四段的标题与一句说明（文案全部来自 i18n 目录） |
//! | [`footer`] | 按段给按钮组合；在等回程时主按钮禁用 |
//!
//! ## 这个文件不是「纯搬移」
//!
//! 它**起于**一次搬移：`app/adopt.rs` 落笔 412 行越过 `components::gate` 的 400 行硬线，
//! 于是把卡壳 / 标题 / 页脚搬到这里。但**同一个提交里**这三块还改过行为与几何，逐条如下
//! （复审对 diff 时按这几条看，别按「一行未改」看）：
//!
//! | 改了什么 | 为什么 |
//! |---|---|
//! | `card` 拆成「定高 + 中间区可滚」（Pick / Failed）与「按内容收高」（Preview / Done）两种几何 | 原来只给 `max_height`，内容一高就被裁：页脚按钮与实例名整排不可见（出图时抓到） |
//! | `footer` 的「关闭」只在 Pick / Preview 出现 | Failed / Done 的主按钮就是关闭，再给一枚文字关闭就是两枚一模一样的按钮 |
//! | `subtitle` 的 Preview 文案改成「版本名 · 目标档」 | 原来打「隔离档 · 目标档」，同屏两个「共用目录」 |
//!
//! 调用方仍是 `app/adopt.rs` 的 `layer` 与 `middle`。

use iced::widget::{column, container, row, scrollable, Space};
use iced::{Background, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space as Sp;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{AdoptStage, AdoptState};

use crate::app::components::buttons::{self, Button};
use crate::pages::frame;
use crate::theme::Ui;

use super::super::message::Message;
use super::text::{target_label, wrapped};
use super::{adopt, middle, AdoptMessage, Form, CARD_MAX_HEIGHT, CARD_WIDTH};

/// 卡片本体：标题 + 一句说明 + 中间（按段） + 页脚按钮。
pub(super) fn card<'a>(
    ui: &'a Ui,
    form: &'a Form,
    adopt_state: &'a AdoptState,
    catalog: Catalog,
) -> Element<'a, Message> {
    let stage = form.stage(adopt_state);
    let middle = middle(ui, form, adopt_state, stage, catalog);
    let head = column![
        frame::text_role(
            ui,
            TypeRole::HeadlineSmall,
            true,
            title_text(stage, catalog),
            ui.color(Role::OnSurface),
        )
        .map(Message::Page),
        wrapped(
            ui,
            subtitle(stage, adopt_state, catalog),
            ui.color(Role::OnSurfaceVariant)
        ),
        frame::divider(ui).map(Message::Page),
    ]
    .spacing(ui.space(Sp::X3))
    .width(Length::Fill);

    let tail = column![
        frame::divider(ui).map(Message::Page),
        footer(ui, form, adopt_state, stage, catalog),
    ]
    .spacing(ui.space(Sp::X3))
    .width(Length::Fill);

    // 列表段（Pick / Failed）**定高 + 中间区可滚**：内容再长，页脚按钮也不会被裁掉
    // （第一版就是被裁掉的）。短内容段（Preview / Done）按内容高收，不留一大片空白。
    let list_stage = matches!(stage, AdoptStage::Pick | AdoptStage::Failed);
    let card = if list_stage {
        let body = column![
            head,
            scrollable(middle)
                .height(Length::Fill)
                .style(frame::scroll_style(ui)),
            tail,
        ]
        .spacing(ui.space(Sp::X3))
        .width(Length::Fill);
        container(body)
            .width(Length::Fixed(CARD_WIDTH))
            .height(Length::Fixed(CARD_MAX_HEIGHT))
    } else {
        let body = column![head, middle, tail]
            .spacing(ui.space(Sp::X3))
            .width(Length::Fill);
        container(body)
            .width(Length::Fixed(CARD_WIDTH))
            .max_height(CARD_MAX_HEIGHT)
    };

    card.padding(ui.space(Sp::X6))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.layer(MaterialLayer::Overlay))),
            border: ui.outlined(ShapeScale::ExtraLarge),
            ..container::Style::default()
        })
        .into()
}

/// 段标题（四段各一句）。
pub(super) fn title_text(stage: AdoptStage, catalog: Catalog) -> &'static str {
    match stage {
        AdoptStage::Pick => catalog.text(Key::AdoptTitle),
        AdoptStage::Preview => catalog.text(Key::AdoptPlanTitle),
        AdoptStage::Failed => catalog.text(Key::AdoptFailureTitle),
        AdoptStage::Done => catalog.text(Key::AdoptDoneTitle),
    }
}

/// 标题下那句说明（Pick 段说清「什么都不复制」；Done 段说清「去实例列表启动」）。
pub(super) fn subtitle(stage: AdoptStage, adopt_state: &AdoptState, catalog: Catalog) -> String {
    match stage {
        AdoptStage::Pick => catalog.text(Key::AdoptSubtitle).to_string(),
        // 预检：版本名 · 目标档（隔离档在正文里单独一行，重复一遍只会更吵）
        AdoptStage::Preview => adopt_state
            .plan
            .as_ref()
            .map(|plan| format!("{} · {}", plan.name, target_label(plan.target, catalog)))
            .unwrap_or_default(),
        AdoptStage::Failed => adopt_state
            .failure
            .as_ref()
            .map(|failure| failure.message.clone())
            .unwrap_or_default(),
        AdoptStage::Done => catalog.text(Key::AdoptDoneHint).to_string(),
    }
}

/// 页脚按钮（按段给不同组合；在等回程时主按钮禁用）。
pub(super) fn footer<'a>(
    ui: &'a Ui,
    form: &'a Form,
    adopt_state: &'a AdoptState,
    stage: AdoptStage,
    catalog: Catalog,
) -> Element<'a, Message> {
    let busy = form.busy.waiting();

    let primary: Element<'a, Message> = match stage {
        AdoptStage::Pick => {
            let ready = form.candidate(adopt_state).is_some_and(|c| c.adoptable()) && !busy;
            if ready {
                buttons::filled(
                    ui,
                    Button::new(catalog.text(Key::AdoptPreview), adopt(AdoptMessage::Check)),
                )
            } else {
                buttons::filled(ui, Button::disabled(catalog.text(Key::AdoptPreview)))
            }
        }
        AdoptStage::Preview if !busy => buttons::filled(
            ui,
            Button::new(
                catalog.text(Key::AdoptConfirm),
                adopt(AdoptMessage::Confirm),
            ),
        ),
        AdoptStage::Preview => {
            buttons::filled(ui, Button::disabled(catalog.text(Key::AdoptConfirm)))
        }
        // 失败 / 完成：主按钮就是关掉（Done 不用「去列表看」——实例已经在列表里了）
        AdoptStage::Failed | AdoptStage::Done => buttons::filled(
            ui,
            Button::new(catalog.text(Key::AdoptClose), adopt(AdoptMessage::Close)),
        ),
    };

    let mut actions = row![].spacing(ui.space(Sp::X2)).width(Length::Fill);
    // Preview / Failed 给一条「退回候选列表」（= 重新扫一次，代价只有一次读目录）
    if matches!(stage, AdoptStage::Preview | AdoptStage::Failed) {
        actions = actions.push(buttons::text(
            ui,
            Button::new(catalog.text(Key::AdoptBack), adopt(AdoptMessage::Back)),
        ));
    }
    actions = actions.push(container(Space::new()).width(Length::Fill));
    // 失败/完成段的主按钮就是「关闭」，再给一枚文字「关闭」就是两枚一模一样的按钮
    if matches!(stage, AdoptStage::Pick | AdoptStage::Preview) {
        actions = actions.push(buttons::text(
            ui,
            Button::new(catalog.text(Key::AdoptClose), adopt(AdoptMessage::Close)),
        ));
    }
    actions = actions.push(primary);

    column![actions].width(Length::Fill).into()
}
