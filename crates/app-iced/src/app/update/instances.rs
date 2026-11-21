//! 实例页「启动参数」区的消息落点（M157）。
//!
//! 三件事：
//! 1. **编辑**只改页面状态（`App::launch_form`）并置脏——不 dispatch，不打网络；
//! 2. **保存**派一条 [`AppCommand::SetInstanceLaunchArgs`]（整份五项补丁），回程由
//!    `InstancesChanged` 快照刷新页面（不做乐观回显，与 M90 的启动/停止同一条纪律）；
//! 3. **恢复跟随全局**只清表单（仍需用户点保存）——「点一下就写盘」会让误触直接生效。
//!
//! **清脏的唯一时机是「回程快照等于表单」**：`LaunchForm::sync` 在「同实例 + 脏」时只在
//! `matches(&view)` 成立时才清脏并重播种，其余情况原样保留草稿。保存本身**不清脏**——落盘成败
//! 要等回程事件，写盘失败时旧快照回来必须留住用户敲的值（M157 r3 复审 P2-2）。

use iced::Task;
use unlucky_presenter::dialogs::{DialogKind, DialogSource};
use unlucky_presenter::dto::{AppCommand, LaunchArgsPatch, ModsCommand, SavesCommand};
use unlucky_presenter::state::SavesOp;

use crate::pages::InstanceMessage;

use super::super::message::Message;
use super::super::state::App;

impl App {
    /// 实例页详情列的一条消息（启动参数表单 + MODS 区）。
    pub(super) fn on_instance_message(&mut self, message: InstanceMessage) -> Task<Message> {
        // M302：扫描超时那一屏的「重新扫描」。**必须在下面那道守卫之前**：那一刻列表还是空的、
        // 根本没有选中实例，掉进 `selected_instance_id()` 守卫就永远派不发——按钮点了没反应。
        if matches!(message, InstanceMessage::RetryScan) {
            self.dispatch(AppCommand::RefreshInstances);
            return Task::none();
        }
        // M455：视图 / 尺寸两个档位**不依赖选中实例**（页头的 chip 在列表页也点得到），
        // 所以必须排在下一条 `selected_instance_id()` 守卫之前——否则列表页上点了毫无反应。
        if let Some(task) = self.on_instance_view_mode(&message) {
            return task;
        }
        // M457：搜索框同理（它在列表页上就是主控件，那一刻多半还没选中实例）。
        if let Some(task) = self.on_instance_search(&message) {
            return task;
        }
        let Some(instance) = self.selected_instance_id() else {
            return Task::none();
        };

        match message {
            // 上面那条提前返回已经处理过了；这里留一支只为保持 match 穷尽（漏一支会编不过）
            InstanceMessage::RetryScan
            | InstanceMessage::SetView(_)
            | InstanceMessage::SetTileSize(_)
            | InstanceMessage::Search(_)
            | InstanceMessage::ClearSearch => {}
            // 真状态等 host 回一份新快照（与 Save 分支同一条纪律）。
            //
            // 这一支在 `bound_to` 守卫**之前**：MODS 区的启停只依赖「当前选中实例」，
            // 与启动参数表单播没播种无关（守卫是给表单草稿用的，见下一条）。
            InstanceMessage::SetModEnabled { file, enabled } => {
                self.state.mods.mark_pending(&file);
                self.dispatch(AppCommand::Mods(Box::new(ModsCommand::SetEnabled {
                    id: instance,
                    file,
                    enabled,
                })));
                return Task::none();
            }
            // M235：重新列一次存档（失败后的「重试」也走这条）。
            //
            // 与 `SetModEnabled` 一样放在 `bound_to` 守卫**之前**：存档区只认"当前选中实例"，
            // 与启动参数表单播没播种无关（那条守卫是给表单草稿用的）。
            InstanceMessage::ReloadSaves => {
                self.dispatch(AppCommand::Saves(Box::new(SavesCommand::List(instance))));
                return Task::none();
            }
            // M235：打开存档文件夹——拉起哪个目录由 host 定（`saves/` → `screenshots/` → 运行目录），
            // 真正的 opener 在 `core::platform::opener`（平台差异不散到 app 这一层）。
            InstanceMessage::OpenSavesFolder => {
                self.dispatch(AppCommand::Saves(Box::new(SavesCommand::OpenFolder {
                    id: instance,
                })));
                return Task::none();
            }
            // M247：行内重命名的三个动作。**输入过程不 dispatch**（只改 presenter 的草稿），
            // 提交才发命令——与启动参数表单「编辑只改状态，保存才派命令」同一条纪律。
            InstanceMessage::StartRenameWorld { dir } => {
                self.state.saves.start_rename(&dir);
                self.view_dirty = true;
                self.refresh_view();
                return Task::none();
            }
            InstanceMessage::RenameWorldChanged(text) => {
                self.state.saves.edit_rename(text);
                self.view_dirty = true;
                self.refresh_view();
                return Task::none();
            }
            InstanceMessage::CommitRenameWorld => {
                let Some(draft) = self.state.saves.take_rename() else {
                    return Task::none();
                };
                // 空名字 / 没改 → 直接收起，不打后台：这两个不是"操作失败"，是"用户改主意了"。
                if draft.text.is_empty() || draft.text == draft.dir {
                    self.view_dirty = true;
                    self.refresh_view();
                    return Task::none();
                }
                self.state.saves.mark_pending(SavesOp::Rename {
                    dir: draft.dir.clone(),
                });
                self.dispatch(AppCommand::Saves(Box::new(SavesCommand::RenameWorld {
                    id: instance,
                    dir: draft.dir,
                    to: draft.text,
                })));
                return Task::none();
            }
            InstanceMessage::CancelRenameWorld => {
                self.state.saves.cancel_rename();
                self.view_dirty = true;
                self.refresh_view();
                return Task::none();
            }
            // M247：删除**先弹确认框**。确认目标是"哪个世界"必须记在状态里——
            // 主按钮落点在 `app/update/dialog.rs`，那里只有 `App`，没有这个页面。
            InstanceMessage::RequestDeleteWorld { dir } => {
                let (title, body, primary, secondary) =
                    crate::pages::saves_delete_prompt(self.lang().catalog(), &dir);
                // **先压框、拿到它的 id，再登记目标**：目标绑在那个框上（复审 r1 P1-1）——
                // 反过来（先登记再压框）会让"这一拍还没有框"的中间态挂在状态里，别的框的主按钮
                // 就有机会把它兑掉。
                let dialog = self.state.dialogs.push(
                    DialogSource::UserAction,
                    title,
                    body,
                    DialogKind::Confirm { primary, secondary },
                );
                self.state.saves.ask_delete(dialog, &dir);
                self.view_dirty = true;
                return Task::none();
            }
            // M247：备份——落点由 host 定（`<data>/exports/saves/`），回执里带完整路径。
            InstanceMessage::BackupWorld { dir } => {
                self.state
                    .saves
                    .mark_pending(SavesOp::Backup { dir: dir.clone() });
                self.dispatch(AppCommand::Saves(Box::new(SavesCommand::BackupWorld {
                    id: instance,
                    dir,
                    dest: None,
                })));
                return Task::none();
            }
            // 表单还没对着这个实例（首帧还没播种）：不接编辑，免得改到别的实例上。
            _ if !self.launch_form.bound_to(&instance) => return Task::none(),
            InstanceMessage::Jvm(text) => {
                self.launch_form.jvm = text;
                self.launch_form.dirty = true;
            }
            InstanceMessage::Game(text) => {
                self.launch_form.game = text;
                self.launch_form.dirty = true;
            }
            InstanceMessage::Memory(text) => {
                self.launch_form.memory = text;
                self.launch_form.dirty = true;
            }
            InstanceMessage::Java(text) => {
                self.launch_form.java = text;
                self.launch_form.dirty = true;
            }
            InstanceMessage::Backend(id) => {
                self.launch_form.backend = id;
                self.launch_form.dirty = true;
            }
            InstanceMessage::Reset => {
                self.launch_form.reset();
            }
            InstanceMessage::Save => {
                // **不在这里清脏**：`dispatch` 只是把命令送出去，落盘成败要等回程事件。
                // 失败时（内存 `4g` / 写盘失败）pump 会带**旧快照**重建视图，脏标记若已清掉，
                // `LaunchForm::sync` 就会用旧值覆盖用户刚敲的草稿（M157 r3 复审 P2-2）。
                // 清脏的唯一时机是「回程快照已经等于表单」——见 `LaunchForm::sync`。
                let patch: LaunchArgsPatch = self.launch_form.patch();
                self.dispatch(AppCommand::set_launch_args(&instance, patch));
            }
        }
        // 纯本地编辑：不重建视图模型（页面直接从表单画），只要一帧重绘即可——iced 在
        // 消息处理完之后本来就会画一帧，所以这里什么都不排。
        Task::none()
    }

    /// 详情页当前选中的实例 id（不在详情页时为 `None`）。
    ///
    /// `pub(in crate::app)`（M247）：删除确认框的主按钮在 `app/update/dialog.rs`，也要问同一个问题。
    pub(in crate::app) fn selected_instance_id(&self) -> Option<String> {
        match self.state.route() {
            unlucky_presenter::router::Route::Detail(id) => Some(id.clone()),
            _ => None,
        }
    }
}

// M455：视图档位 / 磁贴尺寸两个写端（`ui.instances.view` / `tile_size`）
mod view_mode;

// M457：搜索框的写端（`App::instance_query`）+ 「离开实例页就清」那条规矩
mod search;

#[cfg(test)]
#[path = "instances/tests.rs"]
mod tests;

// ------------------------------------------------------------------ M302 首屏三态

#[cfg(test)]
mod scan_phase;
