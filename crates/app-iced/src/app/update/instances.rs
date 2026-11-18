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
        let Some(instance) = self.selected_instance_id() else {
            return Task::none();
        };

        match message {
            // 上面那条提前返回已经处理过了；这里留一支只为保持 match 穷尽（漏一支会编不过）
            InstanceMessage::RetryScan => {}
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
    use unlucky_presenter::dto::{AppCommand, AppEvent, Backend, CoreEvent, LaunchArgsView};
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

    /// **M302：刷新在飞时列表不清空，快照落地后时钟不被钉住**。
    ///
    /// 现场：扫盘搬进后台线程之后，「派发刷新 → 结果回来」之间有一小段真空。这段真空里界面
    /// 只有两条路：**留着旧列表**（用户看到的是「还在，但还没变」）或者**清空再重填**（用户
    /// 看到的是一屏白/空态）。M302 要的是前者；后者等于把卡顿换成闪屏。
    ///
    /// 第二件事是 M298 的型态：`view_dirty` 一旦永久为真，`advance` 就每拍无条件 `refresh_view`
    /// （整页视图模型重建），而「快照落地」这条路最容易被写成「只置脏不重建」。
    ///
    /// 抓得住：① **没有事件的一拍** `pump` 不把 `state.instances` 清空（用例走的是
    /// `backend.dispatch + pump`，所以要防的是「事件落地路径顺手清表」这一类错）；
    /// ② 谁把 `InstancesChanged` 的落地路径写成「置脏不落地」或者让 `refresh_view` 收尾不再复位
    /// `view_dirty`。
    /// 抓不住：**`App::dispatch` 那 5 行**（它置 `watch_until`、再 pump）本身——本用例故意绕过它，
    /// 因为观察窗会让 `needs_frames()` 在 3 s 内为真，与「刷新有没有多订阅帧」这件事混在一起。
    /// 换句话说：这里守的是「一拍无事件 = 列表不动 + 不烧帧」，不是「派发那一刻」。
    /// 抓不住：真线程的时序与竞争（那在 presenter 的 `host::game::refresh::tests` 交错用例里）；
    /// 也抓不住「扫盘真要多久」（那是真实测量，不是单测）。
    #[test]
    fn a_refresh_in_flight_keeps_the_list_and_does_not_pin_the_clock() {
        /// 只喂首屏快照、派发命令一律不回答的后端（= 「刷新还在飞」）。
        #[derive(Debug)]
        struct RecorderBackend {
            seeded: Vec<AppEvent>,
        }

        impl Backend for RecorderBackend {
            fn drain_events(&mut self) -> Vec<AppEvent> {
                std::mem::take(&mut self.seeded)
            }

            fn dispatch(&mut self, _command: AppCommand) {}
        }

        let mut app = App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(RecorderBackend {
                seeded: vec![AppEvent::Core(CoreEvent::InstancesChanged(
                    unlucky_presenter::mock_data::mock_instances(),
                ))],
            }),
            SettingsBridge::memory_only(),
        );
        // 前提：首屏快照已落地、入场动效已经跑完（否则 `needs_frames` 为真是动效的锅，不是刷新的）
        app.pump();
        while crate::app::components::motion::needs_frames() {
            crate::app::components::motion::advance_by(crate::app::components::motion::FRAME_MS);
        }
        let tiles = app.state.tiles().len();
        assert!(tiles > 0, "前提：首屏该有实例");

        // 刷新在飞：命令出去了，后端这一拍一个字都不回
        app.backend.dispatch(AppCommand::RefreshAll);
        app.pump();
        assert_eq!(
            app.state.tiles().len(),
            tiles,
            "刷新在飞时列表被清空了（清空再重填 = 把卡顿换成闪屏）"
        );
        assert!(
            !app.view_dirty,
            "刷新在飞时 `view_dirty` 不许留在真上（M298：每拍重建整页视图模型）"
        );
        assert!(
            !app.needs_frames(),
            "刷新在飞时不该多出帧节拍（真要等回程，走派发后的 3 s 观察窗，不是永久订阅）"
        );

        // 回程到了：整份替换（不是追加），并且这一拍就把脏标收回去
        app.backend = Box::new(RecorderBackend {
            seeded: vec![AppEvent::Core(CoreEvent::InstancesChanged(
                unlucky_presenter::mock_data::mock_instances(),
            ))],
        });
        app.pump();
        assert_eq!(app.state.tiles().len(), tiles, "快照是整体替换");
        assert!(
            !app.view_dirty,
            "快照落地之后 `view_dirty` 必须被 `refresh_view` 收回去（M298）"
        );
    }

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

// ------------------------------------------------------------------ M302 首屏三态

#[cfg(test)]
mod scan_phase;
