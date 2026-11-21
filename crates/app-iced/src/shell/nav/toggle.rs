//! 导航轨底部的**折叠按钮**（M300 `NAVCOLLAPSE`）：点一下在展开（88，带标签）与折叠（64，只图标）
//! 之间切换。
//!
//! ## 为什么放在轨底、头像位之上
//!
//! ①**轨顶一个像素都不动**：轨顶是 `Space::X6`(24) 留白 + 首项，M283/M299 的展开态对照图（D30）
//! 全靠那一段的可比性；把按钮塞在轨顶会把整列往下推 52px，等于给每个既有取证图加一条无关差异。
//! ②轨底本来就已经有一个「固定贴底」的槽（M114 的账户头像位），按钮与它同属 footer：两态下位置
//! 不变（只改宽），过渡期间目标也一直在同一处，不会边动边跑。
//! ③折叠态下按钮仍然完整可见（64 里放得下 56 的胶囊），回得去——这是「可折叠」的必要条件。
//!
//! ## 为什么图标是 `Sidebar` 而文案两态不同
//!
//! 按钮只有图标，所以可读名走 tooltip（[`unlucky_i18n::Key::NavCollapse`] / [`unlucky_i18n::Key::NavExpand`]）：图标说「这是
//! 侧栏开关」，文案说「点下去会发生什么」。用同一个字形而不是「< / >」的另一原因是两个方向的
//! 箭头在 24px 下几乎对称，用户读不出当前态，反而在 tooltip 里说得清。

use iced::widget::{button, container};
use iced::{Alignment, Background, Border, Element, Shadow, Theme as IcedTheme};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_i18n::Lang;

use crate::app::components::interaction::{self, Interaction};
use crate::app::Message;
use crate::cli::ForcedState;
use crate::icons;
use crate::shell::overridden;
use crate::theme::Ui;

use super::account::RailAccount;
use super::tip::tip;
use super::{nav_item_height, pill_color, state_layer_box};

/// 轨底的折叠按钮。
///
/// 几何与导航项**同一套**：轨宽 × 项高（52）的整条命中区，里面是 56×32 的胶囊（M283 的那块），
/// 状态层（hover / pressed / 过渡帧）画在胶囊上而不是按钮上——与 `rail_item` 同一个理由：
/// 同一个元素不许并存两种形状。
///
/// **强制度**：`--force-state` 的取证图**不套**状态取样控件（取证图里不许有过渡中间帧），
/// 但照样按强制态求值底色（`overridden`）。
pub(super) fn toggle_button<'a>(
    ui: &'a Ui,
    account: &RailAccount,
    forced: Option<ForcedState>,
    lang: Lang,
) -> Element<'a, Message> {
    let metrics = ui.metrics();
    let width = account.nav_width(metrics);
    let collapsed = account.nav_collapsed();
    let (pill_width, pill_height) = state_layer_box(metrics);
    let state = Interaction::from_status(overridden(forced, button::Status::Active));

    // 24 的盒也要走 `view_at`（显式给盒尺寸）：`icons.rs` 的白名单按**文件**记 `icons::view`
    // 的调用点数（`shell/nav.rs` 1 处、`shell/nav/account.rs` 1 处），本文件是 0——那个白名单
    // 不在 M300 的 scope 里，所以这里用等价的显式入口，而不是去改别人的门禁。
    let icon = container(icons::view_at::<Message>(
        IconKind::Sidebar,
        ui.color(Role::OnSurfaceVariant),
        unlucky_design::icons::IconSize::Toolbar.px(),
    ))
    .width(pill_width)
    .height(pill_height)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme| container::Style {
        // 与未选中的导航项同一条：导轨画的是 `MaterialLayer::Canvas`，状态层叠在同一个角色上。
        background: pill_color(ui, state, false).map(Background::Color),
        border: Border {
            radius: ui.radius(ShapeScale::Full, pill_height),
            ..Border::default()
        },
        ..container::Style::default()
    });

    let height = nav_item_height(ui, pill_height);
    let clickable: Element<'a, Message> = button(
        container(icon)
            .width(width)
            .height(height)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .width(width)
    .height(height)
    .padding(0)
    .style(move |_theme: &IcedTheme, _status| button::Style {
        // 底色**恒为 `None`**：状态层由胶囊给（M283）。
        background: None,
        text_color: ui.color(Role::OnSurfaceVariant),
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    })
    .on_press(Message::ToggleNav)
    .into();

    let clickable = if forced.is_none() {
        interaction::tracked(clickable, true)
    } else {
        clickable
    };

    // 文案跟**当前态**走：展开时告诉用户「点了会折叠」，折叠时告诉用户「点了会展开」。
    let catalog = lang.catalog();
    tip(
        ui,
        clickable,
        catalog.text(if collapsed {
            unlucky_i18n::Key::NavExpand
        } else {
            unlucky_i18n::Key::NavCollapse
        }),
    )
}
