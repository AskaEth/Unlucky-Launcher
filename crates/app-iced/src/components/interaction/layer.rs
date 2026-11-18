//! **状态层过渡**（M257 / A8）：hover / pressed 的颜色不是硬切，而是按令牌时长插值。
//!
//! ## 这个模块解决什么
//!
//! M3 的状态层不是「另一种灰」，而是**内容色 + 固定 α 叠在容器上**（见 [`super`] 的文档表）。
//! 到 M256 为止，那张表的求值是**瞬时**的：`button::Status` 一变，背景色当帧跳到 8% / 10%。
//! 本模块给这件事加一条**时间轴**，判据是「泵被驱动了几帧 + 端点精确 + 中间态单调」。
//!
//! ## 两条时间轴，一个订阅点
//!
//! 控件状态（hover / pressed）活在各自的 iced `Tree` 里，所以过渡的 `from/to` 也放在那里
//! （见 [`super::hover`] 的包装控件）。本模块只持一条**进程内时钟**与一张**到期表**：
//!
//! | 谁 | 存什么 | 何时推进 |
//! |---|---|---|
//! | 包装控件（`Tree` 状态） | `from` / `to` / 起跑时刻 | `Widget::update` 里改目标 |
//! | 本模块（thread-local） | `NOW` + 到期时刻表 | `motion::advance` 每帧调 [`advance`] |
//!
//! [`is_animating`] 被 `motion::needs_frames()` 问（**唯一的订阅判断点**），所以状态层过渡与
//! 页面 / 模态 / 折叠共用同一条 `window::frames()` 订阅，不另开订阅。
//!
//! ## 起跑必须在 update 侧（M256 的 P1-1 教训）
//!
//! `button` 的 hover 状态只在 `draw` 侧（`style` 闭包）可见，而 `draw` 发生在订阅表求值**之后**
//! ⇒ 若在 draw 侧起跑，「鼠标停住不动」的那段会卡在 `progress = 0`（状态层永远看不见）。
//! 所以目标态的**改变**全部发生在 [`super::hover`] 包装控件的 `Widget::update` 里——那里早于
//! 订阅表求值，帧因此当拍就订阅上。
//!
//! ## `MotionScale::NONE` = 严格等于终值
//!
//! `ms = 0` ⇒ [`Layer::progress`] 恒为 `1.0`、不登记到期 ⇒ [`is_animating`] 不会被它点亮
//! ⇒ **一帧都不订阅**。这不是「很快走完」，是「根本不存在中间态」。

use std::cell::{Cell, RefCell};

use iced::Color;

use unlucky_design::motion::{DurationToken, Easing, MotionScale};

use super::Interaction;
use crate::app::components::motion::{Leg, MAX_STEP_MS};

/// 进（状态层变强：静息→悬停、悬停→按下）：`short1` 50ms + 标准减速。
///
/// 出处：`14-motion-and-animation.md` §3 的 A8 行点名 `Short1`（50ms）。50ms 在 62.5Hz 的采样下
/// 是 3 帧——数量少但**不是硬切**（3 个可判别的中间值），而且 hover 本来就该「快而轻」。
pub const ENTER: Leg = Leg {
    duration: DurationToken::Short1,
    easing: Easing::StandardDecelerate,
};

/// 出（状态层变弱：按下→悬停、悬停→静息）：同档时长 + 标准加速。
///
/// 为什么进出同档而不是「出更短」：50ms 已经是最短档，再短就没有中间态可采样了（回到硬切）。
/// 两条曲线的差别（减速 vs 加速）承担了「进略长于出」的观感差。
pub const EXIT: Leg = Leg {
    duration: DurationToken::Short1,
    easing: Easing::StandardAccelerate,
};

thread_local! {
    /// 进程内时钟（毫秒）。只由 [`advance`] 推。
    #[allow(clippy::missing_const_for_thread_local)]
    static NOW: Cell<f32> = const { Cell::new(0.0) };
    /// 还没到期的过渡（时刻表）。到期的**当场剔除**，否则空闲也一直要帧。
    #[allow(clippy::missing_const_for_thread_local)]
    static DEADLINES: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    /// 正在画的控件栈（[`with_current`] 压、包装控件的 `draw` 用）。
    #[allow(clippy::missing_const_for_thread_local)]
    static STACK: RefCell<Vec<Layer>> = const { RefCell::new(Vec::new()) };
}

/// 当前时刻（毫秒，进程内单调）。
pub fn now_ms() -> f32 {
    NOW.with(Cell::get)
}

/// 推进时钟（`motion::advance` 每帧调一次）。
///
/// 单步上限取 [`MAX_STEP_MS`]（与帧泵同一条）：空闲醒来 / 掉帧时 stale 的帧基准不许把一段
/// 50ms 的过渡一次性抹完（那会让「中间态」在真机上消失，而在单测里照样绿）。
pub fn advance(dt_ms: f64) {
    if !dt_ms.is_finite() || dt_ms <= 0.0 {
        return;
    }
    let step = dt_ms.min(MAX_STEP_MS) as f32;
    NOW.with(|now| now.set(now.get() + step));
    let now = now_ms();
    DEADLINES.with(|list| list.borrow_mut().retain(|deadline| *deadline > now));
}

/// 现在有没有状态层过渡在跑（`motion::needs_frames` 的一个分支）。
pub fn is_animating() -> bool {
    let now = now_ms();
    DEADLINES.with(|list| {
        let mut list = list.borrow_mut();
        list.retain(|deadline| *deadline > now);
        !list.is_empty()
    })
}

/// 一次状态层过渡（值放在控件的 `Tree` 状态里，**不**在这里）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer {
    /// 起点态（颜色从这里出发）。
    from: Interaction,
    /// 目标态（颜色走到这里）。
    to: Interaction,
    /// 起跑时刻。
    at: f32,
    /// 这一腿的时长（已按 `MotionScale` 缩放；`0` = 关着动效）。
    ms: f32,
    /// 这一腿的曲线。
    easing: Easing,
}

impl Layer {
    /// 静息层（还没发生过任何过渡）：进度严格 `1.0`，等价于今天的行为。
    pub const REST: Self = Self {
        from: Interaction::Enabled,
        to: Interaction::Enabled,
        at: 0.0,
        ms: 0.0,
        easing: EXIT.easing,
    };

    /// 出厂的层（与 [`Layer::REST`] 同值；`Default` 便于 `Tree` 状态构造）。
    pub fn new() -> Self {
        Self::REST
    }

    /// 换目标；返回 `true` = 起跑了新的一条腿（调用方该请求重绘）。
    ///
    /// `scale` 是**唯一**的降级口：`NONE` 时 `ms = 0`（进度恒为终值，且不登记到期 ⇒ 零帧）。
    /// 中途反向时 `from` 取**上一个目标**（不是当前插值色）：50ms 的腿上最多差一个 α 档，
    /// 肉眼不可见；换来的是「不用每帧写回状态」（写回会让过渡自我拖动）。
    pub fn retarget(&mut self, state: Interaction, scale: MotionScale) -> bool {
        if state == self.to {
            return false;
        }
        let stronger = rank(state) >= rank(self.to);
        let leg = if stronger { ENTER } else { EXIT };
        self.from = self.to;
        self.to = state;
        self.at = now_ms();
        self.ms = scale.ms(leg.duration);
        self.easing = leg.easing;
        if self.ms > 0.0 {
            // 多留一帧：到期后还至少要再画一次，屏幕上的**最后一帧才是终值**（否则停在 0.97）。
            DEADLINES.with(|list| list.borrow_mut().push(self.at + self.ms + FRAME_GRACE_MS));
        }
        true
    }

    /// 这一帧的进度：`0` = 完全 `from`，`1` = 完全 `to`（**端点逐位精确**）。
    pub fn progress(&self) -> f32 {
        if self.ms <= 0.0 {
            return 1.0;
        }
        let t = (now_ms() - self.at) / self.ms;
        if t >= 1.0 {
            return 1.0;
        }
        if t <= 0.0 {
            return 0.0;
        }
        self.easing.sample(t).clamp(0.0, 1.0)
    }
}

impl Default for Layer {
    fn default() -> Self {
        Self::new()
    }
}

/// 态的「强 → 弱」次序（决定用 [`ENTER`] 还是 [`EXIT`] 那条腿）。
const fn rank(state: Interaction) -> u8 {
    match state {
        Interaction::Disabled => 0,
        Interaction::Enabled => 1,
        Interaction::Hovered => 2,
        Interaction::Focused | Interaction::Pressed => 3,
    }
}

/// 到期之后多等一帧（≈16ms）：保证「最后一帧画的是终值」。
const FRAME_GRACE_MS: f32 = 16.0;

/// 画一棵子树时把它的层压到栈顶（包装控件的 `draw` 里调；`draw` 结束自动弹栈）。
pub fn with_current<R>(layer: Layer, draw: impl FnOnce() -> R) -> R {
    STACK.with(|stack| stack.borrow_mut().push(layer));
    let out = draw();
    STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
    out
}

/// 当前栈顶的过渡；没有包装控件 = `None`（= 调用方按今天的行为求值）。
pub fn current() -> Option<(Interaction, Interaction, f32)> {
    STACK.with(|stack| {
        stack
            .borrow()
            .last()
            .map(|layer| (layer.from, layer.to, layer.progress()))
    })
}

/// 两个不透明色的插值：`p <= 0` 逐位等于 `from`，`p >= 1` 逐位等于 `to`。
///
/// **不产生 α**：两端都是不透明色（D27：GPU/CPU 两档逐像素一致），中间值也是不透明色。
pub fn blend(from: Color, to: Color, p: f32) -> Color {
    if p <= 0.0 {
        return from;
    }
    if p >= 1.0 {
        return to;
    }
    Color {
        r: from.r + (to.r - from.r) * p,
        g: from.g + (to.g - from.g) * p,
        b: from.b + (to.b - from.b) * p,
        a: from.a + (to.a - from.a) * p,
    }
}

#[cfg(test)]
pub fn reset() {
    NOW.with(|now| now.set(0.0));
    DEADLINES.with(|list| list.borrow_mut().clear());
    STACK.with(|stack| stack.borrow_mut().clear());
}

/// 测试专用：按给定毫秒推进（`advance` 读 dt，单测不能等真实时间）。
#[cfg(test)]
pub fn advance_by(ms: f64) {
    advance(ms);
}

#[cfg(test)]
mod tests;
