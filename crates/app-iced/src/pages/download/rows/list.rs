//! 版本行的**行序**与**增删动效**（M305）：键控列 + 逐行淡入淡出 / 高度收放 / 错位浮入。
//!
//! ## 这一层要同时满足三件事
//!
//! | 要求 | 落点 |
//! |---|---|
//! | 删除项**淡出后**才真正移除 | 行序由泵给（[`motion::rows::drawn`]），退场行还在序里、可见度一路走 0 |
//! | **不许整表瞬间重排闪跳** | ①行序不动（退出行钉在原位）②高度逐帧收到 0，下面那些行是滑上来的 |
//! | 增删**只影响变化项** | 列是 `keyed::Column`（按 id 做连续匹配），没变的行 `Tree` 节点原样复用 |
//!
//! ## 为什么是键控列（不是普通 `column`）
//!
//! iced 的 `Column` 按**下标**比对子节点：在第 3 行前面插一行，第 3 行起的子树全部错位重对一遍
//! （hover / 焦点 / 滚动锚点都跟着串台）。`keyed::Column` 用 key（= 版本 id）做
//! `diff_children_custom_with_search`，**没变的 id 直接命中同一个 `Tree` 节点**——这才谈得上
//! 「只影响变化项」。判据在 `tests/list_motion.rs`（变异：把 key 换成下标 ⇒ 那条红）。
//!
//! ## 一行为什么套一层
//!
//! [`motion::revealed`]：裁切框高 = `row_height × 可见度`，内容仍按 `row_height` 排版
//! （否则半行高会让行内文字重排「沸腾」，`motion/clip.rs` 的模块文档记的正是这条）。
//!
//! 这层**恒定存在**（静息时裁切高 = 内容高），所以动画起止不会换掉树的形状——[`motion::shifted`]
//! 的文档记的是同一条纪律（`Tree::diff` 按 widget tag 判等，只在动画期间才套一层的话，动画起止
//! 两侧会各重建一次子树，页面内的滚动位置 / 焦点会被重置）。
//!
//! **M405 订正**：这里原本写「要套**两层**」，第二层是 `motion::faded`（整份色板按可见度预合成）
//! —— 那个符号**全仓从未定义过**：`rg -n "fn faded" crates/` 除本注释外零命中，
//! `git log --oneline -S "fn faded" -- crates/app-iced/src/components/motion/` 零提交。
//! 本文件的动效只有 `motion::revealed` 这一层（行序与采样走 `motion::rows::*`）。是「那一层漏了」
//! 还是「文档一直没落地」从代码看不出，已报 finding；注释按**代码现状**写。

use iced::widget::{keyed, scrollable};
use iced::{Element, Length, Padding};

use unlucky_design::elevation::MaterialLayer;
use unlucky_design::spacing::Space;
use unlucky_i18n::Catalog;

use crate::app::components::motion;
use crate::theme::Ui;

use super::super::super::frame;
use super::super::super::Message;
use super::super::model::{Download, VersionRow};

/// 行高：`ListItem` 的单行档（令牌 `row_height`，桌面密集档 44）。
///
/// **只用来算裁切框**：内容的排版高恒为它，逐帧变的只有裁切框（见模块文档）。
pub(super) fn row_height(ui: &Ui) -> f32 {
    ui.metrics().row_height
}

/// 版本行列表（**独立滚动**：只有列表滚，安装面板钉在底部不跟着滚）。
///
/// `visible` 是在册的行（分类 + 桶过滤之后），`model.leaving` 是**还在退场**的行（已经不在模型
/// 里、但淡出没走完）。画哪一行、按什么序画，由 [`motion::rows::drawn`] 说了算。
pub(super) fn list<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Download,
    visible: &[&'a VersionRow],
    height: Option<f32>,
) -> Element<'a, Message> {
    scrollable(rows_column(ui, catalog, model, visible))
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(frame::SCROLLBAR_WIDTH)
                .scroller_width(frame::SCROLLBAR_WIDTH)
                .margin(frame::SCROLLBAR_MARGIN),
        ))
        // `Some` = 面板先保预算那一档按**整行**取高（三栏档 `tail::whole_rows` /
        // chips 档 `tail::narrow_rows`）；`None` = `Pinned`（面板矮）用 Fill。
        .height(height.map_or(Length::Fill, Length::Fixed))
        .style(frame::scroll_style(ui))
        .into()
}

/// 键控列本身（滚动区**不含**在里面）：单测要直接拿这一层去 `Tree::diff` 验节点复用。
pub(super) fn rows_column<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Download,
    visible: &[&'a VersionRow],
) -> Element<'a, Message> {
    let row_height = row_height(ui);
    // 画哪些行、什么序：**泵说了算**（退场行还钉在原位）。但泵没被喂过任何清单时（截图 harness
    // 的 `ready` 演示、单测直接构造模型、将来某个新入口忘了接线）它一条都给不出——那时**退回模型
    // 自己的顺序**。少了这一条兜底，这些路径的列表会整块空掉（M305 出图时实测抓到过一次：
    // 计数头写「8 versions」而下面一行都没有）。
    let drawn = motion::rows::drawn(motion::rows::List::Versions);
    let drawn: Vec<String> = if drawn.is_empty() {
        visible.iter().map(|row| row.id.clone()).collect()
    } else {
        drawn
    };

    let mut entries = keyed::Column::new()
        // M102：行距归零（见 `rows.rs` 模块文档），只留横向留白
        .padding(Padding::from([0.0, ui.space(Space::X2)]))
        .width(Length::Fill);

    for id in &drawn {
        // 数据池 = 在册 + 退场。找不到就跳过（快照里连版本都没了：没有数据可画）。
        let Some(row) = visible
            .iter()
            .copied()
            .chain(model.leaving.iter())
            .find(|row| &row.id == id)
        else {
            continue;
        };
        let sample = motion::rows::frame(motion::rows::List::Versions, id);
        let visibility = sample.visibility.clamp(0.0, 1.0);
        // 收尾遮罩：与卡片底色同色的 quad，α = 1 − 可见度。静息（1.0）时**不画**（`None`），
        // 于是末帧与静息逐像素相同。为什么不是「把 α 交给行内容」、为什么遮罩必须自成一层，
        // 见 `motion/reveal.rs` 的模块文档（两件事都有实测记录）。
        let veil = (visibility < 1.0).then(|| {
            // **底色是 Canvas，不是 Card**（M305 r1 P2-2）：版本行坐在页面的画布上
            // （行静息 `fill = None`，脚下那层是 `frame::page` 的 Canvas）。取成 Card 会让
            // 遮罩比底色亮一档：浅色 249 vs 247（几乎看不出），深色 tone 6 vs 10（露馅）。
            let under = ui.layer(MaterialLayer::Canvas);
            iced::Color {
                a: (1.0 - visibility).clamp(0.0, 1.0),
                ..under
            }
        });
        entries = entries.push(
            motion::rows::key(id),
            motion::revealed(
                super::version_row(ui, catalog, model, row),
                row_height * visibility,
                row_height,
                sample.offset,
                veil,
            ),
        );
    }

    entries.into()
}
