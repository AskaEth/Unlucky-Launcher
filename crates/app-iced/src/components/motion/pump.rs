//! 帧泵的**纯逻辑**：槽 / 规格表 / 时间轴 / 进度采样。
//!
//! 这一层不认识 iced、不认识线程、不读环境变量——那些都在 [`super`]。分开的理由很实际：
//! 「帧间隔上限」「结束后必须清空」这两条守卫是**算术性质**，用本地 `Pump` 直接摆时间轴就能判死，
//! 不必起窗口、不必等真实时间（那两条最容易写成「看起来在跑」的假绿）。
//!
//! ## 三个概念
//!
//! | 名字 | 是什么 | 谁定 |
//! |---|---|---|
//! | [`Slot`] | 一个**同屏互斥**的动画位（首屏 / 换页 / 列表↔详情） | 本文件 |
//! | [`Spec`] | 该位的时长档 + 缓动档，**只能**是 design 令牌 | [`spec`]（全项目唯一一张表） |
//! | [`Pump`] | 一条时间轴 + 每槽最多一条轨道 | 本文件 |
//!
//! ## 为什么规格表在代码里而不是页面里
//!
//! D31：弹簧/时长令牌是**表**，页面只许按表取、不许自调参数。所以页面拿不到「给我 250ms」这种
//! 口子——它只能说「我要换页了」（[`Pump::enter`]），用哪一档由 [`spec`] 决定。改观感只改这张表。

use std::collections::BTreeMap;

use unlucky_design::motion::{DurationToken, Easing, MotionScale};
use unlucky_design::spacing::Space;

/// 逐帧**推进时间轴**的最短间隔（16 ms ≈ 60 次/秒）。
///
/// ⚠️ **这只卡「采样与布局推进」，不卡整窗重绘。** 两个频率必须分开看：
///
/// | 什么 | 频率 | 由谁决定 |
/// |---|---|---|
/// | 时间轴推进 / 位移采样（本常量） | ≤62.5 次/秒 | 这里 |
/// | **整窗重绘** | **跟随渲染器**：软件渲染实测 ≈**970 帧/秒**（一次 300 ms 的换页 ≈**290 次整窗重绘** ≈ **0.30 核·秒**）；GPU 档被 AutoVsync 限到 40~60 | `window::frames()` 一订阅，iced_winit 每条消息后都 `request_redraw()`（iced_winit-0.14.1 `src/lib.rs:1222-1240`） |
///
/// 所以「动画期间最多 60fps」只对**采样**成立；重绘次数是订阅时长的函数，与 16 ms 无关。
/// 为什么仍然要 16 ms：每帧重绘已经是既成事实（忙碌态本来就这样），16 ms 卡的是**布局重排**
/// 与 `pages::render` 的重建成本；两者的量级差一个数量级以上（见报告的性能记账）。
pub const FRAME_MS: f64 = 16.0;

/// 单次推进的**最大步进**（4 帧 = 64 ms）。
///
/// 为什么需要它：`advance` 的 dt 来自「上一帧到现在」，而空闲期根本没有帧——醒来的第一帧如果
/// 按真实流逝（秒~分钟级）结算，`duration` 当场就被跨过 ⇒ 到期轨道被立刻删除 ⇒ 动画一帧走完
/// （M192 r1 复审 P1-1 就是这么暴露的）。起点由 `motion::observe` 重置，这里再兜一道：
/// **任何一次采样最多只吃掉 4 帧的时长**，掉帧/被挂起时宁可把动画放慢，也不许把它一次性抹掉。
pub const MAX_STEP_MS: f64 = FRAME_MS * 4.0;

/// 一个动效槽。
///
/// 槽是**同屏互斥**的：三个槽都是「页主体入场」的不同触发点，同时跑会让位移叠加
/// （首屏 16 + 换页 12 = 28px）。所以 [`Pump::play`] 起跑新槽时丢掉旧的——语义上也对：
/// 新的一次内容变化取代上一次。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    /// 首屏进入：进程内只在第一帧跑一次。
    Boot,
    /// 换页：路由换了（导航轨点选、返回、启动直达）。
    Screen,
    /// 列表 ↔ 详情：同一页里的子视图切换（实例页点一行）。
    Detail,
}

impl Slot {
    /// 全部槽（遍历顺序稳定，写日志/断言用）。
    pub const ALL: [Slot; 3] = [Slot::Boot, Slot::Screen, Slot::Detail];

    /// 稳定名字（日志与测试失败信息用；只有测试在调，故按 `cfg(test)` 编译）。
    #[cfg(test)]
    pub const fn name(self) -> &'static str {
        match self {
            Slot::Boot => "boot",
            Slot::Screen => "screen",
            Slot::Detail => "detail",
        }
    }
}

/// 一条轨道的规格：**时长档 × 缓动档**，两个都是 design 令牌。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    /// 时长档（毫秒值经 [`MotionScale`] 缩放后生效）。
    pub duration: DurationToken,
    /// 缓动曲线。
    pub easing: Easing,
}

/// 槽 → 规格（**唯一**一张表）。
///
/// 三档时长取自令牌注释里对应场景的那一档（`Medium2` = 导航/tab 切换、`Medium3` = 卡片展开/
/// 列表重排、`Medium4` = 面板开合），曲线**统一用 `Standard`**：
///
/// - 整块页面的位移不是「元素从屏外飞入」，而是「屏内这块内容换了」——M3 把 `Standard` 定义为
///   「起止都在屏内的元素」，`*Decelerate` 是给「已经在高速运动、要迅速落定」的共享元素用的；
/// - 本地实测（`dev` 机器，见报告）：`EmphasizedDecelerate` 把 **75% 的位移压在 51 ms 内**走完
///   （300 ms 档），12px 的整页平移会看起来像「跳一格」；`Standard` 同样时长下是 105 ms，落点更稳。
pub const fn spec(slot: Slot) -> Spec {
    match slot {
        Slot::Boot => Spec {
            duration: DurationToken::Medium4,
            easing: Easing::Standard,
        },
        Slot::Screen => Spec {
            duration: DurationToken::Medium2,
            easing: Easing::Standard,
        },
        Slot::Detail => Spec {
            duration: DurationToken::Medium3,
            easing: Easing::Standard,
        },
    }
}

/// 槽 → 入场位移（像素；正值 = 从右下方向归位，末态恒为 `(0, 0)`）。
///
/// 幅度取**间距令牌**（4px 栅格）而不是新造一组数字：位移是布局量，和间距用同一把尺子，
/// 将来改栅格不会出现「间距 12、位移 14」这种两套刻度。
pub fn offset(slot: Slot) -> (f32, f32) {
    match slot {
        Slot::Boot => (0.0, Space::X4.dp()),
        Slot::Screen => (0.0, Space::X3.dp()),
        Slot::Detail => (Space::X2.dp(), 0.0),
    }
}

/// 一条正在跑的轨道。
#[derive(Debug, Clone, Copy)]
struct Track {
    /// 起跑时刻（动画时间轴的坐标，毫秒）。
    start_ms: f64,
    spec: Spec,
}

/// 帧泵：一条时间轴 + 每槽一条轨道。
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
    tracks: BTreeMap<Slot, Track>,
    screen: Option<u64>,
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
            tracks: BTreeMap::new(),
            screen: None,
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
        self.detail = None;
        self.pending_ms = 0.0;
        if scale.is_off() {
            // 关掉动效 = 直接落终态：把在跑的轨道清掉，而不是让它跑一遍 0 时长。
            self.tracks.clear();
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
        !self.tracks.is_empty()
    }

    /// 起跑一个槽（关着动效时什么都不做——**不是**跑一遍 0 时长动画）。返回 `true` = 真的起跑了。
    ///
    /// 三个槽**互斥**：它们都是「页主体入场」的不同触发点，同时跑会让位移叠加（首屏 + 换页 =
    /// 28px）。所以起跑新槽就丢掉旧的——语义上也对：新的一次内容变化取代上一次。
    pub fn play(&mut self, slot: Slot) -> bool {
        self.tracks.clear();
        if self.scale.is_off() {
            return false;
        }
        // 钉住相位时轨道的起点固定在 0：进度于是恒等于「钉住的毫秒 / 时长」，
        // 与「谁在第几帧起跑」无关——这正是多帧序列图要的可复现性。
        let start_ms = if self.pin.is_some() { 0.0 } else { self.now_ms };
        self.tracks.insert(
            slot,
            Track {
                start_ms,
                spec: spec(slot),
            },
        );
        true
    }

    /// 采样：`0.0` = 刚起跑，`1.0` = 到位（**没有轨道就是到位**）。
    pub fn progress(&self, slot: Slot) -> f32 {
        let Some(track) = self.tracks.get(&slot) else {
            return 1.0;
        };
        let duration = f64::from(self.scale.ms(track.spec.duration));
        if duration <= 0.0 {
            return 1.0;
        }
        let elapsed = self.elapsed_ms() - track.start_ms;
        let t = (elapsed / duration).clamp(0.0, 1.0) as f32;
        track.spec.easing.sample(t)
    }

    /// 当前动画时刻（钉住相位时恒为钉住值）。
    fn elapsed_ms(&self) -> f64 {
        self.pin.unwrap_or(self.now_ms)
    }

    /// 页主体的内容**标识**变了就起跑对应槽；**必须在 update 侧调**（见 `App::sync_motion`）。
    ///
    /// ## 为什么不能放在 `view` 里（M192 r1 复审 P1-1，这是本轮最重要的一行注释）
    ///
    /// iced 0.14 的顺序是「消息 → `update()`（**`program.subscription()` 在这里求值**）→
    /// `build_user_interfaces`（这时才调 `view`）」（iced_winit-0.14.1 `src/lib.rs:1222-1240`，
    /// 订阅表在 `update()` 内部的 `lib.rs:1338` 求值）。所以在 `view` 里插轨道，同一拍的订阅表
    /// **看不见**它，`frames()` 要等下一条消息（最快鼠标、最慢 1 Hz 心跳）才打开——那一段里
    /// 页面已经带着位移画过一帧，然后就没了：用户看到的是一次偏移闪跳。
    ///
    /// 三条判据（顺序即优先级）：
    /// 1. 第一次看到**归 pages 画的屏** → [`Slot::Boot`]；
    /// 2. 屏变了 → [`Slot::Screen`]；
    /// 3. 屏没变但详情变了 → [`Slot::Detail`]。
    ///
    /// `screen == None`（账户页 / AI 独立页：它们不走 `pages::render`）**只记标识、不起跑**——
    /// 否则会白跑 300 ms 的帧却没人采样它；而「从这些页回到某一屏」因为标识从 `None` 变回
    /// `Some`，仍然会跑换页那一档。
    ///
    /// 返回 `true` = 真的起跑了新轨道（调用方要重置帧基准，见 [`MAX_STEP_MS`]）。
    pub fn observe(&mut self, screen: Option<u64>, detail: Option<&str>) -> bool {
        let mut started = false;
        if !self.painted {
            self.painted = true;
            if screen.is_some() {
                started = self.play(Slot::Boot);
            }
        } else if self.screen != screen {
            if screen.is_some() {
                started = self.play(Slot::Screen);
            }
        } else if self.detail.as_deref() != detail && screen.is_some() {
            started = self.play(Slot::Detail);
        }
        self.screen = screen;
        if self.detail.as_deref() != detail {
            self.detail = detail.map(str::to_owned);
        }
        started
    }

    /// 页主体的入场位移（**只采样，无副作用**）：`pages::render` 每帧调一次。
    ///
    /// 三个槽的剩余量求和后**取整**：整像素位移不会让文字在动画期间发虚；末帧全部 `1.0`
    /// ⇒ 恒为 `(0, 0)` ⇒ 与静息的布局逐像素相同。
    pub fn offsets(&self) -> (f32, f32) {
        let mut dx = 0.0_f32;
        let mut dy = 0.0_f32;
        for slot in Slot::ALL {
            let remaining = 1.0 - self.progress(slot);
            let (ox, oy) = offset(slot);
            dx += ox * remaining;
            dy += oy * remaining;
        }
        (dx.round(), dy.round())
    }

    /// 推进时间轴。返回 `true` = 这一帧真的推进了（进度变了 ⇒ 要重画）。
    ///
    /// 四步：①不满 [`FRAME_MS`] 就不动（采样上限）②**单次最多吃 [`MAX_STEP_MS`]**（掉帧 /
    /// 被挂起 / 空闲醒来的第一帧都不许一次性跨过时长）③到期的轨道**当场删除**（这是「空闲不烧帧」
    /// 的唯一出口：删掉最后一个轨道之后 [`is_animating`](Self::is_animating) 立刻为假，下一拍订阅表
    /// 里就不再有 `window::frames()`）④钉住相位时时间轴不前进（取证专用）。
    pub fn advance(&mut self, dt_ms: f64) -> bool {
        if self.tracks.is_empty() {
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

        let now = self.elapsed_ms();
        let finished: Vec<Slot> = self
            .tracks
            .iter()
            .filter(|(_, track)| {
                let duration = f64::from(self.scale.ms(track.spec.duration));
                duration <= 0.0 || now - track.start_ms >= duration
            })
            .map(|(slot, _)| *slot)
            .collect();
        for slot in finished {
            self.tracks.remove(&slot);
        }
        true
    }
}

impl Default for Pump {
    fn default() -> Self {
        Self::new()
    }
}
