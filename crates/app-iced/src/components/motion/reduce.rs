//! 「减少动效」的**合成策略**：系统偏好怎么落到 [`MotionScale`]，以及为什么是「减轻」而不是「全无」。
//!
//! ## 这个模块回答什么
//!
//! 探针（三态、fail-open、Windows 走 `SPI_GETCLIENTAREAANIMATION`）在
//! [`crate::platform::motion`]；这里是**合成**：纯函数 [`effective_scale`] + 产品入口的 [`resolve`](crate::app::components::motion::reduce::resolve)
//! （探针 → 档位 → 一行日志）。`GAPS.md` §2.3 的两块未核之一就是「系统偏好到底把动效减了多少、
//! 依据是什么」——这张表就是答案。
//!
//! | 系统偏好 | 传入档 `S` | 结果 | 为什么 |
//! |---|---|---|---|
//! | [`ReduceMotion::Reduced`](crate::platform::motion::ReduceMotion::Reduced) | 任意 | `min(S, 0.5)` | 系统明确要求减轻 ⇒ **时长减半**，所有转场照旧跑完 |
//! | [`ReduceMotion::AnimationsOn`](crate::platform::motion::ReduceMotion::AnimationsOn) | 任意 | `S` | 系统里动效开着 ⇒ 不改 |
//! | [`ReduceMotion::Unknown`](crate::platform::motion::ReduceMotion::Unknown) | 任意 | `S` | **读不到 ≠ 要减少**（fail-open，见 `platform/motion` 的模块文档） |
//! | 任意（含 `Reduced`） | 任意，且**钉住相位** | `S` | 取证要可复现：钉住时不被系统偏好降档 |
//!
//! ## 为什么不是 `NONE`（M396 改的就是这一格）
//!
//! M256 把 `Reduced` 直接映射成 [`MotionScale::NONE`]，等于「用户只是希望界面安静一点」却拿到
//! 「所有转场瞬间消失」。两件事被混成了一件：
//!
//! - **「减少动效」= 少动、快动**：位置/时长还在，只是挪得没那么久。系统那个开关的字面语义
//!   （Windows「动画效果」、GNOME `enable-animations`、macOS `reduceMotion`）都是「减轻」，不是「没有」。
//! - **「关闭动效」= 不动**：这是我们**自己的**排障开关（`ULL_MOTION=off`）与截图通道（D30 要可复现），
//!   跟用户的偏好不是同一件事。它们照旧给 `NONE`，且**不被本模块改回去**（见下面的「上限」）。
//!
//! ## 减多少：时长减半（`0.5`），以及三条依据
//!
//! 1. **可复算**：所有腿都走 [`MotionScale::ms`]，减半就是「帧数减半」。60 Hz 采样下 `Boot` 那一档
//!    从 19 帧掉到 10 帧（守卫 `tests::modal::reduce_halves_the_frames_instead_of_killing_them` 钉着）。
//! 2. **不跌破令牌阶梯**：最短的腿是 `short3`（150 ms），减半后 75 ms——仍在 design 令牌表的下沿
//!    （`short1` = 50 ms）**之上**，所以「快」但不至于读成一次硬跳
//!    （守卫 `tests::modal::reduced_legs_stay_above_the_shortest_token`）。
//! 3. **同一系数已在生产里跑过**：CPU 档（`tiny-skia`）用的就是 `HALF`（`app/update/boot_env.rs` 的表），
//!    它的末帧与静息逐像素相同这件事全树已有守卫。为一个「减轻」再新造第三个系数没有证据支撑。
//!
//! ## 为什么不能再减「幅度」（这一层的边界，写清楚免得下次有人以为漏了）
//!
//! 减少动效在无障碍口径里减的是**位移/缩放**（前庭敏感），而我们的页面采样 [`crate::app::components::motion::PageFrame`]
//! 只有 `enter` / `exit` 两个位移量与一个 `exiting` 标志，**没有 alpha**。把位移归零 = 新页整块盖住旧页 =
//! 硬跳（旧页那一腿还白跑 250 ms 的帧），那不是「减轻」而是「全无」。真要「换成淡入」得动 `pages/`
//! 那一层（渲两块板 + 交叉 α），不在 M396 的 scope——已作为 follow-up 写进报告。
//!
//! ## 上限语义（别把它写成「等于」）
//!
//! 结果取 `min(传入档, 0.5)`，不是「一律 0.5」：截图通道 / `ULL_MOTION=off` 传进来的 `NONE`（0.0）必须
//! **留在 0**——系统偏好多半只是没读到用户那台机器的开关，凭什么替他把已经关掉的动效又打开？
//! 这条与 fail-open 是同一条纪律的两面：**系统偏好只能往下压，不能往上抬。**
//!
//! ## 日志
//!
//! [`resolve`](crate::app::components::motion::reduce::resolve) 每次定档写一行 `[motion] sys reduce=… asked=… scale=…`（经 `crate::app::diag::emit`：
//! **stderr 与** `<.ULL>/logs/launcher.log` 双写）。取证要靠它：改系统开关 → 重启 → 这一行的
//! `reduce=` 与 `scale=` 都变。光有 `app/update.rs` 那条 `[motion] scale=` 分不清「系数变了」与
//! 「策略变了」——那一条打的是结果，这一条打的是**来源**。

use unlucky_design::motion::MotionScale;

use crate::app::diag;
use crate::platform::motion::ReduceMotion;

/// 系统要求减少动效时的系数：**时长减半**（`0.5`），不是归零。依据见模块文档。
pub const REDUCED_FACTOR: f32 = 0.5;

/// 档位与系统偏好的**合成本身**（纯函数，可断言）。
///
/// `pinned` = 取证相位钉住（`ULL_MOTION_AT`）：多帧序列图要在同一台机器上重复拍到同一相位，
/// 所以钉住时**不**被系统偏好降档（否则取证机一旦开了「减少动效」就拍不到完整序列）。
pub fn effective_scale(scale: MotionScale, reduce: ReduceMotion, pinned: bool) -> MotionScale {
    match reduce {
        ReduceMotion::Reduced if !pinned => capped(scale),
        _ => scale,
    }
}

/// 只往下压：`NONE` 仍是 `NONE`（见模块文档「上限语义」）。
fn capped(scale: MotionScale) -> MotionScale {
    MotionScale::new(scale.factor().min(REDUCED_FACTOR))
}

/// 产品入口的定档：探针 → 合成 → 一行日志，返回**帧泵真正该用的**档位。
///
/// `forced` 是测试专用钉住（`super::configure_for_test`）：给了就既不走探针也不走合成，
/// 用例因此与宿主机的「减少动效」开关无关。
pub fn resolve(asked: MotionScale, pin: Option<f64>, forced: Option<MotionScale>) -> MotionScale {
    let (source, scale) = match forced {
        Some(forced) => ("forced", forced),
        None => {
            let reduce = ReduceMotion::probe();
            // 钉住相位时探针结果照样记进日志（取证要看得见宿主机是哪一档），只是不参与合成。
            (
                reduce.as_str(),
                effective_scale(asked, reduce, pin.is_some()),
            )
        }
    };
    // 走 `diag::emit`：stderr 与 `launcher.log` 双写（截图通道不一定会落到日志目录，stderr 一定在）。
    diag::emit(&format!(
        "[motion] sys reduce={source} asked={:.2} scale={:.2} pinned={}",
        asked.factor(),
        scale.factor(),
        pin.is_some()
    ));
    scale
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **`Reduced` 是减半，不是归零**——M396 的核心判据。
    ///
    /// 抓得住：把 `Reduced` 改回 `MotionScale::NONE`（M256 的老实现，"少一点动效"变成"没有动效"），
    /// 或把它写成「一律 0.5」（那样截图通道的 `NONE` 会被重新打开）。
    /// 抓不住：75 ms 在真机上顺不顺眼（那是观感，要录像，见 `DevDocs/artifacts/motionclose`）。
    #[test]
    fn reduced_halves_instead_of_killing() {
        assert_eq!(
            effective_scale(MotionScale::STANDARD, ReduceMotion::Reduced, false),
            MotionScale::HALF,
            "系统要求减少动效 ⇒ 时长减半（不是 0）"
        );
        assert_eq!(
            effective_scale(MotionScale::HALF, ReduceMotion::Reduced, false),
            MotionScale::HALF,
            "已经半档 ⇒ 不再减（不会越减越小）"
        );
        // 上限语义：只往下压，绝不往上抬。
        assert_eq!(
            effective_scale(MotionScale::NONE, ReduceMotion::Reduced, false),
            MotionScale::NONE,
            "自己关掉的动效（截图通道 / ULL_MOTION=off）不许被系统偏好重新打开"
        );
        assert_eq!(
            effective_scale(MotionScale::new(0.25), ReduceMotion::Reduced, false),
            MotionScale::new(0.25),
            "更小的自定义系数保持原样（只能压到 0.5 以下时不再抬回 0.5）"
        );
    }

    /// 另三态**一律不改档**：`AnimationsOn` / `Unknown` 原样，钉住相位时 `Reduced` 也原样。
    ///
    /// 抓得住：把 `Unknown` 也降档（探针失败就静默减动效）——那会在某些机器上凭空少掉一半动效。
    #[test]
    fn only_an_explicit_reduced_and_unpinned_changes_the_scale() {
        for scale in [MotionScale::STANDARD, MotionScale::HALF, MotionScale::NONE] {
            assert_eq!(
                effective_scale(scale, ReduceMotion::AnimationsOn, false),
                scale
            );
            assert_eq!(effective_scale(scale, ReduceMotion::Unknown, false), scale);
            assert_eq!(effective_scale(scale, ReduceMotion::Reduced, true), scale);
        }
    }
}
