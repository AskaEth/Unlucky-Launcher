//! M3 Expressive 形状刻度 + shape morph 数据。
//!
//! 形状是「唯一真源」的一部分：组件不许写裸半径（`border_radius(8)`），一律引用
//! [`ShapeScale`]；需要动起来的（按下、选中、展开）用 [`ShapeMorph`] 的 from→to 数据。
//!
//! ## 圆角分两层（D32）
//!
//! 官方实现与官方刻度表并不总一致：Checkbox 的圆角是 **2dp**，但 `ShapeScale` 的刻度是
//! `{0,4,8,12,16,20,28,32,48,full}`——2 不在里面。D32 的判决是**分层**而不是放宽判据：
//!
//! - **容器级**（卡片、弹窗、抽屉、按钮、面板、输入框外壳）只许用 [`ShapeScale`] 的刻度值；
//! - **微控件级**（≤24dp 的 Checkbox 等）用 [`ComponentRadius`] 的白名单值。
//!
//! 一个元素该查哪一层由 [`radius_layer_for`] 判定（按尺寸，唯一入口）；微控件层取值走
//! [`ComponentRadius::radius_px`]，**尺寸超界时返回 `None`**——这不是文档约定，是签名保证。
//!
//! ## 窗口**外框**圆角不在这里（M116）
//!
//! `ShapeScale` 管的是**我们画的东西**：卡片、按钮、浮层、面板。窗口的最外层圆角由操作系统
//! 决定（Win11 / Server 2025 走 DWM 合成器，macOS 走 `layer.cornerRadius`，X11/Wayland 看合成器），
//! 系统**不暴露**那个半径值，我们也不自己定一个数字（用户定案：不做 8/12/16 档）。
//! 所以窗口圆角没有令牌：判据在 `unlucky-app-iced` 的 `platform::plan_corner`，
//! 落点在 `platform/win32/transparency`。
//!
//! 反过来说：**别**为了「让窗口看起来是圆的」去给根容器加 `ShapeScale::ExtraLarge`——那会把
//! 四角裁掉内容（背景图/标题栏底色露出方角），而外框仍然是方的。

use crate::motion::Spring;

/// M3 形状刻度（含 Expressive 新增档）。
///
/// 基础档（M3）：none 0 / extra-small 4 / small 8 / medium 12 / large 16 / extra-large 28 / full。
/// Expressive 新增档：`large-increased` 20 / `extra-large-increased` 32 / `extra-extra-large` 48。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShapeScale {
    /// 直角（0dp）：仅用于贴边容器/分隔。
    None,
    /// 4dp：小控件（chip、小徽标）。
    ExtraSmall,
    /// 8dp：控件（按钮、输入框、菜单项）。
    Small,
    /// 12dp：卡片、列表容器（Expressive 里卡片默认升到这一档）。
    Medium,
    /// 16dp：大卡片、面板。
    Large,
    /// 20dp（Expressive）：按下/选中时的「胀开」档。
    LargeIncreased,
    /// 28dp：浮层、对话框。
    ExtraLarge,
    /// 32dp（Expressive）：大浮层、FAB 展开态。
    ExtraLargeIncreased,
    /// 48dp（Expressive）：超大容器、模态全屏卡。
    ExtraExtraLarge,
    /// 药丸/圆：按边长取半径一半（导航指示条、头像、switch 轨道）。
    Full,
}

impl ShapeScale {
    /// 半径（dp；[`ShapeScale::Full`] 返回 `f32::INFINITY`，由 [`ShapeScale::radius_px`] 折算）。
    pub const fn dp(self) -> f32 {
        match self {
            ShapeScale::None => 0.0,
            ShapeScale::ExtraSmall => 4.0,
            ShapeScale::Small => 8.0,
            ShapeScale::Medium => 12.0,
            ShapeScale::Large => 16.0,
            ShapeScale::LargeIncreased => 20.0,
            ShapeScale::ExtraLarge => 28.0,
            ShapeScale::ExtraLargeIncreased => 32.0,
            ShapeScale::ExtraExtraLarge => 48.0,
            ShapeScale::Full => f32::INFINITY,
        }
    }

    /// 实际渲染半径：`Full` 取边长一半，其余不超过边长一半（避免方角变圆角溢出）。
    pub fn radius_px(self, size_px: f32) -> f32 {
        let half = size_px.max(0.0) / 2.0;
        if self == ShapeScale::Full {
            half
        } else {
            self.dp().min(half)
        }
    }

    /// 是否是 Expressive 才有的档位（基础 M3 没有）。
    pub const fn is_expressive(self) -> bool {
        matches!(
            self,
            ShapeScale::LargeIncreased
                | ShapeScale::ExtraLargeIncreased
                | ShapeScale::ExtraExtraLarge
        )
    }

    /// 在形状阶梯上的序号（0 = None，9 = Full），用于断言形变方向是「胀开」而不是「收紧」。
    pub const fn rank(self) -> u8 {
        match self {
            ShapeScale::None => 0,
            ShapeScale::ExtraSmall => 1,
            ShapeScale::Small => 2,
            ShapeScale::Medium => 3,
            ShapeScale::Large => 4,
            ShapeScale::LargeIncreased => 5,
            ShapeScale::ExtraLarge => 6,
            ShapeScale::ExtraLargeIncreased => 7,
            ShapeScale::ExtraExtraLarge => 8,
            ShapeScale::Full => 9,
        }
    }
}

/// 微控件（≤24dp）的尺寸上限（dp）。
///
/// 超过这条线的元素属**容器级**，圆角必须查 [`ShapeScale`] 的严格刻度；[`ComponentRadius`]
/// 只在这条线以内有效——由 [`ComponentRadius::radius_px`] 在运行期拦住（传大尺寸拿不到微半径）。
/// 24 这个门槛不是官方数字，是 D32 给「微控件」划的界（Checkbox 20、指示点、徽标都在以内）。
pub const MICRO_CONTROL_MAX_SIZE: f32 = 24.0;

/// 微控件圆角（D32 第二层）：**来源官方组件实现，不是 [`ShapeScale`] 的成员**。
///
/// 为什么要有这一层：官方 Checkbox 的圆角是 **2dp**（`Checkbox.kt:738 RadiusSize = 2.dp`，
/// `CheckboxTokens.ContainerShape = RoundedCornerShape(2)`），而 M3 形状刻度只有
/// `{0,4,8,12,16,20,28,32,48,full}`。把 2 塞进刻度会废掉「无任意圆角」这条守门（D30 rubric K1），
/// 不收又只能在组件里抄 `2.0` 字面量。D32 的判决是**分层**，不是放宽：
///
/// | 层 | 管什么 | 取值 |
/// |---|---|---|
/// | [`ShapeScale`] | 容器级：卡片/弹窗/抽屉/按钮/面板/输入框外壳 | 严格刻度，不许自造 |
/// | `ComponentRadius` | 微控件级：Checkbox 等 ≤24dp 的元素 | 白名单，当前只有 [`ComponentRadius::Micro`] |
///
/// 用法：先 [`radius_layer_for`] 判层，微控件再 `radius_px(size)`。**返回 `None` 说明尺寸超界，
/// 必须改查 [`ShapeScale`]**，不许把微半径硬套到容器上。
///
/// 公开的**唯一取值口是 [`ComponentRadius::radius_px`]**（带尺寸闸）：本类型的 `dp()` 是
/// `pub(crate)`，故意不对外开放——否则任何调用方都能无条件拿到 `2.0`，「只许 ≤24dp」就成了
/// 自觉而不是约束（D32 的原意是拦住，不是提醒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentRadius {
    /// 2dp：官方 Checkbox 容器圆角。
    Micro,
}

impl ComponentRadius {
    /// 白名单里的全部微半径（D32 当前只有一项；加值必须先在 `design-令牌约定` 登记）。
    ///
    /// 枚举得出的只是**变体**（`Micro` 这个名字），拿不到数值——取值必须走 [`Self::radius_px`]。
    pub const ALL: [ComponentRadius; 1] = [ComponentRadius::Micro];

    /// 半径（dp）。**`pub(crate)`：crate 内推导与自己用，不对外开放。**
    ///
    /// 对任何尺寸都返回同一个数，所以它天然绕开 D32 的尺寸闸；对外只留
    /// [`Self::radius_px`]（`Option<f32>`，>24dp 返回 `None`）。
    pub(crate) const fn dp(self) -> f32 {
        match self {
            ComponentRadius::Micro => 2.0,
        }
    }

    /// 微控件的实际半径：尺寸超过 [`MICRO_CONTROL_MAX_SIZE`] 时返回 **`None`**。
    ///
    /// 这是 D32 里「能编译期拦住就别靠自觉」的落点——文档约定拦不住，签名拦得住：
    /// 拿到 `None` 的调用点没法「顺手用一下 2dp」，只能改查 [`ShapeScale`]。
    /// 另外半径不超过边长一半（2dp 半径放到更小的元素上会溢成圆）。
    pub fn radius_px(self, size_px: f32) -> Option<f32> {
        if size_px > 0.0 && size_px <= MICRO_CONTROL_MAX_SIZE {
            Some(self.dp().min(size_px / 2.0))
        } else {
            None
        }
    }
}

/// 一个元素的圆角该查哪一层（[`radius_layer_for`] 的返回）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RadiusLayer {
    /// 容器级：查 [`ShapeScale`] 的严格刻度。
    Scale,
    /// 微控件级（≤24dp）：查 [`ComponentRadius`] 的白名单。
    Component,
}

/// 纯函数：按元素尺寸判定圆角该查哪一层。
///
/// 「某尺寸的元素该查哪一个」是 D32 里唯一需要判断的地方，所以只留这一个入口——
/// 组件层不许自己写 `if size <= 24.0`，那等于把阈值抄了第二份。
pub fn radius_layer_for(size_px: f32) -> RadiusLayer {
    if size_px > 0.0 && size_px <= MICRO_CONTROL_MAX_SIZE {
        RadiusLayer::Component
    } else {
        RadiusLayer::Scale
    }
}

/// 一次形状形变（Expressive 的「按下胀开」「展开成药丸」等）。
///
/// 数据形态是纯的：起止档位 + 弹簧参数。适配层负责跑时钟，`progress` 由弹簧/缓动给出。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeMorph {
    /// 静息档位。
    pub from: ShapeScale,
    /// 目标档位。
    pub to: ShapeScale,
    /// 驱动这次形变的弹簧。
    pub spring: Spring,
}

impl ShapeMorph {
    /// 恒定弹簧的形变。
    pub const fn new(from: ShapeScale, to: ShapeScale, spring: Spring) -> Self {
        Self { from, to, spring }
    }

    /// 在 `progress`（0..1，已含缓动）处的半径。`Full` 端点按 `size_px` 折算。
    pub fn radius_px(&self, size_px: f32, progress: f32) -> f32 {
        let t = progress.clamp(0.0, 1.0);
        let a = self.from.radius_px(size_px);
        let b = self.to.radius_px(size_px);
        a + (b - a) * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::SPRING_EXPRESSIVE_SPATIAL_DEFAULT;

    #[test]
    fn scale_ladder_is_monotone() {
        let ladder = [
            ShapeScale::None,
            ShapeScale::ExtraSmall,
            ShapeScale::Small,
            ShapeScale::Medium,
            ShapeScale::Large,
            ShapeScale::LargeIncreased,
            ShapeScale::ExtraLarge,
            ShapeScale::ExtraLargeIncreased,
            ShapeScale::ExtraExtraLarge,
        ];
        for pair in ladder.windows(2) {
            assert!(
                pair[0].dp() < pair[1].dp(),
                "{:?} 应小于 {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn full_is_half_of_edge_and_clamped() {
        assert_eq!(ShapeScale::Full.radius_px(48.0), 24.0);
        assert_eq!(ShapeScale::ExtraLarge.radius_px(20.0), 10.0);
        assert_eq!(ShapeScale::ExtraLarge.radius_px(200.0), 28.0);
        assert_eq!(ShapeScale::None.radius_px(200.0), 0.0);
    }

    #[test]
    fn morph_hits_both_endpoints() {
        let morph = ShapeMorph::new(
            ShapeScale::Small,
            ShapeScale::LargeIncreased,
            SPRING_EXPRESSIVE_SPATIAL_DEFAULT,
        );
        assert_eq!(morph.radius_px(200.0, 0.0), 8.0);
        assert_eq!(morph.radius_px(200.0, 1.0), 20.0);
        assert_eq!(morph.radius_px(200.0, 0.5), 14.0);
        // 越界 progress 夹取，不产生负半径/超大半径。
        assert_eq!(morph.radius_px(200.0, -2.0), 8.0);
        assert_eq!(morph.radius_px(200.0, 3.0), 20.0);
    }

    #[test]
    fn component_radius_is_whitelisted_and_refuses_containers() {
        // D32：微半径白名单当前只有 2dp，且永远不是 ShapeScale 的成员。
        assert_eq!(ComponentRadius::ALL.len(), 1);
        assert_eq!(ComponentRadius::Micro.dp(), 2.0);
        for scale in [
            ShapeScale::None,
            ShapeScale::ExtraSmall,
            ShapeScale::Small,
            ShapeScale::Medium,
            ShapeScale::Large,
            ShapeScale::LargeIncreased,
            ShapeScale::ExtraLarge,
            ShapeScale::ExtraLargeIncreased,
            ShapeScale::ExtraExtraLarge,
        ] {
            assert_ne!(scale.dp(), 2.0, "{scale:?} 不该等于微半径（2 不在刻度里）");
        }

        // 20×20 的 checkbox 拿 2dp；超 24 的元素必须拿不到。
        assert_eq!(ComponentRadius::Micro.radius_px(20.0), Some(2.0));
        assert_eq!(ComponentRadius::Micro.radius_px(24.0), Some(2.0));
        assert_eq!(ComponentRadius::Micro.radius_px(24.5), None);
        assert_eq!(ComponentRadius::Micro.radius_px(200.0), None);
        assert_eq!(ComponentRadius::Micro.radius_px(0.0), None);
        // 半径不超过边长一半。
        assert_eq!(ComponentRadius::Micro.radius_px(2.0), Some(1.0));
    }

    #[test]
    fn radius_layer_follows_the_size_threshold() {
        assert_eq!(radius_layer_for(20.0), RadiusLayer::Component);
        assert_eq!(
            radius_layer_for(MICRO_CONTROL_MAX_SIZE),
            RadiusLayer::Component
        );
        assert_eq!(radius_layer_for(24.5), RadiusLayer::Scale);
        assert_eq!(radius_layer_for(320.0), RadiusLayer::Scale);
        assert_eq!(radius_layer_for(0.0), RadiusLayer::Scale);
    }
}
