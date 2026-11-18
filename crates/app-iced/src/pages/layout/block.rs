//! 页面块几何：**块宽 / 块偏移 / 散文列宽**（纯函数，不碰 iced 控件树）。
//!
//! ## 为什么这三个量单独住一个文件（M102 r2）
//!
//! M102 把「一句话多宽」和「整页块多宽」拆成两个量，那段口径原本写在 `layout.rs` 的模块文档里，
//! 连同 1440 上限一起把主文件顶到 **399 行**（CI 的 file-size 门禁是 `wc -l`，只剩 1 行余量）。
//! 挪到本文件：主文件回到安全线以下，**口径与常量同处一室**——谁改上限都得读同一段话。
//!
//! ## 三个量（页面只许从这里取，不许自己算）
//!
//! | 量 | 值 | 用在哪 |
//! |---|---|---|
//! | [`body_width`] | `min(pane, 1440)`；pane = `available`（单列）/ `main_pane`（两列） | 页面块：Tabs / 列表 / 卡片 / 分隔线 |
//! | [`body_offset`] | **0**（贴左） | 块的左缘相对页边距的偏移 —— 0 就是「与页头标题同一条线」 |
//! | [`prose_width`] | [`FrameMetrics::column`]（≤424） | 成段散文（`frame::prose`），保证一行 ≤72 字符 |
//!
//! ## 为什么 1440 是上限（tower 拍板，M102 r2）
//!
//! 块宽**没有上限**时，2560 窗宽下单列页的块会一路长到 2424px：一行 80 个英文字符的列表行
//! 空得发慌，中文正文同一行更能排到 170 字。取 **1440**（落在 4px 栅格上，= 360 × 4）：
//! 在 1180（可用 1044）与 900（764）上**都不生效**，只拦极宽窗。
//!
//! 与 [`super::measure_cap_px`]（424，用 FreeType 实测步进 × 逐行折行模拟推出来的）不同，
//! 1440 是**决策值**不是量出来的：它没有排版学推导，只有「别让列表行排成一条高速路」这一条理由。
//! 所以它只该在这一处出现——谁想改，改这里并同时改 `block/tests.rs` 的断言。
//!
//! 贴左不变：块左缘仍是 `rail 88 + gutter 24 = 112`，封顶后**右侧留白**（不是居中）。
//! 这是有意的：页头标题在左，正文块跟着左缘；把块居中又会回到视觉复审 ① 的「内容缩在中间」。
//!
//! ## 这一层唯一的行为变化，以及它从哪一版开始
//!
//! 封顶是 M102 r2 引入的**有意行为变化**：`≥ 1576` 窗宽（`112 + 1440 + 24`）下块宽不再跟随可用宽度。
//! 在 1180 / 900 上它**不生效**——那两张验收图与 r1 逐字节相同（9/9 `cmp`）。所以对外口径是
//! 「**≤1440 窗宽零像素变化；更宽的窗口按本上限有意改变**」，不是笼统的「零变化」。
//! （`frame::body` 那行 `padding(left(body_offset))` 在同一轮加进来，值恒 0 ⇒ 像素中性。）
//!
//! ## 偏移为什么是个常量函数而不是 `align_x`
//!
//! iced 的 `Element` **没有几何内省接口**，「块摆在哪」在单测里量不到。所以把「贴左」写成一个
//! 可断言的纯量 [`body_offset`]（= 0），让 `frame::body` 从它建盒，再配一条**源码结构**断言
//! （`pages/tests.rs::frame_body_is_built_from_the_block_quantities`）钉住「盒子的左内边距来自它」。
//! 变异实测：把 `frame::body` 换回 `centered(metrics.column, …)`（视觉复审 ① 的原样）⇒ 该断言红。

use super::{FrameMetrics, PageLayout};

/// 页面块宽度上限（逻辑像素，4px 栅格的 1440 = 360dp × 4）。
///
/// 只拦极宽窗：1180 窗宽下可用 1044、900 下 764，都够不到它。
pub const BODY_MAX_WIDTH: f32 = 1440.0;

/// 页面块宽度（px）：pane 宽封顶 1440。
///
/// 单列 = 整条可用宽度；两列 = 主列 pane（列表列 / 侧列已被扣掉）。
pub fn body_width(metrics: FrameMetrics) -> f32 {
    let pane = match metrics.layout {
        PageLayout::SingleColumn => metrics.available,
        PageLayout::SupportingPane | PageLayout::ListDetail => metrics.main_pane,
    };

    if pane < BODY_MAX_WIDTH {
        pane
    } else {
        BODY_MAX_WIDTH
    }
}

/// 页面块相对页边距的**左偏移**（px）：恒为 **0**（贴左）。
///
/// 「贴左」是 M102 的头号交付（视觉复审 ① 是「424 居中 ⇒ 左缘 422 对标题 112」），
/// 所以它必须是一个能被断言的数，而不是一处 `align_x` 的写法。
pub const fn body_offset(_metrics: FrameMetrics) -> f32 {
    0.0
}

/// 散文块宽度（px）= [`FrameMetrics::column`]（≤424 ⇒ 逐行 ≤72 字符）。
///
/// 与 [`body_width`] 是两个量：块可以是 1044，一句话仍然只排 424。
pub const fn prose_width(metrics: FrameMetrics) -> f32 {
    metrics.column
}

#[cfg(test)]
mod tests;
