//! 左侧导航轨（M3 `NavigationRail`）。
//!
//! 判据来源 `research/12b-component-fidelity.md` §5：
//!
//! | 项 | 参考值 | 令牌落点 |
//! |---|---|---|
//! | 轨宽 | 5.1 官方 96（narrow 80）；本项目 **88** | [`nav_rail_width`]（`LayoutMetrics` 骨架值，详见该函数文档的 M69 迁移说明） |
//! | 轨容器 | 5.3 形状 None / L0 / `Surface` | `MaterialLayer::Canvas` |
//! | 选中指示器 | 5.4 **56×32**、圆角 full、`SecondaryContainer` | [`indicator_size`]（令牌推导） |
//! | 图标 | 5.5 24px | `IconSize::Toolbar` |
//! | 标签 | 5.5 `LabelMedium 12/16` | `TypeRole::LabelMedium` |
//! | 颜色 | 5.6 选中图标 `OnSecondaryContainer` / 选中标签 `Secondary` / 未选中 `OnSurfaceVariant` | `Ui::color` |
//! | 项间距 | 5.7 **4** | `LayoutMetrics::nav_item_gap` |

use iced::widget::{button, column, container};
use iced::{Alignment, Background, Border, Element, Length, Padding, Shadow, Theme as IcedTheme};

use unlucky_design::color::{Role, StateLayer};
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::{LayoutMetrics, Space};
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Key, Lang};

mod account;

pub use account::RailAccount;

use account::account_item;

use crate::app::Message;
use crate::cli::ForcedState;
use crate::icons;
use crate::shell::{filled, label, overridden};
use crate::theme::Ui;

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
pub(super) fn rail<'a>(
    ui: &'a Ui,
    selected: NavItem,
    account: &RailAccount,
    forced: Option<ForcedState>,
    lang: Lang,
) -> Element<'a, Message> {
    let metrics = ui.metrics();
    let indicator = indicator_size(metrics);
    let compact = account.compact;

    let items = NavItem::ALL
        .iter()
        // `ALL` 本来就不含 `Account`（它固定在轨底），filter 只是把这条意图写明。
        .filter(|item| **item != NavItem::Account)
        .map(|item| rail_item(ui, *item, *item == selected, indicator, compact, forced, lang))
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
            account_item(ui, selected, account, indicator, forced),
        ]
        .height(Length::Fill),
    )
    .width(nav_rail_width(metrics))
    .height(Length::Fill)
    .padding(Padding::from([ui.space(Space::X6), 0.0]))
    .style(move |_theme| filled(ui.layer(MaterialLayer::Canvas)))
    .into()
}

fn rail_item<'a>(
    ui: &'a Ui,
    item: NavItem,
    selected: bool,
    indicator: (f32, f32),
    compact: bool,
    forced: Option<ForcedState>,
    lang: Lang,
) -> Element<'a, Message> {
    let (indicator_width, indicator_height) = indicator;

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
        .width(indicator_width)
        .height(indicator_height)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme| container::Style {
            background: selected.then(|| Background::Color(ui.color(Role::SecondaryContainer))),
            border: Border {
                radius: ui.radius(ShapeScale::Full, indicator_height),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let item_height = nav_item_height(ui, indicator_height);
    let height = if compact {
        indicator_height
    } else {
        item_height
    };
    // 列必须显式占满轨宽，否则它会 shrink 到 56，`align_x(Center)` 就变成空操作，
    // 药丸会贴左缘（x=0..55 而不是 16..71）——r1 实测踩到的第二个坑。
    let mut stack = column![pill]
        .width(nav_rail_width(ui.metrics()))
        .align_x(Alignment::Center)
        .height(height);
    if !compact {
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

    button(stack)
        .width(nav_rail_width(ui.metrics()))
        .height(height)
        .padding(0)
        .style(move |_theme: &IcedTheme, status| {
            let background = match overridden(forced, status) {
                button::Status::Hovered if !selected => Some(Background::Color(ui.state_over(
                    StateLayer::Hover,
                    Role::OnSurface,
                    Role::Surface,
                ))),
                button::Status::Pressed if !selected => Some(Background::Color(ui.state_over(
                    StateLayer::Pressed,
                    Role::OnSurface,
                    Role::Surface,
                ))),
                _ => None,
            };

            button::Style {
                background,
                text_color: label_color,
                border: Border::default(),
                shadow: Shadow::default(),
                snap: true,
            }
        })
        .on_press(Message::Select(item))
        .into()
}

/// 导航轨宽度（M69 起是令牌层的 `nav_rail_width`）。
///
/// M69（`141e9b1`）把 `LayoutMetrics` 的 nav 字段做了语义纠正：
///
/// | 旧 | 新 |
/// |---|---|
/// | `nav_expanded_width = 88` | **`nav_rail_width = 88`**（值不变，名字从「展开」改成「轨」） |
/// | `nav_collapsed_width = 48` | 删除（48 连 56 宽的选中指示器都放不下） |
/// | —（缺） | `nav_drawer_width = 240`（D33 的展开态抽屉，本 crate 未用） |
///
/// 三处调用点都走这里：将来令牌再改名，仍然只改这一行。
fn nav_rail_width(metrics: LayoutMetrics) -> f32 {
    metrics.nav_rail_width
}

/// 选中指示器尺寸（12b 5.4 参考 56×32），全部由令牌推导、不写死：
///
/// - 宽 = [`nav_rail_width`] − 2 × `Space::X4` = 88 − 32 = **56**（轨的左右内边距 16）
/// - 高 = `control_height − Space::X2` = 40 − 8 = **32**（可见高度上下各让 4）
pub fn indicator_size(metrics: LayoutMetrics) -> (f32, f32) {
    (
        nav_rail_width(metrics) - Space::X4.dp() * 2.0,
        metrics.control_height - Space::X2.dp(),
    )
}

/// 导航项高度 = 指示器 + 4 + 标签行高 = 32 + 4 + 16 = 52（`LM.nav_item_height` 是列表行节奏，
/// 药丸 + 标签的轨项放不进 44，偏差登记在交接文档里）。
pub fn nav_item_height(ui: &Ui, indicator_height: f32) -> f32 {
    indicator_height + ui.space(Space::X1) + ui.style(TypeRole::LabelMedium).line_height_px
}

#[cfg(test)]
mod tests {
    use super::*;
    use unlucky_design::color::SchemeFlavor;
    use unlucky_design::typography::Platform;

    use crate::theme::Families;

    fn ui() -> Ui {
        Ui::with_platform(
            SchemeFlavor::Light,
            Platform::Windows,
            Families {
                text: "Segoe UI",
                display: "Segoe UI",
                mono: "Consolas",
            },
        )
    }

    #[test]
    fn indicator_matches_the_reference_56x32() {
        let (width, height) = indicator_size(LayoutMetrics::DESKTOP);
        assert_eq!((width, height), (56.0, 32.0), "12b 5.4 的参考值");
    }

    #[test]
    fn nav_item_height_is_indicator_plus_gap_plus_label() {
        let ui = ui();
        let (_, indicator_height) = indicator_size(ui.metrics());
        // 32 + 4 + 16
        assert_eq!(nav_item_height(&ui, indicator_height), 52.0);
    }

    #[test]
    fn every_nav_item_has_a_label_and_an_icon() {
        assert!((4..=6).contains(&NavItem::ALL.len()));
        for item in NavItem::ALL {
            assert!(!item.label(Lang::En).is_empty(), "{item:?} 没有文案");
            assert!(!item.icon().icon().path.is_empty(), "{item:?} 没有图标");
        }
    }

    /// **M108**：导航栏上这一项叫 `Versions`（88px 轨放不下全名 `Version management`）。
    ///
    /// 文案本身走目录（M106 的 `label_key()` + `label(lang)`），这条只守**取值**与**宽度余量**：
    ///
    /// - **抓得住**：① 有人把 `Key::NavDownload` 的英文改回「下载」或塞回 18 字符的全名
    ///   （88px 轨上会截断/换行，实测全名 ≈101px）；② 中文被「为了对称」砍成两个字。
    /// - **抓不住**：真画出来的截断（那要出图，见 M108 报告里的产品通道与 pages_demo 图）；
    ///   也抓不住 240 展开抽屉（那一档在产品里还不可达）。
    #[test]
    fn the_download_item_is_labelled_version_management() {
        // 88px 轨的余量：中文 4 字 ≈48px、英文短名 ≈45px，都该有明显的余量
        assert_eq!(NavItem::Download.label(Lang::En), "Versions");
        assert_eq!(NavItem::Download.label(Lang::ZhCn), "版本管理");
        for item in NavItem::ALL {
            let label = item.label(Lang::En);
            assert!(
                !label.to_ascii_lowercase().contains("download"),
                "{item:?} 的英文标签还在说下载：{label}"
            );
            assert!(
                label.chars().count() <= 12,
                "{item:?} 的标签太长，88px 的轨放不下：{label}"
            );
        }
    }
}
