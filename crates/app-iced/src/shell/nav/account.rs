//! 导航轨**底部账户头像位**（M114）：状态结构体 + 那个按钮。
//!
//! 为什么单独一个文件：`shell/nav.rs` 贴着 400 行硬线，而这块是 M114 新增的一整块
//! （未登录/已登录两态、窄高收成图标条的判据、点击跳账户页）。

use iced::widget::{button, column, container, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Theme as IcedTheme};

use unlucky_design::color::{Role, StateLayer};
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::app::Message;
use crate::cli::ForcedState;
use crate::icons;
use crate::shell::overridden;
use crate::theme::Ui;

use super::{nav_item_height, nav_rail_width, NavItem};

/// 导航轨底部的**账户头像位**状态（M114）。
///
/// 为什么单独传而不是塞进 `NavItem`：它不是主序列里的第 6 项（那会让轨道越堆越长），
/// 而是固定在**底部**的一个头像位；未登录/已登录两态要在**形状上**就不同（描边剪影 vs 首字母块）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RailAccount {
    /// 是否已登录（至少有一个账户）
    pub signed_in: bool,
    /// 首字母块的字（已登录时有意义）
    pub initial: String,
    /// 账户名（已登录时作为标签；未登录时是 i18n 的「未登录」）
    pub label: String,
    /// 窄高度/窄宽下的图标条形态（隐藏文字标签，只留头像位）
    pub compact: bool,
}

impl RailAccount {
    /// 未登录态。
    pub fn signed_out(label: impl Into<String>) -> Self {
        Self {
            signed_in: false,
            initial: String::new(),
            label: label.into(),
            compact: false,
        }
    }

    /// 已登录态。
    pub fn signed_in(initial: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            signed_in: true,
            initial: initial.into(),
            label: label.into(),
            compact: false,
        }
    }

    /// 图标条形态（导航项太多/太窄时收起文字标签）。
    #[must_use]
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }
}

/// 轨底的账户头像位：未登录 = 描边剪影，已登录 = 首字母块（两态在**形状**上就不同）。
pub(super) fn account_item<'a>(
    ui: &'a Ui,
    selected: NavItem,
    account: &RailAccount,
    indicator: (f32, f32),
    forced: Option<ForcedState>,
) -> Element<'a, Message> {
    let (indicator_width, indicator_height) = indicator;
    let active = selected == NavItem::Account;
    let label_color = if active {
        ui.color(Role::Secondary)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };

    let avatar: Element<'a, Message> = if account.signed_in {
        let style = ui.emphasized(TypeRole::TitleMedium);
        container(
            text(account.initial.clone())
                .size(style.size_px)
                .line_height(iced::widget::text::LineHeight::Absolute(iced::Pixels(
                    style.line_height_px,
                )))
                .font(ui.font_of(style))
                .color(ui.color(Role::OnSecondaryContainer)),
        )
        .width(Length::Fixed(avatar_size(ui, indicator_width)))
        .height(Length::Fixed(avatar_size(ui, indicator_width)))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.color(Role::SecondaryContainer))),
            border: Border {
                radius: ui.radius(ShapeScale::Full, avatar_size(ui, indicator_width)),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
    } else {
        container(icons::view::<Message>(
            IconKind::User,
            ui.color(Role::OnSurfaceVariant),
        ))
        .width(Length::Fixed(avatar_size(ui, indicator_width)))
        .height(Length::Fixed(avatar_size(ui, indicator_width)))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme| container::Style {
            border: Border {
                color: ui.color(Role::Outline),
                width: 1.0,
                radius: ui.radius(ShapeScale::Full, avatar_size(ui, indicator_width)),
            },
            ..container::Style::default()
        })
        .into()
    };

    let height = nav_item_height(ui, indicator_height);
    let mut stack = column![avatar]
        .spacing(ui.space(Space::X1))
        .width(nav_rail_width(ui.metrics()))
        .align_x(Alignment::Center);
    if !account.compact {
        // 用**拥有的**字符串画标签：`RailAccount` 是调用方临时算出来的状态，
        // 借它的 `&str` 会让整棵树跟着那个临时值一起活（E0515）。
        let style = ui.style(TypeRole::LabelSmall);
        stack = stack.push(
            text(account.label.clone())
                .size(style.size_px)
                .line_height(iced::widget::text::LineHeight::Absolute(iced::Pixels(
                    style.line_height_px,
                )))
                .font(ui.font_of(style))
                .color(label_color),
        );
    }

    button(stack)
        .width(nav_rail_width(ui.metrics()))
        .height(height)
        .padding(0)
        .style(move |_theme: &IcedTheme, status| button::Style {
            background: match overridden(forced, status) {
                button::Status::Hovered if !active => Some(Background::Color(ui.state_over(
                    StateLayer::Hover,
                    Role::OnSurface,
                    Role::Surface,
                ))),
                button::Status::Pressed if !active => Some(Background::Color(ui.state_over(
                    StateLayer::Pressed,
                    Role::OnSurface,
                    Role::Surface,
                ))),
                _ => None,
            },
            text_color: label_color,
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        })
        .on_press(Message::Select(NavItem::Account))
        .into()
}

/// 头像位边长 = 指示器宽 − 16，但不超过 48（12b 里没有这个控件，按令牌栅格取整）。
fn avatar_size(ui: &Ui, indicator_width: f32) -> f32 {
    (indicator_width - Space::X4.dp()).min(ui.space(Space::X8) + Space::X2.dp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_states_are_distinguishable() {
        let out = RailAccount::signed_out("Not signed in");
        assert!(!out.signed_in);
        let on = RailAccount::signed_in("S", "Steve");
        assert!(on.signed_in);
        assert_eq!(on.initial, "S");
        let compacted = on.clone().compact(true);
        assert!(compacted.compact, "compact(true) 必须收成图标条");
    }

    #[test]
    fn the_avatar_slot_fits_the_rail() {
        use unlucky_design::color::SchemeFlavor;
        use unlucky_design::typography::Platform;

        use crate::theme::Families;

        let ui = Ui::with_platform(
            SchemeFlavor::Light,
            Platform::Windows,
            Families {
                text: "Segoe UI",
                display: "Segoe UI",
                mono: "Consolas",
            },
        );
        let width = super::super::indicator_size(ui.metrics()).0;
        let size = avatar_size(&ui, width);
        assert!(size > 0.0);
        assert!(
            size <= nav_rail_width(ui.metrics()),
            "头像位不能比导航轨还宽（{size} > {}）",
            nav_rail_width(ui.metrics())
        );
    }
}
