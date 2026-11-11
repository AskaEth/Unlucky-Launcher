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
        let Some(instance) = self.selected_instance_id() else {
            return Task::none();
        };

        match message {
            // M224：启停一个 mod。**不做乐观翻面**——先把那个开关标成「请求在飞」（置灰），
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

#[cfg(test)]
mod tests {
    use unlucky_design::color::SchemeFlavor;
    use unlucky_presenter::dto::LaunchArgsView;
    use unlucky_presenter::settings_bridge::SettingsBridge;

    use crate::app::state::App;
    use crate::cli::Options;
    use crate::pages::{InstanceMessage, LaunchForm};

    use crate::theme::Families;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    fn app_with_form() -> App {
        let mut app = App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(unlucky_presenter::mock::MockBackend::new()),
            SettingsBridge::memory_only(),
        );
        // 进详情页 + 直接放一份「已播种」的表单：本测试只验消息落点，
        // 不验播种（那在 `LaunchForm::sync` 的单测里）。
        app.state.navigate(unlucky_presenter::router::Route::Detail(
            INSTANCE.to_string(),
        ));
        app.launch_form = LaunchForm::from_view(
            Some(INSTANCE),
            &LaunchArgsView {
                instance_id: INSTANCE.to_string(),
                ..LaunchArgsView::default()
            },
        );
        app
    }

    /// 测试用的实例 id（mock 后端的实例快照里有它）。
    const INSTANCE: &str = "survival";

    /// 编辑 → 脏；恢复 → 五项清空仍需保存；保存前不发命令。
    #[test]
    fn editing_marks_dirty_and_reset_waits_for_save() {
        let mut app = app_with_form();
        let _ = app.on_instance_message(InstanceMessage::Memory("6144".to_string()));
        assert_eq!(app.launch_form.memory, "6144");
        assert!(app.launch_form.dirty);

        let _ = app.on_instance_message(InstanceMessage::Backend("vulkan".to_string()));
        assert_eq!(app.launch_form.backend, "vulkan");

        let _ = app.on_instance_message(InstanceMessage::Reset);
        assert_eq!(app.launch_form.memory, "");
        assert_eq!(app.launch_form.backend, "default");
        assert!(app.launch_form.dirty, "恢复之后还得点保存才落盘");
    }

    /// **MODS 区的启停在表单守卫之前**（M224）：表单还没播种也照样能启停。
    ///
    /// 抓得住：把 `SetModEnabled` 放回 `bound_to` 守卫之后——那时首帧的详情页点了开关
    /// 什么都不会发生（用户看到开关纹丝不动，也没有任何提示）。
    #[test]
    fn mod_toggles_work_before_the_launch_form_is_bound() {
        let mut app = app_with_form();
        // 页面已经打开（真路径上 `refresh_view` 的 `sync_mods` 会做这一步），但表单还没播种。
        app.state.bind_mods(Some(INSTANCE));
        app.launch_form = LaunchForm::from_view(
            Some("other"),
            &LaunchArgsView {
                instance_id: "other".to_string(),
                ..LaunchArgsView::default()
            },
        );

        let _ = app.on_instance_message(InstanceMessage::SetModEnabled {
            file: "sodium.jar".to_string(),
            enabled: false,
        });

        assert!(
            app.state.mods.is_pending("sodium.jar"),
            "启停被表单守卫挡住了（开关点了没反应）"
        );
    }

    /// 表单没绑定到当前实例时不接编辑（否则草稿会写到别的实例上）。
    #[test]
    fn edits_are_ignored_when_the_form_is_bound_elsewhere() {
        let mut app = app_with_form();
        app.launch_form = LaunchForm::from_view(
            Some("other"),
            &LaunchArgsView {
                instance_id: "other".to_string(),
                ..LaunchArgsView::default()
            },
        );
        let _ = app.on_instance_message(InstanceMessage::Memory("9999".to_string()));
        assert_eq!(app.launch_form.memory, "");
        assert!(!app.launch_form.dirty);
    }

    /// **保存只派发、不清脏**（M157 r3 复审 P2-2）：落盘成败要等回程事件，
    /// 派发前清掉脏标记会让失败后的重播覆盖用户草稿。
    ///
    /// 抓得住：Save 分支里又写回 `dirty = false`（那条路径在失败时丢草稿）。
    /// 抓不住：回程事件的真实时序——清脏判据在 `LaunchForm::sync` 的
    /// `confirmed_save_clears_dirty_and_reseeds` / `failed_save_keeps_the_draft` 里。
    #[test]
    fn save_dispatch_keeps_the_dirty_flag() {
        let mut app = app_with_form();
        let _ = app.on_instance_message(InstanceMessage::Memory("4g".to_string()));
        assert!(app.launch_form.dirty);
        let _ = app.on_instance_message(InstanceMessage::Save);
        assert!(
            app.launch_form.dirty,
            "派发后仍要保留脏标记，等回程快照确认"
        );
        assert_eq!(app.launch_form.memory, "4g", "派发不改表单内容");
    }
}
