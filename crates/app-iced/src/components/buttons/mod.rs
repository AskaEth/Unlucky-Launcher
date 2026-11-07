//! 按钮家族（`research/12b` §1 五式 + §2 IconButton）。
//!
//! ## 参考值（全部 `[源]` = A1，`material3-android:1.4.0` 的 `*Tokens.kt` / `Button.kt`）
//!
//! | # | 检查项 | 参考值 | 本模块落点 |
//! |---|---|---|---|
//! | 1.1 | 容器可见高 | **40** | `LM.control_height`（[`Geometry::height`]） |
//! | 1.2 | 圆角 | **full = 高/2 = 20** | `ShapeScale::Full` + `Ui::radius` |
//! | 1.3 | 水平内边距（无图标） | **24 / 24** | `Space::X6` |
//! | 1.4 | 水平内边距（带前置图标） | **16（左）/ 24（右）** | `Space::X4` / `Space::X6` |
//! | 1.5 | 字级 | **LabelLarge 14/20 w500** | `TypeRole::LabelLarge` 强调档 |
//! | 1.6 | 图标 18 + 图标↔文字 8 | **18** = `IconSize::Button`（AAR `ButtonDefaults.IconSize`）；间距 `Space::X2` | [`Geometry`] |
//! | 1.7 | 最小宽 58 | **G15**：iced 的 `Length` 无 min-width，不施加（实测见报告） | — |
//! | 1.8 | 状态层 + 禁用 | hover 8 / focus 10 / pressed 10；容器 **10%**（官方 `DisabledContainerOpacity`；面状控件的 12% 是另一类）& 内容 38%；**内容角色按变体**（[`Variant::disabled_content`]） | [`super::interaction`] |
//! | 1.9 | 五式色彩角色 | Filled `Primary`/`OnPrimary`；Tonal `SecondaryContainer`/`OnSecondaryContainer`；Elevated `SurfaceContainerLow`/`Primary`；Outlined 透明 + 1dp `OutlineVariant` + `OnSurfaceVariant`；Text 透明 + `OnSurfaceVariant` | [`Variant::roles`] |
//! | 2.1 | IconButton 容器 | **40×40**、圆角 full | [`IconGeometry`] |
//! | 2.2 | IconButton 图标 / 边距 | 图标 **24**、边距 **8** | [`IconGeometry`] |
//! | 2.4 | Standard 各态**图标色不变** | 只有状态层变化 | [`Variant::roles`] 与态无关 |
//! | 2.7 | IconButton 禁用 | 内容 **`OnSurface`@38%**（A1；见 [`IconVariant::disabled_content`]） | [`super::interaction::disabled_button_content`]（按钮族 10% 底） |
//! | 2.8 | 命中区 | 容器 40 + 四周各 4 = **48** | `LM.touch_target`（按钮自身不撑高） |
//!
//! ## 与契约 §2 的签名差异（**已登记，请 tower 确认**）
//!
//! 契约 §2 的示例签名是 `filled(ui, label, on_press)`——三个参数装不下
//! 「五式 × 五态 × 前后双图标 × 禁用」。所以本模块的落地是**契约的加法版**：
//! 泛型纪律一字不改（`M: Clone`，不碰 `app::Message`），但把前两个参数收进 [`Button`]：
//!
//! ```text
//! 契约示例：  filled(ui, "保存", Message::Save)
//! 本模块：    filled(ui, Button::new("保存").press(Message::Save))
//! ```
//!
//! 差别只是「label + 消息」两件事从位置参数变成构造器参数，语义一一对应；
//! 要图标/禁用/焦点取证时再加链式调用（`.leading(..)` / `Button::disabled(..)` / `.state(..)`），
//! 不需要第二个签名。**这是本周唯一一处偏离契约示例的地方**，报告「决策」一节有登记。
//!
//! ## 为什么不用 iced 内置的 `theme::Button` 样式
//!
//! iced 的内置按钮样式只有 `primary/success/danger/text` 几档，且**不带 M3 的状态层 α**
//! （读的是 `Palette` 现成色）。D30 要求 hover 8% / pressed 10% 可判定，所以样式自绘；
//! 但**只画颜色与描边**——尺寸仍由 `button` 的 width/height/padding 决定，不手搓几何。

use iced::widget::button;
use iced::{Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::component_map::Component;
use unlucky_design::elevation::Elevation;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;

use crate::icons;
use crate::theme::Ui;

use super::interaction::Interaction;
use super::layout;

mod assemble;
mod geometry;

pub use geometry::{Geometry, IconGeometry};

use assemble::{assemble, style_closure, StyleSpec};

#[cfg(test)]
mod tests;

/// 按钮的五种视觉变体（12b §1.9 的色彩角色，一名一式）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variant {
    /// 实心强调色：**每屏最多一个**（rubric K2）。
    Filled,
    /// 次强调（tonal）：`SecondaryContainer`。
    Tonal,
    /// 带高程：`SurfaceContainerLow` + `Primary` 文本（L1 用色调差，不画阴影）。
    Elevated,
    /// 描边：透明容器 + 1dp `OutlineVariant`。
    Outlined,
    /// 纯文本：最低权重。
    Text,
}

impl Variant {
    /// 五式全序（gallery 的矩阵按这个次序出图）。
    pub const ALL: [Variant; 5] = [
        Variant::Filled,
        Variant::Tonal,
        Variant::Elevated,
        Variant::Outlined,
        Variant::Text,
    ];

    /// 稳定名字（图例 / 文件名 / 失败信息用）。
    pub const fn name(self) -> &'static str {
        match self {
            Variant::Filled => "filled",
            Variant::Tonal => "tonal",
            Variant::Elevated => "elevated",
            Variant::Outlined => "outlined",
            Variant::Text => "text",
        }
    }

    /// 该式在组件表里的档位（圆角/字级）。`Component` 没有 `Outlined/Elevated` 变体，
    /// 借用 `PrimaryButton`：三者判据值相同（`Full` 圆角 + `LabelLarge` + L0）。
    pub(crate) const fn component(self) -> Component {
        match self {
            Variant::Filled => Component::PrimaryButton,
            Variant::Tonal => Component::SecondaryButton,
            Variant::Text => Component::TextButton,
            Variant::Elevated | Variant::Outlined => Component::PrimaryButton,
        }
    }

    /// 圆角档位（五式统一 `Full`，12b 1.2）。
    pub(crate) fn shape(self) -> ShapeScale {
        self.component().spec().shape
    }

    /// **禁用态的内容角色**（A1，逐变体不同——别在这里统一猜）。
    ///
    /// | 变体 | 禁用内容 |
    /// |---|---|
    /// | Filled / Elevated / Outlined / Text | `OnSurfaceVariant` @0.38 |
    /// | **FilledTonal** | **`OnSurface` @0.38** ← A1 的显式例外（`FilledTonalButtonTokens`） |
    ///
    /// 出处：AAR `material3-android:1.4.0` 的 `javap -c`，`FilledButtonTokens.
    /// DisabledLabelTextColor = OnSurfaceVariant`、`FilledTonalButtonTokens` 同一字段 = `OnSurface`。
    pub(crate) const fn disabled_content(self) -> Role {
        match self {
            Variant::Tonal => Role::OnSurface,
            _ => Role::OnSurfaceVariant,
        }
    }

    /// 高程：只有 Elevated 是 L1（`ElevatedButtonTokens.ContainerElevation`），其余 L0。
    ///
    /// **不画阴影**：rubric §7「L0/L1 不应有可见阴影（用 tone 差）」，所以 L1 只体现为
    /// `SurfaceContainerLow` 容器色，`button::Style.shadow` 永远 default。
    pub(crate) const fn elevation(self) -> Elevation {
        match self {
            Variant::Elevated => Elevation::Level1,
            _ => Elevation::Level0,
        }
    }

    /// 色彩角色三元组 `(容器, 内容, 描边)`；`None` = 透明容器 / 无描边。
    pub(crate) const fn roles(self) -> (Option<Role>, Role, Option<Role>) {
        match self {
            Variant::Filled => (Some(Role::Primary), Role::OnPrimary, None),
            Variant::Tonal => (
                Some(Role::SecondaryContainer),
                Role::OnSecondaryContainer,
                None,
            ),
            Variant::Elevated => (Some(Role::SurfaceContainerLow), Role::Primary, None),
            Variant::Outlined => (None, Role::OnSurfaceVariant, Some(Role::OutlineVariant)),
            Variant::Text => (None, Role::OnSurfaceVariant, None),
        }
    }
}

/// IconButton 的三种容器变体（12b §2.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconVariant {
    /// 标准：容器透明、图标 `OnSurfaceVariant`（各态图标色不变，12b 2.4）。
    Standard,
    /// 实心：`Primary` + `OnPrimary`。
    Filled,
    /// Tonal：`SecondaryContainer` + `OnSecondaryContainer`。
    Tonal,
}

impl IconVariant {
    /// 三式全序（gallery 顺序）。
    pub const ALL: [IconVariant; 3] = [
        IconVariant::Standard,
        IconVariant::Filled,
        IconVariant::Tonal,
    ];

    /// 稳定名字。
    pub const fn name(self) -> &'static str {
        match self {
            IconVariant::Standard => "standard",
            IconVariant::Filled => "filled",
            IconVariant::Tonal => "tonal",
        }
    }

    /// **禁用态图标色**：三式都是 `OnSurface` @0.38 —— A1
    /// （`StandardIconButtonTokens.DisabledColor` / `FilledIconButtonTokens.DisabledColor`）。
    pub(crate) const fn disabled_content(self) -> Role {
        Role::OnSurface
    }

    /// 色彩角色三元组（同 [`Variant::roles`] 的口径）。
    pub(crate) const fn roles(self) -> (Option<Role>, Role, Option<Role>) {
        match self {
            IconVariant::Standard => (None, Role::OnSurfaceVariant, None),
            IconVariant::Filled => (Some(Role::Primary), Role::OnPrimary, None),
            IconVariant::Tonal => (
                Some(Role::SecondaryContainer),
                Role::OnSecondaryContainer,
                None,
            ),
        }
    }
}

/// 一个按钮的装配描述（五式共用；**不持有任何状态所有权**）。
///
/// `on_press: Option<M>` 就是 M3 的 enabled/disabled 开关——`None` = 禁用，
/// 与 `button::on_press_maybe` 的语义天然对齐（省掉一个 `disabled: bool` 字段，
/// 也就不会出现「有消息但画成禁用」这种自相矛盾的状态）。
#[derive(Debug, Clone)]
pub struct Button<'a, M> {
    label: &'a str,
    on_press: Option<M>,
    leading: Option<IconKind>,
    trailing: Option<IconKind>,
    state: Option<Interaction>,
}

impl<'a, M> Button<'a, M> {
    /// 一个可点的按钮。
    pub fn new(label: &'a str, on_press: M) -> Self {
        Self {
            label,
            on_press: Some(on_press),
            leading: None,
            trailing: None,
            state: None,
        }
    }

    /// 一个禁用的按钮（无消息）。禁用不是「另一种颜色」，见 `interaction::disabled_button_colors`。
    pub fn disabled(label: &'a str) -> Self {
        Self {
            label,
            on_press: None,
            leading: None,
            trailing: None,
            state: None,
        }
    }

    /// 前置图标（12b 1.4：带图标时左内边距收到 16）。
    pub fn leading(mut self, kind: IconKind) -> Self {
        self.leading = Some(kind);
        self
    }

    /// 后置图标（右内边距保持 24）。
    pub fn trailing(mut self, kind: IconKind) -> Self {
        self.trailing = Some(kind);
        self
    }

    /// 强制视觉态（`--screen=gallery` 的状态矩阵取证用；运行期不要调）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }
}

/// Filled 按钮（实心 `Primary`）。
pub fn filled<'a, M: Clone + 'a>(ui: &'a Ui, button: Button<'a, M>) -> Element<'a, M> {
    assemble(ui, Variant::Filled, button)
}

/// Tonal 按钮（`SecondaryContainer`）。
pub fn tonal<'a, M: Clone + 'a>(ui: &'a Ui, button: Button<'a, M>) -> Element<'a, M> {
    assemble(ui, Variant::Tonal, button)
}

/// Elevated 按钮（`SurfaceContainerLow` + `Primary` 文本，L1 用色调差）。
pub fn elevated<'a, M: Clone + 'a>(ui: &'a Ui, button: Button<'a, M>) -> Element<'a, M> {
    assemble(ui, Variant::Elevated, button)
}

/// Outlined 按钮（透明 + 1dp `OutlineVariant`）。
pub fn outlined<'a, M: Clone + 'a>(ui: &'a Ui, button: Button<'a, M>) -> Element<'a, M> {
    assemble(ui, Variant::Outlined, button)
}

/// Text 按钮（无容器、无描边）。
pub fn text<'a, M: Clone + 'a>(ui: &'a Ui, button: Button<'a, M>) -> Element<'a, M> {
    assemble(ui, Variant::Text, button)
}

/// IconButton（standard / filled / tonal）：40×40、图标 24、命中区 48。
///
/// **图标语汇纪律（视觉 reviewer P2-3）**：同一个控件里**不混用**填充字形（`IconPaint::Fill`，
/// 如 `Play`/`Stop`/`More`）与描边字形（`Stroke`）——混着放会让线宽看起来忽粗忽细。
/// 允许实心字形，但只能出现在「主操作」这一处（例如播放按钮），其余位置一律描边；
/// gallery 的样例因此统一用描边图标（`Download` + `ChevronRight`），并有单测钉住。
///
/// `on_press: Option<M>`（`None` = 禁用）与 `state` 分开给：前者是语义，后者是取证开关。
pub fn icon<'a, M: Clone + 'a>(
    ui: &'a Ui,
    variant: IconVariant,
    kind: IconKind,
    on_press: Option<M>,
    state: Option<Interaction>,
) -> Element<'a, M> {
    let geometry = IconGeometry::of(ui);
    let (fill, content, border) = variant.roles();
    let forced = state.or(if on_press.is_some() {
        None
    } else {
        Some(Interaction::Disabled)
    });

    // Q9：图标**必须在固定尺寸的中层容器里**，否则 Button 的 Fill 约束会让 usvg 按 Contain
    // 撑满内容区（M68：24 的字形渲染成 46px、线宽 1.5 → 2.9）。
    // 这里把盒子做成整个 40×40（而不是 24×24）：`button::layout` 把内容放左上角、不居中，
    // 所以「居中」这件事必须由容器自己写死，别指望 button。
    // 内容色跟态走（禁用 = on-surface@38%），见 `assemble::content_color` 的说明
    let glyph = assemble::content_color(ui, forced, content, fill, variant.disabled_content());
    let icon_box =
        layout::centered_fixed(geometry.side, geometry.side, icons::view::<M>(kind, glyph));

    let style = style_closure(
        ui,
        StyleSpec {
            fill,
            content,
            border,
            // 方形图标按钮同样是 full 圆角（40 → 20），档位取自组件表
            shape: Component::IconButton.spec().shape,
            stroke: 0.0,
            radius_side: geometry.side,
            forced,
        },
    );

    let element = button(icon_box)
        .width(Length::Fixed(geometry.side))
        .height(Length::Fixed(geometry.side))
        .padding(Padding::default())
        .style(style);

    match on_press {
        Some(message) => element.on_press(message).into(),
        None => element.into(),
    }
}
