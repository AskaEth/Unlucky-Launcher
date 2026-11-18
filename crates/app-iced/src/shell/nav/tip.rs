//! M3 气泡 tooltip（导航轨专用）：折叠态里图标是唯一的可见标识，可读名只能靠它。
//!
//! 视觉与 `pages/agent/panel.rs` / `pages/agent/strip.rs` 那两处一致（`Overlay` 层底 +
//! `Small` 圆角 + `BodySmall` 字号）。**没有抽公共件**：那两个文件不在 M300 的 scope 里，
//! 硬抽要动别人的模块；三份拷贝已作为 finding 报给 tower，去重单独立项。

use iced::advanced::text::IntoFragment;
use iced::widget::tooltip::Position;
use iced::widget::{container, text, tooltip};
use iced::{Background, Border, Element, Padding};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

/// 把一个元素包成「悬停出气泡」的形态。`Position::Right`：轨贴窗口左缘，气泡只能往右弹。
///
/// 文案接受 `impl IntoFragment`：导航项的标签是 `&'static str`（目录里的词），而账户名是这一帧
/// 现拼的 `String`（`RailAccount` 是临时值，借它的 `&str` 会让整棵树跟着那个临时值活）。
pub(super) fn tip<'a, M: 'a, S>(
    ui: &'a Ui,
    content: Element<'a, M>,
    text_value: S,
) -> Element<'a, M>
where
    S: IntoFragment<'a>,
{
    let style = ui.style(TypeRole::BodySmall);
    let bubble = container(
        text(text_value)
            .size(style.size_px)
            .font(ui.font_of(style))
            .color(ui.color(Role::OnSurface)),
    )
    .padding(Padding::from([ui.space(Space::X1), ui.space(Space::X2)]))
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ui.layer(MaterialLayer::Overlay))),
        border: Border {
            radius: ui.radius_dp(ShapeScale::Small),
            ..Border::default()
        },
        ..container::Style::default()
    });
    tooltip(content, bubble, Position::Right).into()
}
