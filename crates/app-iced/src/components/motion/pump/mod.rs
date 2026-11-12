//! 帧泵的**纯逻辑**：时间轴 / 页面切换轨道 / 模态轨道 / 采样。
//!
//! 这一层不认识 iced、不认识线程、不读环境变量——那些都在 [`super`]。分开的理由很实际：
//! 「帧间隔上限」「结束后必须清空」这两条守卫是**算术性质**，用本地 `Pump` 直接摆时间轴就能判死，
//! 不必起窗口、不必等真实时间（那两条最容易写成「看起来在跑」的假绿）。
//!
//! ## 两条轨道（M256）：页面的两腿 + 模态的进出，各走各的时间轴（互不替代）
//!
//! | 轨道 | 触发 | 结束 |
//! |---|---|---|
//! | 页面 | 内容标识变了（[`Pump::observe`]） | 两腿里长的那条走完 |
//! | 模态 | 弹窗开 / 关（[`Pump::observe_modal`] / [`Pump::begin_modal_close`]） | 进场转 [`Modal::Open`]（**不再要帧**）、退场转 [`Modal::Hidden`] |
//!
//! D31：时长/缓动是**表**（[`super::transition`]），页面只许按表取——它只能说「我要换页了」。

use unlucky_design::motion::{DurationToken, MotionScale};

mod modal;

use super::transition::{
    enter_offset, exit_offset, modal_spec, page_spec, Kind, PageFrame, PageSpec,
};
pub(crate) use modal::Modal;

/// 逐帧**推进时间轴**的最短间隔（16 ms ≈ 60 次/秒）。
///
/// ⚠️ **这只卡「采样与布局推进」，不卡整窗重绘。** 两个频率必须分开看：
///
/// | 什么 | 频率 | 由谁决定 |
/// |---|---|---|
/// | 时间轴推进 / 位移采样（本常量） | ≤62.5 次/秒 | 这里 |
/// | **整窗重绘** | **跟随渲染器**：软件渲染实测 ≈**970 帧/秒**（一次 300 ms 的换页 ≈**290 次整窗重绘** ≈ **0.30 核·秒**）；GPU 档被 AutoVsync 限到 40~60 | `window::frames()` 一订阅，iced_winit 每条消息后都 `request_redraw()`（iced_winit-0.14.1 `src/lib.rs:1222-1240`） |
pub const FRAME_MS: f64 = 16.0;

/// 单次推进的**最大步进**（4 帧 = 64 ms）。
///
/// 为什么需要它：`advance` 的 dt 来自「上一帧到现在」，而空闲期根本没有帧——醒来的第一帧如果
/// 按真实流逝（秒~分钟级）结算，`duration` 当场就被跨过 ⇒ 到期轨道被立刻删除 ⇒ 动画一帧走完
/// （M192 r1 复审 P1-1 就是这么暴露的）。起点由 `motion::observe` 重置，这里再兜一道：
/// **任何一次采样最多只吃掉 4 帧的时长**，掉帧/被挂起时宁可把动画放慢，也不许把它一次性抹掉。
pub const MAX_STEP_MS: f64 = FRAME_MS * 4.0;

/// 一条正在跑的页面轨道。
#[derive(Debug, Clone, Copy)]
struct PageTrack {
    kind: Kind,
    spec: PageSpec,
    start_ms: f64,
}

/// 帧泵：一条时间轴 + 最多一条页面轨道 + 一个模态状态机。
///
/// **默认是关的**（`MotionScale::NONE`）：产品启动时由 `App::on_boot` 按档位打开，而
/// `pages_demo` 这类截图 harness 从不打开——D30 的「同一份代码出同一张图」因此不受动效影响。
#[derive(Debug)]
pub struct Pump {
    scale: MotionScale,
    /// `Some(ms)` = 钉住相位（多帧序列图取证；见 `ULL_MOTION_AT`）。
    pin: Option<f64>,
    now_ms: f64,
    pending_ms: f64,
    page: Option<PageTrack>,
    modal: Modal,
    /// 退场走完了、等调用方真正关掉那个框（[`Pump::take_modal_closed`] 取走一次）。
    close_pending: bool,
    screen: Option<u64>,
    /// 该屏在**导航轨**上的次序（`shell::nav::NavItem::ALL` 下标；`None` = 不在轨上）。
    rank: Option<u8>,
    detail: Option<String>,
    painted: bool,
    samples: u64,
}

impl Pump {
    /// 出厂态：关着、没跑过、没有轨道。
    pub const fn new() -> Self {
        Self {
            scale: MotionScale::NONE,
            pin: None,
            now_ms: 0.0,
            pending_ms: 0.0,
            page: None,
            modal: Modal::Hidden,
            close_pending: false,
            screen: None,
            rank: None,
            detail: None,
            painted: false,
            samples: 0,
        }
    }

    /// 定档（启动一拍调一次）。
    ///
    /// 顺带**重置内容标识**：iced 会在处理 `Boot` 之前先问一次 `view()`（实测：那一帧早于
    /// `on_boot` 的日志），于是「第一帧」发生在档位还没定下来的时候。不重置的话首屏入场就被那次
    /// 预渲染吃掉了（`painted = true`），之后标识再没变过 ⇒ Boot 槽永远不跑。
    pub fn configure(&mut self, scale: MotionScale, pin: Option<f64>) {
        self.scale = scale;
        self.pin = pin;
        self.painted = false;
        self.screen = None;
        self.rank = None;
        self.detail = None;
        self.pending_ms = 0.0;
        self.modal = Modal::Hidden;
        self.close_pending = false;
        if scale.is_off() {
            // 关掉动效 = 直接落终态：把在跑的轨道清掉，而不是让它跑一遍 0 时长。
            self.page = None;
        }
    }

    /// 当前档位。
    pub fn scale(&self) -> MotionScale {
        self.scale
    }

    /// 已推进的帧数（守卫与 `ULL_TICK_TRACE` 用）。
    pub fn samples(&self) -> u64 {
        self.samples
    }

    /// 时间轴（毫秒）。
    #[cfg(test)]
    pub fn now_ms(&self) -> f64 {
        self.now_ms
    }

    /// 现在有没有动画在跑（= 要不要订阅帧）。
    pub fn is_animating(&self) -> bool {
        self.page.is_some() || matches!(self.modal, Modal::Opening { .. } | Modal::Closing { .. })
    }

    /// 轨道起点的口径：钉住相位时恒为 0（进度只取决于钉住值，与"谁在第几帧起跑"无关）。
    fn start_now(&self) -> f64 {
        if self.pin.is_some() {
            0.0
        } else {
            self.now_ms
        }
    }

    /// 当前动画时刻（钉住相位时恒为钉住值）。
    fn elapsed_ms(&self) -> f64 {
        self.pin.unwrap_or(self.now_ms)
    }

    /// 把某一档时长换算成实际毫秒（`MotionScale` 是唯一缩放口）。
    fn leg_ms(&self, duration: DurationToken) -> f64 {
        f64::from(self.scale.ms(duration))
    }

    // ------------------------------------------------------------ 页面切换

    /// 起跑一次页面切换（关着动效时什么都不做——**不是**跑一遍 0 时长动画）。
    ///
    /// 新轨道**取代**上一条：同一时刻只有一次页面切换（否则位移会叠加）。
    pub fn play_page(&mut self, kind: Kind) -> bool {
        self.page = None;
        if self.scale.is_off() {
            return false;
        }
        let spec = page_spec(kind);
        self.page = Some(PageTrack {
            kind,
            spec,
            start_ms: self.start_now(),
        });
        true
    }

    /// 当前页面轨道（测试与日志用；产品路径只看采样出来的位移）。
    #[allow(dead_code)]
    pub fn kind(&self) -> Option<Kind> {
        self.page.map(|track| track.kind)
    }

    /// 页主体的这一帧采样（**只采样，无副作用**）。
    ///
    /// 两条腿各走各的时长：进场读 `enter.duration`，退场读 `exit.duration`（更短）。
    /// `exiting` 在退场腿走完的那一帧就变假 ⇒ 调用方不再画旧页（不长留一份幽灵树）。
    pub fn page_frame(&self) -> PageFrame {
        let Some(track) = &self.page else {
            return PageFrame::STILL;
        };
        let elapsed = self.elapsed_ms() - track.start_ms;
        let enter_t = leg_fraction(elapsed, self.leg_ms(track.spec.enter.duration));
        let exit_t = leg_fraction(elapsed, self.leg_ms(track.spec.exit.duration));
        PageFrame {
            enter: enter_offset(
                track.spec.enter_from,
                track.spec.enter.easing.sample(enter_t),
            ),
            exit: exit_offset(track.spec.exit_to, track.spec.exit.easing.sample(exit_t)),
            exiting: exit_t < 1.0,
        }
    }

    /// 内容**标识**变了就起跑对应的一档；**必须在 update 侧调**（见 `App::sync_motion`）。
    ///
    /// 三条判据（顺序即优先级）：
    /// 1. 第一次看到**归 pages 画的屏** → [`Kind::Boot`]；
    /// 2. 屏变了 → [`Kind::Peer`]，方向由**屏在导航表里的先后**决定（更靠后的屏 = 向右推进）；
    /// 3. 屏没变但详情变了 → 列表→详情 = [`Kind::Push`]（深一层）、详情→列表 = [`Kind::Pop`]，
    ///    两个不同的详情之间横跳按同级处理。
    ///
    /// `screen == None`（账户页 / AI 独立页：它们不走 `pages::render`）**只记标识、不起跑**。
    ///
    /// 返回 `true` = 真的起跑了新轨道（调用方要重置帧基准，见 [`MAX_STEP_MS`]）。
    pub fn observe(&mut self, screen: Option<u64>, rank: Option<u8>, detail: Option<&str>) -> bool {
        let kind = if !self.painted {
            self.painted = true;
            screen.map(|_| Kind::Boot)
        } else if self.screen != screen {
            match (self.screen, screen, self.rank, rank) {
                // 左右方向 = **用户在导航轨上看到的次序**（`rank`），不是内部屏键的大小：
                // 两者顺序不同（`Screen::ALL` 里 Settings 在 Logs 前，导航轨里 Logs 在 Settings
                // 前）⇒ 用屏键会把 Logs↔Settings 镜像。同 rank（Download / Content 共用导航项）
                // 时退到屏键比大小，方向仍是"向后 / 向前"。
                (Some(old_key), Some(new_key), Some(old_rank), Some(new_rank)) => {
                    Some(Kind::Peer {
                        forward: (new_rank, new_key) > (old_rank, old_key),
                    })
                }
                // 从「不归 pages 画」的屏回来：没有旧页可退，按首屏那一档的入场方向。
                (_, Some(_), _, _) => Some(Kind::Boot),
                _ => None,
            }
        } else if screen.is_some() && self.detail.as_deref() != detail {
            Some(match (self.detail.is_some(), detail.is_some()) {
                (false, true) => Kind::Push,
                (true, false) => Kind::Pop,
                // 两个不同的详情之间横跳：深度没变，按同级换内容处理。
                _ => Kind::Peer { forward: true },
            })
        } else {
            None
        };

        self.screen = screen;
        self.rank = rank;
        if self.detail.as_deref() != detail {
            self.detail = detail.map(str::to_owned);
        }
        kind.is_some_and(|kind| self.play_page(kind))
    }

    // ------------------------------------------------------------ 模态

    // ------------------------------------------------------------ 时间轴

    /// 推进时间轴。返回 `true` = 这一帧真的推进了（进度变了 ⇒ 要重画）。
    ///
    /// 四步：①不满 [`FRAME_MS`] 就不动（采样上限）②**单次最多吃 [`MAX_STEP_MS`]**（掉帧 /
    /// 被挂起 / 空闲醒来的第一帧都不许一次性跨过时长）③到期的轨道**当场删除**（这是「空闲不烧帧」
    /// 的唯一出口）④钉住相位时时间轴不前进（取证专用）。
    pub fn advance(&mut self, dt_ms: f64) -> bool {
        if !self.is_animating() {
            self.pending_ms = 0.0;
            return false;
        }
        if dt_ms.is_finite() && dt_ms > 0.0 {
            // ②：上限不是「省事」，是正确性——stale 的帧基准（空闲整段）会把轨道一帧杀完。
            self.pending_ms += dt_ms.min(MAX_STEP_MS);
        }
        if self.pending_ms < FRAME_MS {
            return false;
        }
        let step = std::mem::take(&mut self.pending_ms);
        if self.pin.is_none() {
            self.now_ms += step;
        }
        self.samples += 1;
        self.expire();
        true
    }

    /// 到期轨道当场清掉（③）。
    fn expire(&mut self) {
        let now = self.elapsed_ms();
        if let Some(track) = self.page {
            let enter = self.leg_ms(track.spec.enter.duration);
            let exit = self.leg_ms(track.spec.exit.duration);
            if now - track.start_ms >= enter.max(exit) {
                self.page = None;
            }
        }
        let spec = modal_spec();
        match self.modal {
            Modal::Opening { start_ms } if now - start_ms >= self.leg_ms(spec.enter.duration) => {
                self.modal = Modal::Open
            }
            Modal::Closing { start_ms, .. }
                if now - start_ms >= self.leg_ms(spec.exit.duration) =>
            {
                self.modal = Modal::Hidden;
                self.close_pending = true;
            }
            _ => {}
        }
    }
}

/// `elapsed / duration` 夹到 `0..=1`（`duration <= 0` ⇒ 直接到位，不产生 NaN）。
fn leg_fraction(elapsed: f64, duration: f64) -> f32 {
    if duration <= 0.0 {
        return 1.0;
    }
    (elapsed / duration).clamp(0.0, 1.0) as f32
}

impl Default for Pump {
    fn default() -> Self {
        Self::new()
    }
}
