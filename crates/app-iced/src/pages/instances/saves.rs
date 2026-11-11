//! 实例详情页的 **SAVES 区**（M235 · STOREMVP 读路径 + M247 · SAVESOPS 写操作）：
//! 存档 / 截图列表 + 打开文件夹 + **每行的重命名 / 备份 / 删除**。
//!
//! # 位置与形状
//!
//! 挂在详情列里、MODS 区之后、启动参数卡**之前**（`pages/instances/detail.rs` 的 `detail_column`）。
//! 与 MODS 区同一套卡片口径：[`frame::card`] + 一个 `LabelMedium` 小标 + 内容。
//!
//! **为什么在启动参数之前**：与 MODS 区同一个理由（M224 出图实测）——详情列整列是一个滚动区，
//! 而启动参数卡本身就有一屏高；排在它后面等于这整块落在折叠线以下、根本看不见。顺序因此是：
//! 身份（页头 + 元信息）→ 内容（MODS → SAVES）→ 选项（启动参数）→ 动作。
//!
//! # 为什么不自己再套一层滚动区
//!
//! 详情列**整列**已经是滚动区（M157 r2）。M224 踩过一次「滚动条里的滚动条」：1180×1000 下两个条
//! 都认得出来，用户不知道该拖哪一个。这里同样只排内容，长度交给外层那一条。
//!
//! # 行操作区（M247）
//!
//! 每个**世界**行右侧三个文字按钮：**重命名 / 备份 / 删除**（截图行没有操作区：截图不在这三个
//! 写操作的范围里）。三档状态：
//!
//! | 状态 | 画什么 | 为什么 |
//! |---|---|---|
//! | 空闲 | 三个按钮 | — |
//! | 这一行的请求在飞 | 一句「正在处理…」顶替按钮 | 连点两次会发两条 rename/delete，第二条必然「世界已不在」 |
//! | 正在行内改名 | 输入框 + 「改名」/「取消」 | 见下 |
//!
//! **不做乐观回显**：行上的名字只来自快照，点完等 host 回一份新快照才变（与 MODS 区同一纪律）。
//!
//! # 重命名为什么是行内输入，不是模态输入框
//!
//! 现成的模态队列有 `DialogKind::Input`，但 app 侧的 `dialog::Message` 只有主/次按钮与关闭，
//! **没有「输入变了」这一条**——用输入模态等于重写一套可编辑模态。行内输入复用
//! `components::inputs` 的单行框（与启动参数区同一个控件），一处新控件都不加。
//! 输入缓冲住在 presenter 的 [`SavesState::draft`]（页面模型每帧从快照重建，缓冲留在页面里会被
//! 每一次事件冲掉）。
//!
//! # 删除为什么必须二次确认
//!
//! 这是本页唯一**不可逆**的动作。点「删除」只发一条
//! [`PageMessage::RequestDeleteWorld`]，真正的删除要等模态确认框的主按钮
//! （`app/update/dialog.rs` 里那一支）。确认框复用仓内现成的模态队列（`state.dialogs`），
//! 页面不新写弹窗。
//!
//! # 失败与重试
//!
//! 读盘失败画一条红字 + 一个「重试」（再发一条 `SavesCommand::List`）。**打开文件夹失败与写操作
//! 失败不画重试**：那个动作的重试就是再点一次按钮，给一个会去重新列目录的「重试」是错的
//! affordance（三种失败共用 `detail`，靠大类与 `op` 分别，见 [`failure_line`]）。
//!
//! 写操作**成功**也有一句话（[`Saves::notice`]）：重命名 / 删除的结果列表上看得见，但备份的落点
//! 看不见——「点了备份然后什么也没发生」是静默失败，所以回执里带上完整路径与字节数。
//!
//! # 截图区是**缩略图墙**（M251 · SAVESHOTS，D54）
//!
//! | 变化 | 落点 |
//! |---|---|
//! | 截图不再是一行字，而是「图 + 副文案」的格子 | [`wall`]（2 列 = 同列元信息卡的栅格，见那个模块的文档） |
//! | 像素**不在**这里解，也不在渲染回调里解 | presenter 的后台线程（`host::saves::thumbs`）解好、随快照过来；本层只把 `Handle` 建一次（`model::from_state`） |
//! | 没有像素的格子**不许留白** | 占位框 + 一句短话；副文案把原因说全（超 8 MiB / 读不出 / 解不出 / 超 60 张预算） |
//!
//! ## 为什么拆成三个文件
//!
//! 加完缩略图墙之后 `saves.rs` 会越过 400 行硬线：模型（[`model`]）、行与操作区（[`rows`]）、
//! 墙（[`wall`]）各占一块，本文件只剩「卡片骨架 + 小标 + 空态 + 错误行」。
//! 路径不变（[`model`] 的东西用 `pub use` 转出），`view/instances.rs` 与用例的 import 一行不改。

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::app::components::buttons::{self, Button};
use crate::app::components::interaction::Interaction;
use crate::theme::Ui;

use super::super::frame;
use super::super::Message;
use super::forced_state;
use super::state::Message as PageMessage;
use super::Texts;

mod model;
mod rows;
mod wall;

/// 句柄 memo 的清空入口（只给用例：它是进程级 memo）。
#[cfg(test)]
pub(super) use model::clear_handles;
pub use model::{failure_line, from_state, Row, Saves, Shot, ShotPreview};
/// 「改名」那条消息的产地（实现在 [`rows`] 里）——只给用例用，与 `op_line` 同一条口径。
#[cfg(test)]
pub(super) use rows::commit_rename;
/// 写操作成功那句话的产地（实现在 [`rows`] 里）。
///
/// 生产代码走 `model::from_state → rows::op_line`；这条 re-export 只给**用例**用
/// （`tests/saves/write.rs` 从这里取），所以挂在 `cfg(test)` 下，免得非测试构建报未用导入。
#[cfg(test)]
pub(super) use rows::op_line;

/// 画 SAVES 区（实例页唯一入口，`detail.rs` 调它）。
pub(super) fn section<'a>(
    ui: &'a Ui,
    saves: &'a Saves,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    let mut body = column![head(ui, saves, forced, texts)]
        .spacing(ui.space(Space::X3))
        .width(Length::Fill);

    if saves.loading {
        body = body.push(note(ui, texts.saves_loading()));
        return frame::card(ui, body);
    }
    if let Some(error) = saves.error.as_deref() {
        body = body.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            error,
            ui.color(Role::Error),
        ));
        if saves.retryable {
            body = body.push(buttons::text(
                ui,
                forced_state(
                    Button::new(
                        texts.saves_retry(),
                        Message::Instances(PageMessage::ReloadSaves),
                    ),
                    forced,
                )
                // M237：卡片里的控件把底传成卡底（hover / 禁用状态层叠卡实色）。
                .base(frame::BASE),
            ));
        }
        // 旧列表还在时照旧画出来（失败不清列表），用户能看见上一次读到的东西；
        // 一份快照都没有时到此为止——**不许**在"读不了"下面再补一句"这里还没有世界"。
        if !saves.has_view {
            return frame::card(ui, body);
        }
    }
    if saves.missing {
        body = body.push(note(ui, texts.saves_missing_dir()));
        return frame::card(ui, body);
    }
    if saves.nothing_yet() {
        body = body.push(note(ui, texts.saves_empty()));
        return frame::card(ui, body);
    }
    // 写操作成功的回执（重命名 / 删除 / 备份）——放在列表**上面**：用户刚点的就是这里。
    if let Some(notice) = saves.notice.as_deref() {
        body = body.push(note(ui, notice));
    }

    // 世界 → 截图，两组各一个小标；某一组为空时只给一句话，不留空小标。
    body = body.push(sub_head(ui, texts.saves_worlds()));
    if saves.worlds.is_empty() {
        body = body.push(note(ui, texts.saves_no_worlds()));
    } else {
        for item in &saves.worlds {
            body = body.push(rows::world(ui, item, saves, forced, texts));
        }
    }
    body = body.push(sub_head(ui, texts.saves_screenshots()));
    if saves.screenshots.is_empty() {
        body = body.push(note(ui, texts.saves_no_screenshots()));
    } else {
        // 缩略图墙（M251）：一行两格（详情列 ≤424 宽，三格装不下），没像素的格子给占位框 + 原因。
        body = body.push(wall::grid(ui, &saves.screenshots));
    }
    if saves.skipped > 0 {
        body = body.push(note(ui, &saves.skipped_note));
    }

    frame::card(ui, body)
}

/// 小标行：`SAVES` + 计数 + 右侧「打开存档文件夹」。
fn head<'a>(
    ui: &'a Ui,
    saves: &'a Saves,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    // 左半是「存档 + 计数」，右半是「打开存档文件夹」：左半用一个吃满宽度的容器顶开，
    // 按钮才会落在卡片右边缘（与 `head` 之外那些卡片的动作行同一套摆法）。
    let mut left = row![frame::text_role(
        ui,
        TypeRole::LabelMedium,
        false,
        texts.saves_title(),
        ui.color(Role::OnSurfaceVariant),
    )]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center);

    // 计数只在"真有一份可用快照"时画：读盘中 / 读失败 / 空目录画 0·0 会像"这里什么都没有"。
    let show_count = saves.has_view
        && saves.error.is_none()
        && !saves.missing
        && !(saves.worlds.is_empty() && saves.screenshots.is_empty());
    if show_count {
        left = left.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            saves.count.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ));
    }

    // 「打开文件夹」始终可点：目录还没有时 host 会退到运行目录，失败也有就地说明
    // （见 `host::saves::open_target`）。
    let open = buttons::text(
        ui,
        forced_state(
            Button::new(
                texts.saves_open_folder(),
                Message::Instances(PageMessage::OpenSavesFolder),
            ),
            forced,
        )
        // M237：这枚按钮在 SAVES 卡里 ⇒ 状态层叠卡底（`frame::BASE`），不是画布底。
        .base(frame::BASE),
    );
    row![
        container(left)
            .width(Length::Fill)
            .align_y(Alignment::Center),
        open
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

fn sub_head<'a>(ui: &'a Ui, text: &'a str) -> Element<'a, Message> {
    frame::text_role(
        ui,
        TypeRole::LabelSmall,
        false,
        text,
        ui.color(Role::OnSurfaceVariant),
    )
}

fn note<'a>(ui: &'a Ui, text: &'a str) -> Element<'a, Message> {
    frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        text,
        ui.color(Role::OnSurfaceVariant),
    )
}
