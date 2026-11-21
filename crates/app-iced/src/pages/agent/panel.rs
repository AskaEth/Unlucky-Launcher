//! 右栗展开档的**面板 + 拖拽缝**（M257 从 `mod.rs` 纯搬移，加上折叠过渡的裁切层）。
//!
//! ## 折叠过渡在这里落地的两件事
//!
//! 1. **宽度**：`rail_width(window, .., progress)` 逐帧给宽度（端点逐位等于静息值）；
//! 2. **内容**：会话视图按**目标宽**排版（每帧不重排 ⇒ 不抖），外面那层按**动画宽**裁切
//!    （⇒ 不溢出）。宽度缩到折叠档时本函数根本不会被调——`docked` 已经切成 48 的图标条
//!    （那 48 列贴在右栗最右端，位置不随宽度变，所以图标不跳）。
//!
//! 为什么不把内容也做成「淡出」：渲染器没有 opacity 原语（`motion/zoom.rs` 已登记：iced 0.14
//! 只有 clip / transformation），D27 又不许为此引入半透明面板。所以第二段是**裁切收尾 +
//! 换挡**，不是逐像素 α。这条在报告里明确划进「没做到的面」。

use iced::widget::{column, container, mouse_area, row, rule, text, tooltip};
use iced::{mouse, Alignment, Background, Border, Element, Length, Padding, Theme as IcedTheme};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};

use crate::app::components::motion;
use crate::theme::Ui;

use super::conversation::{self, Host};
use super::metrics::{gutter_at, rail_width, RESIZE_GUTTER};
use super::{spec, Message, Session};

/// 展开档的面板（外面包一层按动画宽裁切的容器）。
pub(super) fn panel<'a>(
    ui: &'a Ui,
    session: Session<'a>,
    catalog: Catalog,
    window_width: f32,
) -> Element<'a, Message> {
    let agent = session.agent;
    // 这一帧的宽度（过渡中）与目标宽度（静息）。两者只在过渡期间不同。
    let width = agent.rail_width(window_width);
    let settled = rail_width(window_width, agent.rail, agent.expanded(), 1.0);

    let body = container(conversation::view(
        ui,
        spec(session, catalog, Host::Rail),
        catalog,
    ))
    // **目标宽**：内容按最终可用宽排版，动画期间不重排（否则每帧折行 = 抖动）。
    .width(Length::Fixed(settled))
    .height(Length::Fill)
    .padding(Padding {
        top: ui.space(Space::X6),
        right: ui.space(Space::X4),
        bottom: ui.space(Space::X4),
        left: ui.space(Space::X4),
    })
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ui.layer(MaterialLayer::Canvas))),
        ..container::Style::default()
    });

    // 动画宽：只负责**裁切**（内容比它宽的部分画不出来 ⇒ 不溢出到中栏）。
    //
    // 为什么不用 `container(..).clip(true)`：iced 会把父级 max 传给子级，正文于是每帧重新折行
    // （第 02 张取证图实测：空态文案在过渡中折成两行）。[`motion::clipped`] 把「排版宽」与
    // 「裁剪框宽」拆开，过渡期间正文的折行**一次都不变**。
    let clipped = motion::clipped(body, width, settled);

    // 拖拽缝跟着宽度走：宽度回到 48 时它是 0（那时右栗是图标条，没有可拖的东西）。
    if gutter_at(width) > 0.0 {
        row![splitter(ui, settled, catalog), clipped]
            .width(Length::Shrink)
            .height(Length::Fill)
            .into()
    } else {
        clipped
    }
}

/// 8px 拖拽缝：1px 分隔线居中 + 水平缩放光标 + 按下即开拖 + **可读标签**（tooltip）。
///
/// 复审 P3-5：这条 8px 的缝此前没有任何可读文字，而 `AgentResize` / `AgentWidthLabel` 两条 key
/// 在视图里一次没用（只在测试里被断言「不空」）——那是空头支票。现在 tooltip 同时给出
/// 「拖它做什么」（`AgentResize`）与「当前多宽」（`AgentWidthLabel`），两个 key 都真用上了。
fn splitter<'a>(ui: &'a Ui, width: f32, catalog: Catalog) -> Element<'a, Message> {
    let line = container(
        rule::vertical::<IcedTheme>(ui.metrics().divider_thickness).style(move |_theme| {
            rule::Style {
                color: ui.color(Role::OutlineVariant),
                radius: ui.radius_dp(ShapeScale::None),
                fill_mode: rule::FillMode::Full,
                snap: true,
            }
        }),
    )
    .width(Length::Fixed(RESIZE_GUTTER))
    .height(Length::Fill)
    .align_x(Alignment::Center);

    let grab = mouse_area(line)
        .interaction(mouse::Interaction::ResizingHorizontally)
        .on_press(Message::DragStart);

    let tip_style = ui.style(TypeRole::BodySmall);
    let pixels = format!("{width:.0}");
    let tip = container(column![
        text(catalog.text(Key::AgentResize))
            .size(tip_style.size_px)
            .font(ui.font_of(tip_style))
            .color(ui.color(Role::OnSurface)),
        text(catalog.render(Key::AgentWidthLabel, &[("width", pixels.as_str())]))
            .size(tip_style.size_px)
            .font(ui.font_of(tip_style))
            .color(ui.color(Role::OnSurfaceVariant)),
    ])
    .padding(Padding::from([ui.space(Space::X1), ui.space(Space::X2)]))
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ui.layer(MaterialLayer::Overlay))),
        border: Border {
            radius: ui.radius_dp(ShapeScale::Small),
            ..Border::default()
        },
        ..container::Style::default()
    });

    tooltip(grab, tip, tooltip::Position::Left).into()
}
