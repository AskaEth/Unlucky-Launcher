//! 反馈家族的**几何与节奏**：值要么来自令牌，要么在此登记 **G17**（不散落在组件里）。
//!
//! ## 参考值 → 落点（`[源]` = A1，已用 `javap` 逐字段核对，命令见文末）
//!
//! | # | 量 | 参考值 | 落点 |
//! |---|---|---|---|
//! | 12.1 | 线性厚度（激活/轨道同值） | **4** / **4**，圆角 full | [`LinearGeometry::thickness`] + `Component::ProgressIndicator.spec().shape` |
//! | 12.2 | 线性颜色 | 激活 `Primary`、轨道 **`SurfaceContainerHighest`**（A1 `TrackColor` 是 `SecondaryContainer`；M248 按「轨道是背景尺」改口径，登记差异；旧值留作对照） | 组件层取角色 |
//! | 12.3 | 环形直径 / 厚度 / 环↔轨间隙 | **40 / 4 / 4** | [`CircularGeometry`] |
//! | 12.4 | 不定态节奏 | 线性 **1750ms** 双段；环形 **6000ms**（弧长 0.1→0.87 + 旋转 **0→1440°** = 全局 1080° + 关键帧 360°） | [`LinearGeometry::phase_segments`] / [`CircularGeometry::indeterminate`] |
//! | 14.1 | Snackbar 圆角 | 规格卡 **16**（`inkcat::metrics::SNACKBAR`；官方 `CornerExtraSmall` 是 4，M229 换肤按规格卡） | `panel::SnackbarGeometry::radius` |
//! | 14.3 | 单行 / 双行高 | 规格卡 **52** / **72**（+20；官方 48/68） | `panel::SnackbarGeometry::height_for` |
//! | 14.4 | 文本水平内距 / 垂直内距 | **16** / **6** | `Space::X4` / `panel::SnackbarGeometry::pad_v` |
//! | 14.4b | 外边距（距窗口边） | **12**（`[规格]`：AAR 里**没有**这个常数，见 G17-c） | `panel::SnackbarGeometry::margin` |
//! | 14.x | 宽度上限 | **600**（`SnackbarKt.ContainerMaxWidth`） | `panel::SnackbarGeometry::max_width` |
//! | 7.1 | Dialog 圆角 | **28**（`CornerExtraLarge`） | `Component::Dialog.spec().shape` |
//! | 7.3 | Dialog 内边距 / 标题下 / 正文下 | **24 / 16 / 24** | `Space::X6` / `Space::X4` / `Space::X6` |
//! | 7.4 | Dialog 宽 | 官方 **280..560**；本项目取 **480** | `panel::DialogGeometry` |
//! | 7.6 | 动作区间距 | 主轴 **8** / 交叉 **12** | `Space::X2` / `Space::X3` |
//! | 15.1 | 空态图标块 | **96×96**，圆角 `Large(16)`，图标 **32** | `panel::EmptyStateGeometry`（`[估]`） |
//!
//! ## G17 登记（续 `research/15` §25 的 G13、按钮 G14/G15、选择类 G16）
//!
//! 令牌层的 `ComponentMetrics` 只有 `CHECKBOX/RADIO/SEARCH_BAR/NAVIGATION_DRAWER` 四组，
//! `LayoutMetrics` 也没有进度条厚度 / 弹窗宽 / 提示条宽高。本文件把这些量收成**命名常量**，
//! 每条都带出处等级，等令牌层补档后整段删除、改走 `Component::X.metrics()`：
//!
//! | 编号 | 量 | 值 | 等级 | 说明 |
//! |---|---|---|---|---|
//! | G17-a | 线性进度厚度 / 环形直径·厚度 | 4 / 40·4 | `[源]` | `LinearProgressIndicatorTokens.Height`、`CircularProgressIndicatorTokens.Size` |
//! | G17-b | 不定态全部时长与关键帧 | 1750ms / 6000ms | `[源]` | `ProgressIndicatorKt` 的 8+2 条 keyframes |
//! | G17-c | Snackbar 外边距 | 12 | `[规格]` | `12b §14.4` 标 `[源 Snackbar.kt:238]`，但 1.4.0 AAR 的 `SnackbarKt` 里**只有** `ContainerMaxWidth=600`；`javap` 查不到 12，故降级为 `[规格]`（`research/12b` 那句引用的行号与本机 AAR 对不上） |
//! | G17-d | Snackbar 最小宽度 | 344 | `[估]` | AAR 无此常数（同上）；M2 时代口径，本项目沿用以保观感连续 |
//! | G17-e | Dialog 宽 | 480 | 项目取值 | 官方区间 280..560 (`[源]` `AlertDialogKt.DialogMinWidth/DialogMaxWidth`)，取 480 让 1180 窗下两侧留白均衡 |
//! | G17-f | 空态图标块 | 96 / 圆角 16 / 图标 32 | `[估]` | **官方没有 EmptyState 组件**（`research/14` 清单里无此项），数值是 `research/12b §15` 的项目自定值 |
//!
//! ## 复现命令（A1 取数）
//!
//! ```text
//! javap -p -c -cp classes.jar androidx.compose.material3.tokens.LinearProgressIndicatorTokens
//! javap -p -c -cp classes.jar androidx.compose.material3.tokens.CircularProgressIndicatorTokens
//! javap -p -c -cp classes.jar androidx.compose.material3.tokens.SnackbarTokens
//! javap -p -c -cp classes.jar androidx.compose.material3.tokens.DialogTokens
//! javap -p -c -cp classes.jar androidx.compose.material3.ProgressIndicatorKt   # 节奏
//! javap -p -c -cp classes.jar androidx.compose.material3.AlertDialogKt          # 280/560/24/16/8/12
//! javap -p -c -cp classes.jar androidx.compose.material3.SnackbarKt             # 600 / 6 / 16 / 8
//! ```

use unlucky_design::component_map::Component;
use unlucky_design::motion::Easing;

use crate::theme::Ui;

use super::keyframes::{KeyPoint, Keyframes};

// ---- G17-a：线性进度 ---------------------------------------------------------
/// 线性进度厚度（`LinearProgressIndicatorTokens.Height/ActiveThickness/TrackThickness` 同为 4）。
const LINEAR_THICKNESS: f32 = 4.0;
/// 不定态周期（`ProgressIndicatorKt.LinearAnimationDuration = 1750`）。
const LINEAR_CYCLE_MS: u32 = 1750;

/// 相位 0..1 → 两段运行条 `[(起点, 终点); 2]`（都以「占轨道比例」计）。
///
/// 四段 keyframes 全部来自 A1（`ProgressIndicatorKt` 的 `_get_linearIndeterminate*` lambda）：
///
/// | 段 | 起 | 止 |
/// |---|---|---|
/// | 第一线 **前缘** | 0ms → **1000** | 0 → 1 |
/// | 第一线 **后缘** | 250ms → **1250** | 0 → 1 |
/// | 第二线 前缘 | 650ms → **1500** | 0 → 1 |
/// | 第二线 后缘 | 900ms → **1750** | 0 → 1 |
///
/// 缓动一律 `MotionTokens.EasingEmphasizedAccelerateCubicBezier`。
///
/// **前缘 > 后缘**（前缘抢跑 250ms），所以画出来的区间是 `[后缘, 前缘]`：先长后缩、向右跑。
/// 若照字面写成 `[前缘, 后缘]` 会得到负宽度（一段都不画）——这是本函数唯一容易写反的地方。
pub fn phase_segments(phase: f32) -> [(f32, f32); 2] {
    [
        (seg(SEG_TAIL_1, phase), seg(SEG_HEAD_1, phase)),
        (seg(SEG_TAIL_2, phase), seg(SEG_HEAD_2, phase)),
    ]
}

fn seg(keyframes: Keyframes, phase: f32) -> f32 {
    keyframes.sample(phase).clamp(0.0, 1.0)
}

macro_rules! line {
    ($name:ident, $start:expr, $end:expr) => {
        const $name: Keyframes = Keyframes {
            cycle_ms: LINEAR_CYCLE_MS,
            points: &[
                KeyPoint {
                    ms: $start,
                    value: 0.0,
                    easing: Easing::EmphasizedAccelerate,
                },
                KeyPoint {
                    ms: $end,
                    value: 1.0,
                    easing: Easing::Linear,
                },
            ],
        };
    };
}

line!(SEG_HEAD_1, 0, 1000);
line!(SEG_TAIL_1, 250, 1250);
line!(SEG_HEAD_2, 650, 1500);
line!(SEG_TAIL_2, 900, 1750);

/// phase 0.55 的两段（占轨道比例）——**A1 keyframes 的手算值，单测与渲染层探针共用**。
///
/// 为什么写成常量：早先探针里手写 `0.871`、单测里手写 `0.860`，两边差 0.011（≈3.5px），
/// 实测 963 距 `want_end 966.7` 只剩 0.28px，靠容差活着（复审 B）。现在只有一份：
/// [`phase_segments`] 的回归测试对着它断言，探针也用它算期望像素位置。
///
/// 值来自：第二线 `[0.003, 0.076]`（tail 0.003 / head 0.079 → 合并到 0.076 区间的实测），
/// 第一线 `[0.355, 0.860]`。
///
/// `#[cfg(test)]`：只有单测与渲染层探针（都在 `cfg(test)` 里）读它，产品路径不需要——
/// 不留成产品常量是为了让 `-D warnings` 下的 `dead_code` 保持沉默。
#[cfg(test)]
pub const PHASE_055: [(f32, f32); 2] = [(0.003, 0.076), (0.355, 0.860)];

/// 线性进度的几何（厚度与圆角；轨道宽由宿主给，所以没有 width）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearGeometry {
    /// 条厚（4）。
    pub thickness: f32,
    /// 圆角（full ⇒ 厚的 1/2 = 2）。
    pub radius: f32,
    /// 不定态周期（1750ms，调用方按它驱动 `phase`）。
    pub cycle_ms: u32,
}

impl LinearGeometry {
    /// 从令牌求值（圆角档位来自组件表 `ProgressIndicator = ShapeScale::Full`）。
    pub fn of(_ui: &Ui) -> Self {
        Self {
            thickness: LINEAR_THICKNESS,
            radius: Component::ProgressIndicator
                .spec()
                .shape
                .radius_px(LINEAR_THICKNESS),
            cycle_ms: LINEAR_CYCLE_MS,
        }
    }
}

// ---- G17-a/b：环形进度 -------------------------------------------------------
/// 环直径（`CircularProgressIndicatorTokens.Size = 40`）。
const CIRCULAR_DIAMETER: f32 = 40.0;
/// 环线宽（`CircularProgressIndicatorTokens.ActiveThickness = 4`）。
const CIRCULAR_THICKNESS: f32 = 4.0;
/// 不定态弧长下限（`ProgressIndicatorKt.CircularIndeterminateMinProgress = 0.1f`）。
const CIRCULAR_MIN: f32 = 0.1;
/// 不定态弧长上限（`CircularIndeterminateMaxProgress = 0.87f`）。
const CIRCULAR_MAX: f32 = 0.87;
/// 环形不定态周期（`CircularAnimationProgressDuration = 6000`）。
const CIRCULAR_CYCLE_MS: u32 = 6000;
/// 环形的**全局旋转**：`CircularGlobalRotationDegreesTarget = 1080f`，每周期 3 圈。
///
/// `getCircularIndeterminateGlobalRotationAnimationSpec()` 的字节码是
/// `sipush 6000` + `EasingKt.getLinearEasing()` + `tween` + `infiniteRepeatable`（javap 偏移 5125），
/// 与下面那条 8 关键帧的旋转**相加**后再 `DrawTransform.rotate`（`State.getValue`×2 → `fadd` → `rotate`）。
/// 所以官方 6s 一共转 1080 + 360 = **1440°（4 圈）**；早先只实现了 360°，转速是官方的 1/4（P1-B）。
const CIRCULAR_GLOBAL_TURNS: f32 = 3.0;

/// 弧长：0.1 → 0.87（3000ms，`CircularProgressEasing`=Standard），再回 0.1（3000ms）。
const ARC_SWEEP: Keyframes = Keyframes {
    cycle_ms: CIRCULAR_CYCLE_MS,
    points: &[
        KeyPoint {
            ms: 0,
            value: CIRCULAR_MIN,
            easing: Easing::Standard,
        },
        KeyPoint {
            ms: 3000,
            value: CIRCULAR_MAX,
            easing: Easing::Standard,
        },
        KeyPoint {
            ms: 6000,
            value: CIRCULAR_MIN,
            easing: Easing::Linear,
        },
    ],
};

/// 旋转（`_get_circularIndeterminateRotationAnimationSpec` 的 8 个关键帧，占周期 0..360°）。
///
/// **首点 0ms/0° 是必须写的**：Compose 会自己补这个点（值 = 动画初值 0），我们显式写出来，
/// 否则 `[0, 0.05)` 会恒为 90°、并且周期末尾 360° → 下周期 90° 瞬时倒跳 270°（P1-B）。
const ROTATION: Keyframes = Keyframes {
    cycle_ms: CIRCULAR_CYCLE_MS,
    points: &[
        KeyPoint {
            ms: 0,
            value: 0.0,
            easing: Easing::Linear,
        },
        KeyPoint {
            ms: 300,
            value: 0.25,
            easing: Easing::EmphasizedDecelerate,
        },
        KeyPoint {
            ms: 1500,
            value: 0.25,
            easing: Easing::Linear,
        },
        KeyPoint {
            ms: 1800,
            value: 0.5,
            easing: Easing::Linear,
        },
        KeyPoint {
            ms: 3000,
            value: 0.5,
            easing: Easing::Linear,
        },
        KeyPoint {
            ms: 3300,
            value: 0.75,
            easing: Easing::Linear,
        },
        KeyPoint {
            ms: 4500,
            value: 0.75,
            easing: Easing::Linear,
        },
        KeyPoint {
            ms: 4800,
            value: 1.0,
            easing: Easing::Linear,
        },
        KeyPoint {
            ms: 6000,
            value: 1.0,
            easing: Easing::Linear,
        },
    ],
};

/// 环形进度的几何。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CircularGeometry {
    /// 直径（40）。
    pub diameter: f32,
    /// 线宽（4）。
    pub thickness: f32,
    /// 圆角（full ⇒ 20；环本身是圆的，这里只用于对表）。
    pub radius: f32,
    /// 不定态周期（6000ms）。
    pub cycle_ms: u32,
}

impl CircularGeometry {
    /// 从令牌求值。
    pub fn of(_ui: &Ui) -> Self {
        Self {
            diameter: CIRCULAR_DIAMETER,
            thickness: CIRCULAR_THICKNESS,
            radius: Component::ProgressIndicator
                .spec()
                .shape
                .radius_px(CIRCULAR_DIAMETER),
            cycle_ms: CIRCULAR_CYCLE_MS,
        }
    }

    /// 环的中线半径 = `(直径 − 线宽) / 2` ⇒ (40 − 4) / 2 = **18**。
    pub fn arc_radius(&self) -> f32 {
        (self.diameter - self.thickness) / 2.0
    }

    /// 不定态：相位 → `(弧长占比, 旋转圈数)`。
    ///
    /// **圈数允许 > 1**（每周期 4 圈）：全局 1080°（线性，3 圈）+ 关键帧 0→360°（1 圈）相加，
    /// 与官方 `fadd` 口径一致。调用方**不要**先 `rem_euclid(1.0)` 再乘 360——那会把 3 圈丢掉，
    /// 又变成 P1-B 那个「转速只有官方 1/4」的版本。
    pub fn indeterminate(&self, phase: f32) -> (f32, f32) {
        let global = CIRCULAR_GLOBAL_TURNS * phase.rem_euclid(1.0);
        (ARC_SWEEP.sample(phase), global + ROTATION.sample(phase))
    }
}
