//! AppBar（small）（`research/12b` §16）：页头 56、标题 + 副标题、leading / trailing 槽位、滚动换色。
//!
//! ## 参考值（`[源]` = A1，本轮 `javap` 实读本机 AAR）
//!
//! | # | 检查项 | 参考值 | 本项目 |
//! |---|---|---|---|
//! | 16.1 | 页头高 | `AppBarSmallTokens.ContainerHeight` = **64** | **56** = `LM.page_header_height`（登记偏离 −8） |
//! | 16.3 | 容器 | `AppBarTokens.ContainerColor = Surface` + L0；**滚动后** `OnScrollContainerColor = SurfaceContainer` + `OnScrollContainerElevation`（L2） | 同 |
//! | 16.4 | 字级 | `TitleFont = TitleLarge`（emphasized w500）· `SubtitleFont = LabelMedium` + `SubtitleColor = OnSurfaceVariant` | 同 |
//! | 16.5 | 图标 | `IconSize = 24`；leading `OnSurface`、trailing `OnSurfaceVariant` | 同 |
//! | 16.6 | 左右内边距 | `LeadingSpace = 4` / `TrailingSpace = 4`（+ IconButton 自带 40 宽 → 视觉起手 20） | 同 |
//!
//! ## 「滚动时的高度变化」——**规范上不存在**（不是我们没做）
//!
//! A1 的 `AppBarTokens` 全字段实读（`javap -p -c androidx.compose.material3.tokens.AppBarTokens`）：
//!
//! ```text
//! AvatarSize 32 · ContainerColor Surface · ContainerElevation · ContainerShape CornerNone
//! IconButtonSpace · IconSize 24 · LeadingIconColor · LeadingSpace 4
//! OnScrollContainerColor SurfaceContainer · OnScrollContainerElevation   ← 滚动只改这两项
//! SubtitleColor/SubtitleFont · TitleColor/TitleFont · TrailingIconColor · TrailingSpace 4
//! ```
//!
//! **整个 tokens 包里 `OnScroll*` 只有两个字段**（颜色与高程，见 `grep` 全量核对的报告），
//! 没有任何 `OnScroll…Height`；高度只在**尺寸档**（`AppBarSmallTokens.ContainerHeight = 64`）里，
//! 而那是 `small` / `medium` / `large` 三个**不同组件**的分档，不是滚动驱动的形变。
//! 所以滚动时**高度不变**、只换容器色 + 升到 L2 阴影——这是规范，不是偏离。
//!
//! 我们与规范的两处差异各登记一条：
//!
//! - **D-M79-2** 高度 56 而不是 64（项目骨架口径 `LM.page_header_height`，已有页面按 56 排版）；
//! - 不做 `Medium` / `Large` 的**折叠（collapsing）**形态——那是另外两个组件（`AppBarMediumTokens`
//!   / `AppBarLargeTokens`），本 mission 的 §16 只要求 small。

use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{button, container, text};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding, Pixels, Shadow, Vector,
};

use unlucky_design::color::Role;
use unlucky_design::elevation::Elevation;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::typography::TypeRole;

use crate::icons;
use crate::theme::Ui;

use super::super::interaction::{self, Interaction};
use super::super::layout;
use super::geometry::AppBarGeometry;

/// 页头里的一个图标动作。
#[derive(Debug, Clone)]
pub struct AppBarAction<M> {
    icon: IconKind,
    on_press: Option<M>,
}

impl<M> AppBarAction<M> {
    /// 一个可点的图标动作。
    pub fn new(icon: IconKind, on_press: M) -> Self {
        Self {
            icon,
            on_press: Some(on_press),
        }
    }

    /// 一个装饰性图标（无消息）。
    pub fn inert(icon: IconKind) -> Self {
        Self {
            icon,
            on_press: None,
        }
    }
}

/// 一条 AppBar small。
#[derive(Debug, Clone)]
pub struct AppBar<'a, M> {
    title: &'a str,
    subtitle: Option<&'a str>,
    leading: Option<AppBarAction<M>>,
    trailing: Vec<AppBarAction<M>>,
    scrolled: bool,
}

impl<'a, M> AppBar<'a, M> {
    /// 标题（`TitleLarge`，emphasized）。
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            subtitle: None,
            leading: None,
            trailing: Vec::new(),
            scrolled: false,
        }
    }

    /// 副标题（`LabelMedium` + `OnSurfaceVariant`）。
    pub fn subtitle(mut self, subtitle: &'a str) -> Self {
        self.subtitle = Some(subtitle);
        self
    }

    /// 前置动作（导航返回 / 汉堡；`OnSurface`）。
    pub fn leading(mut self, action: AppBarAction<M>) -> Self {
        self.leading = Some(action);
        self
    }

    /// 追加一个后置动作（`OnSurfaceVariant`）。
    pub fn trailing(mut self, action: AppBarAction<M>) -> Self {
        self.trailing.push(action);
        self
    }

    /// 滚动状态：`true` = 容器色 `SurfaceContainer` + L2 阴影（A1 `OnScroll*`）。
    pub fn scrolled(mut self, scrolled: bool) -> Self {
        self.scrolled = scrolled;
        self
    }
}

/// 一个图标动作按钮（40×40 命中区、图标 24 经 `layout::fixed`——Q9）。
fn action<'a, M: Clone + 'a>(
    ui: &'a Ui,
    action: AppBarAction<M>,
    geometry: AppBarGeometry,
    color: Color,
) -> Element<'a, M> {
    let hovered =
        interaction::overlay_color(ui, Interaction::Hovered, Role::OnSurface, Role::Surface);
    let icon = layout::fixed(
        geometry.icon,
        geometry.icon,
        icons::view::<M>(action.icon, color),
    );
    let hit = ui.metrics().control_height;
    // 图标要落在 40×40 命中区的**正中**：`Container` 默认 `horizontal_alignment: Left`
    // （`iced_widget-0.14.2/src/container.rs`），只给 `align_y` 会让 24px 图标贴左缘——
    // 视觉 r1 实测前导墨迹 `38..58`（应 `44..68`）。所以**两轴都显式给**（规则 Q21）。
    let button = button(
        container(icon)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .width(Length::Fixed(hit))
    .height(Length::Fixed(hit))
    .padding(0)
    .style(move |_theme, status| button::Style {
        background: some_if_hovered(status, hovered),
        text_color: color,
        border: Border {
            // IconButton 的圆角是 full：按命中区边长折算（`radius_dp` 不接受 Full）
            radius: ui.radius(ShapeScale::Full, hit),
            ..Border::default()
        },
        shadow: Shadow::default(),
        snap: true,
    });
    match action.on_press {
        Some(message) => button.on_press(message).into(),
        None => button.into(),
    }
}

/// hover 时给背景（`None` 的其他态不叠状态层）。
fn some_if_hovered(status: button::Status, hovered: Option<Color>) -> Option<Background> {
    match status {
        button::Status::Hovered => hovered.map(Background::Color),
        _ => None,
    }
}

/// AppBar small（页头 56 / Surface / 滚动换色）。
pub fn app_bar<'a, M: Clone + 'a>(ui: &'a Ui, bar: AppBar<'a, M>) -> Element<'a, M> {
    let geometry = AppBarGeometry::of(ui);

    let title_style = ui.emphasized(TypeRole::TitleLarge);
    let mut texts = iced::widget::Column::new().spacing(0);
    texts = texts.push(
        text(bar.title)
            .size(title_style.size_px)
            .line_height(LineHeight::Absolute(Pixels(title_style.line_height_px)))
            // 单行：页头高固定 56，长标题折行会顶穿页头并与 trailing 动作重叠（偏离 D-M79-5）
            .wrapping(Wrapping::None)
            .font(ui.font_of(title_style))
            .color(ui.color(Role::OnSurface)),
    );
    if let Some(subtitle) = bar.subtitle {
        let style = ui.style(TypeRole::LabelMedium);
        texts = texts.push(
            text(subtitle)
                .size(style.size_px)
                .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
                .wrapping(Wrapping::None)
                .font(ui.font_of(style))
                .color(ui.color(Role::OnSurfaceVariant)),
        );
    }

    let mut row = iced::widget::Row::new()
        .align_y(Alignment::Center)
        .spacing(ui.space(unlucky_design::spacing::Space::X2));
    if let Some(leading) = bar.leading {
        // 16.5：leading 图标 `OnSurface`
        row = row.push(action(ui, leading, geometry, ui.color(Role::OnSurface)));
    }
    // 文本块吃掉剩余宽度并**裁切**：这一条同时解决两件事——
    // ① 标题与 trailing 之间必须有 Fill（M77 Q8 同款），② 单行超长标题切在动作按钮之前，
    //    不会重叠（`clip(true)` 是 iced 里唯一能裁子元素越界的方式；没有 ellipsis，见 D-M79-5）。
    row = row.push(container(texts).width(Length::Fill).clip(true));
    for action_item in bar.trailing {
        // 16.5：trailing 图标 `OnSurfaceVariant`
        row = row.push(action(
            ui,
            action_item,
            geometry,
            ui.color(Role::OnSurfaceVariant),
        ));
    }

    let background = if bar.scrolled {
        ui.color(Role::SurfaceContainer)
    } else {
        ui.color(Role::Surface)
    };
    let elevation = scroll_elevation(bar.scrolled);
    let shadow = if matches!(elevation, Elevation::Level0) {
        Shadow::default()
    } else {
        let spec = elevation.shadow();
        Shadow {
            color: Color {
                a: spec.alpha,
                ..ui.color(Role::Shadow)
            },
            offset: Vector::new(0.0, spec.offset_y_px),
            blur_radius: spec.blur_px,
        }
    };

    container(row)
        .width(Length::Fill)
        .height(Length::Fixed(geometry.height))
        // 内容行（40 高的动作按钮 + 文本块）必须落在 56 页头的**垂直中线**上：
        // `Container` 的 vertical_alignment 默认 Top，只给 `width` 会让整行贴顶
        // （视觉 r1 实测前导图标墨迹 y=204..223，居中应是 209..227）。
        .align_y(Alignment::Center)
        .padding(Padding {
            top: 0.0,
            right: geometry.edge,
            bottom: 0.0,
            left: geometry.edge,
        })
        .style(move |_theme| container::Style {
            background: Some(Background::Color(background)),
            border: Border {
                // 16.3：ContainerShape = CornerNone
                radius: ui.radius_dp(ShapeScale::None),
                ..Border::default()
            },
            shadow,
            ..container::Style::default()
        })
        .into()
}

/// 滚动态的高程档（A1 `AppBarTokens.OnScrollContainerElevation`；静息 L0 = `ContainerElevation`）。
fn scroll_elevation(scrolled: bool) -> Elevation {
    if scrolled {
        Elevation::Level2
    } else {
        Elevation::Level0
    }
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

    /// 16.1：页头高 = `LM.page_header_height`（56），并**钉死**它与 A1 的 64 不同。
    #[test]
    fn height_is_pinned_to_the_page_header_token() {
        let ui = ui();
        let geometry = AppBarGeometry::of(&ui);
        assert_eq!(geometry.height, ui.metrics().page_header_height);
        assert_eq!(geometry.height, 56.0);
        assert_ne!(
            geometry.height, 64.0,
            "64 是 A1 AppBarSmallTokens.ContainerHeight；本项目按骨架令牌取 56（D-M79-2）"
        );
    }

    /// 16.5 / 16.6：图标 24、左右内边距 4、头像 32。
    #[test]
    fn icon_and_edges_come_from_tokens() {
        let geometry = AppBarGeometry::of(&ui());
        assert_eq!(geometry.icon, 24.0);
        assert_eq!(geometry.edge, 4.0);
        assert_eq!(geometry.avatar, 32.0);
    }

    /// 「滚动只换色 + 升 L2」：这条把 `OnScroll*` 的两个字段钉住（高度**不在**这里面）。
    #[test]
    fn scroll_switches_color_and_elevation_only() {
        let ui = ui();
        assert_eq!(scroll_elevation(false), Elevation::Level0);
        assert_eq!(scroll_elevation(true), Elevation::Level2);
        // 高度与滚动无关：同一个几何（这条是「滚动不改高度」的可执行声明）
        assert_eq!(
            AppBarGeometry::of(&ui).height,
            ui.metrics().page_header_height,
            "高度只由骨架令牌决定，没有 OnScroll…Height"
        );
    }

    /// 树能建出来（leading + 副标题 + 两个 trailing + 滚动态）。
    #[test]
    fn view_builds_a_tree() {
        let ui = ui();
        let _: Element<'_, ()> = app_bar(
            &ui,
            AppBar::new("Instances")
                .subtitle("4 instances")
                .leading(AppBarAction::new(IconKind::Instances, ()))
                .trailing(AppBarAction::new(IconKind::Settings, ()))
                .trailing(AppBarAction::inert(IconKind::Logs))
                .scrolled(true),
        );
    }
}
