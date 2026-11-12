//! 动效**规格表**与位移数学（M256）：一次页面切换的方向/深度、两腿时长、模态的进出。
//!
//! 这一层是纯函数：不认识 iced、不认识时钟、不读环境变量。改观感只改这里的一张表。
//!
//! ## 为什么要有「方向」和「两腿」
//!
//! M192 只做了一次「新页从下方归位」（三个槽一张表）。M3 Expressive 的页面过渡不是这样：
//! 它是 **shared-axis**——同级换页走水平轴、进深层走纵深轴，而且**旧页反向退场**（两腿同时跑）。
//! 所以本表对每种切换给出四件东西：进场腿、退场腿、新页起点位移、旧页终点位移。
//!
//! | 切换 | 轴 | 新页从哪来 | 旧页去哪 | 进场 / 退场 |
//! |---|---|---|---|---|
//! | [`Kind::Boot`] 首屏 | 纵向 | 下方 `+X4`（16） | —— | `medium2` 300 / —— |
//! | [`Kind::Peer`] 同级换页 | 水平 | 右/左 `±X3`（±12） | 反向 `∓X2`（∓8） | `medium2` 300 / `medium1` 250 |
//! | [`Kind::Push`] 进深层（列表→详情） | 纵向 | 下方 `+X4`（16） | 上方 `-X2`（-8） | `medium1` 250 / `short4` 200 |
//! | [`Kind::Pop`] 出深层（详情→列表） | 纵向 | 上方 `-X4`（-16） | 下方 `+X2`（8） | `medium1` 250 / `short4` 200 |
//!
//! 三条口径（都有守卫）：①**进略长于出**（300>250、250>200）——M3 的进场要看得清，退场只是让位；
//! ②幅度全部落在 4px 栅格上（`Space` 令牌，不新造数字），且退场幅度**小于**进场幅度；
//! ③进/退曲线不同：进场 `Standard`（屏内元素起止都平滑），退场 `StandardAccelerate`
//! （M3 的「离场加速」口径，让旧页干脆地让出注意力）。

use unlucky_design::motion::{DurationToken, Easing, DIALOG_ENTER_SCALE};
use unlucky_design::spacing::Space;

/// 一次页面切换的方向与深度来源（判据在 `Pump::observe`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 首屏进入：进程内第一次看到归 pages 画的屏。没有旧页。
    Boot,
    /// **同级**换页：新页在导航表里更靠右（`forward`）或更靠左。外屏（账户 / AI）来回也走这一档。
    Peer {
        /// `true` = 新页在导航顺序里更靠后（向右推进）。
        forward: bool,
    },
    /// **进深层**：同一屏里从列表进详情（导航深度 +1）。
    Push,
    /// **出深层**：详情退回列表（导航深度 -1）。
    Pop,
}

impl Kind {
    /// 稳定名字（日志 / 断言失败信息用；产品路径不读这一档）。
    #[allow(dead_code)]
    pub const fn name(self) -> &'static str {
        match self {
            Kind::Boot => "boot",
            Kind::Peer { forward: true } => "peer-forward",
            Kind::Peer { forward: false } => "peer-back",
            Kind::Push => "push",
            Kind::Pop => "pop",
        }
    }

    /// 这一档有没有旧页参与退场（首屏没有）。
    #[allow(dead_code)]
    pub const fn has_old_page(self) -> bool {
        !matches!(self, Kind::Boot)
    }
}

/// 一条腿：时长档 × 缓动档（两个都是 design 令牌，不是毫秒字面量）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leg {
    /// 时长档（经 `MotionScale` 缩放后生效）。
    pub duration: DurationToken,
    /// 缓动曲线。
    pub easing: Easing,
}

/// 一次页面切换的完整规格。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSpec {
    /// 新页那一腿。
    pub enter: Leg,
    /// 旧页那一腿（退场）。
    pub exit: Leg,
    /// 新页的**起点**位移（终点恒为 `(0, 0)`）；正值 = 内容先被推到右下再归位。
    pub enter_from: (f32, f32),
    /// 旧页的**终点**位移（起点恒为 `(0, 0)`）。
    pub exit_to: (f32, f32),
}

/// 切换 → 规格（**唯一**一张页面动效表）。
pub const fn page_spec(kind: Kind) -> PageSpec {
    // 进场腿：屏内整块内容换人，用 Standard（起止都平滑）；退场腿用 StandardAccelerate
    // （M3 的离场加速，旧页干脆让位）。
    const ENTER: Leg = Leg {
        duration: DurationToken::Medium1,
        easing: Easing::Standard,
    };
    const EXIT: Leg = Leg {
        duration: DurationToken::Short4,
        easing: Easing::StandardAccelerate,
    };
    const PEER_ENTER: Leg = Leg {
        duration: DurationToken::Medium2,
        easing: Easing::Standard,
    };
    const PEER_EXIT: Leg = Leg {
        duration: DurationToken::Medium1,
        easing: Easing::StandardAccelerate,
    };

    // 幅度：只从间距令牌取（4 的倍数），退场幅度**小于**进场幅度（M3：退场不跟进场抢注意力）。
    let far = Space::X4.dp(); // 16
    let near = Space::X3.dp(); // 12
    let short = Space::X2.dp(); // 8

    match kind {
        Kind::Boot => PageSpec {
            enter: PEER_ENTER,
            exit: PEER_EXIT,
            enter_from: (0.0, far),
            exit_to: (0.0, 0.0),
        },
        Kind::Peer { forward: true } => PageSpec {
            enter: PEER_ENTER,
            exit: PEER_EXIT,
            enter_from: (near, 0.0),
            exit_to: (-short, 0.0),
        },
        Kind::Peer { forward: false } => PageSpec {
            enter: PEER_ENTER,
            exit: PEER_EXIT,
            enter_from: (-near, 0.0),
            exit_to: (short, 0.0),
        },
        Kind::Push => PageSpec {
            enter: ENTER,
            exit: EXIT,
            enter_from: (0.0, far),
            exit_to: (0.0, -short),
        },
        Kind::Pop => PageSpec {
            enter: ENTER,
            exit: EXIT,
            enter_from: (0.0, -far),
            exit_to: (0.0, short),
        },
    }
}

/// 模态的进 / 出两腿。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModalSpec {
    /// 进场：强调减速（M3 里"出现在屏上"的主曲线）。
    pub enter: Leg,
    /// 退场：强调加速，且**短于**进场。
    pub exit: Leg,
}

/// 模态规格（唯一来源）。
///
/// 时长取 `short4`（200ms）/ `short3`（150ms）：对话框是「用户刚做了一件事的回应」，
/// 属于 M3 的短档；比起页面切换它必须更短，否则会觉得界面在拖。
#[allow(clippy::missing_const_for_fn)]
pub const fn modal_spec() -> ModalSpec {
    ModalSpec {
        enter: Leg {
            duration: DurationToken::Short4,
            easing: Easing::EmphasizedDecelerate,
        },
        exit: Leg {
            duration: DurationToken::Short3,
            easing: Easing::EmphasizedAccelerate,
        },
    }
}

/// 一帧页面采样（**只读**：`pages::render` 每帧取一次，不起轨道）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageFrame {
    /// 新页位移（终点恒 `(0,0)`）。
    pub enter: (f32, f32),
    /// 旧页位移（起点恒 `(0,0)`）。
    pub exit: (f32, f32),
    /// 旧页是否还该被画出来（退场腿没走完）。`false` ⇒ 调用方**不要**再画旧页。
    pub exiting: bool,
}

impl PageFrame {
    /// 静息：新页在位、没有旧页。`MotionScale::NONE` 下**恒为**这个值（位移严格 0）。
    pub const STILL: Self = Self {
        enter: (0.0, 0.0),
        exit: (0.0, 0.0),
        exiting: false,
    };
}

/// 一帧模态采样。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModalFrame {
    /// 要不要把这一层画出来（`false` ⇒ 不画遮罩也不画卡片）。
    pub visible: bool,
    /// `0` = 刚起手（缩小 + 遮罩最淡），`1` = 完全展开（缩放 1.0 + 遮罩最浓）。
    pub progress: f32,
}

impl ModalFrame {
    /// 没有弹窗。
    pub const HIDDEN: Self = Self {
        visible: false,
        progress: 0.0,
    };
}

/// 模态的起始缩放（design 令牌 `DIALOG_ENTER_SCALE` 的再导出：视图侧只认这一条路径）。
pub const MODAL_SCALE_FROM: f32 = DIALOG_ENTER_SCALE;

/// 模态当前缩放：`progress = 0` 时是 [`MODAL_SCALE_FROM`]，`1` 时是 1.0。
pub fn modal_scale(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    MODAL_SCALE_FROM + (1.0 - MODAL_SCALE_FROM) * p
}

/// 端点固定的线性插值：`progress = 0` → `from`，`progress = 1` → `0`。取整到整像素。
///
/// 为什么取整：整页位移落在整像素上，动画期间文字不会因亚像素采样而发虚（M192 的同一口径）；
/// 末帧恒为 `(0, 0)` ⇒ 与静息逐像素相同。
pub fn enter_offset(from: (f32, f32), progress: f32) -> (f32, f32) {
    let remaining = 1.0 - progress.clamp(0.0, 1.0);
    ((from.0 * remaining).round(), (from.1 * remaining).round())
}

/// 从 `(0, 0)` 走到 `to` 的位移（旧页退场）。取整口径同上。
pub fn exit_offset(to: (f32, f32), progress: f32) -> (f32, f32) {
    let done = progress.clamp(0.0, 1.0);
    ((to.0 * done).round(), (to.1 * done).round())
}
