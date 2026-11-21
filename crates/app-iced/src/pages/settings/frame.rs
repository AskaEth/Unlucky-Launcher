//! 设置页骨架：页头、左列分组导航、表单列的滚动区、演示卡片。
//!
//! ## 为什么这里还再导出一批 `pages::frame` 的名字
//!
//! M99 拆目录时立的硬要求是**纯搬移**：调用点一字不改。设置页原来的代码从 `super::frame` 取词
//! （`frame::text_role` / `frame::card` / `frame::ICON_PX` …）；拆成目录后 `frame` 这个名字落到了
//! 本文件上，于是把用到的那几件**再导出**一次，`mod.rs` / `tests.rs` 的调用点就能原样保留。
//! 这不是新 API，只是一条转发（新增控件仍归 [`crate::pages::frame`]）。

pub(super) use super::super::frame::{
    card, divider, glyph_box, left_aligned, page, panes, scroll_style, single_column, text_action,
    text_role, Header, ICON_PX, SCROLLBAR_MARGIN, SCROLLBAR_WIDTH,
};

use iced::widget::{button, column, container, row};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Shadow, Theme};

use unlucky_design::color::Role;
use unlucky_design::component_map::Component;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Key, Lang};

use crate::app::components::interaction::{self, Interaction};
use crate::pages::appearance::Change;
use crate::pages::{Action, Message};
use crate::theme::Ui;

use super::Group;

/// 页头：标题 + 副标题 + 两个占位动作。
///
/// M106：三条文案走 catalog（`lang` 由 `settings::view` 传下来）。页标题复用 `NavSettings`——
/// 左列与页头说的是同一件事，不该有第二条译文。
pub(super) fn header<'a>(ui: &'a Ui, lang: Lang) -> Header<'a> {
    let catalog = lang.catalog();
    Header::new(catalog.text(Key::NavSettings))
        .subtitle(catalog.text(Key::SettingsSubtitle))
        .action(text_action(
            ui,
            catalog.text(Key::SettingsResetSection),
            Action::ResetSection,
        ))
        .action(text_action(
            ui,
            catalog.text(Key::SettingsSave),
            Action::SaveSettings,
        ))
}

// --------------------------------------------------------------- 侧列

/// 左列分组导航（supporting-pane 的辅助列，宽 = 令牌 `nav_drawer_width` = 240）。
///
/// `forced`（M299）是 `--force-state` 的落点，口径与 [`super::resolve_state`] 相同：`Some` 时整列
/// 按那一态画（取证），`None` 时按观测到的 `button::Status` 画（产品路径的常态）。
pub(super) fn group_nav<'a>(
    ui: &'a Ui,
    selected: usize,
    lang: Lang,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let catalog = lang.catalog();
    let items = column(
        super::GROUPS
            .iter()
            .enumerate()
            .map(|(index, group)| {
                let label = super::GROUP_LABELS
                    .get(index)
                    .map_or(group.name, |key| catalog.text(*key));
                group_item(ui, group, label, index, index == selected, forced)
            })
            .collect::<Vec<_>>(),
    )
    .spacing(ui.space(Space::X1))
    .padding(Padding::from([ui.space(Space::X2), ui.space(Space::X2)]))
    .width(Length::Fill);

    container(items)
        .width(Length::Fill)
        .height(Length::Fill)
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

/// 一枚分组项：**真按钮** + 胶囊底（M299）。
///
/// ## 为什么从 `mouse_area` 换成 `button`
///
/// 用户原话：「侧边栏按钮 hover 和 click 状态是父容器整个方形区域颜色加深，而不是 MD3E 应有的
/// 胶囊 UI」。此前这里是 `mouse_area` 包一个静态实色容器：`mouse_area` 根本不出
/// `button::Status`，于是选中的那一组**一个像素都不变**（没有 hover、没有 pressed），未选中的
/// 那一组也毫无反馈——「加深」的只剩别处（外壳导航轨）的方形命中区。
///
/// ## 几何没动（M283 定稿的那套形状语汇）
///
/// 高 = `LM.row_height`、圆角 = [`ShapeScale::Full`]、宽 = 列宽，内边距 `Space::X3`。**胶囊是量出来
/// 的，不是猜的**：从取证图 `DevDocs/artifacts/m299/new/settings-general-light-hover.png` 量到的
/// 选中项底色 bbox 是 y240-283（高 **44**）、x120-343，且顶边下 1px 处的横向内缩是 **15px**
/// ——半径 22 的胶囊在这个位置的内缩是 r−√(r²−(r−1)²)=15.4，半径 8 的圆角只有 4.1。所以
/// **半径 = 高/2 = 22，已经是胶囊**，本 mission 只换颜色入口：选中支走
/// [`interaction::selected_fill`]，未选中支走 [`interaction::layer_color`]（叠在侧列自己的卡底
/// `frame::BASE` 上）。
///
/// ## 内容色为什么还是静态的
///
/// 图标与文案由 `glyph_box` / `text_role` 各自**显式着色**（不继承 `button::Style::text_color`），
/// 所以 hover / pressed 只改底不改墨色。MD3 里未选中项 hover 时文字提到 `OnSurface`——这一条
/// **没有实现**（要重建整棵内容树才做得到），已作为 finding 报给塔，别把它当已覆盖。
fn group_item<'a>(
    ui: &'a Ui,
    group: &'a Group,
    label: &'a str,
    index: usize,
    selected: bool,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let spec = Component::NavigationItem.spec();
    let height = ui.metrics().row_height;
    let radius = ui.radius(ShapeScale::Full, height);

    let content_color = if selected {
        ui.color(Role::OnSecondaryContainer)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };

    let inner = row![
        glyph_box(group.icon, content_color, ICON_PX),
        text_role(ui, spec.text, false, label, content_color),
    ]
    .spacing(ui.space(Space::X3))
    .width(Length::Fill)
    .align_y(Alignment::Center);

    let pill = container(inner)
        .width(Length::Fill)
        .height(height)
        .padding(Padding::from([0.0, ui.space(Space::X3)]))
        .align_y(Alignment::Center);

    // 点选切分组：走页面自己的 `Message::Appearance(SelectGroup)`（只改视图状态，不落盘）。
    button(pill)
        .width(Length::Fill)
        .height(height)
        .padding(0)
        .style(move |_theme, status| button::Style {
            background: group_fill(ui, selected, forced, status).map(Background::Color),
            text_color: content_color,
            border: Border {
                radius,
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        })
        .on_press(Message::Appearance(Change::SelectGroup(index)))
        .into()
}

/// 分组项的底：选中支叠在实色 `SecondaryContainer` 上，未选中支只有状态层（M299）。
///
/// 两条分支不许合并（与 `shell/nav.rs` 的药丸、`drawer/item.rs` 的 `pill_color` 同一条口径）：
/// 选中项的静息**有底**，未选中的静息**没有底**。
fn group_fill(
    ui: &Ui,
    selected: bool,
    forced: Option<Interaction>,
    status: button::Status,
) -> Option<Color> {
    let state = super::resolve_state(forced, status);
    if selected {
        Some(interaction::selected_fill(
            ui,
            state,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer,
        ))
    } else {
        interaction::layer_color(ui, state, Role::OnSurface, super::super::frame::BASE.role())
    }
}

// --------------------------------------------------------------- 表单列

/// 表单列的滚动区：只有正文滚，页头不动（与实例页列表列同一套口径）。
///
/// 「外观」分组有 7 个控件 + 最多 3 行诊断，1180×760 下会超出可视区，所以这一层是必需的。
///
/// ## M173：滚动条**并排**在内容右侧
///
/// 原来这里是 `scrollable(content)` 裸调 + 轨宽 6 / 内缩 2。iced 的竖直滚动条是浮层：轨道按
/// 视口右边缘定位（`iced_widget-0.14.2/src/scrollable.rs:1970`），而内容拿到的横向约束是**整个
/// 视口宽**（同文件 `:437`，`Scrollbar::spacing` 默认 `None` ⇒ 右内边距 0）。于是滚动条占的那条
/// 10dp 竖带（轨 6 + 两侧内缩 2×2）整条压在表单列最右边——用户看到的就是「滚动条骑在卡片上」。
///
/// 改法在 [`crate::app::components::scroll::vertical`]：内容右侧恒留一条 10dp 的槽，轨道画在槽里。
/// 槽宽来自同一个式子 `轨宽 + 2 × margin`，所以：
///
/// - 内容可用宽度 = 视口宽（424）− 槽宽（10）= **414**；
/// - 长页面（出滑块）与短页面（不出滑块）**内容宽度一致**——槽是内边距，不依赖 iced 的
///   `is_scrollbar_visible`（那正是 `Scrollbar::spacing` 不能用在这里的理由：它只在可见时预留，
///   滑块一出现内容就横跳 10dp）。
///
/// 数值由 `components/scroll.rs` 与 `tests/scroll_fit.rs` 的**布局级**断言钉住（`Renderer = ()`
/// 真跑 iced 的 `layout()`），不是看图。
///
/// 只泛型到 `Renderer` 是为了让上面那条断言能在 `()` 渲染器上跑；产品调用点不写泛型。
pub(super) fn scroll<'a, Renderer>(
    ui: &'a Ui,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Element<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::text::Renderer + 'a,
{
    crate::app::components::scroll::vertical(
        SCROLLBAR_WIDTH,
        SCROLLBAR_MARGIN,
        scroll_style(ui),
        content,
    )
}

/// 演示行构成的一组设置 = 一张卡，行间 1px 分隔线（M3 设置列表的常规做法）。
///
/// 已经接了真控件的分组（`GROUP.body` 是 `Some`）不走这里。
pub(super) fn demo_card<'a>(
    ui: &'a Ui,
    catalog: unlucky_i18n::Catalog,
    rows: &'a [(Key, &'static str)],
) -> Element<'a, Message> {
    let mut stacked = column![].spacing(0.0).width(Length::Fill);

    for (index, (label, value)) in rows.iter().enumerate() {
        if index > 0 {
            stacked = stacked.push(divider(ui));
        }
        // 标签走词表、值原样（M314 r1：值是不可翻的数据，见 `registry.rs::Group::rows`）。
        stacked = stacked.push(setting_row(ui, catalog.text(*label), value));
    }

    card(ui, stacked)
}

fn setting_row<'a>(ui: &'a Ui, label: &'a str, value: &'a str) -> Element<'a, Message> {
    let height = ui.metrics().control_height + Space::X2.dp();

    row![
        container(text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            label,
            ui.color(Role::OnSurface),
        ))
        .width(Length::Fill)
        .align_y(Alignment::Center),
        text_role(
            ui,
            TypeRole::BodySmall,
            false,
            value,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .height(height)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .into()
}

// --------------------------------------------------------------- M196 守门

// M299：守门本体搬进 `frame/no_stray_empty_state.rs`。只为行数——分组导航改成真按钮之后
// 本文件会超过 400 行硬线（AGENTS.md §3-1）。判据一条没改，只是 `include_str!` 的深度变了。
#[cfg(test)]
mod no_stray_empty_state;

/// 分组项的底色（M299）：选中支四条断言 + 未选中支的对照。
///
/// 抓得住：①把选中支改回「选中就不叠层」（`selected.then(|| ui.color(SecondaryContainer))`）⇒
/// hover / pressed 都等于静息，前三条红；②把 hover 的 α 抄成 pressed 的 10% ⇒ `hover != pressed`
/// 红；③未选中支的合成底换成画布（侧列画的是 `MaterialLayer::Card`）⇒ 最后一条红。
/// 抓不住：像素（`DevDocs/artifacts/m299/settings-frame-*-hover|pressed.png`，`--force-state`
/// 经 `settings::view` 递到 `group_nav` 的 `forced`）；也抓不住「自己抄了一份**正确**的叠层」——
/// 那是 `selection/tests/single_entry.rs` 的接线扫描管的事。
#[cfg(test)]
mod group_fill_tests {
    use super::*;

    use unlucky_design::color::{SchemeFlavor, StateLayer};
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
    fn a_selected_group_reacts_to_hover_and_press() {
        let ui = ui();
        let fill = |status| group_fill(&ui, true, None, status).expect("选中项静息也有底");
        let rest = fill(button::Status::Active);
        let hover = fill(button::Status::Hovered);
        let pressed = fill(button::Status::Pressed);

        assert_eq!(
            rest,
            ui.color(Role::SecondaryContainer),
            "选中静息 = 实色容器"
        );
        assert_ne!(hover, rest, "选中 + hover 必须与静息不同");
        assert_ne!(pressed, rest, "选中 + pressed 必须与静息不同");
        assert_ne!(hover, pressed, "8% 与 10% 必须能区分");
        assert_eq!(
            pressed,
            ui.state_over(
                StateLayer::Pressed,
                Role::OnSecondaryContainer,
                Role::SecondaryContainer
            )
        );

        // 未选中：静息**没有底**（那是状态层自己的事），hover 叠在侧列的卡底上
        assert_eq!(group_fill(&ui, false, None, button::Status::Active), None);
        assert_eq!(
            group_fill(&ui, false, None, button::Status::Hovered),
            Some(ui.state_over(
                StateLayer::Hover,
                Role::OnSurface,
                MaterialLayer::Card.surface_role()
            )),
            "未选中项的 hover 必须叠在卡底（不是画布）上"
        );
    }

    /// `--force-state` 经 `forced` 覆盖观测 status（与 `settings::resolve_state` 同一条口径）。
    ///
    /// 抓得住：`group_fill` 忘了把 `forced` 递给 `resolve_state`（那三个像素图的判据就假了）。
    /// 抓不住：像素通道到底有没有把 `forced` 传到（那是 `pages/settings/mod.rs` 的接线 + 图）。
    #[test]
    fn the_forced_state_wins_over_the_observed_status() {
        let ui = ui();
        let hover = group_fill(&ui, true, None, button::Status::Hovered);
        assert_eq!(
            group_fill(
                &ui,
                true,
                Some(Interaction::Hovered),
                button::Status::Active
            ),
            hover,
            "forced 必须覆盖观测到的 Active"
        );
        assert_eq!(
            group_fill(
                &ui,
                true,
                Some(Interaction::Enabled),
                button::Status::Pressed
            ),
            group_fill(&ui, true, None, button::Status::Active),
            "forced=静息时必须把 pressed 压回静息（取证图要能拍「无状态层」）"
        );
    }
}
