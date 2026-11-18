//! 提示条的**高度预算**（M357 / P1-1）：条自己多高、它从页面高度里吃掉多少。
//!
//! 提示条与页面是**竖排**的（`pages::body` 的 `column![banner, page]`），所以它占掉的高度必须
//! 真的从页面预算里扣掉，否则页面「规划用 380、实得 304」——M356 在 880×560 上量到的那个差
//! （条 64 + 间距 12 = 76）就是这么来的。
//!
//! 两个读取点读**同一个数**：
//!
//! | 谁 | 读什么 | 用途 |
//! |---|---|---|
//! | [`super::banner`] | [`Banner::height`] | 把条的盒子**钉死**成这个高（不再由内容撑） |
//! | [`Update::banner_reserve`] | 同一个值 + 间距 | 灌进 `layout::FrameMetrics::banner` |
//!
//! ## 为什么是公式而不是量出来的常数
//!
//! 文本按 [`TypeRole`] 的 `line_height_px` 排（`frame::text_role` 给 `Text` 设的是
//! `LineHeight::Absolute`），所以令牌行高就是渲染行高。实测（`pages_demo`，
//! `--scale-factor=1`，880 与 1180 同值）：有正文的三种形态条高 **64**、`Applying`（只有标题、
//! 连按钮都没有）**44**，与公式逐像素一致。
//!
//! ## 边界（没有假装）
//!
//! 条高是**钉死**的：正文折行会被裁。880 是产品最小窗（`app/mod.rs` 的 `MIN_SIZE`），
//! 三态（有新版 / 进行中 / 失败）实测都不折行；更窄的窗口只出现在 harness 里。

use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use super::{Banner, Update};

/// 条高公式：上下内边距 `Space::X3` ×2 + 标题行 + 有正文时的（`Space::X1` + 正文行）。
fn box_height(has_body: bool) -> f32 {
    let pad = 2.0 * Space::X3.dp();
    let title = TypeRole::LabelLarge.style().line_height_px;
    if has_body {
        pad + title + Space::X1.dp() + TypeRole::BodySmall.style().line_height_px
    } else {
        pad + title
    }
}

impl Banner<'_> {
    /// 这一形态的**条高**（dp，见模块文档）。
    ///
    /// `Applying` 是唯一没有正文行的形态，所以也是唯一矮一档的。
    #[must_use]
    pub fn height(self) -> f32 {
        box_height(!matches!(self, Banner::Applying))
    }
}

impl Update {
    /// 提示条这一帧从**页面高度预算**里占掉的总量（dp）：条高 + 它与页面之间的 `Space::X3`
    /// 间距；没有提示条 = `0.0`。
    ///
    /// 这是「提示条吃高度」的唯一出口：`pages::body` 把它灌进
    /// `layout::FrameMetrics::banner`，页面再经 `frame::body_height` 拿到扣完的正文高。
    #[must_use]
    pub fn banner_reserve(&self) -> f32 {
        match self.banner() {
            Some(kind) => kind.height() + Space::X3.dp(),
            None => 0.0,
        }
    }
}
