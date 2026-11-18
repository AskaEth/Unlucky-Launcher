//! **主题交叉淡入**的轨道（M313）：换浅 / 深色时两块板之间插值，不许整窗闪切。
//!
//! ## 只差一件事：颜色是逐帧从 `Ui` 出去的
//!
//! 整窗的每一个像素都经 [`crate::theme::Ui`]（`color` / `layer` / `state_over`）取色，所以
//! 「交叉淡入」不需要任何遮罩或第二棵树：**只要这一帧的 `Ui` 拿的是一套插值出来的方案**，
//! 卡片、文字、导航轨就一起走在两块板之间。起手那一帧逐位等于旧板（看不出变化），末帧逐位
//! 等于新板（与 `rebuild_ui` 的产物完全相同）——两端都没有台阶。
//!
//! ## 时长 / 曲线不新造：复用模态进场那条腿
//!
//! 换主题与「弹出一个框」是同一类事（用户刚做了一件事，界面给出回应），所以直接取
//! [`super::super::transition::modal_spec()`] 的 `enter` 腿（`Short4` 200ms + `EmphasizedDecelerate`，
//! 都是 design 的令牌）。**没有**为本轨道新增任何时长 / 曲线常数——M313 的任务明确禁止。
//!
//! ## `MotionScale::NONE`（含「减少动效」开启）
//!
//! [`Pump::observe_theme`] 在 `is_off()` 时**不插轨道**：记账的 `to` 仍是目标板，于是
//! [`Pump::theme_frame`] 从第一帧起就给出**终点方案**（`progress = 1.0`、`running = false`）——
//! 也就是「瞬时切换且颜色是最终值」，不是「跑一遍 0 时长动画停在中途」。
//!
//! ## 起手之后不会停在中途
//!
//! 轨道到期被删掉之后，记账的 `to` 还在：采样退化成「终点方案 + `running = false`」。
//! 于是**任何**一帧的采样都只会是「中间值」或「严格的终点」，不存在「轨道没了、界面留在
//! 上次采到的 87%」那种收尾残差。

use unlucky_design::color::{Role, Scheme};
use unlucky_design::motion::Easing;

use super::super::transition::modal_spec;
use super::{leg_fraction, Pump};

/// 这一帧主题该画成什么样（视图侧只看它，**不含任何时长**）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeFrame {
    /// 这一帧的 49 角色方案（端点逐位等于起手 / 终点那块板）。
    pub scheme: Scheme,
    /// `0` = 起手那块板，`1` = 终点那块板。
    pub progress: f32,
    /// 还有帧要跑（`false` = 已经落到终点，调用方可以不再续帧）。
    pub running: bool,
}

/// 一条正在跑的淡入（`from → to`，单向）。
#[derive(Debug, Clone, Copy)]
struct ThemeTrack {
    /// 起手那块板。
    from: Scheme,
    /// 终点那块板。
    to: Scheme,
    /// 起跑时刻（钉住相位时恒为 0）。
    start_ms: f64,
    /// 这条腿的时长（已按 `MotionScale` 缩放）。
    duration_ms: f64,
    /// 这条腿的曲线。
    easing: Easing,
}

impl ThemeTrack {
    /// `elapsed` 毫秒之后的插值进度（端点**逐位**是 0 / 1）。
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

/// 主题淡入的账：起手 / 终点两块板 +（可能有的）那条腿。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Theme {
    /// 起手那块板（`None` = 还没换过主题）。
    from: Option<Scheme>,
    /// 终点那块板（= 最近一次 `rebuild_ui` 的产物）。
    to: Option<Scheme>,
    /// 在跑的腿（`None` = 已经到位 / 关着动效 / 没换过）。
    track: Option<ThemeTrack>,
}

impl Theme {
    /// 出厂态：没换过主题，没有轨道。
    pub(super) const fn new() -> Self {
        Self {
            from: None,
            to: None,
            track: None,
        }
    }

    /// 定档 / 复位：连「上次是什么板」一起忘掉（换档之后的第一帧不该淡入）。
    pub(super) fn clear(&mut self) {
        self.from = None;
        self.to = None;
        self.track = None;
    }

    /// 换主题：记两块板，必要时起跑一条腿。返回 `true` = 真的起跑了。
    ///
    /// `from == to`（例如只改面板 α 的那次重建）既不起跑也不记账：没有颜色变化可动。
    pub(super) fn observe(
        &mut self,
        from: Scheme,
        to: Scheme,
        start_ms: f64,
        duration_ms: f64,
        easing: Easing,
        off: bool,
    ) -> bool {
        self.from = Some(from);
        self.to = Some(to);
        self.track = None;
        if from == to || off {
            return false;
        }
        self.track = Some(ThemeTrack {
            from,
            to,
            start_ms,
            duration_ms,
            easing,
        });
        true
    }

    /// 这一帧该用哪套方案（**只采样、无副作用**）。
    ///
    /// 从没换过主题时给 `None`（调用方保持 `rebuild_ui` 刚算好的那份 `Ui`，一帧都不动）。
    pub(super) fn frame(&self, now_ms: f64) -> Option<ThemeFrame> {
        let to = self.to?;
        let Some(track) = self.track else {
            // 已经到位（或本来就没动）：终点方案 + `running = false`。
            return Some(ThemeFrame {
                scheme: to,
                progress: 1.0,
                running: false,
            });
        };
        let progress = track.sample(now_ms - track.start_ms);
        Some(ThemeFrame {
            scheme: crossfade(track.from, track.to, progress),
            progress,
            running: progress < 1.0,
        })
    }

    /// 有没有腿在跑（= 要不要订阅帧）。
    pub(super) fn is_animating(&self) -> bool {
        self.track.is_some()
    }

    /// 到期即删（`Pump::expire` 调）。记的 `to` **留着**，见模块文档「不会停在中途」。
    pub(super) fn expire(&mut self, now_ms: f64) {
        if let Some(track) = self.track {
            if track.done(now_ms) {
                self.track = None;
            }
        }
    }
}

/// 两块板之间按 `t` 逐角色插值：端点 `t <= 0` / `t >= 1` **逐位**返回入参那块板。
///
/// 用 `Color::lerp`（8 bit sRGB 逐通道线性）：与 `motion/fade.rs::mix` 同一条纪律——产物
/// **恒不透明**（两块板都不透明），所以中间帧不新增任何 α（D27 的 CPU 档口径不被破）。
pub(crate) fn crossfade(from: Scheme, to: Scheme, t: f32) -> Scheme {
    if t <= 0.0 {
        return from;
    }
    if t >= 1.0 {
        return to;
    }
    let colors = std::array::from_fn(|index| {
        let role = Role::ALL[index];
        from.get(role).lerp(to.get(role), t)
    });
    Scheme::from_colors(to.flavor(), colors)
}

impl Pump {
    /// 换主题：记两块板并起跑淡入（关着动效时只记账、**不插轨道**）。
    ///
    /// **必须在 update 侧调**（`app/state/theme.rs::rebuild_ui`）：iced 先算订阅表、后建视图，
    /// 在 `view` 里起跑的轨道当拍订阅不上（`app/subscription.rs` 的模块文档有源码行号）。
    ///
    /// 时长与曲线取模态进场的 `enter` 腿（见模块文档），本函数**不新造**任何常数。
    pub fn observe_theme(&mut self, from: Scheme, to: Scheme) -> bool {
        let leg = modal_spec().enter;
        let duration_ms = self.leg_ms(leg.duration);
        let start_ms = self.start_now();
        let off = self.scale.is_off();
        self.theme
            .observe(from, to, start_ms, duration_ms, leg.easing, off)
    }

    /// 这一帧该用哪套方案（**只采样、无副作用**）。
    pub fn theme_frame(&self) -> Option<ThemeFrame> {
        self.theme.frame(self.elapsed_ms())
    }

    /// 主题淡入是否在跑（日志与断言用；`needs_frames` 走的是泵的 `is_animating`）。
    #[allow(dead_code)]
    pub fn theme_animating(&self) -> bool {
        self.theme.is_animating()
    }

    /// 相位是否被钉住（`ULL_MOTION_AT`；取证通道专用，产品路径恒为 `false`）。
    pub fn pinned(&self) -> bool {
        self.pin.is_some()
    }
}
