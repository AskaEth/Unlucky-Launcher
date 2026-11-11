//! 弹窗消息的落点：关闭 / 展开 / 导出 / 打开文件夹 / 复制路径。
//!
//! 只有两类副作用：**剪贴板**（返回 `Task`，必须交回 runtime）与**弹 Toast**（就地推）。
//! 导出与打开文件夹是同步的本地动作（写一个 zip / 拉起一个 `explorer`），耗时都在毫秒级，
//! 不为它们单开线程——真要慢（网络盘）也是用户能感知的一次点击。

use iced::Task;
use unlucky_presenter::toast::ToastKind;

use crate::app::dialog::{export, Message as DialogMessage};
use crate::app::state::App;

pub(super) fn on_dialog(app: &mut App, message: DialogMessage) -> Task<crate::app::Message> {
    match message {
        DialogMessage::Primary => {
            // M247：删除存档的二次确认——主按钮就是**真删**（这是本页唯一不可逆的动作）。
            // 目标记在 presenter 的状态里（`SavesState::confirm`）：模态队列只画框，不记"要删谁"。
            //
            // **认框不认人**（复审 r1 P1-1）：只核销"当前这个框"登记的目标。哪怕某条关闭路径漏清，
            // 崩溃卡片的「导出诊断包」也拿不走它——否则就是一次静默删世界。
            let current = app.state.dialogs.current().map(|request| request.id);
            if let Some(dir) = current.and_then(|id| app.state.saves.take_confirm(id)) {
                app.close_dialog();
                if let Some(instance) = app.selected_instance_id() {
                    app.state
                        .saves
                        .mark_pending(unlucky_presenter::state::SavesOp::Delete {
                            dir: dir.clone(),
                        });
                    app.dispatch(unlucky_presenter::dto::AppCommand::Saves(Box::new(
                        unlucky_presenter::dto::SavesCommand::DeleteWorld { id: instance, dir },
                    )));
                }
                app.refresh_view();
                return Task::none();
            }
            // 崩溃卡片的主按钮是「导出诊断包」；普通确认框的主按钮就是「关掉」。
            let Some(card) = app.dialog_card.clone() else {
                app.close_dialog();
                return Task::none();
            };
            let done = export::export(&card.report, app.lang());
            // 结论**画在卡片里**：导出这种「必须看见落到哪」的结果不能只靠 Toast
            // （app-iced 的 Toast 至今没有渲染点）。成败按 `write_bundle` 的真实结果判，
            // 不看「launcher_logs 有没有值」——那只是入参齐全度（r1 复审 P3-2）。
            app.state.toasts.push(
                if done.ok {
                    ToastKind::Success
                } else {
                    ToastKind::Error
                },
                done.text.clone(),
            );
            if let Some(state) = &mut app.dialog_card {
                state.outcome = Some(done.text);
            }
            app.view_dirty = true;
            app.refresh_view();
            Task::none()
        }
        DialogMessage::Secondary | DialogMessage::Dismiss => {
            // M247：取消/关掉删除确认框 ⇒ 忘掉那个目标（不然下一次点确定会删上一个）。
            // **只核销当前这个框的目标**（复审 r2 加固）：别人的框关掉时不该动我们的。
            if let Some(current) = app.state.dialogs.current().map(|request| request.id) {
                app.state.saves.close_confirm(current);
            }
            app.close_dialog();
            // 立即重画：空闲态只有 1 Hz 心跳，只置 `view_dirty` 会让弹窗再留最多 1 秒
            // （r1 复审 P3-1）。
            app.refresh_view();
            Task::none()
        }
        DialogMessage::Toggle => {
            app.toggle_dialog_detail();
            Task::none()
        }
        DialogMessage::OpenFolder => {
            let Some(card) = app.dialog_card.clone() else {
                return Task::none();
            };
            let problem = export::open_folder(&card.report, app.lang());
            if !problem.is_empty() {
                if let Some(state) = &mut app.dialog_card {
                    state.outcome = Some(problem.clone());
                }
                app.state.toasts.push(ToastKind::Error, problem);
                app.view_dirty = true;
                app.refresh_view();
            }
            Task::none()
        }
        DialogMessage::CopyPaths => {
            let Some(card) = app.dialog_card.clone() else {
                return Task::none();
            };
            app.state.toasts.push(
                ToastKind::Info,
                app.lang()
                    .catalog()
                    .text(unlucky_i18n::catalog::Key::CrashDialogCopied),
            );
            iced::clipboard::write(export::paths_text(&card.report))
        }
        DialogMessage::CopyPath(index) => {
            let Some(card) = app.dialog_card.clone() else {
                return Task::none();
            };
            let Some((_, path)) = card.report.path_lines().into_iter().nth(index) else {
                return Task::none();
            };
            app.state.toasts.push(
                ToastKind::Info,
                app.lang()
                    .catalog()
                    .text(unlucky_i18n::catalog::Key::CrashDialogCopied),
            );
            iced::clipboard::write(path.display().to_string())
        }
    }
}

/// `Esc` 关弹窗（有弹窗时**先吃这一条**，不让它落到 AI 栏的 Esc 收起上）。
pub(super) fn escape(app: &mut App) -> bool {
    let Some(current) = app.state.dialogs.current().map(|request| request.id) else {
        return false;
    };
    // M247（复审 r1 P1-1）：**三条关闭路径都要清**——次按钮/遮罩清了，`Esc` 这条以前漏了，
    // 于是"删存档 → Esc → 崩溃卡片 → 导出诊断包"会把那个世界静默删掉。
    // 现在还有 id 绑定兜一道（`take_confirm(current)`），这里是纵深防御的第二层。
    // 复审 r2 加固：清的时候**只清这个框自己的目标**（`close_confirm`），不碰别人的。
    app.state.saves.close_confirm(current);
    app.close_dialog();
    app.refresh_view();
    true
}

#[cfg(test)]
mod tests;
