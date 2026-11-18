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
    fn settle_amount(&self, now_ms: f64) -> f32 {
        let Some(settle) = self.settle else {
            return 0.0;
        };
        let t = leg_fraction(now_ms - settle.start_ms, settle.duration_ms);
        if t >= 1.0 || t <= 0.0 {
            return 0.0;
        }
        // 0 → 1 → 0 的一次正弦脉冲：两端**严格**是 0（末帧与静息逐位相同）。
        (std::f32::consts::PI * t).sin()
    }

    /// 清掉走完的轨道。返回 `true` = **这一帧到达了 100%**，调用方要排一次收束。
    fn expire(&mut self, now_ms: f64) -> bool {
        if let Some(settle) = self.settle {
            if now_ms - settle.start_ms >= settle.duration_ms {
                self.settle = None;
            }
        }
        let Some(track) = self.track else {
            return false;
        };
        if now_ms - track.start_ms < track.duration_ms {
            return false;
        }
        self.track = None;
        // 到达 1.0 才排收束（中途的一次跳变不是「完成」）；已经在收束里就不重排。
        track.to >= 1.0 && self.settle.is_none()
    }

    fn schedule_settle(&mut self, now_ms: f64, duration_ms: f64) {
        self.settle = Some(Settle {
            start_ms: now_ms,
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
        self.progress.settle_amount(self.elapsed_ms())
    }

    /// 进度轨道在不在跑（日志与断言用；`needs_frames` 走的是 `is_animating`）。
    #[allow(dead_code)]
    pub fn progress_animating(&self) -> bool {
        self.progress.is_animating()
    }

    /// 到期清理（`Pump::expire` 调；收束的时长要按档缩放，所以排在这里而不是 [`Progress`] 里）。
    pub(super) fn expire_progress(&mut self, now_ms: f64) {
        if self.progress.expire(now_ms) {
            let duration = f64::from(self.scale.ms(progress_spec().arrive.duration));
            self.progress.schedule_settle(now_ms, duration);
        }
    }

    /// 定档 / 复位（`Pump::configure` 用）。
    pub fn reset_progress(&mut self) {
        self.progress.clear();
    }
}
