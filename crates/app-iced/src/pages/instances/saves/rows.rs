//! SAVES 区的一行与它的操作区（M247）。
//!
//! 从 `saves.rs` 拆出：那个文件加完写操作的行内输入与三个按钮之后会越过 400 行硬线
//! （拆法与仓库其它 `-p1/-p2` 一致——**纯搬移 + 一个函数一族**，判据留在 `saves.rs`）。
//!
//! | 函数 | 画什么 |
//! |---|---|
//! | [`world`] | 一个世界：名字 + 副文案 +（空闲）重命名/备份/删除、（在飞）「正在处理…」、（改名中）输入框 + 改名/取消 |
//! | [`world_detail`] / [`shot_detail`] | 两行副文案（`shot_detail` 现在喂缩略图墙的格子，见 [`super::wall`]） |
//!
//! M251：原来给截图用的只读行 [`world_plain`] **已删**——截图改画缩略图墙（`super::wall`），
//! 留着一个没人调的 `pub(super) fn` 只会在 `-D warnings` 下变成死代码告警。
//!
//! 三枚按钮与「打开存档文件夹」同一条口径：`buttons::text` + `frame::BASE`（M237：卡片里的控件
//! 把底传成卡底，hover/禁用状态才叠在卡实色上），出图取证时整行一起跟 `--force-state`。
//!
//! 输入框用 `components::inputs::text_field`（与启动参数区同一个控件）。**回车 = 点「改名」**
//! （M252 / P18）：`Field::on_submit` 与右边那枚按钮共用 [`commit_rename`] 这一个消息产地，
//! 所以空名 / 原名不变 / 非法字符走的仍是原来那条校验（`app/update/instances.rs` 的空名与
//! 「没改」两条短路 + `core::game::saves::write::boundary::check_name` 的路径规则），
//! **回车没有绕开任何一道边界**。

use iced::widget::{column, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_presenter::dto::{human_bytes, SavesOped};

use crate::app::components::buttons::{self, Button};
use crate::app::components::inputs::{text_field, Field};
use crate::app::components::interaction::Interaction;
use crate::theme::Ui;

use super::super::forced_state;
use super::super::frame;
use super::super::state::Message as PageMessage;
use super::super::Message;
use super::super::Texts;
use super::{Row, Saves};

/// 一个世界行：名字 + 副文案 + 操作区（三档状态见模块文档与 `saves.rs`）。
pub(super) fn world<'a>(
    ui: &'a Ui,
    item: &'a Row,
    saves: &'a Saves,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    if let Some(editing) = saves.editing.as_ref().filter(|edit| edit.dir == item.name) {
        return rename_row(ui, editing.text.as_str(), forced, texts);
    }
    let label = labels(ui, item, Role::OnSurface);
    let dir = item.name.clone();
    let actions: Element<'a, Message> = if saves.busy_with(&dir) {
        text_note(ui, texts.saves_busy(), Role::OnSurfaceVariant)
    } else {
        row![
            action(
                ui,
                texts.saves_rename(),
                PageMessage::StartRenameWorld { dir: dir.clone() },
                forced
            ),
            action(
                ui,
                texts.saves_backup(),
                PageMessage::BackupWorld { dir: dir.clone() },
                forced
            ),
            action(
                ui,
                texts.saves_delete(),
                PageMessage::RequestDeleteWorld { dir },
                forced
            ),
        ]
        .spacing(ui.space(Space::X1))
        .align_y(Alignment::Center)
        .into()
    };
    row![label, actions]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

/// 行内重命名：输入框 + 「改名」/「取消」（占满整行，副文案让位）。
fn rename_row<'a>(
    ui: &'a Ui,
    text: &'a str,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    let field = text_field(
        ui,
        Field::new(text)
            // M237：输入框住在卡里 ⇒ 状态层与描边叠卡底。
            .base(frame::BASE)
            .placeholder(texts.saves_rename_placeholder())
            .width(Length::Fill)
            // P18：回车与右枚按钮**同一个**消息产地（见 `commit_rename` 的文档）。
            .on_submit(|| Message::Instances(commit_rename())),
        |value| Message::Instances(PageMessage::RenameWorldChanged(value)),
    );
    row![
        field,
        action(ui, texts.saves_rename_confirm(), commit_rename(), forced),
        action(
            ui,
            texts.saves_cancel(),
            PageMessage::CancelRenameWorld,
            forced
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

/// 「改名」那一下要发的那条页面意图——**按钮与回车共用的唯一产地**（M252 / P18）。
///
/// 为什么抽成一个函数：「回车走的是不是同一条路」在 iced 里量不到（`Element` 没有内省接口，
/// 见 `pages::layout::source` 的模块文档），能钉住的只有**两条路拿的是同一个 Message**——
/// 于是「空名 / 原名不变 / 非法字符不许提交」那几道校验自动对回车同样成立。
pub(crate) fn commit_rename() -> PageMessage {
    PageMessage::CommitRenameWorld
}

/// 名字 + 副文案（两行）。
fn labels<'a>(ui: &'a Ui, item: &'a Row, color: Role) -> iced::widget::Column<'a, Message> {
    column![
        frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            item.name.as_str(),
            ui.color(color)
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            item.detail.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
}

/// 一枚行内文字按钮（统一套 `frame::BASE` 与 `--force-state`）。
fn action<'a>(
    ui: &'a Ui,
    label: &'a str,
    message: PageMessage,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    buttons::text(
        ui,
        forced_state(Button::new(label, Message::Instances(message)), forced).base(frame::BASE),
    )
}

fn text_note<'a>(ui: &'a Ui, text: &'a str, color: Role) -> Element<'a, Message> {
    frame::text_role(ui, TypeRole::BodySmall, false, text, ui.color(color))
}

/// 写操作成功后的一句话（**唯一产地**：卡片里那条成功提示）。
///
/// 备份那句必须带**完整路径**：落点在文件系统某处，用户得知道去哪找。
#[must_use]
pub(crate) fn op_line(op: &SavesOped, texts: Texts) -> String {
    match op {
        SavesOped::Renamed { from, to } => texts.saves_renamed(from, to),
        SavesOped::Deleted { dir } => texts.saves_deleted(dir),
        SavesOped::BackedUp {
            dir,
            path,
            bytes,
            skipped,
            ..
        } => {
            let mut line = texts.saves_backed_up(dir, path, &human_bytes(*bytes));
            if *skipped > 0 {
                // 备份跳过链接这件事必须说出来：静默少打包几个文件比失败更坏。
                line.push_str(" · ");
                line.push_str(&texts.saves_backup_skipped(*skipped));
            }
            line
        }
    }
}

/// 世界那一行的副文案：`2024-05-01 10:00 UTC`，有图标时再挂一个「有图标」。
pub(super) fn world_detail(icon: bool, modified_ms: Option<u64>, texts: Texts) -> String {
    let stamp = stamp(modified_ms, texts);
    if icon {
        format!("{stamp} · {}", texts.saves_world_icon())
    } else {
        stamp
    }
}

/// 截图那一行的副文案：`1.2 MiB · 2024-05-01 10:00 UTC`。
pub(super) fn shot_detail(bytes: u64, modified_ms: Option<u64>, texts: Texts) -> String {
    format!("{} · {}", human_bytes(bytes), stamp(modified_ms, texts))
}

/// mtime（UTC 毫秒）→ `2024-05-01 10:00 UTC`；取不到就说"时间未知"（不拿 0 或当前时间糊）。
///
/// 换算走 `core::launch::time` 的 RFC3339 格式化，再按 `app/view/instances.rs::last_played`
/// 同一刀切开——**UTC 是 core 的口径，不做本地时区换算**（那属时间格式 mission）。
fn stamp(modified_ms: Option<u64>, texts: Texts) -> String {
    let Some(text) = modified_ms.and_then(stamp_utc) else {
        return texts.saves_time_unknown().to_string();
    };
    text
}

/// 毫秒 → `YYYY-MM-DD HH:MM UTC`（`None` = 早于 1970 / 溢出，不猜）。
fn stamp_utc(ms: u64) -> Option<String> {
    let at = std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_millis(ms))?;
    let text = unlucky_core::launch::time::format_rfc3339_millis(at);
    // `2024-05-01T10:00:00.000Z` → `2024-05-01 10:00 UTC`（切两段，不解析、不重排）。
    // 模板与 `app/view/instances.rs::last_played` **一字不差**：同一句话只能有一份写法，
    // 页面源码的字面量白名单里也只有这一条（`instances/tests/i18n.rs` 的 `ALLOWED_LITERALS`）。
    let date = text.get(0..10)?;
    let time = text.get(11..16)?;
    Some(format!("{date} {time} UTC"))
}
