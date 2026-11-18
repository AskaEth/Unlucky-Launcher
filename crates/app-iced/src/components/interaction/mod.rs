//! 交互状态层：hover / focus / pressed / disabled 的**唯一实现点**（D39 §3）。
//!
//! ## 为什么必须只有一处
//!
//! M3 的状态层不是「另一种灰」，而是**内容色自身 + 固定 alpha** 叠在容器色上
//! （`unlucky_design::color::state` 的文档）。五个家族各写一遍
//! `if hovered { 0.08 }`，第一周就会出现 0.08 / 0.1 / 0.12 三种 hover——
//! 这一层存在的意义就是让「组件里出现 α 字面量」变成不可能：
//!
//! | 态 | α | 令牌来源 |
//! |---|---|---|
//! | hover | 0.08 | [`StateLayer::Hover`] |
//! | focus | 0.10 | [`StateLayer::Focus`] |
//! | pressed | 0.10 | [`StateLayer::Pressed`] |
//! | dragged | 0.16 | [`StateLayer::Dragged`]（按钮家族不用，列表拖拽用） |
//! | disabled 容器（**按钮家族**） | 0.10 | [`DISABLED_CONTAINER_ALPHA`]（按钮 `*ButtonTokens.DisabledContainerOpacity`） |
//! | disabled 容器/描边（**面状控件族**） | 0.12 | [`DISABLED_SURFACE_ALPHA`]（chip / slider 轨道 / switch 轨道 / TextField 禁用描边） |
//! | disabled 内容 | 0.38 | [`DISABLED_CONTENT_ALPHA`] |
//!
//! ## ⚠️ 禁用 α 有**两类**（M86 收口，别再合成一个数）
//!
//! 官方禁用 α 从来不是「一个 12%」：
//!
//! - **按钮容器 0.10**：`BaselineButtonTokens` / `FilledButtonTokens` / `OutlinedButtonTokens` /
//!   `TextButtonTokens` / `ElevatedButtonTokens` / `TonalButtonTokens` 的 `DisabledContainerOpacity`
//!   = `0.1f`（AAR 字节码 `<clinit>` 里的 `ldc 0.1f`）；
//! - **面状控件 0.12**：`AssistChipTokens.FlatDisabledOutlineOpacity` / `FilterChipTokens.
//!   FlatDisabledSelectedContainerOpacity` / `SliderTokens.DisabledInactiveTrackOpacity` /
//!   `SwitchTokens.DisabledTrackOpacity` 都是 `0.12f`。
//!
//! 本模块过去只有一个 12% 常量供两条线共用：**按钮那半边一直错、chip/slider/switch 那半边一直对**。
//! 现在按族分两个入口，谁也别再拿一个数服务两族：
//!
//! | 入口 | α | 谁用 |
//! |---|---|---|
//! | [`disabled_button_colors`] / [`disabled_button_content`] | 0.10 | **只有按钮家族**（`buttons::assemble`） |
//! | [`disabled_colors`] / [`disabled_content_over_role`] | 0.12 | **面状控件族**：chip 容器与描边、switch 轨道、slider 未激活轨、TextField 禁用描边（`disabled_colors` 这个名字是历史遗留，**语义是面状控件族**） |
//!
//! 上表右列是**唯一**允许的取值口；左列只是给人看的，代码里一个数字都不许出现
//! （`components::gate` 会扫）。
//!
//! ## 合成的「底」是参数，不是常量（M234）
//!
//! 四个入口都收一个 [`Base`]——控件**脚下那一层的底**，由画那个底的容器导出
//! （`surfaces::elevated::BASE` / `inputs::menu::PANEL_BASE` / 各控件的 `.base(..)`）。
//! M229 之前这个值恒等于 `Role::Surface`，于是卡片/面板里的禁用控件把 10%/12% 叠在画布色上
//! （M223 的「CR 1.05、形丢失」）。**不许**在入口内部再写一个默认值回来。
//!
//! ## 选中态有它自己的入口（M296）
//!
//! 选中项的底是**实色**（`SecondaryContainer`），状态层叠在它上面 —— 所以它不是
//! [`layer_color`] 的「`None` 就是不画底」那种情形，而是 [`selected_fill`]（静息 = 实色容器）。
//! 菜单项 / 列表行 / 导航项 / 版本选择行一律调那一个入口，**不许**各写一份
//! `overlay_color(..).or_else(|| 实色)`。两条分支不许合并：未选中的静息没有底，选中项的静息有底。
//!
//! ## 两个消费形态
//!
//! 1. **iced `button` 系**（本 mission 的按钮家族）：拿 [`Interaction`] 直接配 [`overlay_color`]；
//! 2. **自定义 `Widget`**（后续若需要自绘轨道/滑块）：用 [`Tracker`] 把事件折成五态，
//!    `update()` 的返回值告诉你「要不要重绘」，`state()` 给出当前态。
//!
//! ## 与 `Ui::state_over` 的关系
//!
//! 颜色一律经 [`crate::theme::Ui::state_over`] 求值（它返回**不透明**结果，所以 GPU/CPU 两档
//! 逐像素一致，D27）。本模块只做「哪一态叠哪一层」的映射，不做任何颜色算术。

use iced::widget::button;
use iced::Color;

use unlucky_design::color::{Role, StateLayer};

use crate::theme::Ui;

mod hover;
mod tracker;

/// 状态层**过渡的时间轴**（M257 / A8）：hover / pressed 按令牌时长插值，不再是硬切。
pub mod layer;

// 事件 → 态的**状态机**在 [`tracker`]：M257 起它有真消费者——[`hover`] 的取样控件。
pub use hover::tracked;
pub use tracker::Tracker;

// 本模块会被四条家族线各扩一次：加东西前先看行数余额（单文件 ≤400，AGENTS.md §3-1）。
// `mod.rs` 只装「五态 + 颜色映射」这类纯函数；事件 → 态的**状态机在 [`tracker`]**，
// 需要新的状态机就加到那边，别把本文件再撑到红线上。

/// 一个控件的视觉态（五态，M3 的完整集合）。
///
/// `Enabled` 是**静息态**（不叠状态层）；`Focused` / `Pressed` 在 M3 里 α 相同（都 10%），
/// 所以两者在图上会一模一样——这是规范，不是漏画（12b §1.8 判据只要求 α 对得上）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Interaction {
    /// 静息：可交互，不叠状态层。
    Enabled,
    /// 悬停：叠 8%。
    Hovered,
    /// 键盘焦点：叠 10%。
    Focused,
    /// 按下：叠 10%。
    Pressed,
    /// 禁用：按钮容器 10% / 面状控件 12% / 内容 38%，**不叠状态层**。
    Disabled,
}

impl Interaction {
    /// 五态全序（gallery 的状态矩阵按这个次序出图）。
    pub const ALL: [Interaction; 5] = [
        Interaction::Enabled,
        Interaction::Hovered,
        Interaction::Pressed,
        Interaction::Focused,
        Interaction::Disabled,
    ];

    /// 稳定名字（截图文件名 / 图例 / 测试失败信息用）。
    pub const fn name(self) -> &'static str {
        match self {
            Interaction::Enabled => "enabled",
            Interaction::Hovered => "hovered",
            Interaction::Focused => "focused",
            Interaction::Pressed => "pressed",
            Interaction::Disabled => "disabled",
        }
    }

    /// 是否禁用（禁用不是「第六态」，它要不要画容器由组件决定）。
    pub const fn is_disabled(self) -> bool {
        matches!(self, Interaction::Disabled)
    }

    /// 该态要叠的状态层；`None` = 不叠（静息与禁用）。
    pub const fn layer(self) -> Option<StateLayer> {
        match self {
            Interaction::Enabled | Interaction::Disabled => None,
            Interaction::Hovered => Some(StateLayer::Hover),
            Interaction::Focused => Some(StateLayer::Focus),
            Interaction::Pressed => Some(StateLayer::Pressed),
        }
    }

    /// 从 iced 的 [`button::Status`] 推导（`button` 只有四态，没有 focus）。
    ///
    /// 用 `button` 做组件的家族一律经这里，**不许自己 match status 再各写一遍颜色**。
    pub const fn from_status(status: button::Status) -> Self {
        match status {
            button::Status::Active => Interaction::Enabled,
            button::Status::Hovered => Interaction::Hovered,
            button::Status::Pressed => Interaction::Pressed,
            button::Status::Disabled => Interaction::Disabled,
        }
    }
}

/// 当前态叠在 `container` 上的**不透明**颜色；`None` = 该态不叠。
///
/// `content` 是容器上的前景角色（Filled 按钮用 `OnPrimary`；中性表面上的控件用 `OnSurfaceVariant`）。
pub fn overlay_color(ui: &Ui, state: Interaction, content: Role, container: Role) -> Option<Color> {
    state
        .layer()
        .map(|layer| ui.state_over(layer, content, container))
}

/// 这一帧的状态层颜色（**组件唯一入口**，A8 起带时间轴）。
///
/// 三种情形，前两种都**逐位等于** M257 之前的输出：
///
/// | 情形 | 返回 | 为什么 |
/// |---|---|---|
/// | 没有取样控件（gallery 强制态 / 未包装路径） | [`overlay_color`] | 取证图与旧基线一字不差 |
/// | 有取样控件且过渡已到位（`p >= 1`） | 目标态的颜色 | 静息与终态都是**精确**的令牌求值 |
/// | 过渡中（`0 < p < 1`） | 两态不透明色的插值 | 中间帧既不是起点也不是终点（可判别的中间态） |
///
/// 中间态为什么能插值而不产生半透明：两端都是 `state_over` 合成的**不透明**色，线性插值的结果
/// 也不透明（D27：GPU/CPU 两档逐像素一致）。`back` 是状态层合成的底（填充按钮 = 容器角色，
/// 透明容器 = [`Base`]），当前态不叠层时以它为端点色（那正是「没画状态层」时看到的颜色）。
pub fn layer_color(ui: &Ui, state: Interaction, content: Role, back: Role) -> Option<Color> {
    let Some((from, to, p)) = layer::current() else {
        return overlay_color(ui, state, content, back);
    };
    let source = overlay_color(ui, from, content, back);
    let target = overlay_color(ui, to, content, back);
    if p >= 1.0 {
        return target;
    }
    if p <= 0.0 {
        return source;
    }
    // 端点缺失（静息态不叠层）时用**底**补一个不透明色：那正是用户眼里的「没有状态层」。
    let backdrop = ui.color(back);
    Some(layer::blend(
        source.unwrap_or(backdrop),
        target.unwrap_or(backdrop),
        p,
    ))
}

mod base;
mod disabled;
mod selected;

pub use base::Base;
pub use disabled::{
    disabled_button_colors, disabled_button_content, disabled_colors, disabled_content_over_role,
};
pub use selected::selected_fill;

#[cfg(test)]
mod tests {
    use super::*;
    use unlucky_design::color::SchemeFlavor;
    use unlucky_design::typography::Platform;

    use crate::theme::Families;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    fn ui(flavor: SchemeFlavor) -> Ui {
        Ui::with_platform(flavor, Platform::Windows, FAMILIES)
    }

    /// 契约 §3 的状态层 α 全表：**钉住令牌值**（改令牌会让这条红，改组件抓不到）。
    ///
    /// 禁用态的两族 α（按钮 0.10 / 面状控件 0.12）在 [`super::disabled`] 的测试里，
    /// 那边还负责「两族不能同源」的判据。
    #[test]
    fn state_layer_alphas_are_the_contract_values() {
        assert!((StateLayer::Hover.alpha() - 0.08).abs() < f32::EPSILON);
        assert!((StateLayer::Focus.alpha() - 0.10).abs() < f32::EPSILON);
        assert!((StateLayer::Pressed.alpha() - 0.10).abs() < f32::EPSILON);
        assert!((StateLayer::Dragged.alpha() - 0.16).abs() < f32::EPSILON);
    }

    #[test]
    fn five_states_map_onto_four_layers() {
        assert_eq!(Interaction::ALL.len(), 5);
        assert_eq!(Interaction::Enabled.layer(), None);
        assert_eq!(Interaction::Disabled.layer(), None);
        assert_eq!(Interaction::Hovered.layer(), Some(StateLayer::Hover));
        assert_eq!(Interaction::Focused.layer(), Some(StateLayer::Focus));
        assert_eq!(Interaction::Pressed.layer(), Some(StateLayer::Pressed));
        // focus 与 pressed 同 α（M3 口径）——图上一样是**对的**
        assert_eq!(
            Interaction::Focused.layer().map(StateLayer::alpha),
            Interaction::Pressed.layer().map(StateLayer::alpha)
        );
    }

    #[test]
    fn overlay_color_is_the_token_blend_not_local_math() {
        for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
            let ui = ui(flavor);
            for (state, layer) in [
                (Interaction::Hovered, StateLayer::Hover),
                (Interaction::Focused, StateLayer::Focus),
                (Interaction::Pressed, StateLayer::Pressed),
            ] {
                assert_eq!(
                    overlay_color(&ui, state, Role::OnPrimary, Role::Primary),
                    Some(ui.state_over(layer, Role::OnPrimary, Role::Primary)),
                    "{flavor:?}/{state:?} 必须逐位等于令牌求值结果"
                );
                assert!(
                    overlay_color(&ui, state, Role::OnSurface, Role::Surface).is_some(),
                    "可交互态必须给颜色"
                );
            }
            assert_eq!(
                overlay_color(&ui, Interaction::Enabled, Role::OnSurface, Role::Surface),
                None
            );
            assert_eq!(
                overlay_color(&ui, Interaction::Disabled, Role::OnSurface, Role::Surface),
                None,
                "禁用态不叠状态层（它换的是容器/内容色）"
            );
        }
    }

    /// **没有取样控件时，[`layer_color`] 逐位等于 M257 之前的 [`overlay_color`]**。
    ///
    /// 抓得住：有人把时间轴接进求值路径时顺手改了静息/终态的颜色（D30 的旧基线会漂）。
    /// 抓不住：过渡中间帧（那是下一条用例与 `layer::tests` 的事）。
    #[test]
    fn layer_color_without_a_sampler_is_bit_identical_to_overlay_color() {
        for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
            let ui = ui(flavor);
            for state in Interaction::ALL {
                for (content, back) in [
                    (Role::OnPrimary, Role::Primary),
                    (Role::OnSurface, Role::Surface),
                    (Role::OnSurfaceVariant, Role::SurfaceContainerLow),
                ] {
                    assert_eq!(
                        layer_color(&ui, state, content, back),
                        overlay_color(&ui, state, content, back),
                        "{flavor:?}/{state:?}/{content:?} 必须逐位退回旧口径"
                    );
                }
            }
        }
    }

    /// **过渡中间帧：颜色落在两端之间，且不透明**（D27：不许把 α 交给渲染器）。
    ///
    /// 抓得住：①中间帧被写成硬切（`p` 被忽略 ⇒ 起点色或终点色，`assert_ne` 红）；
    /// ②有人为了「淡出」改用 `Color { a: 0.5 }`（末尾的「必须不透明」红）。
    /// 抓不住：观感（8%/10% 的差在图上只有几级色阶，好不好看得看图）。
    #[test]
    fn a_running_transition_yields_an_opaque_colour_between_the_two_ends() {
        use super::layer;

        let ui = ui(SchemeFlavor::Light);
        layer::reset();
        let resting = overlay_color(&ui, Interaction::Enabled, Role::OnSurface, Role::Surface);
        let hovered = overlay_color(&ui, Interaction::Hovered, Role::OnSurface, Role::Surface)
            .expect("hover 有颜色");

        let mut state = layer::Layer::new();
        state.retarget(
            Interaction::Hovered,
            unlucky_design::motion::MotionScale::STANDARD,
        );
        layer::advance_by(16.0);

        let mid = layer::with_current(state, || {
            layer_color(&ui, Interaction::Hovered, Role::OnSurface, Role::Surface)
        })
        .expect("过渡中必须有颜色");

        assert_eq!(mid.a, 1.0, "中间帧也必须不透明（D27）");
        assert!(
            resting.is_none(),
            "前提：静息态本来不叠层（透明容器的情形）"
        );
        assert_ne!(mid, hovered, "中间帧不许等于终点色（那就是硬切）");
        let backdrop = ui.color(Role::Surface);
        assert_ne!(mid, backdrop, "中间帧也不许还停在起点色的底上");
        layer::reset();
    }

    #[test]
    fn hover_is_weaker_than_pressed_on_the_same_container() {
        let ui = ui(SchemeFlavor::Light);
        let hover = overlay_color(&ui, Interaction::Hovered, Role::OnSurface, Role::Surface)
            .expect("hover 有颜色");
        let pressed = overlay_color(&ui, Interaction::Pressed, Role::OnSurface, Role::Surface)
            .expect("pressed 有颜色");
        assert_ne!(hover, pressed, "8% 与 10% 必须能区分");
        assert_eq!(hover.a, 1.0, "结果必须不透明（GPU/CPU 逐像素一致）");
    }
}
