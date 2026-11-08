//! 页面骨架：**页头**（标题 + 副标题 + 可选操作区）+ **内容区**（列宽由 [`super::layout`] 决定）
//! + **空态位**。
//!
//! ## 边界
//!
//! 这里只放「页面级」的东西：页头、页边距、内容块、两列骨架、空态位的摆放。
//! 具体控件（卡片 / 分隔线 / 文本按钮 / 图标）在 [`pieces`]，它们是**占位实现**
//! （同一份令牌，没有完整状态机），等 M75 的 `components/` 家族合入后由另一条 mission 换掉。
//!
//! ## 对齐口径（M102）：页头标题左缘 = 正文块左缘
//!
//! 页头带与内容区共用同一个 `gutter`（`Padding::from([0.0, metrics.gutter])` / `padding(gutter)`），
//! 所以页头标题的左缘 = `rail 88 + gutter 24 = 112`。正文块必须从**同一条线**起：
//!
//! - 单列页用 [`body`]（贴左 + 宽 `layout::block::body_width` = pane 宽，封顶 1440）；
//! - 成段散文再套一层 [`prose`]（宽 `layout::block::prose_width` ≤424，保证一行 ≤72 字符）；
//! - 两列页的正文在 [`panes`] 的主列里，左缘 = `112 + 列表列 336 + 间隙 24 = 472`（有据的分栏，不是错位）。
//!
//! 旧 [`single_column`] 把 424 的块**居中**摆进 1044，于是左缘 422、标题 112——视觉复审的 ① 就是这个。
//! 它保留给设置页（`pages/settings/**`，几何归设置页线）。
//!
//! **这三个量都在 [`super::layout::block`]（纯函数 + 单测）**；[`body`] / [`prose`] 只是把
//! 「从哪个数建盒」写死在一处，`pages/tests.rs` 有两条**源码结构断言**盯着这两个盒子
//! （iced 的 Element 没有几何内省，量不到摆法，只能断言「照哪个数建的」）。
//!
//! ## 判据落点（`research/12b`）
//!
//! | 部件 | 参考 | 落点 |
//! |---|---|---|
//! | 页头 | 16.1 AppBar 高 56（登记偏离官方 64）/ 两行 84 | `LayoutMetrics::header_height` |
//! | 页头标题 | 16.4 `TitleLarge 22/28` | `Component::TopAppBar.spec().text` |
//! | 页头副标题 | 16.4 `LabelMedium 12/16` `OnSurfaceVariant` | 同上 `.support` |
//! | 空态 | 15.1 图标块 96×96 / `Large(16)`；15.2 标题 `HeadlineSmall 24`；15.3 副文案 `BodySmall 12` | `Component::EmptyState` |

mod pieces;

#[cfg(test)]
mod tests;

pub use pieces::{
    card, divider, empty_state, filled, gap, glyph, glyph_box, scroll_style, text_action,
    text_role, text_role_centered, text_role_owned, ICON_PX, ICON_SMALL_PX, SCROLLBAR_MARGIN,
    SCROLLBAR_WIDTH,
};

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::component_map::Component;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::layout::{self, block, FrameMetrics};
use super::Message;

/// 页头操作区之间的间隔（`Space::X2`）。
const HEADER_ACTION_GAP: f32 = Space::X2.dp();

/// 页头内容。
pub struct Header<'a> {
    /// 标题（`TitleLarge`）。
    pub title: &'a str,
    /// 副标题（`LabelMedium`，给了页头就是两行高）。
    pub subtitle: Option<&'a str>,
    /// 右侧操作区（text button 占位，数量 ≤3）。
    pub actions: Vec<Element<'a, Message>>,
}

impl<'a> Header<'a> {
    /// 单行页头。
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            subtitle: None,
            actions: Vec::new(),
        }
    }

    /// 加副标题（页头自动变两行高）。
    #[must_use]
    pub fn subtitle(mut self, subtitle: &'a str) -> Self {
        self.subtitle = Some(subtitle);
        self
    }

    /// 追加一个右侧操作。
    #[must_use]
    pub fn action(mut self, action: Element<'a, Message>) -> Self {
        self.actions.push(action);
        self
    }
}

/// 一整页：页头 + 内容区（页边距已由布局系统决定）。
pub fn page<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    header: Header<'a>,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let bar = header_bar(ui, metrics, header);
    let content = container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(metrics.gutter);

    column![bar, content]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// 页面正文的**可用高度**（窗口高 − 自绘标题栏 − 页头带 − 上下页边距）。
///
/// M161：内容库要靠它决定「矮窗口换版面」。三个被减掉的量都取自 [`page`]/[`header_bar`]
/// 用的**同一个**来源（`Ui::metrics` 与 [`FrameMetrics::gutter`]），不另抄字面量——
/// 谁改了页头高，这里跟着变。
#[must_use]
pub fn body_height(ui: &Ui, metrics: FrameMetrics, window_height: f32, has_subtitle: bool) -> f32 {
    let chrome = ui.metrics().titlebar_height
        + ui.metrics().header_height(has_subtitle)
        + 2.0 * metrics.gutter;
    (window_height - chrome).max(0.0)
}

fn header_bar<'a>(ui: &'a Ui, metrics: FrameMetrics, header: Header<'a>) -> Element<'a, Message> {
    let spec = Component::TopAppBar.spec();
    // 16.4：标题 TitleLarge（emphasized w500），副标题 LabelMedium + OnSurfaceVariant
    let mut titles = column![text_role(
        ui,
        spec.text,
        true,
        header.title,
        ui.color(Role::OnSurface)
    )]
    .spacing(ui.space(Space::X1));
    if let Some(subtitle) = header.subtitle {
        titles = titles.push(text_role(
            ui,
            spec.support.unwrap_or(TypeRole::LabelMedium),
            false,
            subtitle,
            ui.color(Role::OnSurfaceVariant),
        ));
    }

    let actions = row(header.actions)
        .spacing(HEADER_ACTION_GAP)
        .align_y(Alignment::Center);

    // 页头带也要有面板底色（M93）：否则「开背景图 + 面板 α<1」时标题/副标题直接压在图上，
    // 浅色主题的副标题（OnSurfaceVariant）对比度掉得厉害。底色同源于 `Ui::layer`（含面板 α）。
    container(
        row![titles.width(Length::Fill), actions]
            .width(Length::Fill)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(ui.metrics().header_height(header.subtitle.is_some()))
    .align_y(Alignment::Center)
    .padding(Padding::from([0.0, metrics.gutter]))
    .style(move |_theme| filled(ui.layer(MaterialLayer::Canvas)))
    .into()
}

/// 单列版面的正文列：宽 = 行宽上限内的实际列宽，**水平居中**。
///
/// > **M102 起产品页不再用它**：居中会把 424 摆到 1044 的正中（左缘 422 对页头标题 112），
/// > 视觉复审判为「内容缩在中间一条窄栏」。设置页（`pages/settings/**`）仍走这条，
/// > 它的几何归设置页线，本 mission 不动；下载 / 日志 / 窄窗实例改用 [`body`]。
pub fn single_column<'a>(
    metrics: FrameMetrics,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    centered(metrics.column, content)
}

/// **页面块宽度**：pane 宽（单列 = 可用宽度 / 两列 = 主列 pane）封顶 1440。
///
/// 真源在 [`super::layout::block::body_width`]（纯量 + 单测）；这里只是转发，**不另算一份**。
/// 与 [`FrameMetrics::column`]（一句话多宽，≤424）是两个量。
pub use super::layout::block::body_width;

/// **正文块**：从页边距起（= 页头标题那条左缘）、宽 [`body_width`]、左内边距 [`body_offset`]。
///
/// 盒子**从纯量建**：宽度取 [`body_width`]、左内边距取 [`super::layout::block::body_offset`]
/// （恒 0）、对齐 `Start`。这三件事的顺序就是契约，`pages/tests.rs` 的
/// `frame_body_is_built_from_the_block_quantities` 逐条扫这段源码（iced 的 Element 没有几何
/// 内省，量不到「摆在哪」，只能断言「照哪个数建的」）。
pub fn body<'a>(
    metrics: FrameMetrics,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(content)
        .width(Length::Fixed(body_width(metrics)))
        .height(Length::Fill)
        .align_x(Alignment::Start)
        .align_y(Alignment::Start)
        .padding(Padding::default().left(block::body_offset(metrics)))
        .into()
}

/// **散文块**：宽度锁在 [`super::layout::block::prose_width`]（= `metrics.column`，≤424 ⇒ 一行 ≤72 字符）。
///
/// 只包「成段读的文字」（实例详情正文那样的 prose）；卡片、列表、Tabs 吃的是 [`body_width`]，
/// 它们的宽度不承担行宽承诺（列宽只影响每行放几个字段，不影响读一句话的舒适度）。
pub fn prose<'a>(
    metrics: FrameMetrics,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(content)
        .width(Length::Fixed(block::prose_width(metrics)))
        .into()
}

/// 两列版面：左列 `secondary`（独立滚动区由调用方自己加 `scrollable`）+ 右列 `main`。
///
/// 两列都**显式**取布局系统给的宽度，不吃 Fill：列表列必须固定 336，
/// 详情列必须窄于可用宽度，否则又回到「全宽单卡」。
pub fn panes<'a>(
    metrics: FrameMetrics,
    secondary: impl Into<Element<'a, Message>>,
    main: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let width = metrics.secondary.unwrap_or(0.0);
    row![
        container(secondary)
            .width(Length::Fixed(width))
            .height(Length::Fill),
        container(main)
            .width(Length::Fixed(metrics.main_pane))
            .height(Length::Fill),
    ]
    .spacing(layout::PANES_GAP)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// 把内容塞进固定宽度并水平居中（`align_x` 不会覆盖子元素的宽度，见 Q8）。
pub fn centered<'a>(width: f32, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(container(content).width(Length::Fixed(width)))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Start)
        .into()
}

/// 与 [`centered`] 同宽，但**贴左**：列表右侧的详情列要贴着自己的 pane 左边，不再二次居中。
pub fn left_aligned<'a>(
    width: f32,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(container(content).width(Length::Fixed(width)))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Start)
        .align_y(Alignment::Start)
        .into()
}
