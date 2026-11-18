//! 实例列表的**行序**与**增删动效**（M313）：键控列 + 逐行淡入淡出 / 高度收放 / 错位浮入。
//!
//! ## 这一层不写动效逻辑
//!
//! 「退场行钉原位 / 行高收到 0 / 进出一条时钟 / 未变行同一个状态盒」那四件事是 M305 做成的
//! **唯一入口**（`components/motion/rows.rs` + `motion/{reveal,fade}.rs` + `pump/rows.rs`）。
//! 本文件只做「泵给的这一帧怎么落到树上」，与下载页的 `pages/download/rows/list.rs` 同一条形状：
//!
//! | 要求 | 落点 |
//! |---|---|
//! | 删除项**淡出后**才真正移除 | 行序由泵给（`motion::rows::drawn(List::Instances)`），退场行还在序里 |
//! | **不许整表瞬间重排闪跳** | ①行序不动（退场行钉在原位）②高度逐帧收到 0（同行随之上滑） |
//! | 增删**只影响变化项** | 列是 `keyed::Column`（按 id 做连续匹配），没变的行 `Tree` 节点原样复用 |
//!
//! ## 一行为什么要套两层（与下载页同一条理由）
//!
//! 1. [`motion::revealed`]：裁切框高 = 行高 × 可见度，**内容仍按原行高排版**（否则半行高会让
//!    行内文字重排「沸腾」——`motion/reveal.rs` 的模块文档记的正是这条）；
//! 2. 收尾遮罩：一层与**行脚下那层**同色的 quad（`α = 1 − 可见度`）。实例列表坐在
//!    `MaterialLayer::Card` 的卡里（`list_column` 的 style），所以底色是 Card——取成 Canvas
//!    会让遮罩比卡面暗一档（下载页反过来：它的行坐在页画布上，见 M305 r1 P2-2）。
//!
//! 两层**恒定存在**（静息时裁切高 = 行高、遮罩不画），所以动画起止不会换掉树的形状。
//!
//! ## 行高从哪来
//!
//! 不写死数字：`ListItem` 自己的几何（[`ListItemGeometry`]，44 / 60 / 76）与它自己的行数判据
//! （[`ListItem::lines`]）。行高若与组件各说一套，裁切框就会切在字中间。

use iced::widget::keyed;
use iced::{Element, Length};

use unlucky_design::elevation::MaterialLayer;

use crate::app::components::interaction::Interaction;
use crate::app::components::motion;
use crate::app::components::navigation::geometry::ListItemGeometry;
use crate::app::components::navigation::list_item::list_item;
use crate::theme::Ui;

use super::super::frame;
use super::super::model::{Instances, Row};
use super::super::Message;
use super::{is_selected, row_item};

/// 行容器（键控列）：画哪些行、什么序，由 `motion::rows` 说了算。
pub(super) fn column<'a>(
    ui: &'a Ui,
    block: &'a Instances,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let geometry = ListItemGeometry::of(ui);

    let mut cells = keyed::Column::new().width(Length::Fill);
    for row in drawn(block) {
        let id = &row.id;
        let item = row_item(row, is_selected(block, row), forced);
        // 行高**只从组件自己的几何来**（行数也由它自己数：`ListItem::lines`）。
        let height = geometry.height(item.lines());
        let sample = motion::rows::frame(motion::rows::List::Instances, id);
        let visibility = sample.visibility.clamp(0.0, 1.0);
        // 收尾遮罩：与卡片同色的 quad，α = 1 − 可见度。静息（1.0）时**不画**（`None`），
        // 于是末帧与静息逐像素相同（见模块文档「一行为什么要套两层」）。
        let veil = (visibility < 1.0).then(|| {
            let under = ui.layer(MaterialLayer::Card);
            iced::Color {
                a: (1.0 - visibility).clamp(0.0, 1.0),
                ..under
            }
        });
        cells = cells.push(
            motion::rows::key(id),
            motion::revealed(
                list_item(ui, item),
                height * visibility,
                height,
                sample.offset,
                veil,
            ),
        );
    }
    cells.into()
}

/// 列表滚动视口的**定高**（M357 / P2-2）：正文可用高 − 列头 − 卡片底部内缩，再按整行取整。
///
/// 列头 = 上内边距 `Space::X3` + 标签行（`LabelMedium` 行高）+ 下 `Space::X1`（见
/// `instances.rs::list_column` 里那个 `head` 容器）。**改列头结构要同步 [`HEAD_HEIGHT`]。**
pub(super) fn viewport_height(
    ui: &Ui,
    block: &Instances,
    forced: Option<Interaction>,
    body_height: f32,
) -> f32 {
    let room = body_height - HEAD_HEIGHT - ui.space(super::LIST_BOTTOM_INSET);
    whole_rows_height(ui, block, forced, room)
}

/// 列表列头（`All instances` 那行）的高度：12 + 16（`LabelMedium` 行高）+ 4 = **32**。
const HEAD_HEIGHT: f32 = 32.0;

/// 这一帧画出来的行，按**真实行高**累加、装进 `room` 的整行高度之和（M357 / P2-2）。
///
/// 与 [`column`] 读同一份行序、同一个 `ListItemGeometry`：调用方拿这个数当滚动视口的**定高**
/// （`Length::Fixed`），于是裁切线永远落在两行之间，不会切在某一行的字中间。
/// 行距是 `ROW_SPACING`（当前 `None` = 0），所以节距就是行高本身，不另加一项。
///
/// 为什么不是「一条常数节距取整」（版本管理页那种 `ROW_PITCH`）：这里的行高随行数分 44 / 60 / 76
/// 三档（`ListItemGeometry`），用 60 取整会把一条 76 的行切掉 16px。
fn whole_rows_height(ui: &Ui, block: &Instances, forced: Option<Interaction>, room: f32) -> f32 {
    let geometry = ListItemGeometry::of(ui);
    let spacing = super::ROW_SPACING.map_or(0.0, |space| ui.space(space));

    let heights = drawn(block).into_iter().flat_map(|row| {
        let item = row_item(row, is_selected(block, row), forced);
        [geometry.height(item.lines()), spacing]
    });

    frame::whole_rows_by(room, heights)
}

/// 这一帧要画的行（泵给的序；泵没被喂过清单时退回模型自己的顺序，理由见 [`column`] 的旧注释：
/// 演示模型 / 单测 / 将来某个新入口忘了接线时，泵一条都给不出，列表会整块空掉）。
fn drawn(block: &Instances) -> Vec<&Row> {
    let pumped = motion::rows::drawn(motion::rows::List::Instances);
    let ids: Vec<String> = if pumped.is_empty() {
        block.rows.iter().map(|row| row.id.clone()).collect()
    } else {
        pumped
    };

    // 数据池 = 在册 + 退场。找不到就跳过（快照里连这条实例都没了：没有数据可画）。
    ids.iter()
        .filter_map(|id| {
            block
                .rows
                .iter()
                .chain(block.leaving.iter())
                .find(|row| &row.id == id)
        })
        .collect()
}
