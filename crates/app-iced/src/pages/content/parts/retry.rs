//! 安装失败后的「只重试这些」面板（M191）。
//!
//! ## 为什么这块画在内容库，而不是复用下载页那块失败面板
//!
//! 下载页的失败面板讲的是「停在哪一步 + 打开日志」，而这里要讲的是**哪几项没装上、属于哪种
//! 没装上、以及能不能只重试它们**。两件事混在一个块里会得到「失败 + 一堆条目 + 两个关闭」。
//! 所以：有可重试名单时（[`RetryPanel::of`] 给了 `Some`）这块**顶替**通用失败面板，
//! 一屏只有一个失败块。
//!
//! ## 三个控件行为
//!
//! | 控件 | 消息 | 落点 |
//! |---|---|---|
//! | 只重试这 N 个 | [`ContentMessage::RetryFailed`] | `AppCommand::InstallContent{retry: true}`（host 只重解失败项） |
//! | 关闭 | [`PageMessage::CancelInstall`] | 清 install 快照（对没在跑的安装是安全 no-op，与下载页同一个出口） |
//!
//! 两个按钮都**按不动就不画**：没有名单时整块不存在（不是画一个灰按钮）。

use iced::widget::{column, container, row};
use iced::{Alignment, Background, Border, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::theme::Ui;

use super::super::model::{Content, RetryPanel};
use super::intent;
use crate::pages::content::Message as ContentMessage;
use crate::pages::frame;
use crate::pages::Message as PageMessage;

/// 整块面板（`model.retry` 为 `None` 时调用方不该调它——判据在模型里，不在这里）。
pub(crate) fn retry_panel<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    panel: &'a RetryPanel,
) -> Element<'a, PageMessage> {
    let head = row![
        frame::glyph(IconKind::Error, ui.color(Role::Error), frame::ICON_PX),
        frame::text_role(
            ui,
            TypeRole::TitleSmall,
            true,
            panel.title.as_str(),
            ui.color(Role::Error),
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center);

    let mut body = column![head]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill);

    // 两档分成两段：**各自的标题 + 各自的条目**（合并成一段就说不出「换版本还是检查网络」）。
    // 条目之间用 ASCII 逗号：项目键是站点数据，句读不是文案，不该在这里挑中文的「、」。
    for group in &panel.groups {
        let items = group.items.join(", ");
        body = body.push(
            column![
                frame::text_role(
                    ui,
                    TypeRole::LabelMedium,
                    true,
                    catalog.text(group.title),
                    ui.color(Role::OnErrorContainer),
                ),
                frame::text_role_owned(
                    ui,
                    TypeRole::BodySmall,
                    false,
                    items,
                    ui.color(Role::OnErrorContainer),
                ),
            ]
            .spacing(ui.space(Space::X1)),
        );
    }

    body = body.push(frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        catalog.text(Key::ContentInstallRetryHint),
        ui.color(Role::OnErrorContainer),
    ));

    let actions = row![
        // 主按钮：只重试这些（这个块存在的**唯一**理由）
        buttons::filled(
            ui,
            Button::new(panel.action.as_str(), intent(ContentMessage::RetryFailed))
                .leading(IconKind::Refresh),
        ),
        buttons::text(
            ui,
            // 复用下载页那条现有的「关闭」：同一句话不另立一条要同步的 key（r1 复审 N2）
            Button::new(
                catalog.text(Key::DownloadDismiss),
                PageMessage::CancelInstall
            ),
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center);

    body = body.push(actions);

    container(body)
        .width(Length::Fill)
        .padding(ui.space(Space::X4))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.color(Role::ErrorContainer))),
            border: Border {
                radius: ui.radius_dp(ShapeScale::Large),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// 整页底部那块（判据在 [`Content::retry`]；`None` ⇒ 一帧都不画）。
pub(crate) fn maybe<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
) -> Option<Element<'a, PageMessage>> {
    model
        .retry
        .as_ref()
        .map(|panel| retry_panel(ui, catalog, panel))
}
