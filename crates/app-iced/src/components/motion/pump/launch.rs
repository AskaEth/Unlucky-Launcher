//! **启动成败交叉淡入**的轨道（M314 / A6）：实例页那条状态行在两档之间插值，不许瞬切。
//!
//! ## 为什么是「两端角色 + 进度」而不是一个现成的颜色
//!
//! 页面拿到的 [`LaunchFrame`] 是 `from` / `to` 两个**角色**加一个进度。理由有两条：
//!
//! 1. **主题交叉淡入期间 `Ui` 每帧都在变**（M313）。泵里要是记「实色」，换肤那一拍记下的红
//!    就会与这一帧的板对不上（红句不跟着换肤）；记角色、由页面用**这一帧**的 `Ui` 求值，
//!    两件事天然同源。
//! 2. 合成必须是**外不透明**的：两端都是 `Ui::color` 的实色，走
//!    `interaction::layer::blend`（D27：中间帧不新增 α，GPU/CPU 两档逐像素一致），端点在
//!    `progress = 0 / 1` 时**逐位**等于两端。
//!
//! ## 时长 / 曲线不新造
//!
//! 取 [`super::super::transition::status_spec`]（`medium1` 250ms + `Standard`，都是 design
//! 的令牌）。本模块**没有**任何时长 / 曲线常数。
//!
//! ## 一个**登记过的边界**：二次换档的接续点是上一个目标档，不是中间色
//!
//! 角色空间里没有「两个角色之间的中间角色」可记，所以 [`Launch::observe`] 中途换目标时取
//! `self.value()` = `track.to`（上一个目标）作为 `from`。后果：250ms 内连换两次档会先跳到上一个
//! 目标档再淡向新档（≈20% 处一格台阶）。**真正的分段接续未做**——那要把「未走完的腿 + 新腿」
//! 合成一条 `LaunchFrame`，是独立技术活——按复审 r1 的 p2-1 记进债务台账（塔记），不在 M314 的
//! 实现范围内。
//!
//! ## 三条判据（同 `pump/progress.rs` 的口径）
//!
//! 1. **换了实例就落档**：上一实例的红句不许染到新实例上（那不是「交叉淡入」，是串色）。
//! 2. **状态行出现 / 消失不起轨道**：那是**版面**的变化（多一行 / 少一行），没有颜色可插；
//!    两端都有状态行时才谈得上颜色交叉。
//! 3. **目标不符就给目标**：泵记的 key 与调用方问的不一致（截图 harness / 单测直接构造模型、
//!    或者没走过 [`Pump::observe_launch`]）⇒ [`Launch::frame`] 返回 `None`，调用方按自己的
//!    兜底色画——**宁可无动效，也不许把界面卡在中间值**。
//!
//! ## `MotionScale::NONE`（含「减少动效」开启）
//!
//! [`Launch::observe`] 的 `off` 分支不插轨道，但**照记目标角色** ⇒ [`Launch::frame`] 从第一帧起
//! 就给 `progress = 1.0` / `running = false`（= 瞬时切到终值），`needs_frames` 恒假 ⇒ 一帧都不订阅。

use unlucky_design::color::Role;
use unlucky_design::motion::Easing;

use super::super::transition::status_spec;
use super::{leg_fraction, Pump};

/// 这一帧状态行该画成什么（视图侧只看它，**不含任何时长**）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaunchFrame {
    /// 起手那一档的**角色**（`progress = 0` 时状态行就是这个颜色）。
    pub from: Role,
    /// 到位那一档的角色（`progress = 1` 时就是它）。
    pub to: Role,
    /// `0` = 起手那一档，`1` = 到位。
    pub progress: f32,
    /// 还有帧要跑（`false` = 已经落到终点）。
    pub running: bool,
}

/// 一条正在跑的淡入（`from → to`，单向）。
#[derive(Debug, Clone, Copy)]
struct LaunchTrack {
    /// 起手角色。
    from: Role,
    /// 到位角色。
    to: Role,
    /// 起跑时刻（钉住相位时恒为 0）。
    start_ms: f64,
    /// 这一腿的时长（已按 `MotionScale` 缩放）。
    duration_ms: f64,
    /// 这一腿的曲线。
    easing: Easing,
}

impl LaunchTrack {
    /// `elapsed` 毫秒之后的进度（端点**逐位**是 0 / 1）。
    fn sample(&self, elapsed_ms: f64) -> f32 {
        let t = leg_fraction(elapsed_ms, self.duration_ms);
        if t >= 1.0 {
            1.0
        } else if t <= 0.0 {
            0.0
        } else {
            self.easing.sample(t)
        }
    }

    /// 这一腿走完了。
    fn done(&self, now_ms: f64) -> bool {
        now_ms - self.start_ms >= self.duration_ms
    }
}

/// 状态行的时间轴（**一份**：屏幕上同时只有一个实例详情）。
#[derive(Debug)]
pub struct Launch {
    /// 这账认谁（实例 id；`None` = 还没见过任何详情页）。
    key: Option<String>,
    /// 这一档该用的角色（`None` = 这一档**没有**状态行）。
    tone: Option<Role>,
    /// 在跑的腿（`None` = 已经到位 / 关着动效 / 没变过）。
    track: Option<LaunchTrack>,
}

impl Launch {
    /// 出厂态：没认过任何实例，没有轨道。
    pub(super) const fn new() -> Self {
        Self {
            key: None,
            tone: None,
            track: None,
        }
    }

    /// 定档 / 复位：连「上次认的是谁」一起忘掉（换档之后的第一帧不该淡入）。
    pub(super) fn clear(&mut self) {
        self.key = None;
        self.tone = None;
        self.track = None;
    }

    /// 有没有腿在跑（= 要不要订阅帧）。
    pub(super) fn is_animating(&self) -> bool {
        self.track.is_some()
    }

    /// 这一帧画出来的颜色档（没有轨道 = 记的目标角色）。
    fn value(&self) -> Option<Role> {
        match self.track {
            Some(track) => Some(track.to),
            None => self.tone,
        }
    }

    /// 状态行的档变了：记目标，必要时起跑一条腿。返回 `true` = 真的起跑了。
    ///
    /// `off` = 关着动效（`MotionScale::NONE` / 系统「减少动效」）：只记账、不插轨道。
    pub(super) fn observe(
        &mut self,
        key: &str,
        tone: Option<Role>,
        start_ms: f64,
        duration_ms: f64,
        easing: Easing,
        off: bool,
    ) -> bool {
        // ①换了实例（或第一次看到）：**落档**，不跨实例淡——上一实例的红句不该染到新实例上。
        if self.key.as_deref() != Some(key) {
            self.key = Some(key.to_owned());
            self.tone = tone;
            self.track = None;
            return false;
        }

        let previous = self.tone;
        if previous == tone {
            // 目标没变：不插 `from == to` 的空轨道（那会白烧 250ms 的帧，同 `observe_progress`）。
            return false;
        }
        // ②起跑点：有轨道时取 `self.value()`——它给的是 `track.to`（**上一个目标档**），不是这一帧
        // 采样出来的中间色。所以 250ms 内二次换档（Busy → Running → Failed 的「起跑即崩」）会先
        // 跳到上一个目标档再淡向新档（≈20% 处有一格台阶）。
        //
        // **这是登记过的边界，不是笔误**：真正的分段接续（一段未走完的腿 + 一段新腿合成一条
        // `LaunchFrame`）是独立技术活，按复审 r1 的 p2-1 记进债务台账（塔记），本 mission 不实现。
        // 角色空间的接续点因此是「上一个目标」，不是「中间色」。
        let from = if self.track.is_some() {
            self.value().or(previous)
        } else {
            previous
        };
        self.tone = tone;

        // ③两端都得有状态行：出现 / 消失是**版面**的事，没有颜色可插（见模块文档判据 2）。
        let (Some(from), Some(to)) = (from, tone) else {
            self.track = None;
            return false;
        };
        if off {
            self.track = None;
            return false;
        }
        self.track = Some(LaunchTrack {
            from,
            to,
            start_ms,
            duration_ms,
            easing,
        });
        true
    }

    /// 这一帧的状态行采样（**只采样、无副作用**）。
    ///
    /// `None` = 这账不认 `key`（没走过 `observe` 的模型：harness / 单测 / 演示数据），
    /// 调用方按自己的兜底色画，**不许**拿一个猜出来的中间值顶上。
    pub(super) fn frame(&self, key: &str, now_ms: f64) -> Option<LaunchFrame> {
        if self.key.as_deref() != Some(key) {
            return None;
        }
        let Some(track) = self.track else {
            // 已经到位（或本来就没动）：目标角色 + `running = false`。
            return self.tone.map(|tone| LaunchFrame {
                from: tone,
                to: tone,
                progress: 1.0,
                running: false,
            });
        };
        let progress = track.sample(now_ms - track.start_ms);
        Some(LaunchFrame {
            from: track.from,
            to: track.to,
            progress,
            running: progress < 1.0,
        })
    }

    /// 到期即删（`Pump::expire` 调）。记的 `tone` **留着**：删掉之后采样退化成「目标角色 +
    /// `running = false`」，于是永远不存在「轨道没了、界面留在上次采到的 87%」那种收尾残差。
    pub(super) fn expire(&mut self, now_ms: f64) {
        if let Some(track) = self.track {
            if track.done(now_ms) {
                self.track = None;
            }
        }
    }
}

impl Pump {
    /// 状态行的档变了：记目标并起跑淡入（关着动效时只记账、**不插轨道**）。
    ///
    /// **必须在 update 侧调**（`app/state/refresh.rs::sync_launch_tone`）：iced 先算订阅表、
    /// 后建视图，在 `view` 里起跑的轨道当拍订阅不上（见 `components/motion/theme.rs` 的模块文档）。
    pub fn observe_launch(&mut self, key: &str, tone: Option<Role>) -> bool {
        let leg = status_spec();
        let duration_ms = self.leg_ms(leg.duration);
        let start_ms = self.start_now();
        let off = self.scale.is_off();
        self.launch
            .observe(key, tone, start_ms, duration_ms, leg.easing, off)
    }

    /// 这一帧的状态行采样（**只采样、无副作用**）。
    pub fn launch_frame(&self, key: &str) -> Option<LaunchFrame> {
        self.launch.frame(key, self.elapsed_ms())
    }

    /// 状态行淡入是否在跑（日志与断言用；`needs_frames` 走的是泵的 `is_animating`）。
    #[allow(dead_code)]
    pub fn launch_animating(&self) -> bool {
        self.launch.is_animating()
    }
}
