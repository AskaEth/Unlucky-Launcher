//! 页面切换轨道（M256 建，M432 拆出并接上弹簧腿）：两腿的时间轴 + 每帧位移采样。
//!
//! ## 为什么单独一个文件
//!
//! `pump/mod.rs` 收工在 400 行（硬线）。M432 要给它加一条腿，又不肯压行凑数（删注释是最坏的做法），
//! 于是把「页面切换」这一整段——结构体 + 起跑 / 采样 / 到期判据——**原样**搬到这里。搬的是位置，
//! 不是行为：改前 / 改后的逐帧轨迹由 `tests/page_leg.rs` 钉死（日志 `diff` 为空）。
//!
//! ## 弹簧腿（M432）：默认关闭，只接一条消费点
//!
//! [`Pump::use_page_spring`] 被**显式**调用时，进场腿改由 `Spring::sample` 驱动（公式与阈值口径
//! 见 `pump/spring_leg.rs`）。没调用 = `None` = 令牌时长 + `Easing`，**逐帧不变**。
//!
//! 只接**进场腿**（新页位移）：退场腿是「让位」，不是「到位」——弹簧的回弹在退场上读起来像犹豫。
//! 两腿的时长关系要如实说：稳定时间 236.5ms（12px）/ 255.0ms（16px）与令牌退场**相当**，其中 12px 的
//! 同级换页上进场比 `Medium1`(250) **短 13.5ms**（§7-1 的「进略长于出」在这里被**翻转**），16px 的三档
//! （boot / push / pop）才是略长（255 vs 250 / 200）。两者都没破 §7-1 的 300ms 线，但「12px 上进场短于
//! 退场」是否可接受**留给接线 mission 判**——本文件只把数摆出来，不替它下结论。
//!
//! ## 到期判据必须问腿自己
//!
//! `elapsed >= 令牌时长` 这条旧判据对弹簧腿是错的：弹簧活多久由 `settle_ms(阈值)` 决定，不由
//! `enter.duration` 决定。按令牌判会把还在走的弹簧当场掐掉（末帧从中间值跳回终点）。
//! 所以 [`Pump::page_lifetime_ms`] 是唯一口径，[`Pump::expire`] 只问它。

use super::super::transition::{enter_offset, exit_offset, page_spec, Kind, PageFrame, PageSpec};
use super::{leg_fraction, Pump, SpringLeg};
use unlucky_design::motion::Spring;

/// 一条正在跑的页面轨道。
#[derive(Debug, Clone, Copy)]
pub(crate) struct PageTrack {
    pub(crate) kind: Kind,
    pub(crate) spec: PageSpec,
    pub(crate) start_ms: f64,
    /// 进场腿若由弹簧驱动，这里记它的时间轴；`None` = 令牌时长 + 缓动（旧路径，默认）。
    pub(crate) enter_spring: Option<SpringLeg>,
}

/// 这条腿的位移幅度（px）：取两轴里**大的**那个。
///
/// 为什么不是斜边：`enter_offset` 是**逐轴**取整的，「画出来已经是终点」要求每一轴都到 —— 残差的
/// 上界由最大的那一轴决定，所以幅度取 `max(|dx|, |dy|)`（见 `spring_leg::settle_threshold`）。
fn amplitude((dx, dy): (f32, f32)) -> f32 {
    dx.abs().max(dy.abs())
}

impl Pump {
    /// 页面切换**进场腿**改用弹簧驱动（M432）：`None` = 旧的令牌时长 + 缓动（默认，逐帧不变）。
    ///
    /// 只有一条消费点：新页位移（[`Pump::page_frame`] 的 `enter`）。退场腿不受影响。
    /// 必须在**起跑之前**调用——腿在 [`Pump::play_page`] 那一刻就定好了。
    ///
    /// `#[allow(dead_code)]`：产品路径还没有调用方（接线点在 `app/update/motion_sync.rs`，不在 M432 的
    /// scope 内），目前只有本 crate 的用例在调。这与 `Pump::kind` 的处置一致。
    #[allow(dead_code)]
    pub fn use_page_spring(&mut self, spring: Option<Spring>) {
        self.page_spring = spring;
    }

    /// 起跑一次页面切换（关着动效时什么都不做——**不是**跑一遍 0 时长动画）。
    ///
    /// 新轨道**取代**上一条：同一时刻只有一次页面切换（否则位移会叠加）。
    pub fn play_page(&mut self, kind: Kind) -> bool {
        self.page = None;
        if self.scale.is_off() {
            return false;
        }
        let spec = page_spec(kind);
        // M432：调用方显式选过弹簧才走那条腿；`None` 时 `enter_spring` 也是 `None` ⇒ 旧路径。
        let enter_spring = self
            .page_spring
            .map(|spring| SpringLeg::new(spring, amplitude(spec.enter_from), self.scale));
        self.page = Some(PageTrack {
            kind,
            spec,
            start_ms: self.start_now(),
            enter_spring,
        });
        true
    }

    /// 当前页面轨道（测试与日志用；产品路径只看采样出来的位移）。
    #[allow(dead_code)]
    pub fn kind(&self) -> Option<Kind> {
        self.page.map(|track| track.kind)
    }

    /// 这条轨道活多久（到期判据）：进场腿**与**退场腿里长的那条。
    pub(crate) fn page_lifetime_ms(&self, track: &PageTrack) -> f64 {
        let enter = match track.enter_spring {
            Some(leg) => leg.total_ms(),
            None => self.leg_ms(track.spec.enter.duration),
        };
        enter.max(self.leg_ms(track.spec.exit.duration))
    }

    /// 页主体的这一帧采样（**只采样，无副作用**）。
    ///
    /// 两条腿各走各的时长：进场读 `enter.duration`（或弹簧的稳定时间），退场读 `exit.duration`（更短）。
    /// `exiting` 在退场腿走完的那一帧就变假 ⇒ 调用方不再画旧页（不长留一份幽灵树）。
    pub fn page_frame(&self) -> PageFrame {
        let Some(track) = &self.page else {
            return PageFrame::STILL;
        };
        let elapsed = self.elapsed_ms() - track.start_ms;
        let exit_t = leg_fraction(elapsed, self.leg_ms(track.spec.exit.duration));
        // 进场腿两条路：弹簧（`Some`）或令牌时长 + 缓动（`None`；表达式与 M256 逐字相同）。
        let enter_progress = match track.enter_spring {
            Some(leg) => leg.progress(elapsed),
            None => track.spec.enter.easing.sample(leg_fraction(
                elapsed,
                self.leg_ms(track.spec.enter.duration),
            )),
        };
        PageFrame {
            enter: enter_offset(track.spec.enter_from, enter_progress),
            exit: exit_offset(track.spec.exit_to, track.spec.exit.easing.sample(exit_t)),
            exiting: exit_t < 1.0,
        }
    }
}
