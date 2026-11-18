//! Snackbar（`research/12b` §14）：底/顶部提示条 + 动作 + 关闭。
//!
//! ## 官方值（`[源]` = A1，`SnackbarTokens` + `SnackbarKt`，`javap` 核对）
//!
//! | # | 项 | 值 | 落点 |
//! |---|---|---|---|
//! | 14.1 | 圆角 | **16**（规格卡 `rx=16`；官方 `CornerExtraSmall` 是 4，M229 换肤按规格卡） | `inkcat::metrics::SNACKBAR` |
//! | 14.2 | 容器色 | **`InverseSurface`**（高程 L3 是设计口头径，见下「不画阴影」） | [`Role::InverseSurface`] |
//! | 14.3 | 高 | 单行 **52**（规格卡 340×52；官方 48）/ 双行 **72**（+20） | [`SnackbarGeometry::height_for`] |
//! | 14.4 | 内距 | 水平 **16** / 垂直 **6** / 文本↔动作 **8** | `Space::X4` / 6 / `Space::X2` |
//! | 14.6 | 颜色 | 文本 `InverseOnSurface`、动作 **`InversePrimary`**、图标 24 `InverseOnSurface` | 组件层取角色 |
//! | — | 宽度上限 | **600**（`SnackbarKt.ContainerMaxWidth`） | `max_width` |
//!
//! ## 宽度上下限（一条**真实**限制，登记 G17-d）
//!
//! 官方只有上限 600，没有下限常数；本项目取 **344 `[估]`**（M2 时代口径，观感连续）。
//! iced 的 `Length` **没有 min-width**（与按钮家族 G15「最小宽 58 不施加」同一堵墙），
//! 所以：**上限在组件里强制**（`container::max_width`），**下限由宿主保证**——页面给提示条
//! 的区域至少 344 宽。gallery 用三个宿主宽度把这条限制摊开（300 → 300、344 → 344、800 → 600），
//! 渲染层断言量的是这三种实测值，不假装下限被组件强制住了。
//!
//! ## 超时与 dismiss：状态归调用方
//!
//! 组件**不计时**：显示时长、自动消失、`on_timeout` 全在调用方（页面）手里。
//! 组件的语义只有三条：`placement` 决定贴顶还是贴底（并给出 12 的外边距）、
//! `action` 给一个文字动作、`dismiss` 给关闭键。消息要不要触发「撤销」由调用方决定。
//!
//! ## 不画阴影
//!
//! 官方是 L3，但 D27（CPU 档禁模糊）与令牌层 `ShadowSpec` 仍是 M63 占位值这两条都指向
//! 「先不画」——层级靠 `InverseSurface` 与页面的 scrim/留白表达。登记为偏离 **D-4**
//! （D-3 是进度条的波浪降级，别混号）。

use iced::widget::{container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::super::interaction::Interaction;
use super::action;
use super::panel::SnackbarGeometry;

/// 贴底还是贴顶（官方两种都有；本项目默认贴底）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// 贴底（桌面默认：不挡页头，也不挡主操作）。
    Bottom,
    /// 贴顶（页头有刷新按钮时用于「结果在页头附近」的场景）。
    Top,
}

impl Placement {
    /// 垂直对齐：贴底 = `End`，贴顶 = `Start`。
    fn vertical(self) -> Alignment {
        match self {
            Placement::Bottom => Alignment::End,
            Placement::Top => Alignment::Start,
        }
    }
}

/// 一条提示条的装配描述（**不持有任何状态所有权**）。
#[derive(Debug, Clone)]
pub struct Snackbar<'a, M> {
    text: &'a str,
    lines: u8,
    placement: Placement,
    action: Option<(&'a str, M)>,
    dismiss: Option<M>,
    state: Option<Interaction>,
}

impl<'a, M> Snackbar<'a, M> {
    /// 单行提示条（高 52，规格卡口径）。
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            lines: 1,
            placement: Placement::Bottom,
            action: None,
            dismiss: None,
            state: None,
        }
    }

    /// 双行（高 68）。**行数由调用方声明**：iced 没有 `heightIn(min=…)`，官方那两条高度值
    /// 只能显式给（文本自己换行与否不改变容器高，这是 M3 的口径）。
    pub fn two_lines(mut self) -> Self {
        self.lines = 2;
        self
    }

    /// 贴底 / 贴顶。
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }

    /// 右侧文字动作（`InversePrimary`）。
    pub fn action(mut self, label: &'a str, on_press: M) -> Self {
        self.action = Some((label, on_press));
        self
    }

    /// 关闭键（`InverseOnSurface`）。
    pub fn dismiss(mut self, on_press: M) -> Self {
        self.dismiss = Some(on_press);
        self
    }

    /// 强制动作件的视觉态（gallery 取证用；运行期不要调）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }
}

/// 画一条提示条。
///
/// 返回的元素是 **`Fill` 高度的容器**：贴底/贴顶由它内部的对齐决定，调用方只要给它一块
/// 可用区（`stack` 在页面上、或 gallery 的宿主盒），它就是那个位置上的居中提示条。
pub fn snackbar<'a, M: Clone + 'a>(ui: &'a Ui, spec: Snackbar<'a, M>) -> Element<'a, M> {
    let geometry = SnackbarGeometry::of(ui);
    let style = ui.style(TypeRole::BodyMedium);
    let height = geometry.height_for(spec.lines);
    let surface = ui.color(Role::InverseSurface);
    let radius = iced::border::Radius::new(geometry.radius);

    let mut content = row![]
        .align_y(Alignment::Center)
        .spacing(geometry.gap)
        .push(
            text(spec.text)
                .size(style.size_px)
                .font(ui.font_of(style))
                .color(ui.color(Role::InverseOnSurface))
                .width(Length::Fill),
        );

    if let Some((label, message)) = spec.action {
        content = content.push(action::text_action(
            ui,
            label,
            Role::InversePrimary,
            Role::InverseSurface,
            Some(message),
            spec.state,
        ));
    }

    if let Some(message) = spec.dismiss {
        content = content.push(action::icon_action(
            ui,
            IconKind::Close,
            Role::InverseOnSurface,
            Role::InverseSurface,
            Some(message),
            spec.state,
        ));
    }

    let bar = container(content)
        .width(Length::Fill)
        .max_width(geometry.max_width)
        .height(Length::Fixed(height))
        .padding(Padding {
            top: geometry.pad_v,
            right: geometry.pad_h,
            bottom: geometry.pad_v,
            left: geometry.pad_h,
        })
        .style(move |_theme| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                radius,
                ..Border::default()
            },
            ..container::Style::default()
        });

    // 位置层：`Fill` 高度 + 外边距 12 + 水平居中；贴底/贴顶只改 `align_y`（Q8：不用 center_*）。
    container(bar)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::from(geometry.margin))
        .align_x(Alignment::Center)
        .align_y(spec.placement.vertical())
        .into()
}
