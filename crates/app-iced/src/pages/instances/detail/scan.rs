//! 首屏三态的分派（M302；从 `detail.rs` 拆出来守 400 行硬线）。
//!
//! `detail_side` 是**唯一**分派点：`pages/instances.rs` 的宽窗与窄窗两条调用都走它，
//! 于是「未就绪」不可能只在一个版面上生效。

use iced::Element;

use unlucky_design::icons::IconKind;

use crate::app::components::buttons::{self, Button};
use crate::theme::Ui;

use super::super::super::frame;
use super::super::super::model::{Instances, ScanPhase};
use super::super::state::Message as InstanceMessage;
use super::super::{Message, Texts};
use super::detail_empty;

/// 没选中实例时，详情列画什么（**M302 的三态唯一分派点**）。
///
/// | `block.scan` | 画什么 | 抓的是什么 |
/// |---|---|---|
/// | `Scanning` | [`Texts::scanning`]：一行「正在扫描实例…」 | 首个快照还没到 |
/// | `Stalled` | [`Texts::scan_stalled`]：一句话 + 可点的「重新扫描」 | 扫描线程没回来（`host::game::refresh::SCAN_STALL`） |
/// | `Ready` | [`detail_empty`]：两种空态（没选中 / 一个都没有） | 数据是真的 |
///
/// 顺序是判据：**先看 `scan` 再看 `rows`**。反过来的话，冷启动首帧（`scan = Scanning`、
/// `rows` 空）就会被 `!rows.is_empty()` 判成「一个实例都没有」，画出那句假话（复审 r1 P2-1）。
pub(in crate::pages) fn detail_side<'a>(
    ui: &'a Ui,
    block: &'a Instances,
    texts: Texts,
) -> Element<'a, Message> {
    match block.scan {
        ScanPhase::Scanning => scan_placeholder(ui, texts.scanning(), None),
        ScanPhase::Stalled => scan_placeholder(
            ui,
            texts.scan_stalled(),
            Some(buttons::filled(
                ui,
                Button::new(
                    texts.scan_retry(),
                    Message::Instances(InstanceMessage::RetryScan),
                ),
            )),
        ),
        ScanPhase::Ready => detail_empty(ui, texts, !block.rows.is_empty()),
    }
}

/// 首屏占位（M302）：只画标题 + 可选的一条动作，**不**带空态 CTA。
///
/// 为什么单独一个函数而不是复用 [`super::empty_no_instances`]：那条支的语义是「盘上真的没有实例，
/// 所以请去导入」。「还在读盘」这件事跟它相反——用户可能什么都有，只是还没读完。
fn scan_placeholder<'a>(
    ui: &'a Ui,
    title: &'a str,
    action: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    frame::empty_state(
        ui,
        IconKind::Instances,
        title,
        "",
        action.into_iter().collect(),
    )
}
