//! **进度插值**的轨道（M305）：真实进度一跳，画出来的值沿一条腿追上去。
//!
//! ## 为什么不能直接画 `ratio`
//!
//! 后端给的是**离散读数**（`files_done / files_total`，一次跳 3% 或 20%），直接画就是
//! 「整数值硬跳」——M3E 的 PROGRESS 类动效要的是**连续推进**：值在两次事件之间也一直在走。
//!
//! | 项 | 值 | 出处 |
//! |---|---|---|
//! | 追值一腿 | `short4` 200ms + `Standard` | `transition.rs::progress_spec`（单调、不回弹：进度倒退读起来像卡了） |
//! | 到达后收束 | `short3` 150ms 一次的脉冲 0→1→0 | 同上（**不循环**：循环会一直在闪） |
//!
//! ## 三条判据
//!
//! 1. **目标不符就给目标**（同 `motion::rail` 的口径）：泵记的目标与调用方此刻给的不一致，
//!    说明有人改了值却没走 [`Pump::observe_progress`]（截图 harness / 单测直接构造模型）——
//!    那时**宁可无动效，也不许把界面卡在中间值**。
//! 2. **`MotionScale::NONE` 严格等于目标**：不插轨道，`progress_value` 直接返回目标。
//! 3. **收束只跑一次**：脉冲走完就删（[`Pump::expire_progress`]），下一次到达才重新排。
//!
//! ## 脉冲锚在**到达时刻**（不是「发现到期的帧」）—— **别改回去**
//!
//! 脉冲的时间原点是 `track.start_ms + duration_ms`（这条腿真正走完的那一刻），**不是** `expire`
//! 发现它到期的那一帧的 `now_ms`。
//!
//! **理由**：脉冲是「进度条到位」的视觉收束，它的时间原点就是**到位那一刻**。锚在「发现到期」的帧
//! 等于把动画时序耦合到帧调度 —— 应用哪一帧恰好抽到采样，就决定了脉冲从哪里开始。
//!
//! **这条改动不是 I1 出图所必需的**（如实记，别让后人以为是取证的必需品）：出图 harness 不调
//! `motion::advance()`，脉冲靠下面「采样侧对越过终点但未清掉的轨道现算」那一支（`settle_start`
//! 的第二支）就能拍出来。改 `expire` 的理由是**一致性**：同一段动画只许有一个时间原点，
//! 否则 pin 通道与产品通道各算一套，下一次改动必然踩到两套语义的缝（旧口径还让钉住相位
//! **结构上拍不到脉冲**：pin 下 `now_ms ≡ pin` ⇒ `t = (pin−pin)/150 = 0`）。
//!
//! **代价（如实记）**：相位比旧口径提前 `[0, MAX_STEP_MS)` —— 常态一帧（≈8ms），掉帧最坏 64ms；
//! 每帧强度最多差 0.17（常态）。波形 `sin(π·t/150)`、时长 150ms、只闪一次、清理路径**逐值未变**，
//! `Progress` 的字段也没增。M386 r1 复审量过这张账，塔裁决「接受」。
//!
//! ⚠️ **改回 `now_ms` 等于让同一段动画有两个时间原点**（pin 通道一套、产品通道另一套），以后更难讲清；
//! 而且会让 `pinned_phase_reaches_the_pulse_without_a_tick` 与 `the_pulse_is_anchored_at_the_arrival_instant`
//! 两条用例同时红。这两条就是这一段的判据。
//!
//! ## 钉住相位（`ULL_MOTION_AT`）：哪一段钉得住，哪一段钉不住（M386）
//!
//! 逐行核过 `Pump::start_now` / `Pump::elapsed_ms` / 本文件：
//!
//! | 相位 | 钉得住？ | 依据 |
//! |---|---|---|
//! | 追值**中段** | **能** | `start_now()` 在 pin 下给 0、`elapsed_ms()` 给 pin ⇒ `value = from + (to−from)·easing(pin/dur)`，`dur` = `short4` 200ms。pin=100 ⇒ 中段 |
//! | 收束**脉冲** | **能**（M386 之前不能） | 脉冲起点 = 轨道**到达时刻**（`start+duration`）。早先排的是 `now_ms`，而 pin 下 `now_ms ≡ pin` ⇒ `t = (pin−pin)/150 = 0` ⇒ 恒为 0，脉冲一帧都拍不到 |
//!
//! 第二行还差一个出口：`pages_demo` 出图 harness **不调** `motion::advance()`（它的
//! `Message::Frame` 只数帧与派采样），`expire` 因此在那条通道里根本不跑。所以采样侧
//! （`Progress::settle_start`）允许「轨道还挂着但已经越过终点」时按轨道终点现算脉冲 ——
//! 产品路径走不到这一支（`App::on_frame` 每帧先 `advance` → `expire`）。

use super::super::transition::{progress_spec, Leg};
use super::{leg_fraction, Pump};

/// 一帧的进度采样（值与收束强度一起给：页面一次取完）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgressFrame {
    /// 这一帧画出来的值（`0..1`）。
    pub value: f32,
    /// 收束强度（`0` = 静息；只有到达 100% 那一下非零）。
    pub settle: f32,
}

impl ProgressFrame {
    /// 静息：值 = 目标、没有收束。产品路径由 [`Pump::progress_value`] 直接给目标、不经过它；
    /// 留着当「没有轨道」的默认口径（同 `PageFrame::STILL`）。
    #[allow(dead_code)]
    pub const REST: Self = Self {
        value: 0.0,
        settle: 0.0,
    };
}

/// 一条正在跑的进度轨道。
#[derive(Debug, Clone, Copy)]
struct ProgressTrack {
    /// 起跑那一刻画出来的值（连跳时接着走 ⇒ 不跳变）。
    from: f32,
    /// 目标（后端此刻的真值）。
    to: f32,
    start_ms: f64,
    duration_ms: f64,
}

/// 到达 100% 之后的那一次收束（脉冲）。
#[derive(Debug, Clone, Copy)]
struct Settle {
    start_ms: f64,
    duration_ms: f64,
}

/// 进度条的时间轴：**一根条一份**（用 `key` 认：换了一个安装就是另一根条）。
#[derive(Debug, Default)]
pub(crate) struct Progress {
    /// 这根条认谁（`None` = 还没见过任何目标）。
    key: Option<String>,
    track: Option<ProgressTrack>,
    settle: Option<Settle>,
    /// **上一次报上来的值**（没有轨道时「这一帧画多少」的答案）。
    ///
    /// 为什么必须记它：第一次报一个 key 时没有「上一根」可追，画的就是目标本身；之后每一次跳变
    /// 都从**这里**起跑。少了它，第二次报会把起跑点当成目标（等价于硬跳——复审就是这么抓到的）。
    shown: f32,
}

impl Progress {
    /// 出厂态：没有条、没有轨道、没有收束。
    pub(super) const fn new() -> Self {
        Self {
            key: None,
            track: None,
            settle: None,
            shown: 0.0,
        }
    }

    pub(super) fn clear(&mut self) {
        self.key = None;
        self.track = None;
        self.settle = None;
        self.shown = 0.0;
    }

    pub(super) fn is_animating(&self) -> bool {
        self.track.is_some() || self.settle.is_some()
    }

    /// 这一帧画出来的值（没有轨道 = 上一次报上来的值）。
    fn value(&self, now_ms: f64) -> Option<f32> {
        let Some(track) = self.track else {
            return Some(self.shown);
        };
        let t = leg_fraction(now_ms - track.start_ms, track.duration_ms);
        if t >= 1.0 {
            Some(track.from + (track.to - track.from) * progress_spec().advance.easing.sample(t))
        } else if t <= 0.0 {
            Some(track.from)
        } else {
            Some(track.from + (track.to - track.from) * progress_spec().advance.easing.sample(t))
        }
    }

    /// 收束脉冲的强度（`0` = 没有；`1` = 脉冲顶点）。走完**当场删**（不循环）。
    ///
    /// `duration_ms` 由泵按档给（`Pump::settle_ms`）：**排**脉冲与**采样**脉冲用同一份时长，
    /// 否则钉住相位时两侧算的不是同一段时间轴。
    fn settle_amount(&self, now_ms: f64, duration_ms: f64) -> f32 {
        let Some(start) = self.settle_start(now_ms) else {
            return 0.0;
        };
        let t = leg_fraction(now_ms - start, duration_ms);
        if t >= 1.0 || t <= 0.0 {
            return 0.0;
        }
        // 0 → 1 → 0 的一次正弦脉冲：两端**严格**是 0（末帧与静息逐位相同）。
        (std::f32::consts::PI * t).sin()
    }

    /// 这一帧的脉冲起点（`None` = 没有脉冲）。
    ///
    /// 两个来源，同一条时间轴：
    ///
    /// 1. 已经排上的 [`Settle`]（产品路径：`expire` 在追值那一腿走完时排）；
    /// 2. 轨道**还挂着**但已经越过终点：出图 harness（`pages_demo`）不调 `motion::advance()`，
    ///    `expire` 在那条通道里永远不跑 ⇒ 不现算就一帧脉冲都拍不到（M386 的 I1 出口）。
    ///    产品路径走不到这一支：`App::on_frame` 每帧先 `advance` → `expire`。
    fn settle_start(&self, now_ms: f64) -> Option<f64> {
        if let Some(settle) = self.settle {
            return Some(settle.start_ms);
        }
        let track = self.track?;
        let arrived = track.start_ms + track.duration_ms;
        (track.to >= 1.0 && now_ms >= arrived).then_some(arrived)
    }

    /// 清掉走完的轨道。返回 `Some(到达时刻)` = **这一帧到达了 100%**，调用方要排一次收束。
    ///
    /// 返回的是**到达时刻**（`轨道起点 + 时长`）而不是 `now_ms`：脉冲必须与插值终点同源。
    /// 早先排的是 `now_ms`，于是钉住相位时 `now_ms ≡ pin` ⇒ `t = (pin−pin)/150 = 0` ⇒ 脉冲
    /// 恒为 0，收束那一帧**结构上拍不出来**（M386 / I1 就是卡在这里）。
    fn expire(&mut self, now_ms: f64) -> Option<f64> {
        if let Some(settle) = self.settle {
            if now_ms - settle.start_ms >= settle.duration_ms {
                self.settle = None;
            }
        }
        let track = self.track?;
        if now_ms - track.start_ms < track.duration_ms {
            return None;
        }
        self.track = None;
        // 到达 1.0 才排收束（中途的一次跳变不是「完成」）；已经在收束里就不重排。
        let arrived = track.start_ms + track.duration_ms;
        (track.to >= 1.0 && self.settle.is_none()).then_some(arrived)
    }

    fn schedule_settle(&mut self, arrived_ms: f64, duration_ms: f64) {
        self.settle = Some(Settle {
            start_ms: arrived_ms,
            duration_ms,
        });
    }
}

impl Pump {
    /// 进度目标变了：起跑一腿追上去。返回 `true` = 真的起跑了。
    ///
    /// `key` 认「哪一根条」（安装是版本 id）：换了 key 就**直接落到目标**（新安装不该从上一根的
    /// 位置追过来）。**必须在 update 侧调**（`pages/download/model.rs::from_snapshot`）。
    pub fn observe_progress(&mut self, key: &str, target: f32) -> bool {
        let target = target.clamp(0.0, 1.0);
        if self.progress.key.as_deref() != Some(key) {
            // 换了一根条：不追值，直接落目标（收束也一起清掉）。
            self.progress.clear();
            self.progress.key = Some(key.to_owned());
            self.progress.shown = target;
            return false;
        }
        // **没有轨道但目标没变** = 这条事件什么也没带来：不许为了「再更新一次」插一条
        // `from == to` 的空轨道。少了这一行，脉冲走完之后**每一条 stage 事件都会再闪一次**
        // （`ratio()` 在最后一个文件下完就恒为 1.0，而 Installing/Registering 阶段每条事件都
        // 会走一遍 `observe`），既违反「一次收束不循环」，也空烧 200ms + 150ms 的帧
        // ——这正是 M305 r1 复审的 P1-1。
        if self.progress.track.is_none() && (target - self.progress.shown).abs() < f32::EPSILON {
            return false;
        }
        if self
            .progress
            .track
            .is_some_and(|track| (track.to - target).abs() < f32::EPSILON)
        {
            return false;
        }
        if self.scale.is_off() {
            // 关掉动效 = 值严格等于目标：清轨道、连收束也不排。
            self.progress.track = None;
            self.progress.settle = None;
            self.progress.shown = target;
            return false;
        }
        let now = self.elapsed_ms();
        let from = self.progress.value(now).unwrap_or(target);
        self.progress.shown = target;
        // 到顶之后又变了（重装 / 回落）：上一根的收束作废。
        if self.progress.track.is_none() || target < 1.0 {
            self.progress.settle = None;
        }
        self.progress.track = Some(self.progress_track(from, target));
        true
    }

    fn progress_track(&self, from: f32, to: f32) -> ProgressTrack {
        let leg: Leg = progress_spec().advance;
        ProgressTrack {
            from,
            to,
            start_ms: self.start_now(),
            duration_ms: f64::from(self.scale.ms(leg.duration)),
        }
    }

    /// 这一帧画出来的进度值。`key` / `target` 与泵记的不一致 ⇒ **直接给 `target`**
    /// （宁可无动效，也不许把界面卡在中间值）。
    pub fn progress_value(&self, key: &str, target: f32) -> f32 {
        let target = target.clamp(0.0, 1.0);
        if self.progress.key.as_deref() != Some(key) {
            return target;
        }
        match self.progress.track {
            // 目标对得上才采轨道；对不上（有人改了模型却没走 `observe`）就**给目标**。
            Some(track) if (track.to - target).abs() < f32::EPSILON => {
                self.progress.value(self.elapsed_ms()).unwrap_or(target)
            }
            _ => target,
        }
    }

    /// 这一帧的收束脉冲强度（`0` = 静息；完成那一下才非零，**不循环**）。
    pub fn progress_settle(&self, key: &str, target: f32) -> f32 {
        if self.progress.key.as_deref() != Some(key) || target.clamp(0.0, 1.0) < 1.0 {
            return 0.0;
        }
        self.progress
            .settle_amount(self.elapsed_ms(), self.settle_ms())
    }

    /// 收束脉冲的时长（按档缩放）：**排**脉冲（[`Pump::expire_progress`]）与**采样**脉冲
    /// （[`Pump::progress_settle`]）共用这一份，两侧才不会各算一段。
    fn settle_ms(&self) -> f64 {
        f64::from(self.scale.ms(progress_spec().arrive.duration))
    }

    /// 取证通道的自证行（**只在钉住相位时**给 `Some`，产品路径恒 `None`）。
    ///
    /// 为什么自证必须写在日志里：图上「进度条停在哪」是像素，而**相位**只有泵知道——
    /// `ULL_MOTION_AT` 钉住的毫秒、这条 track 的 `from → to`、这一帧采样出的 `value` / `settle`。
    /// 三样写在同一行，图才自证得起来：`probe.py` 量出条填到 37%，日志说 `phase=100ms` 时
    /// `value=0.374` —— 少任何一半都只是「看起来像动效」。
    pub fn progress_probe_line(
        &self,
        key: &str,
        target: f32,
        frame: &ProgressFrame,
    ) -> Option<String> {
        let pin = self.pin?;
        let leg = self.progress.track.map_or_else(
            || "<none>".to_owned(),
            |track| format!("{:.3}->{:.3}", track.from, track.to),
        );
        Some(format!(
            "[motion] progress key={key} target={target:.3} leg={leg} phase={pin}ms \
             value={:.4} settle={:.4}",
            frame.value, frame.settle
        ))
    }

    /// 进度轨道在不在跑（日志与断言用；`needs_frames` 走的是 `is_animating`）。
    #[allow(dead_code)]
    pub fn progress_animating(&self) -> bool {
        self.progress.is_animating()
    }

    /// 到期清理（`Pump::expire` 调；收束的时长要按档缩放，所以排在这里而不是 [`Progress`] 里）。
    pub(super) fn expire_progress(&mut self, now_ms: f64) {
        if let Some(arrived) = self.progress.expire(now_ms) {
            let duration = self.settle_ms();
            self.progress.schedule_settle(arrived, duration);
        }
    }

    /// 定档 / 复位（`Pump::configure` 用）。
    pub fn reset_progress(&mut self) {
        self.progress.clear();
    }
}
