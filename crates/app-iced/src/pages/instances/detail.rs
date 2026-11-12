//! 实例页的**详情列**（M90 r2 从 `instances.rs` 拆出来：那边超过 400 行会被 guards 的 file-size 判红）。
//!
//! 结构：标题行（名字 + 版本 + Play/Stop）→ 元信息网格（列数由 `layout::meta_columns` 决定）
//! → 一条分隔线 → 正文段 / 运行状态行 → 动作行（Duplicate / Open folder / Delete）。
//! 窄窗（<840）时列表退成上一屏，本文件只画详情 + 一个「返回」按钮。

use iced::widget::{column, container, row, scrollable};
use iced::{Alignment, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_presenter::router::Route;

use crate::app::components::buttons::{self, Button};
use crate::app::components::divider;
use crate::app::components::interaction::Interaction;
use crate::app::components::surfaces::elevated;
use crate::theme::Ui;
use unlucky_design::icons::IconKind;

use super::super::frame;
use super::super::layout::{self, FrameMetrics};
use super::super::model::Detail;
use super::super::{Action, Message};
use super::forced_state;
use super::state::LaunchForm;
use super::Texts;

// --------------------------------------------------------------- 详情列

pub(super) fn detail_column<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    detail: &'a Detail,
    form: &'a LaunchForm,
    mods: &'a super::Mods,
    saves: &'a super::Saves,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    let mut body = column![
        detail_head(ui, detail, forced, texts),
        meta_grid(ui, metrics, detail),
        divider::full(ui),
    ]
    .spacing(ui.space(Space::X4))
    .width(Length::Fill);

    body = push_text(ui, metrics, body, detail);
    // M224：MODS 区排在启动参数**之前**——详情列是一个滚动区，而启动参数卡本身就有一屏高，
    // 排在它后面意味着「本地 mod 列表」永远在折叠线以下（M224 出图实测：1180×1000 完全看不见）。
    // 顺序因此是：身份（页头 + 元信息）→ 内容（MODS）→ 选项（启动参数）→ 动作。
    body = body.push(super::mods::section(ui, mods, forced, texts));
    // M235：SAVES 区紧跟 MODS 区（同为"内容"），理由与 MODS 一样：排在启动参数卡后面会落到
    // 折叠线以下——详情列是一个滚动区，而启动参数卡本身就有一屏高。
    body = body.push(super::saves::section(ui, saves, forced, texts));
    // M157：启动参数区在正文段之后、动作行之前（详情列 424px 内，卡片式）。
    body = body.push(super::launch_args::section(ui, &detail.launch, form, texts));

    // 页头 + 元信息网格 + **一条**分隔线以下才是正文与操作区（M84 那两条线是占位时期的排版残留）。
    //
    // M157 r2（D30 复审）：详情列的内容（正文段 + 启动参数卡 + 动作行）在 1180×1000 下**超出一屏**，
    // 而这里原来没有滚动区 ⇒ Max memory 以下的控件被静默裁掉、够不着。现在整列套一个滚动区
    // （与左列列表同一套滚动条样式与宽度），被裁内容可达；滚动条是唯一入口，没有第二份状态。
    let content = body.push(action_row(ui, forced, texts));
    frame::left_aligned(metrics.column, scroller(ui, content))
}

pub(super) fn narrow_detail<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    detail: &'a Detail,
    form: &'a LaunchForm,
    mods: &'a super::Mods,
    saves: &'a super::Saves,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    // 窄窗：列表退成上一屏，本屏只放详情（官方 compact 的 list-detail 口径）。
    let mut body = column![
        row![buttons::text(
            ui,
            forced_state(
                Button::new(texts.back(), Message::Navigate(Route::Instances)),
                forced,
            ),
        )]
        .width(Length::Fill)
        .align_y(Alignment::Center),
        detail_head(ui, detail, forced, texts),
        meta_grid(ui, metrics, detail),
        divider::full(ui),
    ]
    .spacing(ui.space(Space::X4))
    .width(Length::Fill);

    body = push_text(ui, metrics, body, detail);
    // 顺序与宽窗那一支一致：内容（MODS）在选项（启动参数）之前，理由见 `detail_column`。
    body = body.push(super::mods::section(ui, mods, forced, texts));
    body = body.push(super::saves::section(ui, saves, forced, texts));
    body = body.push(super::launch_args::section(ui, &detail.launch, form, texts));

    body = body.push(frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        texts.narrow_note(),
        ui.color(Role::OnSurfaceVariant),
    ));

    frame::body(
        metrics,
        scroller(ui, body.push(action_row(ui, forced, texts))),
    )
}

/// 详情列的滚动区（M157 r2）：与左列列表同一套滚动条几何（宽 6 / 内缩 2 / 同一 `style`）。
///
/// 右侧留 `Space::X2` 内缩：滚动条是**覆盖**在内容上的，不留这条缝时卡片的 1dp 描边会被
/// 滚动条压住（D30 复审里「卡片右边框缺一截」就是它）。
fn scroller<'a>(ui: &'a Ui, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let content = container(content)
        .width(Length::Fill)
        .padding(Padding::default().right(ui.space(Space::X2)));
    scrollable(content)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(frame::SCROLLBAR_WIDTH)
                .scroller_width(frame::SCROLLBAR_WIDTH)
                .margin(frame::SCROLLBAR_MARGIN),
        ))
        .height(Length::Fill)
        .style(frame::scroll_style(ui))
        .into()
}

/// 正文段（演示数据才有）与运行状态行（真数据才有）——两者都是可选的，顺序固定。
///
/// 散文段套 [`frame::prose`]：M102 起页面块吃满页宽，但「一行 ≤72 字符」的承诺仍然只由
/// 散文承担（`frame::body_width` 管块、`metrics.column` 管一句话）。
fn push_text<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    mut body: iced::widget::Column<'a, Message>,
    detail: &'a Detail,
) -> iced::widget::Column<'a, Message> {
    if let Some(note) = &detail.note {
        body = body.push(frame::prose(
            metrics,
            frame::text_role(
                ui,
                TypeRole::BodyMedium,
                false,
                note.as_str(),
                ui.color(Role::OnSurfaceVariant),
            ),
        ));
    }
    if let Some(status) = &detail.status {
        body = body.push(frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            status.as_str(),
            ui.color(Role::OnSurface),
        ));
    }
    body
}

fn detail_head<'a>(
    ui: &'a Ui,
    detail: &'a Detail,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    let titles = column![
        frame::text_role(
            ui,
            TypeRole::TitleLarge,
            true,
            detail.name.as_str(),
            ui.color(Role::OnSurface)
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            detail.subtitle.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    // 主操作：**全屏唯一**实心 accent（rubric K2）。不可用时退化成禁用态（只清消息，不改身份）。
    //
    // M237：这一行**不在卡片里**（详情列直接铺在画布上），所以这两个按钮的底保持
    // `Base::CANVAS`——把卡底传进来反而会让禁用色叠错层。卡片内的控件（MODS / SAVES /
    // 启动参数区）各自传 `frame::BASE`，见那几个模块。
    let launch = if detail.can_launch {
        buttons::filled(
            ui,
            forced_state(
                Button::new(texts.play(), Message::Launch(detail.id.clone())),
                forced,
            ),
        )
    } else {
        buttons::filled(ui, Button::disabled(texts.play()))
    };
    let stop = if detail.can_stop {
        buttons::text(
            ui,
            forced_state(
                Button::new(texts.stop(), Message::Stop(detail.id.clone())),
                forced,
            ),
        )
    } else {
        buttons::text(ui, Button::disabled(texts.stop()))
    };

    row![titles, launch, stop]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .into()
}

/// 元信息网格：列数由 [`layout::meta_columns`]（按正文列宽）决定 —— **1180 与 900 都是 2 列**。
///
/// 3 列需要 `3 × META_CARD_MIN_WIDTH + 2 × META_GAP` = 504px，而行宽上限是 424，**数学上不可行**；
/// 硬塞回 3 列会让 `NeoForge 20.4.237`（118px）这类值按词折行、同排卡底不齐。
fn meta_grid<'a>(ui: &'a Ui, metrics: FrameMetrics, detail: &'a Detail) -> Element<'a, Message> {
    let mut grid = column![].spacing(layout::META_GAP).width(Length::Fill);
    let columns = metrics.meta_columns.clamp(1, detail.meta.len().max(1));

    for chunk in detail.meta.chunks(columns) {
        let mut cells_row = row![].spacing(layout::META_GAP).width(Length::Fill);
        for cell in chunk {
            // M229：元信息卡 = 规格卡的 **ELEVATED CARD**（无描边、`SurfaceContainer` 底、16 圆角）。
            // 内距 20 + 值 **12/400**（见 `elevated::PAD` / `elevated::TITLE_ROLE` 的登记）：
            // 外框 160 − 2×20 = 120 必须放下
            // 最长值串，24 + 16 会让时戳折行、同排卡底不齐（复审 P1-2）。
            let card = elevated::card(
                ui,
                elevated::texts(
                    ui,
                    elevated::Texts {
                        overline: Some(cell.label.as_str()),
                        title: cell.value.as_str(),
                        support: None,
                    },
                ),
            );
            cells_row = cells_row.push(container(card).width(Length::Fill));
        }
        grid = grid.push(cells_row);
    }

    grid.into()
}

fn action_row<'a>(ui: &'a Ui, forced: Option<Interaction>, texts: Texts) -> Element<'a, Message> {
    row![
        buttons::text(
            ui,
            forced_state(
                Button::new(texts.duplicate(), Message::Action(Action::Duplicate)),
                forced,
            )
        ),
        buttons::text(
            ui,
            forced_state(
                Button::new(texts.open_folder(), Message::Action(Action::OpenFolder)),
                forced,
            )
        ),
        buttons::text(
            ui,
            forced_state(
                Button::new(texts.delete(), Message::Action(Action::Delete)),
                forced
            ),
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center)
    .into()
}

pub(super) fn detail_empty<'a>(ui: &'a Ui, texts: Texts) -> Element<'a, Message> {
    frame::empty_state(
        ui,
        IconKind::Instances,
        texts.empty_title(),
        texts.empty_body(),
        Vec::new(),
    )
}
