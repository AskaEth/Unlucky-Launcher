//! **动效帧泵**（M192）：design 的 motion 令牌 → iced 的**有界**帧订阅。
//!
//! ## 这个模块解决什么
//!
//! `crates/design/src/motion/` 里令牌一直齐着（16 档时长 / 可采样缓动 / 二维弹簧表 /
//! `MotionScale::NONE` 降级），但适配层从没消费过 ⇒ 所有状态变化都是硬切。本模块是那条缺失的
//! 中间层：**谁能动**（[`Slot`]）、**动多久**（[`spec`] 的令牌表）、**什么时候该订阅帧**
//! （[`needs_frames`]）。
//!
//! ## 有界泵的契约（四条，都有守卫）
//!
//! | 契约 | 为什么 | 守在哪 |
//! |---|---|---|
//! | 只在**有轨道**时订阅帧 | `window::frames()` 一订阅就自持重绘循环（实测几百帧/秒） | `pump::tests::a_finished_animation_stops_asking_for_frames` |
//! | 轨道到期**当场删除**，不靠「下次状态变化」 | 删晚了就是空闲烧帧 | 同上（把 `advance` 的清理删掉就红） |
//! | **时间轴推进**不密于 [`FRAME_MS`]（16 ms ≈ 62.5/s），且单次最多吃 [`pump::MAX_STEP_MS`] | 帧到得再快也只该改这么多次布局；空闲醒来时 stale 的帧基准不许把动画一帧杀完 | `pump::tests::frames_are_capped_at_sixty_per_second`、`pump::tests::a_stale_baseline_cannot_kill_the_track_in_one_sample` |
//! | 起跑**必须发生在 `update` 侧**（[`observe`]），`view` 侧只采样（[`sample`]） | iced 先算订阅表（`update()` 内部）后建视图 ⇒ 在 `view` 里插轨道当拍订阅不上，动画会退化成「一帧偏移 + 闪跳」 | `app::subscription::tests::a_state_change_opens_the_frame_subscription_in_the_same_tick` |
//!
//! ⚠️ **16 ms 只卡「采样与布局推进」，不卡整窗重绘。** 整窗重绘**跟随渲染器**：
//! 软件渲染实测 ≈**970 帧/秒**（一次 300 ms 的换页 ≈ **290 次整窗重绘** ≈ **0.30 核·秒**），
//! GPU 档被 AutoVsync 限到 40~60 帧/秒。重绘次数是「订阅开着多久」的函数，与闸门无关。
//!
//! 接法是**加进**既有那条链，不是另起一条：`App::needs_frames()` 多一个「有轨道」的分支，
//! 订阅表仍然是**一处** `if`（`app/subscription.rs`），所以与 `watch_until` / Busy 续订/ 心跳
//! 天然不会双订阅——它们共享同一个布尔。
//!
//! ## `MotionScale::NONE` 是**真归零**
//!
//! 关掉动效 ≠ 跑一遍 0 时长动画：`Pump::play` 在 `is_off()` 时**不插入轨道**，`progress` 于是
//! 直接返回 `1.0`（到位），`needs_frames()` 恒为假 ⇒ **一帧都不订阅**。守卫见
//! `pump::tests::none_scale_never_starts_a_track`。
//!
//! ## 为什么是 thread-local 而不是 `App` 的字段
//!
//! 它是**时间轴**（`(槽 → 进度)` 一张表 + 一个时钟），不是应用状态：没有任何业务语义，也不需要
//! 跨进程持久。iced 在事件循环线程上串行调 `update` / `view`（这与 `App::update(&mut self)` 能存在
//! 是同一条纪律），所以一个线程局部就够，且**测试天然隔离**（`cargo test` 每个用例一个线程，
//! 不会出现「另一条线的动画把 `needs_frames()` 顶成真」的假红）。
//!
//! 仓内已有同样的先例（`pages/appearance/probe.rs`、`core/src/addon/pack/format/probe.rs` 都用了
//! `#[allow(clippy::missing_const_for_thread_local)]`），所以这不是给本 crate 开的新口子。
//!
//! ## 谁在什么时候调
//!
//! | 时机 | 调用 | 在哪 |
//! |---|---|---|
//! | 每条消息处理完（**订阅表求值之前**） | [`observe`]：内容标识变了就起跑 | `App::sync_motion`（`app/update/motion.rs`） |
//! | 每帧画视图 | [`sample`]：只读当前位移 | `pages::render` |
//! | 每帧 | [`advance`]：推进时间轴 | `App::on_frame` |
//!
//! 页面**拿不到**时长/缓动/弹簧参数——那是 [`spec`] 的事。
//!
//! 位移走 `container` 的 `padding`：动画期间内容比容器少几像素高，末帧归零后与静息**逐像素相同**。
//! 不做逐像素 α（D27：CPU 档零运行时半透明）也不做模糊（同上），所以两个渲染后端出图一致。

mod pump;

// 本 mission 里这五个只被测试与文档引用（产品路径只经 `observe`/`sample`/`shifted`，拿不到时长/缓动），
// 所以显式 `allow` 一下——与 `interaction::Tracker` 那条同一条理由：它们是对外口径的一部分
// （`spec` 是唯一一张规格表），不该因为「暂时没人调」就被删掉。
#[allow(unused_imports)]
pub use pump::{offset, spec, Slot, Spec, FRAME_MS};

use std::cell::{Cell, RefCell};
use std::time::Instant;

use iced::widget::container;
use iced::{Element, Length, Padding};

use unlucky_design::motion::MotionScale;

// clippy 1.99 的 `missing_const_for_thread_local` 对**已经是** `const { … }` 的初始化也报
// （换过三种写法都照报，属于误报；复审 r1 已复核并确认，仓内 `pages/appearance/probe.rs` 等
// 两处已有同样的 `allow`）。初始化确实已是常量，所以按项显式放过（上在宏外面不算，lint 是对
// 展开后的 item 报的）。
thread_local! {
    /// 进程内唯一时间轴（见模块文档「为什么是 thread-local」）。
    #[allow(clippy::missing_const_for_thread_local)]
    static PUMP: RefCell<pump::Pump> = const { RefCell::new(pump::Pump::new()) };
    /// 上次收到帧的时刻（`advance` 用它把「帧到达」换算成流逝毫秒）。
    #[allow(clippy::missing_const_for_thread_local)]
    static LAST_FRAME: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// 定档：`on_boot` 调一次（截图档给 `NONE`，CPU 档给 `HALF`，见 `app/update/boot_env.rs`）。
pub fn configure(scale: MotionScale, pin: Option<f64>) {
    PUMP.with(|pump| pump.borrow_mut().configure(scale, pin));
}

/// 当前档位。
pub fn scale() -> MotionScale {
    PUMP.with(|pump| pump.borrow().scale())
}

/// 已推进的帧数。
pub fn samples() -> u64 {
    PUMP.with(|pump| pump.borrow().samples())
}

/// 现在有没有动画在跑（`App::needs_frames` 的一个分支）。
pub fn needs_frames() -> bool {
    PUMP.with(|pump| pump.borrow().is_animating())
}

/// 页主体的内容**标识**：**每条消息处理完**调一次（`App::sync_motion`），标识变了就此起跑。
///
/// `screen` 是 `pages::Screen` 的稳定键（`None` = 这一屏不归 pages 画：账户页 / AI 独立页），
/// `detail` 是详情页的实例 id。
///
/// ## 为什么必须在 update 侧（M192 r1 复审 P1-1）
///
/// iced 0.14 的顺序是「消息 → `update()`（订阅表在**这里**求值）→ `build_user_interfaces`
/// （这时才调 `view`）」（iced_winit-0.14.1 `src/lib.rs:1222-1240`）。轨道若在 `view` 里插，
/// 同一拍的订阅表看不见它 ⇒ `frames()` 要等下一条消息（最快鼠标 / 最慢 1 Hz 心跳）才打开，
/// 而那一帧的 dt 是整段空闲时长 ⇒ 动画一帧走完，用户只看到一次偏移闪跳。
///
/// 起跑时**顺带把帧基准重置到「现在」**：空闲期没有帧，`LAST_FRAME` 是几分钟前的值，
/// 不重置的话第一帧仍会按整段空闲时长结算（第二道兜底是 [`pump::MAX_STEP_MS`]）。
pub fn observe(screen: Option<u64>, detail: Option<&str>) {
    let started = PUMP.with(|pump| pump.borrow_mut().observe(screen, detail));
    if started {
        LAST_FRAME.with(|last| last.set(Some(Instant::now())));
    }
}

/// 当前入场位移（**只采样、无副作用**）：`pages::render` 每帧调一次。
pub fn sample() -> (f32, f32) {
    PUMP.with(|pump| pump.borrow().offsets())
}

/// 推进时间轴（`App::on_frame` 每帧调一次）。
pub fn advance() {
    let now = Instant::now();
    let elapsed_ms = LAST_FRAME.with(|last| {
        let previous = last.replace(Some(now));
        previous.map_or(0.0, |previous| {
            now.saturating_duration_since(previous).as_secs_f64() * 1000.0
        })
    });
    PUMP.with(|pump| {
        pump.borrow_mut().advance(elapsed_ms);
    });
}

/// 把入场位移套到树上。
///
/// **总是**套这一层（位移为 `(0,0)` 时只是 `padding = 0`）：它是树的**根**，而 iced 的
/// `Tree::diff` 按 widget tag 判等——根 tag 一换就重建整棵子树。若只在动画期间才套，
/// 动画起止两侧会各重建一次，页面内的滚动位置 / 焦点会被重置（复审 r1 提示的风险）。
/// 常驻一层的代价是布局上多一个 `Fill` 容器（零内距时逐像素相同，出图基线已复验）。
pub fn shifted<'a, M: 'a>(
    element: impl Into<Element<'a, M>>,
    (dx, dy): (f32, f32),
) -> Element<'a, M> {
    container(element.into())
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: dy,
            right: 0.0,
            bottom: 0.0,
            left: dx,
        })
        .into()
}

#[cfg(test)]
pub fn reset() {
    PUMP.with(|pump| *pump.borrow_mut() = pump::Pump::new());
    LAST_FRAME.with(|last| last.set(None));
}

/// 测试专用：按给定毫秒推进（`advance` 读真实时钟，单元测试不能等它）。
#[cfg(test)]
pub fn advance_by(ms: f64) {
    PUMP.with(|pump| {
        pump.borrow_mut().advance(ms);
    });
}

#[cfg(test)]
mod tests;
