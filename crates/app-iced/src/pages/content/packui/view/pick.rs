//! 冲突清单那一屏的画法（M160）：**分页** + 三个入口 + 计数摘要 + 确认/取消。
//!
//! ## 版面
//!
//! ```text
//! ┌ 导入前：有 312 个文件已存在且内容不同 ───────────────┐
//! │ 保留 309 · 覆盖 3 · 覆盖将丢弃 2.4 MB                │
//! │ 没有改动的一律按「保留」：…                            │
//! │ ⚠ 还有 4 条远端条目要下载后才知道路径，一律按「保留」处置。│
//! │ [全部保留我的] [全部覆盖]                             │
//! ├ 第 1-40 项，共 312 项          [上一页] [下一页]      │
//! │ config/sodium-options.json  包内 overrides/ · 本来就在 │
//! │                       你的：4.1 KB · 包内：3.8 KB [覆盖]│
//! │ …（**只画当前这一页**）                                │
//! ├ ⚠ 要处理：2 条（M341：预检注记按级别分两段，与报告同一套画法）│
//! │   基线读不出来，本次按「没有基线」处理…                 │
//! ├ 只是告知：1 条                                       │
//! │   预检前按 sha512 从 Modrinth 补全了 12 条…             │
//! ├ [按这些选择导入]  [取消导入]                          │
//! └─────────────────────────────────────────────────────┘
//! ```
//!
//! ## 三处「不误导」
//!
//! 1. **缺省说明写在按钮下面**：用户不必猜"没点过的那 309 条会怎样"；
//! 2. **计数摘要里的丢弃量只算真选了覆盖的**：用最坏数字吓人不算如实；
//! 3. **按钮写的是"点了会变成什么"**（当前保留 ⇒ 按钮写「覆盖」），所以点一下就知道后果。
//!
//! ## 为什么分页而不是滚动
//!
//! 几千条冲突一次画完 = 几千个 widget：布局与滚动都卡。分页把 widget 数量钉在
//! `model/pick.rs::PAGE_SIZE`（40 条）以内，翻页只是换数据。

use iced::widget::{column, container, row};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::Catalog;
use unlucky_presenter::dto::ImportIssueLevelView;

use crate::app::components::buttons::{self, Button};
use crate::app::components::selection::{chip, Chip};
use crate::pages::frame;
use crate::pages::Message as PageMessage;
use crate::theme::Ui;

use super::super::marker::{self, SectionMark};
use super::super::model::{Pick, NOTES_ROWS_THRESHOLD};
use super::super::PackMessage;
use super::intent;
use super::parts::{bounded as rows_bounded, issue_section};
use super::pick_rows::{pager, pick_row};

/// 分页清单的高度（px）：**对齐到行边界**（M345）。
///
/// 行节奏 = 行首 pill（`control_height` 40）+ 4 + 详情行 16（`BodySmall`）+ 4 ⇒ 一行 64；顶上分页行
/// 也是 40 + 4 ⇒ 表头 44。整行高度只能是 `44 + 64k`（切在下一行开头）或 `44 + 64k + 60`（切在行尾）：
/// 375 = 44+64×5+11 两者都不是（底边切在第 6 行的行首格内部），364 = 44+64×5 正好落在第 6 行开头。
const LIST_HEIGHT: f32 = 364.0;

/// 有预检注记时分页清单**让出**多少（px）。
///
/// 为什么必须让：内容库页的正列没有整页滚动（见 `view.rs` 的注释），两块固定高度的兄弟加起来超过
/// 窗口，最后那行「按这些选择导入 / 取消导入」就被切掉——出图实测（1180×900）：清单仍是 375px 时，
/// 两段注记把动作行推出屏外，只露出半个按钮。M345 起让出量跟着 [`LIST_HEIGHT`] 对齐行边界：
/// 有注记时清单 = 364 − 192 = 172 = 44+64×2（分页头 + 两整行）。
///
/// **M350 从注记区拿走的那 8px 不从这里出**：清单要的是「整行节奏」（44+64k），多让 8 就退回
/// 「两行半」；那 8px 由卡片整体变高承担（注记视口 172 → [`notes_cap`] = 180），代价是正列下方
/// 的结果列表矮 8px——它本来就是 `Length::Fill`，让得起。实测（1180×900）：卡片底边 711 → 719，
/// 搜索条/筛选 chip 仍在屏内（见 `DevDocs/artifacts/pickrow/MEASURE-BANDS.md`）。
const NOTES_RESERVE: f32 = 192.0;

/// 注记区的**行量子**（M350）：正文行框（`BodySmall`）+ 一个行距单位（`Space::X1`）= **20** @ scale 1，
/// 注记区里**每一行**（段头、正文、段内折行）的行框高与行距都用它。
///
/// 这不是新量：20 就是 M345 那一版「两条注记之间」的实际节奏（正文行框 16 + 行距 4）。M350 只是
/// 把它**推广到段头与段内折行**——此前段头行框 24（`Warn` 图标 24 主导）、段内折行 16、条间 20，
/// 一段注记里同时存在三种竖向量，**任何固定高度的视口底边都可能落进某个行框内部**
/// （见 [`notes_cap`] 的相位论证）。
fn note_quantum(ui: &Ui) -> f32 {
    ui.style(TypeRole::BodySmall).line_height_px + ui.space(Space::X1)
}

/// 注记视口里能完整画出的行数（`m`）：段头 + 7 行告警 + 第二段段头（`pick_many` 常驻夹具）。
///
/// `m` **只决定显示几行**，与「切不切字」无关（那是 [`notes_cap`] 的结构性结论）：内容折行数怎么
/// 变，底边始终落在行框线上，只是显示的条数跟着变。
const NOTE_ROWS: f32 = 9.0;

/// 注记视口的高度 = **整数个行量子**（[`NOTE_ROWS`] 行）。
///
/// 这一条就是「末行不切」的全部机制，而且是**相位**论证、不是「行框装不下字」那套：
///
/// - 注记区里每一行（段头/正文/段内折行）的行框都落在 `q` 的整数倍上（见 [`note_quantum`]）；
/// - 视口高度也取 `q` 的整数倍 ⇒ **视口底边永远落在行框线上**，所以底边只会夹在两行之间：
///   最后一行整行可见、下一行整行不可见，**任何一行都不可能被底边劈开**——这一条与内容
///   写了几条、每条折几行都无关。
/// - 余量是另一件事（实测，不是保证）：墨迹在行框里**居中没有贴底**（量子格内实测 `+3..+17`，
///   英文降部到 `+18`），所以折叠行前至少还剩 1 行空白——实测 **1~4 行**（1180×900 中文 2 /
///   英文 1、换内容 3、1440×900 4）。注意这里**一行 = 一个像素行**，不是 20px 的格：英文那 1 行
///   就是 `pick_many` en 的 671 这一像素行（格 652..672、墨迹 658..670、折叠行 672），已经贴到
///   边界。两种语言都完整，但**只有中文有富余**。
///
/// 为什么 M345 的 172 不行：那时注记区是**混合栅格**——段头行框 24（`Warn` 图标 `ICON_PX`=24
/// 主导）、段内折行 16、条与条之间 20，任何固定高度都可能**落在某个行框内部**（当年折线 648
/// 就落在第二段段头的行框 632..652 里，行框里有字 ⇒ 看起来像「切字」）。那不是字迹溢出，
/// 是折线穿过了行框。现在是结构性的：`m` 只决定显示几行，底边相位恒为「行框线」。
fn notes_cap(ui: &Ui) -> f32 {
    note_quantum(ui) * NOTE_ROWS
}

/// 整块。
pub(super) fn pick_block<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    pick: &'a Pick,
) -> Element<'a, PageMessage> {
    let mut block = column![
        frame::text_role(
            ui,
            TypeRole::TitleSmall,
            true,
            pick.title.as_str(),
            ui.color(Role::OnSurface),
        ),
        // 计数摘要：这是"我要丢掉多少"的唯一一处数字，用强调色
        frame::text_role(
            ui,
            TypeRole::BodyMedium,
            true,
            pick.summary.as_str(),
            ui.color(Role::Primary),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            pick.default_note,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    if let Some(note) = &pick.unknown_note {
        block = block.push(
            row![
                frame::glyph(IconKind::Warn, ui.color(Role::Tertiary), frame::ICON_PX),
                frame::text_role(
                    ui,
                    TypeRole::BodySmall,
                    false,
                    note.as_str(),
                    ui.color(Role::Tertiary),
                ),
            ]
            .spacing(ui.space(Space::X1))
            .align_y(Alignment::Center),
        );
    }

    // 三个入口：整批保留 / 整批覆盖（逐项就是下面每行那个按钮）
    block = block.push(
        row![
            chip(
                ui,
                Chip::filter(
                    pick.keep_all,
                    pick.keep_all_active,
                    intent(PackMessage::PickKeepAll),
                ),
            ),
            chip(
                ui,
                Chip::filter(
                    pick.overwrite_all,
                    pick.overwrite_all_active,
                    intent(PackMessage::PickOverwriteAll),
                ),
            ),
        ]
        .spacing(ui.space(Space::X2)),
    );

    // **M341**：预检注记按级别分两段。分档、段标题、画法**全部与导入报告共用**
    // （`model::report_issue::section` + [`issue_section`]）——同一批 core 注记，两个阶段一种说法。
    // 位置也照报告那一段：逐条段之后、动作行之前（先看完"要决定什么"，再看"还有什么要留意"）。
    // 每段进一个自证标记：用例顺着真控件树读回"这一段真的被 `push` 进来了"（`packui/marker.rs`）。
    //
    // **M350**：两段都按行量子排（`Some(q)`）——段头、正文、段内折行落在同一套行框上，
    // 视口底边才有一个与内容无关的安全落点（[`notes_cap`]）。
    let quantum = note_quantum(ui);
    let mut notes: Vec<Element<'a, PageMessage>> = Vec::new();
    let mut note_lines = 0;
    for (level, title, lines, name) in [
        (
            ImportIssueLevelView::Warning,
            &pick.warnings_title,
            &pick.warnings,
            marker::PICK_WARNINGS,
        ),
        (
            ImportIssueLevelView::Note,
            &pick.notes_title,
            &pick.notes,
            marker::PICK_NOTES,
        ),
    ] {
        // 空的一档整段不画（段标题 `None` ⇒ 连空段头都不出现）。
        let Some(title) = title else { continue };
        note_lines += lines.len();
        notes.push(marker::tagged(
            issue_section(ui, level, title.clone(), lines, Some(quantum)),
            SectionMark {
                name,
                rows: lines.len(),
            },
        ));
    }

    // 清单先让出注记要占的高度：两块的 height 都是固定的，正列又不滚，加在一起的后果就是
    // 动作行被切掉（出图实测，见 [`NOTES_RESERVE`]）。
    let list_height = if notes.is_empty() {
        LIST_HEIGHT
    } else {
        LIST_HEIGHT - NOTES_RESERVE
    };

    // 分页头（当前页之外的行一律不画）
    block = block.push(bounded(
        column(
            std::iter::once(pager(ui, catalog, pick))
                .chain(pick.rows.iter().map(|item| pick_row(ui, item)))
                .collect::<Vec<_>>(),
        )
        .spacing(ui.space(Space::X1))
        .width(Length::Fill),
        list_height,
    ));

    // 两段注记合成**一个**有界区（总高 = [`notes_cap`]）：20 条注记也顶不爆这一屏。
    //
    // **M350**：段间 spacing 是 0——行距已经含在行量子（= 行框高）里；底部再留一个行距单位，
    // 于是**滚到底**时末行墨迹（实测占格内 `+3..+18`）与视口底边之间仍留着空白，不是贴着边。
    if !notes.is_empty() {
        let content = container(column(notes).spacing(0).width(Length::Fill)).padding(Padding {
            bottom: ui.space(Space::X1),
            ..Padding::ZERO
        });
        block = block.push(rows_bounded(
            ui,
            content,
            note_lines,
            NOTES_ROWS_THRESHOLD,
            notes_cap(ui),
        ));
    }

    // 列表与动作行之间留出呼吸（复审 r1 视觉 2）：滚到底时最后一行会紧贴动作行，靠 padding 分开（调列 spacing 会连行距一起拉开）。
    block = block.push(
        container(
            row![
                buttons::filled(
                    ui,
                    Button::new(pick.confirm.as_str(), intent(PackMessage::ConfirmImport))
                        .leading(IconKind::Check),
                ),
                buttons::text(
                    ui,
                    Button::new(pick.cancel, intent(PackMessage::CancelImport)),
                ),
            ]
            .spacing(ui.space(Space::X2))
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: ui.space(Space::X2),
            ..Padding::ZERO
        }),
    );

    container(block)
        .width(Length::Fill)
        .padding(ui.space(Space::X3))
        .style(move |_theme| container::Style {
            border: Border {
                color: ui.color(Role::Outline),
                width: 1.0,
                radius: ui.radius_dp(ShapeScale::Large),
            },
            background: Some(Background::Color(ui.color(Role::Surface))),
            ..container::Style::default()
        })
        .into()
}

/// 有界容器：内容多时自己滚（与结果块的 `bounded` 同一条口径，但这里限高不同）。
fn bounded<'a>(
    content: impl Into<Element<'a, PageMessage>>,
    height: f32,
) -> Element<'a, PageMessage> {
    iced::widget::scrollable(container(content).width(Length::Fill))
        .height(Length::Fixed(height))
        .into()
}
