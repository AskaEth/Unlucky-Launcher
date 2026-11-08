//! 实例页：**list-detail** 的唯一用例，也是 M90 唯一打通到「真组件 + 真数据 + 真命令」的页。
//!
//! ## 版式（由 [`super::layout`] 决议，页面不自己算宽度）
//!
//! | 版面 | 结构 | 退化行为 |
//! |---|---|---|
//! | `ListDetail`（窗口 ≥840） | 左列 336 固定、**独立滚动**；右列详情，散文 ≤424 | — |
//! | `SingleColumn`（<840） | 列表与详情**整页切换**：本页只渲染详情列（块贴左吃满页宽，散文 ≤424） | 顶部给「Back to instances」，列表退成上一屏 |
//!
//! 为什么窄窗不是「上下堆」：官方 compact 下 list-detail 的口径是一次只显示一个 pane（列表 → 详情，
//! 整页切换）。上下堆会把详情挤到折叠线以下，等于详情不存在。
//!
//! ## M90：占位控件 → 真组件（逐项对照）
//!
//! | 位置 | M84（占位） | M90（真组件） |
//! |---|---|---|
//! | 列表行 | 手搓 `container` + `text_role`（行高 56） | `navigation::list_item`（一/两/三行 44/60/76，五态状态层） |
//! | 页头动作 | `frame::text_action`（纯 `text` + 手搓 hover） | `buttons::text`（`Button` 五式 + 状态层） |
//! | 「Play」 | `frame::text_action` | `buttons::filled`（**全屏唯一实心 accent**，rubric K2） |
//! | 分隔线 | `frame::divider` | `divider::full` |
//! | 数据 | `DEMO` 常量 | [`super::model::Instances`]（真页面由 presenter 快照转，演示由 `DEMO` 转） |
//!
//! 判据没有因此放松：列宽仍由 [`super::layout`] 给（见 `pages/tests.rs` 的 424 断言）。
//!
//! ## 行高为什么从 56 变成 60（M90 登记）
//!
//! M84 的行高是「`row_height` 44 + `Space::X3` 12 = 56」的手工值；真 `ListItem` 的两行档是
//! **60**（令牌 `row_height` 44 + 每多一行 16，A1 是 72）。差异来自组件自己的几何真源，
//! 页面不许覆盖——这正是「接真组件」的应有结果。列表仍高于可视区，独立滚动条照样可见。
//!
//! ## 行距归零 + 底部内缩（M102，视觉复审 ②）
//!
//! 视觉复审在 1180×760 的截图里抓到「末行 Redstone Lab 被卡片底边切掉半行（副标题消失）」。
//! 根因是**两道页级附加量**：`Space::X1` 的行距让行节奏变成 64，与可视高度除不尽；
//! 列表滚动区贴到卡片的 1px 圆角描边上，切点正好落在描边线上。修法两件：
//!
//! | 改什么 | 值 | 结果 |
//! |---|---|---|
//! | 页级行距 | 4 → **0**（真源 [`ROW_SPACING`] = `None`） | 行节奏回到组件的 60 |
//! | 滚动区底部内缩 | 0 → **`Space::X2` 8**（真源 [`LIST_BOTTOM_INSET`]） | 切点落在卡片的内边距带里，不压描边 |
//!
//! 于是可视区 = 580 − 列头 32 − 内缩 8 = **540 = 9 × 60**，正好 9 整行：末行完整可见，
//! 第 10 行整体落在裁切线之外。两个数都**只有一个真源**，页面与护栏都从它取
//! （`instances/tests.rs` 引 [`LIST_BOTTOM_INSET`].`dp()` 与 [`ROW_SPACING`] 算 pitch，
//! 另有一条源码结构断言盯着 `list_column()` 不许再叠字面量间距）。
//! 列表内容 12×60 = 720 仍高过可视区，独立滚动条照样可见。

mod data;
mod demo;
mod detail;
mod launch_args;
mod state;
mod text;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use data::{DEMO, NOTE, SELECTED};
pub(crate) use demo::{demo, demo_form};
pub use state::{LaunchForm, Message as InstanceMessage};
pub(super) use text::Texts;

use iced::widget::{column, container, row, scrollable};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::app::components::buttons::{self, Button};
use crate::app::components::interaction::Interaction;
use crate::app::components::navigation::list_item::{list_item, ListItem};
use crate::theme::Ui;
use unlucky_presenter::router::Route;

use super::frame::{self, Header};
use super::layout::{FrameMetrics, PageLayout};
use super::model::{Instances, Model, Row};
use super::{Action, Message};

/// 列表行距：**`None` = 这一层不加间距**（页面与护栏**共用这一个真源**）。
///
/// M102 的 ② 就是页级附加量：`ListItem` 自己的 60 高已含上下 8px 内边距，页面再叠一道
/// `Space::X1`(4) 就把行节奏变成 64、与可视高度除不尽（1180×760 下末行被切半行）。
/// 所以行距不写成字面量、也不散落在 `list_column` 里，而是这个常量——
/// 页面 `match` 它，`instances/tests.rs` 也从它算 pitch（改这里，测试立刻红）。
pub(super) const ROW_SPACING: Option<Space> = None;

/// 列表滚动区的**底部内缩**：让裁切线落在卡片的内边距带里，而不是压在 1px 描边上。
///
/// 与 [`ROW_SPACING`] 是一对（行距管「切在哪」、内缩管「切在什么底色上」），
/// 两个数都由 `instances/tests.rs` 的几何断言直接引用。
pub(super) const LIST_BOTTOM_INSET: Space = Space::X2;

/// 整页（`block.detail = None` 时详情区显示空态）。
///
/// 文案全部走 [`Texts`]（`model.lang` 的语言）：这个函数是实例页唯一的取词入口，
/// 页面里不留任何可见英文字面量（守门见 `instances/tests/i18n.rs`）。
pub fn view<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    model: &'a Model,
    form: &'a LaunchForm,
) -> Element<'a, Message> {
    let block = &model.instances;
    let forced = model.forced;
    let texts = Texts::of(model.lang.catalog());
    let header = Header::new(texts.title())
        .subtitle(texts.subtitle())
        .action(buttons::text(
            ui,
            forced_state(
                Button::new(texts.new_instance(), Message::Action(Action::NewInstance)),
                forced,
            ),
        ))
        .action(buttons::text(
            ui,
            forced_state(
                Button::new(texts.import(), Message::Action(Action::Import)),
                forced,
            ),
        ));

    let body = match metrics.layout {
        PageLayout::ListDetail => frame::panes(
            metrics,
            list_column(ui, block, forced, texts),
            match &block.detail {
                Some(detail) => detail::detail_column(ui, metrics, detail, form, forced, texts),
                None => detail::detail_empty(ui, texts),
            },
        ),
        _ => match &block.detail {
            Some(detail) => detail::narrow_detail(ui, metrics, detail, form, forced, texts),
            None => frame::body(metrics, detail::detail_empty(ui, texts)),
        },
    };

    frame::page(ui, metrics, header, body)
}

// --------------------------------------------------------------- 列表列

fn list_column<'a>(
    ui: &'a Ui,
    block: &'a Instances,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    let head = container(
        row![
            container(frame::text_role(
                ui,
                TypeRole::LabelMedium,
                false,
                texts.all_title(),
                ui.color(Role::OnSurfaceVariant),
            ))
            .width(Length::Fill)
            .align_y(Alignment::Center),
            frame::text_role_owned(
                ui,
                TypeRole::LabelMedium,
                false,
                block.rows.len().to_string(),
                ui.color(Role::OnSurfaceVariant),
            ),
        ]
        .width(Length::Fill)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding {
        top: ui.space(Space::X3),
        right: ui.space(Space::X3),
        bottom: ui.space(Space::X1),
        left: ui.space(Space::X3),
    });

    let mut rows = column(
        block
            .rows
            .iter()
            .map(|entry| list_row(ui, entry, is_selected(block, entry), forced))
            .collect::<Vec<_>>(),
    )
    .padding(Padding::from([0.0, ui.space(Space::X2)]))
    .width(Length::Fill);

    // 行距只从 `ROW_SPACING` 来（当前是 None = 不加间距）。别在这里写字面量 `.spacing(…)`：
    // 那正是视觉复审 ② 的那道页级附加量（`pages/tests.rs` 有一条源码结构断言盯着这段）。
    if let Some(spacing) = ROW_SPACING {
        rows = rows.spacing(ui.space(spacing));
    }

    // **独立滚动**：只有列表列这一条 scrollable，详情列不跟着滚。
    let scroller = scrollable(rows)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(frame::SCROLLBAR_WIDTH)
                .scroller_width(frame::SCROLLBAR_WIDTH)
                .margin(frame::SCROLLBAR_MARGIN),
        ))
        .height(Length::Fill)
        .style(frame::scroll_style(ui));

    // 底部内缩：裁切线落在卡片的内边距带里，而不是压在圆角描边上（M102 / 视觉复审 ②）。
    container(column![head, scroller].height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::default().bottom(ui.space(LIST_BOTTOM_INSET)))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.layer(MaterialLayer::Card))),
            border: Border {
                radius: ui.radius_dp(ShapeScale::Large),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn is_selected(block: &Instances, entry: &Row) -> bool {
    block
        .detail
        .as_ref()
        .is_some_and(|detail| detail.id == entry.id)
}

/// `--force-state` 的落点：只改**画出来的**状态，不动任何状态机（cli `ForcedState` 的同一条纪律）。
pub(super) fn forced_state<'a, M>(
    button: Button<'a, M>,
    forced: Option<Interaction>,
) -> Button<'a, M> {
    match forced {
        Some(state) => button.state(state),
        None => button,
    }
}

/// 一行（`ListItem` 的两/三行档：overline = 运行状态，headline = 名字，supporting = 版本 + 加载器）。
///
/// 点一行 = **压一层路由栈**（`Route::Detail(id)`），页面自己不留「选中项」副本——
/// 于是「当前选中」与「返回栈」永远是同一份真相（`presenter::router`）。
fn list_row<'a>(
    ui: &'a Ui,
    entry: &'a Row,
    selected: bool,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let mut item = ListItem::new(
        entry.name.as_str(),
        Message::Navigate(Route::Detail(entry.id.clone())),
    )
    .supporting(entry.supporting.as_str())
    .leading(IconKind::Instances)
    .trailing(IconKind::ChevronRight);

    if let Some(status) = &entry.status {
        item = item.overline(status.as_str());
    }
    if selected {
        item = item.selected();
    }
    if let Some(state) = forced {
        item = item.state(state);
    }

    list_item(ui, item)
}
