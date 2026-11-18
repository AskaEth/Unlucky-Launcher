//! 左侧导航轨（M3 `NavigationRail`）。
//!
//! 判据来源 `research/12b-component-fidelity.md` §5：
//!
//! | 项 | 参考值 | 令牌落点 |
//! |---|---|---|
//! | 轨宽 | 5.1 官方 96（narrow 80）；本项目 **88**（展开）/ **64**（M300 折叠） | [`nav_rail_width`] / [`nav_width`]（`LayoutMetrics` 骨架值，详见该函数文档的 M69 迁移说明） |
//! | 轨容器 | 5.3 形状 None / L0 / `Surface` | `MaterialLayer::Canvas` |
//! | 选中指示器 | 5.4 **56×32**、圆角 full、`SecondaryContainer` | [`indicator_size`]（令牌推导） |
//! | 图标 | 5.5 24px | `IconSize::Toolbar` |
//! | 标签 | 5.5 `LabelMedium 12/16` | `TypeRole::LabelMedium` |
//! | 颜色 | 5.6 选中图标 `OnSecondaryContainer` / 选中标签 `Secondary` / 未选中 `OnSurfaceVariant` | `Ui::color` |
//! | 项间距 | 5.7 **4** | `LayoutMetrics::nav_item_gap` |

use iced::widget::{button, column, container};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding, Shadow, Theme as IcedTheme,
};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Key, Lang};

mod account;
// M300：纯几何（两个宽度档的插值 + 药丸矩形），拆出去守 400 行门禁
mod geometry;
// M300：气泡 tooltip（折叠态的可读名靠它）
mod tip;
// M300：轨底的折叠按钮
mod toggle;

pub use account::RailAccount;

use account::account_item;
use toggle::toggle_button;

// 几何按**原名**再导出：`shell/tests.rs` 的 M283/M299 判据写的是 `super::nav::<名字>`，
// 搬文件不许改断言（见 `geometry.rs` 的模块文档）。
pub(super) use geometry::{indicator_size, nav_item_height, nav_width, state_layer_box};

use crate::app::components::interaction::{self, Interaction};
use crate::app::Message;
use crate::cli::ForcedState;
use crate::icons;
use crate::shell::{filled, label, overridden};
use crate::theme::Ui;

use tip::tip;

/// 导航占位项（任务要求 4~6 个；真正的页面由后续 mission 接管）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavItem {
    /// 实例列表。
    Instances,
    /// 版本管理（显示名；变体名按用户令「只改显示层」不改，标识符改名后置）。
    Download,
    /// 日志。
    Logs,
    /// 设置。
    Settings,
    /// AI 助手。
    Ai,
    /// 账户页（**不进主序列**：入口是轨底的头像位，见 [`RailAccount`]）。
    Account,
}

impl NavItem {
    /// 全部**主序列**项（顺序 = 导航栏自上而下）。账户不在其中：它固定贴在轨底。
    pub const ALL: [NavItem; 5] = [
        NavItem::Instances,
        NavItem::Download,
        NavItem::Logs,
        NavItem::Settings,
        NavItem::Ai,
    ];

    /// 显示名的 key（M106 起走 catalog；两套目录都有词由 `catalog` 的完整性测试守）。
    pub const fn label_key(self) -> Key {
        match self {
            NavItem::Instances => Key::NavInstances,
            NavItem::Download => Key::NavDownload,
            NavItem::Logs => Key::NavLogs,
            NavItem::Settings => Key::NavSettings,
            NavItem::Ai => Key::NavAi,
            // M114：轨底头像位的可访问名（不在主序列里，但 `router` 的 `nav()` 会问到它）。
            NavItem::Account => Key::AccountTitle,
        }
    }

    /// 显示名（按当前界面语言）。
    ///
    /// 为什么带 `lang` 而不是读全局：这个函数要被单测与 harness 调，全局状态会让「同一份代码在
    /// 不同语言下画什么」不可断言。
    pub fn label(self, lang: Lang) -> &'static str {
        lang.catalog().text(self.label_key())
    }

    /// 令牌图标。
    pub const fn icon(self) -> IconKind {
        match self {
            NavItem::Instances => IconKind::Instances,
            NavItem::Download => IconKind::Download,
            NavItem::Logs => IconKind::Logs,
            NavItem::Settings => IconKind::Settings,
            NavItem::Ai => IconKind::Ai,
            NavItem::Account => IconKind::User,
        }
    }
}

/// 整条导航轨。
///
/// ## 几何来源（M300 起有**两个**宽度档）
///
/// 轨宽 = [`RailAccount::nav_width`]：展开档 [`LayoutMetrics::nav_rail_width`]（88，带标签）与
/// 折叠档 [`LayoutMetrics::nav_collapsed_width`]（64，只留图标）之间按动效进度插值。折叠档的
/// 算式与「为什么不是 48」写在 `design/spacing.rs` 的 `nav_collapsed_width` 上。
///
/// ## 轨底那一摞（自下而上）
///
/// 账户头像位（M114，**永远贴底**）→ 折叠按钮（M300）→ 一段 `Fill` → 主序列项。
/// 两个 footer 元素都在 `Fill` 之下，所以它们的位置与页数、窗口高无关；折叠只改**宽**。
pub(super) fn rail<'a>(
    ui: &'a Ui,
    selected: NavItem,
    account: &RailAccount,
    forced: Option<ForcedState>,
    lang: Lang,
) -> Element<'a, Message> {
    let metrics = ui.metrics();
    let indicator = indicator_size(metrics);
    let base = ItemView {
        selected: false,
        compact: account.compact,
        collapsed: account.nav_collapsed(),
        width: account.nav_width(metrics),
    };

    let items = NavItem::ALL
        .iter()
        // `ALL` 本来就不含 `Account`（它固定在轨底），filter 只是把这条意图写明。
        .filter(|item| **item != NavItem::Account)
        .map(|item| {
            rail_item(
                ui,
                *item,
                ItemView {
                    selected: *item == selected,
                    ..base
                },
                forced,
                lang,
            )
        })
        .collect::<Vec<_>>();

    // 轨顶留白走 container padding，而不是「先塞一个 spacer 元素」：后者会被 column 的
    // spacing（4）再加一档，实测变 28（视觉复审 P3-5）。
    //
    // 轨底色与其它面板同源（M93）：`ui.layer` 已含面板 α，导航轨是面板层的一部分。
    //
    // 账户头像位**固定贴在轨底**（M114）：主序列四项在上，中间一段 Fill 把头像顶到底部——
    // 于是「账户」不是第 6 个导航项，轨的长度也不会随页数增长。
    container(
        column![
            column(items).spacing(metrics.nav_item_gap),
            iced::widget::space::vertical(),
            toggle_button(ui, account, forced, lang),
            account_item(ui, selected, account, indicator, forced),
        ]
        .height(Length::Fill),
    )
    .width(base.width)
    .height(Length::Fill)
    .padding(Padding::from([ui.space(Space::X6), 0.0]))
    .style(move |_theme| filled(ui.layer(MaterialLayer::Canvas)))
    .into()
}

/// 药丸底色（**几何**的唯一实现点）：hover / pressed / 过渡帧都画在这块 56×32 上。
///
/// **颜色**只有一个来源：选中支走 [`interaction::selected_fill`]（全仓唯一的选中底色入口，
/// M299 收口了这里的私有拷贝），未选中支走 [`interaction::layer_color`]。选中/未选中**两条分支
/// 不许合并**：选中项静息是实色 `SecondaryContainer`（`selected_fill` 的兜底），未选中项静息
/// 不画药丸（`layer_color` 给 `None`）。两条都经状态层求值 ⇒ 没有取样控件时**逐位**等于原来的
/// `state_over(..)` / `ui.color(..)`（`--force-state=rest` 的取证图因此一字不变）。
pub(in crate::shell) fn pill_color(ui: &Ui, state: Interaction, selected: bool) -> Option<Color> {
    if selected {
        // 选中项走**全仓唯一的**选中底色入口（M299）：静息 = 实色 `SecondaryContainer`，
        // hover / pressed / 过渡帧叠在它上面。此前这里有一份私有拷贝，M296 的变异实验证明
        // 它不跟助手一起动。
        Some(interaction::selected_fill(
            ui,
            state,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer,
        ))
    } else {
        // 导航轨自己画的就是 `MaterialLayer::Canvas`（见 [`rail`] 的 `.style(..)`）：状态层要叠在
        // **同一个**角色上，否则 hover 色与轨底色对不上。
        interaction::layer_color(
            ui,
            state,
            Role::OnSurface,
            MaterialLayer::Canvas.surface_role(),
        )
    }
}

/// 一项导航项这一帧的样子（M300 抽出来：折叠态多了「可见性」与「实时轨宽」两件）。
///
/// 用结构体而不是继续加形参：`rail_item` 原本已经 6 个形参（clippy 的上限是 7），折叠要再加两个。
/// 把这些值捆起来的另一个好处是「同一项的四处几何读的是同一拍的值」。
#[derive(Debug, Clone, Copy)]
pub(super) struct ItemView {
    /// 是否是当前页（选中指示器 / 实色胶囊）。
    pub(super) selected: bool,
    /// 窄高窗口把标签收掉（M114，窗口高 < 720）。
    pub(super) compact: bool,
    /// 用户主动折叠（M300）：只留图标 + tooltip。
    pub(super) collapsed: bool,
    /// 这一帧的轨宽（动效中间帧既不等于 88 也不等于 64）。
    pub(super) width: f32,
}

impl ItemView {
    /// 文字标签这一帧画不画（两种收法：窄高 / 用户折叠）。
    ///
    /// **判据写在一处**：`rail_item` 的标签与 tooltip 两支、以及 `shell` 的用例都读它，于是
    /// 「折叠态没有标签」与「折叠态的图标有可读名」不会各判一次而漂。
    pub(super) const fn labels_visible(self) -> bool {
        !self.compact && !self.collapsed
    }
}

pub(super) fn rail_item<'a>(
    ui: &'a Ui,
    item: NavItem,
    view: ItemView,
    forced: Option<ForcedState>,
    lang: Lang,
) -> Element<'a, Message> {
    let selected = view.selected;
    // 状态层（hover / pressed / 过渡帧）落在**药丸**上，与选中指示器同一块几何（M283）。
    let (pill_width, pill_height) = state_layer_box(ui.metrics());
    // 取证态与运行期态是**同一个** `Interaction`：强制态由 `overridden` 直接推（`None` → `Active`
    // → `Enabled`），运行期的真态由 `interaction::tracked` 的取样层在 draw 里给。
    let state = Interaction::from_status(overridden(forced, button::Status::Active));

    let icon_color = if selected {
        ui.color(Role::OnSecondaryContainer)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };
    // 12b 5.6：选中标签用 `Secondary`，未选中 `OnSurfaceVariant`
    let label_color = if selected {
        ui.color(Role::Secondary)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };

    // **不要用 `center_x/center_y(Fill)`**：那会把容器自己的 width/height 改成 Fill，
    // 于是 56 被父级（button 宽 88）覆盖成 88、32 靠布局凑巧（视觉复审 P2-1）。
    // 尺寸只认令牌推导值，居中交给父 column 的 `align_x`。
    let pill = container(icons::view::<Message>(item.icon(), icon_color))
        .width(pill_width)
        .height(pill_height)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme| container::Style {
            // 状态层画在**药丸**上（M283）：hover / pressed / 过渡帧都只这一块 56×32。
            // 之前它摊在 88×52 的按钮上（`Border::default()` ⇒ r=0），同一个元素并存两种形状。
            background: pill_color(ui, state, selected).map(Background::Color),
            border: Border {
                radius: ui.radius(ShapeScale::Full, pill_height),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let item_height = nav_item_height(ui, pill_height);
    let height = if view.compact {
        pill_height
    } else {
        // M300：折叠**不改纵节奏**（项高仍是 52）——否则切换时图标会上下跳，而 52 ≥ 命中下限 48。
        item_height
    };
    // 列必须显式占满轨宽，否则它会 shrink 到 56，`align_x(Center)` 就变成空操作，
    // 药丸会贴左缘（x=0..55 而不是 16..71）——r1 实测踩到的第二个坑。
    let mut stack = column![pill]
        .width(view.width)
        .align_x(Alignment::Center)
        .height(height);
    if view.labels_visible() {
        // M114 的「窄高收成图标条」+ M106 的「按语言取词」并存。
        stack = stack.push(label(
            ui,
            TypeRole::LabelMedium,
            false,
            item.label(lang),
            label_color,
        ));
    }
    stack = stack.spacing(ui.space(Space::X1));

    let clickable: Element<'a, Message> = button(stack)
        .width(view.width)
        .height(height)
        .padding(0)
        .style(move |_theme: &IcedTheme, _status| button::Style {
            // 底色**恒为 `None`**：状态层由药丸给（M283，同一个元素不许并存两种形状）。
            background: None,
            text_color: label_color,
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        })
        .on_press(Message::Select(item))
        .into();
    // A8：运行期态套状态取样控件（强制态 = 取证，**不套**：取证图不许有过渡中间帧）。
    let clickable = if forced.is_none() {
        interaction::tracked(clickable, true)
    } else {
        clickable
    };
    // M300：标签不画的时候，图标是这一项**唯一**的可见标识 ⇒ 必须有可读名，否则折叠态就是一列
    // 无名符号（用户点名不许）。tooltip 是这条判据的落点（与 `pages/agent` 同一条纪律）。
    if view.labels_visible() {
        clickable
    } else {
        tip(ui, clickable, item.label(lang))
    }
}

#[cfg(test)]
mod tests;
