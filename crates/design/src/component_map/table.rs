//! 组件档位数据表（**下标必须等于 `Component as usize`**，由测试钉死）。
//!
//! 一行一个组件。改值就是在改全站观感——同步协作文档再改。
//!
//! **末尾 5 行（M69 G9）** 的来源是 `research/15` §18~§23（同一套 `material3-android:1.4.0` 源码）：
//! Checkbox / Radio / Search / NavigationDrawer / Divider。其中 Checkbox 的 **2dp** 圆角走
//! [`ComponentRadius`]（D32 第二层，不塞进 `ShapeScale`），Divider 的厚度是一像素线（G12）而不在这张表里。
//!
//! **高程/形状取值来源（A 级）**：`androidx.compose.material3.tokens` 的各组件 `<X>Tokens.kt`
//! （FilledCard/OutlinedCard/ElevatedCard/Menu/Dialog/Snackbar/AppBar/List）。已登记的两处偏离：
//! - `Tooltip`：`PlainTooltipTokens.kt` 只有 Corner 行、**没有 Elevation 行**，Table 的 Level2 是
//!   B 级（沿用 M3 旧规范「浮层 3dp」的直觉），有官方值后以官方为准；
//! - `ListItem`：官方 `ListTokens` 用 `CornerNone`（直角行），本项目取 `Small(8)`——列表行在本
//!   启动器里是**卡片内的独立可点块**（不是贴边通栏行），圆角与卡片同族更协调；偏差 8dp 已登记。

use super::{Component, ComponentSpec};
use crate::elevation::Elevation;
use crate::motion::{
    DurationToken, Easing, SPRING_EXPRESSIVE_SPATIAL_DEFAULT, SPRING_EXPRESSIVE_SPATIAL_FAST,
};
use crate::shape::{ComponentRadius, ShapeMorph, ShapeScale};
use crate::typography::TypeRole;

/// 一个组件的档位行（`const fn` 便捷构造，字段名在 [`ComponentSpec`] 里）。
const fn spec(
    component: Component,
    shape: ShapeScale,
    elevation: Elevation,
    text: TypeRole,
    support: Option<TypeRole>,
    motion: Easing,
    duration: DurationToken,
    morph: Option<ShapeMorph>,
) -> ComponentSpec {
    ComponentSpec {
        component,
        shape,
        micro_radius: None,
        elevation,
        text,
        support,
        motion,
        duration,
        morph,
    }
}

/// 圆角走「微控件层」（D32）的档位行：`shape` 自动填占位 `None`，半径取白名单值。
///
/// 为什么单独一个构造器：`micro_radius` 与 `shape = None` 必须成对出现，用构造器保证配对，
/// 不让 25 行表里出现「填了微半径忘了把 shape 清掉」。
const fn spec_micro(
    component: Component,
    micro_radius: ComponentRadius,
    elevation: Elevation,
    text: TypeRole,
    support: Option<TypeRole>,
    motion: Easing,
    duration: DurationToken,
    morph: Option<ShapeMorph>,
) -> ComponentSpec {
    ComponentSpec {
        component,
        shape: ShapeScale::None,
        micro_radius: Some(micro_radius),
        elevation,
        text,
        support,
        motion,
        duration,
        morph,
    }
}

/// 25 个组件的档位表。
pub(super) const SPECS: [ComponentSpec; 25] = [
    spec(
        Component::TopAppBar,
        ShapeScale::None,
        Elevation::Level0,
        TypeRole::TitleLarge,
        Some(TypeRole::BodySmall),
        Easing::Standard,
        DurationToken::Medium2,
        None,
    ),
    spec(
        Component::NavigationItem,
        ShapeScale::Full,
        Elevation::Level0,
        TypeRole::LabelLarge,
        None,
        Easing::Emphasized,
        DurationToken::Medium2,
        Some(ShapeMorph::new(
            ShapeScale::Large,
            ShapeScale::Full,
            SPRING_EXPRESSIVE_SPATIAL_FAST,
        )),
    ),
    spec(
        Component::PrimaryButton,
        ShapeScale::Full,
        Elevation::Level0,
        TypeRole::LabelLarge,
        None,
        Easing::Emphasized,
        DurationToken::Short4,
        None,
    ),
    spec(
        Component::SecondaryButton,
        ShapeScale::Full,
        Elevation::Level0,
        TypeRole::LabelLarge,
        None,
        Easing::Standard,
        DurationToken::Short4,
        None,
    ),
    spec(
        Component::TextButton,
        ShapeScale::Full,
        Elevation::Level0,
        TypeRole::LabelLarge,
        None,
        Easing::Standard,
        DurationToken::Short4,
        None,
    ),
    spec(
        Component::IconButton,
        ShapeScale::Full,
        Elevation::Level0,
        TypeRole::LabelMedium,
        None,
        Easing::Standard,
        DurationToken::Short3,
        Some(ShapeMorph::new(
            ShapeScale::Medium,
            ShapeScale::Full,
            SPRING_EXPRESSIVE_SPATIAL_FAST,
        )),
    ),
    // FilledCardTokens.ContainerElevation = Level0（hover 才到 L1、dragged 到 L3）。
    spec(
        Component::OutlinedCard,
        ShapeScale::Medium,
        Elevation::Level0,
        TypeRole::TitleMedium,
        Some(TypeRole::BodySmall),
        Easing::Standard,
        DurationToken::Medium1,
        None,
    ),
    spec(
        Component::FilledCard,
        ShapeScale::Medium,
        Elevation::Level0,
        TypeRole::TitleMedium,
        Some(TypeRole::BodySmall),
        Easing::Standard,
        DurationToken::Medium1,
        Some(ShapeMorph::new(
            ShapeScale::Medium,
            ShapeScale::Large,
            SPRING_EXPRESSIVE_SPATIAL_DEFAULT,
        )),
    ),
    spec(
        Component::ListItem,
        ShapeScale::Small,
        Elevation::Level0,
        TypeRole::BodyLarge,
        Some(TypeRole::BodySmall),
        Easing::Standard,
        DurationToken::Short4,
        None,
    ),
    spec(
        Component::TextField,
        ShapeScale::ExtraSmall,
        Elevation::Level0,
        TypeRole::BodyLarge,
        Some(TypeRole::BodySmall),
        Easing::Standard,
        DurationToken::Short4,
        None,
    ),
    spec(
        Component::SegmentedButton,
        ShapeScale::Full,
        Elevation::Level0,
        TypeRole::LabelLarge,
        None,
        Easing::Emphasized,
        DurationToken::Medium2,
        None,
    ),
    spec(
        Component::Chip,
        ShapeScale::Small,
        Elevation::Level0,
        TypeRole::LabelMedium,
        None,
        Easing::Standard,
        DurationToken::Short3,
        Some(ShapeMorph::new(
            ShapeScale::ExtraSmall,
            ShapeScale::Small,
            SPRING_EXPRESSIVE_SPATIAL_FAST,
        )),
    ),
    spec(
        Component::Switch,
        ShapeScale::Full,
        Elevation::Level0,
        TypeRole::BodyMedium,
        Some(TypeRole::BodySmall),
        Easing::Emphasized,
        DurationToken::Medium2,
        None,
    ),
    spec(
        Component::Slider,
        ShapeScale::Full,
        Elevation::Level0,
        TypeRole::BodyMedium,
        Some(TypeRole::BodySmall),
        Easing::Standard,
        DurationToken::Short2,
        None,
    ),
    spec(
        Component::ProgressIndicator,
        ShapeScale::Full,
        Elevation::Level0,
        TypeRole::LabelMedium,
        None,
        Easing::Linear,
        DurationToken::Long2,
        None,
    ),
    // DialogTokens.ContainerElevation = Level3，CornerExtraLarge = 28。
    spec(
        Component::Dialog,
        ShapeScale::ExtraLarge,
        Elevation::Level3,
        TypeRole::HeadlineSmall,
        Some(TypeRole::BodyMedium),
        Easing::EmphasizedDecelerate,
        DurationToken::Long4,
        Some(ShapeMorph::new(
            ShapeScale::Large,
            ShapeScale::ExtraLarge,
            SPRING_EXPRESSIVE_SPATIAL_DEFAULT,
        )),
    ),
    // MenuTokens：CornerExtraSmall(4) + Level2。
    spec(
        Component::Menu,
        ShapeScale::ExtraSmall,
        Elevation::Level2,
        TypeRole::BodyMedium,
        None,
        Easing::StandardDecelerate,
        DurationToken::Short4,
        None,
    ),
    spec(
        Component::Snackbar,
        ShapeScale::ExtraSmall,
        Elevation::Level3,
        TypeRole::BodyMedium,
        None,
        Easing::EmphasizedDecelerate,
        DurationToken::Medium2,
        None,
    ),
    spec(
        Component::Tooltip,
        ShapeScale::ExtraSmall,
        Elevation::Level2,
        TypeRole::BodySmall,
        None,
        Easing::StandardDecelerate,
        DurationToken::Short2,
        None,
    ),
    spec(
        Component::EmptyState,
        ShapeScale::Large,
        Elevation::Level0,
        TypeRole::HeadlineSmall,
        Some(TypeRole::BodySmall),
        Easing::Standard,
        DurationToken::Medium2,
        None,
    ),
    // ---- M69 G9：末尾追加的 5 个（`research/15` §18~§23；前 20 行次序不许动）----
    // Checkbox：圆角 **2dp** 不在 `ShapeScale` 刻度里 → 走 `spec_micro`（D32 第二层）。
    // 高程 L0：`CheckboxTokens` 没有 Elevation 行，容器是「描边/填充」两态而非浮层。
    // 字级 `BodyLarge` 是**团队口径**：官方没有「框 + 标签」组合 token（§18.9），取列表行同一档。
    // 动效取 M3 motion scheme 的 fast-effects（100ms standard）——[推导]，非 token 原文。
    spec_micro(
        Component::Checkbox,
        ComponentRadius::Micro,
        Elevation::Level0,
        TypeRole::BodyLarge,
        None,
        Easing::Standard,
        DurationToken::Short2,
        None,
    ),
    // Radio：真圆（`RadioButtonTokens.IconSize = 20` + `CornerFull`），内点/环线宽见
    // `ComponentMetrics::RADIO`。字级同 Checkbox（团队口径）。
    spec(
        Component::Radio,
        ShapeScale::Full,
        Elevation::Level0,
        TypeRole::BodyLarge,
        None,
        Easing::Standard,
        DurationToken::Short2,
        None,
    ),
    // Search bar：圆角 `CornerFull` / `SearchBarCornerRadius = 56/2 = 28`。
    // **高程取 L0**——`SearchBarTokens.ContainerElevation = Level3` 与
    // `SearchBarDefaults.TonalElevation/ShadowElevation = Level0` 冲突（§26.2 矛盾 ②），
    // 判据按观感取「无可见阴影 + tone 分层」，这条已在 `design-令牌约定` 登记。
    // 无 shape morph：bar ⇄ docked 展开时**半径不变**（28 → 28），只长面积（§20C）。
    spec(
        Component::Search,
        ShapeScale::Full,
        Elevation::Level0,
        TypeRole::BodyLarge,
        Some(TypeRole::BodySmall),
        Easing::Emphasized,
        DurationToken::Medium2,
        None,
    ),
    // Navigation drawer：官方 `ContainerShape = CornerLargeEnd`（只有靠内容那侧两角 16，
    // 贴边侧直角）。`ShapeScale` 表达不了「部分角」，这里记内侧角的 `Large(16)`，
    // 哪两个角由组件按侧位决定；写四角 16 = 变成浮动卡片（§21.2 判据）。
    // 常驻形态 `Surface` + L0（模态才用 `SurfaceContainerLow` + scrim，本项目只在 <600 窗宽兜底）。
    spec(
        Component::NavigationDrawer,
        ShapeScale::Large,
        Elevation::Level0,
        TypeRole::LabelLarge,
        Some(TypeRole::TitleSmall),
        Easing::Emphasized,
        DurationToken::Medium2,
        None,
    ),
    // Divider：无高程、无圆角、**无文字**（`text` 是占位值，消费方不得据此排版）；
    // 厚度是一像素线 `LayoutMetrics::divider_thickness`，颜色 `OutlineVariant`
    // （search view 头部分隔是 `Outline`，两个不同角色，§20.8）。
    // **`motion` / `duration` 也是占位值**（`ComponentSpec.motion` 不是 `Option`）：Divider 没有
    // 官方状态过渡令牌，消费方**不得据此排动效**；下轮把该字段改成 `Option` 更彻底。
    spec(
        Component::Divider,
        ShapeScale::None,
        Elevation::Level0,
        TypeRole::BodySmall,
        None,
        Easing::Standard,
        DurationToken::Short2,
        None,
    ),
];
