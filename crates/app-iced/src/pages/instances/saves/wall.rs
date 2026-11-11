//! 截图**缩略图墙**（M251 · SAVESHOTS，D54）。
//!
//! ## 画什么
//!
//! 每格 = 「图（或占位框）」+ 下面一行副文案，按 [`PER_ROW`] 列换行排。图的最大边由
//! `palette_extract::THUMBNAIL_MAX_EDGE = 160` 定（解码那一侧），这里只负责**摆**：
//!
//! | 格子内容 | 画什么 |
//! |---|---|
//! | [`ShotPreview::Ready`] | `iced::widget::image::Handle` 已经是 RGBA 像素，直接画（**不再解码**） |
//! | [`ShotPreview::Missing`] | `160×90` 的描边占位框 + 一句短话（「未生成预览」/ 具名原因） |
//!
//! ## 为什么不用 `grid_row`
//!
//! iced 0.14 的基础控件里没有流式网格，而本仓没有引 `iced_aw` 之类的扩展 crate（多一个依赖要过
//! `deny.toml`）。墙的行数是 `ceil(n / PER_ROW)`，用手写的 `column` + `row` 摆——比引一个
//! 扩展 crate 便宜，也比让它自己换行可靠（换行行为不会被窗口宽度悄悄改掉）。
//!
//! ## 宽高比
//!
//! 缩略图的**长边是 160**，短边按原图比例（截图通常 16:9 ⇒ 160×90）。所以宽度钉死、高度
//! `Shrink`：`iced` 的 image 控件在 `width = Fixed` / `height = Shrink` 下按原图比例取高，
//! 不拉伸（`ContentFit::Contain` 默认）。占位框反而钉死 `160×90`——它是"这里本该有一张图"的
//! 占位，尺寸要可预测，不能让一句话把它撑变形。
//!
//! ## 一行放几格（这条是**量出来的**，不是拍的）
//!
//! 详情列的宽度由 `pages::layout` 的 `FrameMetrics::column` 定，而它 `≤ measure_cap_px()`
//! （1180 窗宽下 = 424，见 `layout.rs` 的文档）。卡片还有左右内边距 ⇒ 实际内宽约 **370**。
//!
//! 于是：**3 格 = 3 × 160 + 2 × 12 = 504px 装不下**（M251 第一次出图实测：第 3 格被挤成一条
//! 竖条，`未生成预览` 四个字竖着排）。2 格 = `2 × 160 + 12 = 332px`，与同一列里元信息卡的
//! 两列栅格（`META_CARD_MIN_WIDTH × 2 + META_GAP = 332`）**同宽**——这不是巧合，是同一列里
//! 已经成立的节奏。
//!
//! 代价写在明处：60 张 = 30 行，要滚很久。换 3 列只能把格子缩到 ~112px（图更小），D54 的
//! `THUMBNAIL_MAX_EDGE = 160` 也就白解了；要真正做到「宽度自适应列数」得引一个栅格控件，
//! 那是新依赖，不在本 mission 的取舍里。

use iced::widget::{column, container, image, row};
use iced::{Alignment, Background, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::super::frame;
use super::super::Message;
use super::{Shot, ShotPreview};

/// 一格缩略图的宽（= 解码侧的最大边，两者必须同源：图比格子小会留白，比格子大会被缩）。
const THUMB_WIDTH: f32 = 160.0;

/// 占位框的高（16:9，与常见的截图宽高比一致）。
const THUMB_HEIGHT: f32 = 90.0;

/// 一行放几格（= 同一列里元信息卡的两列栅格；理由见模块文档）。
const PER_ROW: usize = 2;

/// 整面墙（`saves.rs` 的 `section` 调；空列表不该走到这里）。
pub(super) fn grid<'a>(ui: &'a Ui, shots: &'a [Shot]) -> Element<'a, Message> {
    let mut wall = column![].spacing(ui.space(Space::X3)).width(Length::Fill);
    for chunk in shots.chunks(PER_ROW) {
        let mut line = row![]
            .spacing(ui.space(Space::X3))
            .align_y(Alignment::Start);
        for shot in chunk {
            line = line.push(cell(ui, shot));
        }
        wall = wall.push(line);
    }
    wall.into()
}

/// 一格：图或占位框 + 副文案。
fn cell<'a>(ui: &'a Ui, shot: &'a Shot) -> Element<'a, Message> {
    let picture: Element<'a, Message> = match &shot.preview {
        ShotPreview::Ready { handle } => image(handle.clone())
            .width(Length::Fixed(THUMB_WIDTH))
            .height(Length::Shrink)
            .border_radius(ui.radius_dp(ShapeScale::Small))
            .into(),
        ShotPreview::Missing { note } => placeholder(ui, note.as_str()),
    };
    column![
        picture,
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            shot.detail.as_str(),
            ui.color(Role::OnSurfaceVariant),
        )
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fixed(THUMB_WIDTH))
    .into()
}

/// 占位框：描边 + 容器色底 + 居中的那句短话。
///
/// 颜色全部走令牌（D55 第 3 条：组件源码里不许出现字面色值）；底用 `SurfaceContainerHigh`
/// 而不是画布色，是为了在卡片（`SurfaceContainerLow`）上还能看出"这里有一个格子"。
fn placeholder<'a>(ui: &'a Ui, note: &'a str) -> Element<'a, Message> {
    container(frame::text_role_centered(
        ui,
        TypeRole::BodySmall,
        false,
        note,
        ui.color(Role::OnSurfaceVariant),
    ))
    .width(Length::Fixed(THUMB_WIDTH))
    .height(Length::Fixed(THUMB_HEIGHT))
    .padding(ui.space(Space::X2))
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ui.color(Role::SurfaceContainerHigh))),
        border: ui.outlined(ShapeScale::Small),
        ..container::Style::default()
    })
    .into()
}
