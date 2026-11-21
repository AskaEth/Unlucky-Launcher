//! 宽度折叠轨道（M257 / A2 建右栗，M300 加左轨）：`48 ⇄ 展开` / `64 ⇄ 88` 那一段的**时间轴**。
//!
//! 与页面 / 模态两条轨道并列的第三条（M300 起是**两条**宽度轨道，左 `nav` / 右 `rail`）：
//! 它们只管「这一帧折叠到几成」，不碰宽度怎么算（宽度是 `pages/agent/metrics.rs` /
//! `shell/nav.rs` 的纯函数）、也不碰画什么（那是视图的事）。
//!
//! | 项 | 值 | 出处 |
//! |---|---|---|
//! | 展开（进） | `medium1` 250ms + `Emphasized` | `14-motion-and-animation.md` §3 A2（token 列）+ §7-1「强调曲线优先」「进略长于出」 |
//! | 收起（出） | `short4` 200ms + `EmphasizedAccelerate` | 同上（退场比进场短 50ms） |
//!
//! ## 为什么左轨另开一条轨道，而不是复用右栗那条
//!
//! 两条轨是**两个独立的用户动作**：`Pump::rail_open` 只有单个值，左轨一折叠就会把右栗的目标
//! 改掉（`motion::rail(true)` 走「目标不符」兜底 ⇒ 右栗自己的动画被吞掉）。那是暗耦合：
//! 一个功能的动画吃掉另一个功能的动画，出问题时很难查。所以 M300 给它开一条并列轨道
//! （[`Pump::observe_nav`]），**时长与曲线复用同一张 [`rail_spec`]**，零新常数。
//!
//! ## 为什么拆出去档（`Detached`）**不**走这条轨道
//!
//! 拆分是**布局切换**（右栗消失、会话搬到独立页），不是宽度的连续变化：把 0 也塞进这条轨道，
//! 「48 → 0」会变成一次毫无意义的缩水动画，而且中途页面会按一个不存在的宽度重排。所以
//! [`Pump::snap_rail`] 是它的入口：目标记下来、轨道清空，进度**严格**是终值。
//!
//! ## 起跑只在 update 侧
//!
//! [`Pump::observe_rail`] 由 `pages/agent/state.rs` 的三个档位迁移（`collapse` / `expand` /
//! `dock`）调用——那三处都在 `update` 里发生。[`Pump::observe_nav`] 由
//! `app/update/nav.rs` 的 `Message::ToggleNav` 落点调用，同样在 `update` 里。`view` 侧只经
//! `motion::rail(open)` / `motion::nav(open)` **采样**。

use super::super::transition::rail_spec;
use super::{leg_fraction, Pump};

/// 一条正在跑的折叠轨道（0 = 折叠档，1 = 展开档）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct RailTrack {
    /// 起跑时已经在几成（中途反向时不为 0/1，保证**不跳变**）。
    pub(crate) from: f32,
    /// 目标（0 或 1）。
    pub(crate) to: f32,
    /// 这一腿的时长（已按 `MotionScale` 缩放）。
    pub(crate) duration_ms: f64,
    /// 这一腿的曲线。
    pub(crate) leg_easing: unlucky_design::motion::Easing,
    pub(crate) start_ms: f64,
}

impl RailTrack {
    /// `elapsed` 毫秒之后的进度（端点**逐位**等于 `from` / `to`）。
    pub(crate) fn sample(&self, elapsed_ms: f64) -> f32 {
        let t = leg_fraction(elapsed_ms, self.duration_ms);
        if t >= 1.0 {
            return self.to;
        }
        if t <= 0.0 {
            return self.from;
        }
        self.from + (self.to - self.from) * self.leg_easing.sample(t)
    }
}

impl Pump {
    /// 折叠目标态（`true` = 展开档）。出厂是收起（`pages/agent/state.rs` 的默认档）。
    pub fn rail_open(&self) -> bool {
        self.rail_open
    }

    /// 起跑一条宽度轨道（左右两条轨**共用**这张表与这条时间轴）。
    ///
    /// `from` 是起跑时已经在几成（中途反向时不为 0/1 ⇒ 不跳变）；`open` 只决定用哪条腿
    /// （进 `medium1` 250ms / 出 `short4` 200ms）与终点。
    ///
    /// **关掉动效时返回 `None`**（`MotionScale::NONE`）：调用方必须把轨道清成 `None`，
    /// 而不是插一条 0 时长轨道 —— 那会让 `is_animating()` 有一拍为真（多要一帧，
    /// `MotionScale::NONE` 的「一帧都不要」就破了）。
    fn rail_track(&self, open: bool, from: f32) -> Option<RailTrack> {
        if self.scale.is_off() {
            return None;
        }
        let spec = rail_spec();
        let leg = if open { spec.enter } else { spec.exit };
        Some(RailTrack {
            from,
            to: if open { 1.0 } else { 0.0 },
            duration_ms: f64::from(self.scale.ms(leg.duration)),
            leg_easing: leg.easing,
            start_ms: self.start_now(),
        })
    }

    /// 起跑一次折叠 / 展开。返回 `true` = 真的起跑了轨道（调用方要重置帧基准）。
    ///
    /// **关掉动效时不插轨道**（`MotionScale::NONE`）：进度从此恒为终值，`needs_frames()` 不会
    /// 因为折叠而变真——这是「NONE = 严格等于终值」在那一条轨道上的落点。
    pub fn observe_rail(&mut self, open: bool) -> bool {
        if self.rail_open == open && self.rail.is_none() {
            return false;
        }
        let from = self.rail_progress();
        self.rail_open = open;
        self.rail = self.rail_track(open, from);
        self.rail.is_some()
    }

    /// 瞬时落档（拆分成独立页 / 窄窗退化）：**不**起轨道，进度严格等于终值。
    ///
    /// 返回 `true` = 原来有轨道被掐掉了（那一帧要重画成终值）。
    pub fn snap_rail(&mut self, open: bool) -> bool {
        self.rail_open = open;
        self.rail.take().is_some()
    }

    /// 这一帧的折叠进度（0 = 48 图标条，1 = 展开档）。
    ///
    /// 没有轨道时是**严格**的 `0.0` / `1.0`（不是「差不多到位」）。
    pub fn rail_progress(&self) -> f32 {
        match self.rail {
            Some(track) => track.sample(self.elapsed_ms() - track.start_ms),
            None if self.rail_open => 1.0,
            None => 0.0,
        }
    }

    // ---------------------------------------------------------------- 左轨（导航轨，M300）

    /// 左轨的折叠目标态（`true` = 展开 88，`false` = 折叠 64）。出厂是展开。
    pub fn nav_open(&self) -> bool {
        self.nav_open
    }

    /// 起跑一次左轨折叠 / 展开（与右栗**并列**的另一条轨道，理由见模块文档）。
    ///
    /// 返回 `true` = 真的起跑了轨道（调用方要重置帧基准）。
    pub fn observe_nav(&mut self, open: bool) -> bool {
        if self.nav_open == open && self.nav.is_none() {
            return false;
        }
        let from = self.nav_progress();
        self.nav_open = open;
        self.nav = self.rail_track(open, from);
        self.nav.is_some()
    }

    /// 瞬时落档（启动时按落盘的折叠档对表）：**不**起轨道，进度严格等于终值。
    ///
    /// 为什么需要它：泵的目标出厂是「展开」，而用户可能上次就是折叠着退出的。不对表的话，
    /// 启动后第一次**展开**会被 `observe_nav` 的「目标没变」分支判成空操作 ⇒ 跳变没有动画。
    ///
    /// 返回 `true` = 原来有轨道被掐掉了（那一帧要重画成终值）。
    pub fn snap_nav(&mut self, open: bool) -> bool {
        self.nav_open = open;
        self.nav.take().is_some()
    }

    /// 这一帧的左轨进度（0 = 折叠 64，1 = 展开 88）。只采样、无副作用。
    pub fn nav_progress(&self) -> f32 {
        match self.nav {
            Some(track) => track.sample(self.elapsed_ms() - track.start_ms),
            None if self.nav_open => 1.0,
            None => 0.0,
        }
    }
}
