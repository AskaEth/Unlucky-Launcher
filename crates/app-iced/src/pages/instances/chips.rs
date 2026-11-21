//! 实例页页头的两组 chip（M455）：视图模式 + 磁贴尺寸。
//!
//! 从 `instances.rs` 拆出：那边 M455 加完网格那一支后到了 455 行（400 行硬线）。这一卷只有
//! 「消息 → 控件」，没有任何状态——当前值由 `App` 递进来（`pages::ViewMode`）。
//!
//! 用 chip 而不是新控件：下载页的分类栏、内容库的筛选项、设置页的分段单选都是这一族
//! （`selection::chip` + `Chip::filter`），页头另造一个自绘 toggle 会是第三套语汇。

use iced::widget::row;
use iced::{Alignment, Element};

use unlucky_design::spacing::Space;
use unlucky_presenter::prefs::{InstanceView, TileSize};

use crate::app::components::interaction::Interaction;
use crate::app::components::selection::{chip, Chip};
use crate::theme::Ui;

use super::super::Message;
use super::{InstanceMessage, Texts};

/// 视图切换（`Tiles` / `List`）：两枚 Filter chip，选中的那枚就是当前模式。
///
/// 用 chip 而不是新控件：实例页/设置页/下载页的「分段单选」都是这一族（`selection::chip`，
/// 下载页分类栏 `download/rails.rs` 同一条写法），页头换成自绘的 toggle 会是第三套语汇。
pub(super) fn view_chips<'a>(
    ui: &'a Ui,
    texts: Texts,
    current: InstanceView,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let mut line = row![]
        .spacing(ui.space(Space::X1))
        .align_y(Alignment::Center);
    for mode in [InstanceView::Tiles, InstanceView::List] {
        let label = match mode {
            InstanceView::Tiles => texts.view_tiles(),
            InstanceView::List => texts.view_list(),
        };
        let mut spec = Chip::filter(
            label,
            mode == current,
            Message::Instances(InstanceMessage::SetView(mode)),
        );
        if let Some(state) = forced {
            spec = spec.state(state);
        }
        line = line.push(chip(ui, spec));
    }
    line.into()
}

/// 磁贴尺寸（小 / 中 / 大）：三枚 Filter chip，顺序与 `TileSize::columns()` 的密度序一致。
pub(super) fn size_chips<'a>(
    ui: &'a Ui,
    texts: Texts,
    current: TileSize,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let mut line = row![]
        .spacing(ui.space(Space::X1))
        .align_y(Alignment::Center);
    for size in [TileSize::Small, TileSize::Medium, TileSize::Large] {
        let label = match size {
            TileSize::Small => texts.tile_small(),
            TileSize::Medium => texts.tile_medium(),
            TileSize::Large => texts.tile_large(),
        };
        let mut spec = Chip::filter(
            label,
            size == current,
            Message::Instances(InstanceMessage::SetTileSize(size)),
        );
        if let Some(state) = forced {
            spec = spec.state(state);
        }
        line = line.push(chip(ui, spec));
    }
    line.into()
}
