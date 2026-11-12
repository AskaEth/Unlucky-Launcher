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
//! | 1.9 | 五式色彩角色 | Filled `Primary`/`OnPrimary`；Tonal `SecondaryContainer`/`OnSecondaryContainer`；Elevated = **`Elevation::Level1.surface_role()`**（= `SurfaceContainerLow`）/`Primary`；Outlined 透明 + 1dp `Outline` + `OnSurfaceVariant`；Text 透明 + `OnSurfaceVariant` | [`Variant::roles`] |
//! | 1.9b | 与 A1 的登记差异（产品口径，M248） | Outlined 描边 A1 是 `OutlineVariant`，本项目取 **`Outline`**：1dp 边界属于**非文字元素**（目标 3:1，`design/color/mod.rs` 的对比度纪律），`OutlineVariant` 是装饰分隔线的角色；且 `IconVariant::Outlined` 一直是 `Outline`，两族由此统一 | [`Variant::roles`] |
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
use unlucky_design::inkcat::size::ControlSize;
use unlucky_design::shape::ShapeScale;

use crate::icons;
use crate::theme::Ui;

use super::interaction::{Base, Interaction};
use super::layout;

mod assemble;
mod fab;
mod geometry;
mod icon_variant;

pub use fab::{fab, Fab};
pub use geometry::{Geometry, IconGeometry};
pub use icon_variant::IconVariant;

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
    /// 带高程：L1 的容器面（`Elevation::Level1.surface_role()` = `SurfaceContainerLow`）+ `Primary` 文本。
    Elevated,
    /// 描边：透明容器 + 1dp `Outline`（非文字元素角色；装饰分隔线才用 `OutlineVariant`）。
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
            // 容器面由**高程令牌**推导（L1 → `SurfaceContainerLow`）：不许在组件里再抄一遍角色名，
            // 换层级口径只改 `elevation.rs` 一处（M248：P2 的落地方式是改来源，不是改色值）。
            Variant::Elevated => (Some(self.elevation().surface_role()), Role::Primary, None),
            // 1dp 边界取 `Outline`（非文字元素、目标 3:1）；`OutlineVariant` 是装饰分隔线的角色，
            // 拿它画边界在 Ink/Silver 上都到不了 3:1，也和 `IconVariant::Outlined` 不一致（M248：P3）。
            Variant::Outlined => (None, Role::OnSurfaceVariant, Some(Role::Outline)),
            Variant::Text => (None, Role::OnSurfaceVariant, None),
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
    /// 尺寸档（规格卡 `BUTTONS / XS - XL`）。产品默认 **S（40 高）**，与 M3 `control_height` 同值。
    size: ControlSize,
    /// **脚下那一层的底**（M234）：禁用态的 10% 容器与 38% 内容叠在它上面。
    base: Base,
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
            size: ControlSize::S,
            base: Base::CANVAS,
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
            size: ControlSize::S,
            base: Base::CANVAS,
        }
    }

    /// 换尺寸档（`XS 32 / S 40 / M 56 / L 96 / XL 136`，圆角一律药丸）。
    ///
    /// 产品页只用 **S**（= M3 `control_height` 40）；M / L / XL 是规格卡的梯子档，给
    /// 空态主行动与展示型入口留着，`--screen=gallery` 的矩阵会把五档一起拍出来。
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
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

    /// **告诉按钮它现在踩在什么底上**（M234）：禁用态的 10% 容器与 38% 内容都叠在这个底上。
    ///
    /// 放进卡片/面板/浮层里**必须**给——不给就按 [`Base::CANVAS`] 合成，禁用态会与容器底
    /// 脱节（M223 的「CR 1.05、形丢失」）。容器自己的 `BASE` 常量就是标准答案：
    /// 卡片 = [`super::surfaces::elevated::BASE`]、菜单项 = `super::inputs::menu::PANEL_BASE`。
    ///
    /// iced 没有环境上下文（样式闭包只拿得到 `&Theme`），所以这条信息只能显式传：
    /// 这是「组件不猜宿主」的代价，也是 M229 那条「参数在、全传死值」的修法。
    pub fn base(mut self, base: impl Into<Base>) -> Self {
        self.base = base.into();
        self
    }

    /// 视觉态：强制的先，否则「没有消息就是禁用」。
    pub(super) fn forced(&self) -> Option<Interaction> {
        self.state.or(if self.on_press.is_none() {
            Some(Interaction::Disabled)
        } else {
            None
        })
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

/// Elevated 按钮（L1 的容器面 + `Primary` 文本，用色调差表达高程、不画阴影）。
pub fn elevated<'a, M: Clone + 'a>(ui: &'a Ui, button: Button<'a, M>) -> Element<'a, M> {
    assemble(ui, Variant::Elevated, button)
}

/// Outlined 按钮（透明 + 1dp `Outline`）。
pub fn outlined<'a, M: Clone + 'a>(ui: &'a Ui, button: Button<'a, M>) -> Element<'a, M> {
    assemble(ui, Variant::Outlined, button)
}

/// Text 按钮（无容器、无描边）。
pub fn text<'a, M: Clone + 'a>(ui: &'a Ui, button: Button<'a, M>) -> Element<'a, M> {
    assemble(ui, Variant::Text, button)
}

/// 图标按钮的描边宽度：**有描边角色就有那一圈，没有就是 0**。
pub(crate) fn icon_stroke(variant: IconVariant) -> f32 {
    if variant.roles().2.is_some() {
        crate::theme::STROKE_PX
    } else {
        0.0
    }
}

/// 图标按钮的**样式描述**（从 `icon()` 抽出来，好让它能被单测直接钉住）。
///
/// 为什么抽：`Style` 闭包不可内省，`icon()` 返回 `Element` 也看不进去——复审把
/// `StyleSpec.stroke` 写死成 `0.0` 时，`icon_stroke()` 的纯函数测试照样绿（它测的是函数，
/// 不是**接线**）。现在「角色 → 填充/内容/描边」这件事在 `icon_style_spec` 里一次成型，
/// 单测断言的就是 `icon()` 真正塞进 `style_closure` 的那个结构体。
pub(crate) fn icon_style_spec(
    variant: IconVariant,
    side: f32,
    forced: Option<Interaction>,
    base: Base,
) -> StyleSpec {
    let (fill, content, border) = variant.roles();
    StyleSpec {
        fill,
        content,
        border,
        // 方形图标按钮同样是 full 圆角（40 → 20），档位取自组件表
        shape: Component::IconButton.spec().shape,
        // M229 复审 P1-1：这里曾硬编码 `0.0`，于是 `Outlined` 与 `Plain` 逐像素相同。
        stroke: icon_stroke(variant),
        radius_side: side,
        base,
        forced,
    }
}

/// IconButton（standard / filled / tonal）：40×40、图标 24、命中区 48。
///
/// **图标语汇纪律（视觉 reviewer P2-3）**：同一个控件里**不混用**填充字形（`IconPaint::Fill`，
/// 如 `Play`/`Stop`/`More`）与描边字形（`Stroke`）——混着放会让线宽看起来忽粗忽细。
/// 允许实心字形，但只能出现在「主操作」这一处（例如播放按钮），其余位置一律描边；
/// gallery 的样例因此统一用描边图标（`Download` + `ChevronRight`），并有单测钉住。
///
/// `on_press: Option<M>`（`None` = 禁用）与 `state` 分开给：前者是语义，后者是取证开关。
///
/// 这是 **画布版**（禁用底 = [`Base::CANVAS`]）：放进卡片/面板里请用 [`icon_on`] 给真实父底。
pub fn icon<'a, M: Clone + 'a>(
    ui: &'a Ui,
    variant: IconVariant,
    kind: IconKind,
    on_press: Option<M>,
    state: Option<Interaction>,
) -> Element<'a, M> {
    icon_on(ui, variant, kind, on_press, state, Base::CANVAS)
}

/// 带**父底**的 IconButton（M234）：禁用态的 10% 容器与 38% 内容叠在 `base` 上。
#[allow(clippy::too_many_arguments)]
pub fn icon_on<'a, M: Clone + 'a>(
    ui: &'a Ui,
    variant: IconVariant,
    kind: IconKind,
    on_press: Option<M>,
    state: Option<Interaction>,
    base: impl Into<Base>,
) -> Element<'a, M> {
    let base = base.into();
    let geometry = IconGeometry::of(ui);
    let (fill, content, _) = variant.roles();
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
    let glyph =
        assemble::content_color(ui, base, forced, content, fill, variant.disabled_content());
    let icon_box =
        layout::centered_fixed(geometry.side, geometry.side, icons::view::<M>(kind, glyph));

    let style = style_closure(ui, icon_style_spec(variant, geometry.side, forced, base));

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
