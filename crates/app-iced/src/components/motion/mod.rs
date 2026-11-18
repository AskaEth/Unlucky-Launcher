//! **动效帧泵**（M192 建，M256 接上页面方向与模态）：design 的 motion 令牌 → iced 的**有界**帧订阅。
//!
//! ## 这个模块解决什么
//!
//! `crates/design/src/motion/` 里令牌一直齐着（16 档时长 / 可采样缓动 / 二维弹簧表 /
//! `MotionScale::NONE` 降级），但适配层长期只有一个消费点。本模块是那条中间层：
//! **谁在动**（页面轨道 / 模态轨道）、**动多久朝哪动**（[`pump`] 与 `transition` 的表）、
//! **什么时候该订阅帧**（[`needs_frames`]）。
//!
//! ## 有界泵的契约（五条，都有守卫）
//!
//! | 契约 | 为什么 | 守在哪 |
//! |---|---|---|
//! | 只在**有轨道**时订阅帧 | `window::frames()` 一订阅就自持重绘循环（实测几百帧/秒） | `pump/tests.rs`、`app/subscription.rs` |
//! | 轨道到期**当场删除**，不靠「下次状态变化」 | 删晚了就是空闲烧帧 | `pump/tests.rs` |
//! | **模态完全展开后不占帧** | 对话框开着可能几分钟，动画只有开/关那两段 | `pump/tests.rs::an_open_modal_stops_requesting_frames` |
//! | **时间轴推进**不密于 [`pump::FRAME_MS`]（16 ms ≈ 62.5/s），且单次最多吃 [`pump::MAX_STEP_MS`] | 帧到得再快也只该改这么多次布局；空闲醒来时 stale 的帧基准不许把动画一帧杀完 | `pump/tests.rs` |
//! | 起跑**必须发生在 `update` 侧**（[`observe`] / [`observe_modal`]），`view` 侧只采样 | iced 先算订阅表（`update()` 内部）后建视图 ⇒ 在 `view` 里插轨道当拍订阅不上 | `app::subscription::tests` |
//!
//! ## 两条列表 + 一条换肤轨道（M305 / M313）
//!
//! 行增删的轨道**按列表分账**：`motion::rows::List::{Versions, Instances}` 共用一套算术、各记
//! 一份清单（不分账会让两张列表互相把对方的行判成退场，见 `pump/lists.rs`）。主题交叉淡入走
//! `motion::theme`：换浅 / 深色时在两块板之间插值，复用模态进场那条腿（不新造时长与曲线）。
//! 启动成败交叉淡入（M314 / A6）走 `motion::launch`：实例详情那条状态行在旧档与**目标档**之间
//! 走一条 `medium1` 的腿，页面只负责把两端角色按进度合成（同一个采样口、同一条时间轴）。
//!
//! ## `MotionScale::NONE` 是**真归零**
//!
//! 关掉动效 ≠ 跑一遍 0 时长动画：`Pump::play_page` 在 `is_off()` 时**不插入轨道**，
//! [`frame`] 于是返回 [`PageFrame::STILL`]（位移**严格** `(0,0)`、不画旧页），[`modal`] 要么
//! 不画要么 `progress = 1.0`（缩放严格 1.0），`needs_frames()` 恒为假 ⇒ **一帧都不订阅**。
//!
//! ## 系统「减少动效」（M256 接的真入口）
//!
//! [`configure`] 收的是一个**已经算好**的 `MotionScale`（来源在 `app/update/boot_env.rs`：
//! 截图 / CPU / env）。系统偏好是**上限**，在这里与它合成：
//!
//! | 系统偏好 | 结果 | 为什么 |
//! |---|---|---|
//! | [`crate::platform::motion::ReduceMotion::Reduced`] | `NONE` | 用户明确要求减少动效，优先于默认档 |
//! | `AnimationsOn` | 传入的档 | —— |
//! | [`crate::platform::motion::ReduceMotion::Unknown`] | 传入的档 | **不许**因为探针失败就静默关掉动效（那会变成「某些机器上动效神秘消失」） |
//!
//! 探针本身**只在启动时读一次并缓存**（`OnceLock`），不在帧路径上。
//!
//! ## 为什么是 thread-local 而不是 `App` 的字段
//!
//! 它是**时间轴**（一张轨道表 + 一个时钟），不是应用状态：没有业务语义、不需要跨进程持久。
//! iced 在事件循环线程上串行调 `update` / `view`，所以一个线程局部就够，且**测试天然隔离**。
//! 仓内已有同样的先例（`pages/appearance/probe.rs`、`core/src/addon/pack/format/probe.rs`）。
//!
//! ## 位移走 `padding`
//!
//! 位移套在 `container` 的 `padding` 上：动画期间内容比容器少几像素，末帧归零后与静息**逐像素相同**。
//! 不做逐像素 α（D27：CPU 档禁面板半透明）也不做模糊（同上），所以两个渲染后端出图一致。
//! 模态的**缩放**走 `renderer.with_transformation`（两个后端都实现，见 [`scaled`]）。

mod clip;
mod fade;
// M314：启动成败交叉淡入的公开采样口（页面只拿得到「这一帧画成什么样」）
pub mod launch;
pub mod progress;
mod pump;
mod reveal;
pub mod rows;
pub mod theme;
mod transition;
mod zoom;

// `Pump` / `MAX_STEP_MS` 只被测试与文档引用（产品路径拿不到时长与时间轴），显式放过：
// 它们是对外口径的一部分（`FRAME_MS` 被 `app/subscription.rs` 的守卫用）。
#[allow(unused_imports)]
pub use pump::{Pump, FRAME_MS, MAX_STEP_MS};
// 规格表与采样形状全部经这里导出：页面只拿得到「这一帧怎么画」，拿不到「用哪一档、多久」。
// （`page_spec`/`Leg`/`*Spec` 只被测试与文档引用，产品路径只用 `Kind` 与采样形状。）
pub use clip::clipped;
pub use fade::mix;
pub use reveal::revealed;
#[allow(unused_imports)]
pub use transition::{
    enter_offset, exit_offset, modal_scale, modal_spec, page_spec, rail_spec, Kind, Leg,
    ModalFrame, ModalSpec, PageFrame, PageSpec, RailSpec, MODAL_SCALE_FROM,
};
pub use zoom::scaled;
// M305：列表增删 + 进度插值。页面只拿得到「这一帧怎么画」，轨道与时长都在泵里
// （两个入口都是 `pub mod`：`motion::rows::observe/frame`、`motion::progress::observe/value`）。

use std::cell::{Cell, RefCell};
use std::time::Instant;

use iced::widget::container;
use iced::{Element, Length, Padding};

use unlucky_design::motion::MotionScale;

use crate::platform::motion::ReduceMotion;

// clippy 1.99 的 `missing_const_for_thread_local` 对**已经是** `const { … }` 的初始化也报
// （换过三种写法都照报，属于误报；仓内 `pages/appearance/probe.rs` 等两处已有同样的 `allow`）。
thread_local! {
    /// 进程内唯一时间轴（见模块文档「为什么是 thread-local」）。
    #[allow(clippy::missing_const_for_thread_local)]
    static PUMP: RefCell<pump::Pump> = const { RefCell::new(pump::Pump::new()) };
    /// 上次收到帧的时刻（`advance` 用它把「帧到达」换算成流逝毫秒）。
    #[allow(clippy::missing_const_for_thread_local)]
    static LAST_FRAME: Cell<Option<Instant>> = const { Cell::new(None) };
    /// **测试专用**：`Some(档)` = 钉住这个档，[`configure`] 既不再读系统偏好、也不吃传入值。
    #[allow(clippy::missing_const_for_thread_local)]
    static FORCED_SCALE: Cell<Option<MotionScale>> = const { Cell::new(None) };
}

/// 定档：`on_boot` 调一次（截图档给 `NONE`，CPU 档给 `HALF`，见 `app/update/boot_env.rs`）。
///
/// **系统「减少动效」在这里合成**（见模块文档的表）：`Reduced` ⇒ `NONE`；`Unknown` ⇒ 不动。
pub fn configure(scale: MotionScale, pin: Option<f64>) {
    // 三条定档路径**都汇到这里**：①测试/`on_boot` 直接调；②`App::on_boot` → `configure_motion`
    // → 这里（`app/update.rs:276`）；③`boot_env::motion_scale(..)` 算出的 env / 后端 tier 也是经②
    // 喂进来的。所以「钉住某个档」必须在这个函数里生效——堵调用点或堵 `boot_env` 都会被后一次
    // `on_boot` 覆盖（r2 复审实测：`idle_app()` 先钉 STANDARD，Boot 之后变 HALF/NONE）。
    let scale = match FORCED_SCALE.with(Cell::get) {
        Some(forced) => forced,
        None => effective_scale(scale, ReduceMotion::probe(), pin.is_some()),
    };
    PUMP.with(|pump| pump.borrow_mut().configure(scale, pin));
}

/// **测试专用**定档：绕过系统偏好探针，只落档位（M256 r2 / 复审 P2-5）。
///
/// 为什么必须有它：`configure` 会读宿主机「减少动效」开关，而用例在关了系统动画的机器上会红；
/// 用例一律走这里才与宿主机无关。**至少保留一条**故意走真探针的用例
/// （`tests::modal::configure_goes_through_the_real_os_probe`），否则等于把「OS 偏好真的参与合成」
/// 这条判据悄悄删掉。
#[cfg(test)]
pub fn configure_for_test(scale: MotionScale, pin: Option<f64>) {
    FORCED_SCALE.with(|forced| forced.set(Some(scale)));
    configure(scale, pin);
}

/// 档位与系统偏好的**合成本身**（纯函数，可断言）。
///
/// `pinned` = 取证相位钉住（`ULL_MOTION_AT`）：多帧序列图要在同一台机器上重复拍到同一相位，
/// 所以钉住时**不**被系统偏好降级（否则取证机一旦开了"减少动效"就一张图都拍不出来）。
pub fn effective_scale(scale: MotionScale, reduce: ReduceMotion, pinned: bool) -> MotionScale {
    match reduce {
        ReduceMotion::Reduced if !pinned => MotionScale::NONE,
        _ => scale,
    }
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
///
/// **两条时间轴都问**：本模块的轨道（页面 / 模态 / 折叠）与 `interaction::layer` 的状态层过渡。
/// 后者有自己的时钟（状态层要按控件逐个推进，而控件状态在各自的 `Tree` 里），所以它只借这里的
/// 帧订阅：`App::subscription` 仍然只认这一个函数（**没有第二个判断点**）。
pub fn needs_frames() -> bool {
    PUMP.with(|pump| pump.borrow().is_animating()) || super::interaction::layer::is_animating()
}

/// 起跑之后把帧基准重置到「现在」（与 `observe` / `observe_modal` 同一个动作，抽出来给 M305 的
/// 两条轨道共用：空闲期没有帧，不重置的话第一帧会按整段空闲时长结算——`MAX_STEP_MS` 是第二道兜底）。
fn reset_frame_clock() {
    LAST_FRAME.with(|last| last.set(Some(Instant::now())));
}

/// 页主体的内容**标识**：**每条消息处理完**调一次（`App::sync_motion`），标识变了就此起跑。
///
/// `screen` 是 `pages::Screen` 的稳定键（`None` = 这一屏不归 pages 画：账户页 / AI 独立页），
/// `rank` 是这一屏在**导航轨**（`shell::nav::NavItem::ALL`）里的次序——**左右方向只看它**，
/// 不看屏键大小（两处顺序不同：`Screen::ALL` 里 Settings 在 Logs 前，导航轨里反过来），
/// `detail` 是详情页的实例 id。方向与深度由 [`pump::Pump::observe`] 判：
/// **同级换页走水平、进/出深层走纵向，旧页反向退场**。
///
/// 起跑时**顺带把帧基准重置到「现在」**：空闲期没有帧，`LAST_FRAME` 是几分钟前的值，
/// 不重置的话第一帧仍会按整段空闲时长结算（第二道兜底是 [`pump::MAX_STEP_MS`]）。
pub fn observe(screen: Option<u64>, rank: Option<u8>, detail: Option<&str>) {
    let started = PUMP.with(|pump| pump.borrow_mut().observe(screen, rank, detail));
    if started {
        LAST_FRAME.with(|last| last.set(Some(Instant::now())));
    }
}

/// 当前这一帧的页面位移（**只采样、无副作用**）：`pages::render` 每帧调一次。
pub fn frame() -> PageFrame {
    PUMP.with(|pump| pump.borrow().page_frame())
}

/// 弹窗的**逻辑**开关（每帧 update 侧同步一次）：`true` = 队列里有框。
///
/// 返回 `true` = 起跑了进场轨道（调用方要重置帧基准）。
pub fn observe_modal(open: bool) -> bool {
    let started = PUMP.with(|pump| pump.borrow_mut().observe_modal(open));
    if started {
        LAST_FRAME.with(|last| last.set(Some(Instant::now())));
    }
    started
}

/// 请求关掉弹窗。返回 `true` = **已经关了**（关着动效 / 本来就没开），调用方立刻
/// `close_dialog()`；返回 `false` = 退场动画在跑，真正的关闭由 [`take_modal_closed`] 交回。
pub fn begin_modal_close() -> bool {
    let done = PUMP.with(|pump| pump.borrow_mut().begin_modal_close());
    if !done {
        LAST_FRAME.with(|last| last.set(Some(Instant::now())));
    }
    done
}

/// 退场是否已走完（取走一次）。**在 `update` 侧调**：拿到 `true` 就把框真正关掉。
pub fn take_modal_closed() -> bool {
    PUMP.with(|pump| pump.borrow_mut().take_modal_closed())
}

/// 退场是否正在跑（`app/update/dialog.rs` 据此吞掉弹窗消息：框已经在缩小，再点一次「删除」
/// 不该再生效）。
pub fn modal_closing() -> bool {
    PUMP.with(|pump| pump.borrow().modal_closing())
}

/// 这一帧模态该画成什么样（缩放进度 + 遮罩浓度都用它）。
pub fn modal() -> ModalFrame {
    PUMP.with(|pump| pump.borrow().modal_frame())
}

/// 当前页面轨道是哪一档（`None` = 静息；日志与断言用，产品路径只采样位移）。
#[allow(dead_code)]
pub fn kind() -> Option<Kind> {
    PUMP.with(|pump| pump.borrow().kind())
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
    // A8：状态层过渡的时钟（单步上限与本模块同一条 [`MAX_STEP_MS`]，掉帧不许把过渡一帧抹掉）。
    super::interaction::layer::advance(elapsed_ms);
}

/// 右栗折叠的**目标档**变了（`pages/agent/state.rs` 的三个档位迁移里调，都在 update 侧）。
///
/// 返回 `true` = 起跑了过渡（调用方不必做事：帧基准由本模块自己重置）。
pub fn observe_rail(open: bool) -> bool {
    let started = PUMP.with(|pump| pump.borrow_mut().observe_rail(open));
    if started {
        LAST_FRAME.with(|last| last.set(Some(Instant::now())));
    }
    started
}

/// 瞬时落档（拆分成独立页 / 窄窗退化）：**不**起轨道，进度严格等于终值。
pub fn snap_rail(open: bool) {
    let was_running = PUMP.with(|pump| pump.borrow_mut().snap_rail(open));
    if was_running {
        LAST_FRAME.with(|last| last.set(Some(Instant::now())));
    }
}

/// 这一帧的折叠进度（0 = 48 图标条，1 = 展开档）。**只采样、无副作用**。
///
/// `open` 是调用方**此刻**的目标档。泵记的目标与它不一致，说明有人改了档位却没走
/// [`observe_rail`]（截图 harness / 单测直接构造状态）——那时直接给终值：
/// 宁可无动效，也不许把界面卡在中间。
pub fn rail(open: bool) -> f32 {
    PUMP.with(|pump| {
        let pump = pump.borrow();
        if pump.rail_open() == open {
            pump.rail_progress()
        } else if open {
            1.0
        } else {
            0.0
        }
    })
}

/// 左轨（导航轨）的折叠档变了（M300）：`Message::ToggleNav` 的落点调它，**在 update 侧**。
///
/// 返回 `true` = 起跑了过渡（帧基准由本模块自己重置）。与 [`observe_rail`] 是**两条并列的
/// 轨道**：左轨折叠不会改右栗的目标（理由见 `pump/rail.rs` 模块文档）。
pub fn observe_nav(open: bool) -> bool {
    let started = PUMP.with(|pump| pump.borrow_mut().observe_nav(open));
    if started {
        LAST_FRAME.with(|last| last.set(Some(Instant::now())));
    }
    started
}

/// 瞬时落档（启动时按落盘的折叠档对表）：**不**起轨道，进度严格等于终值。
///
/// 只在「没有左轨轨道在跑」时调用（`app/update/motion_sync.rs` 的判据就是
/// [`nav_running`]），否则会把正在跑的过渡当场掐掉。
pub fn snap_nav(open: bool) {
    let was_running = PUMP.with(|pump| pump.borrow_mut().snap_nav(open));
    if was_running {
        LAST_FRAME.with(|last| last.set(Some(Instant::now())));
    }
}

/// 左轨的宽度轨道是否在跑（`motion_sync` 的「该不该对表」判据）。
pub fn nav_running() -> bool {
    PUMP.with(|pump| pump.borrow().nav_animating())
}

/// 这一帧的左轨折叠进度（0 = 折叠 64，1 = 展开 88）。**只采样、无副作用**。
///
/// 目标不符时给终值，理由同 [`rail`]：宁可无动效，也不许把界面卡在中间那一帧。
pub fn nav(open: bool) -> f32 {
    PUMP.with(|pump| {
        let pump = pump.borrow();
        if pump.nav_open() == open {
            pump.nav_progress()
        } else if open {
            1.0
        } else {
            0.0
        }
    })
}

/// 把位移套到树上。
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
    // 钉住也要清：「故意走真探针」的那条用例靠 `reset()` 回到未钉住状态。
    FORCED_SCALE.with(|forced| forced.set(None));
    // 状态层那条时钟是另一个 thread-local（`interaction::layer`），一起清。
    super::interaction::layer::reset();
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
