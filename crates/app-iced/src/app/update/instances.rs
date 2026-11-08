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
use unlucky_presenter::dto::{AppCommand, LaunchArgsPatch};

use crate::pages::InstanceMessage;

use super::super::message::Message;
use super::super::state::App;

impl App {
    /// 启动参数区的一条消息。
    pub(super) fn on_instance_message(&mut self, message: InstanceMessage) -> Task<Message> {
        let Some(instance) = self.selected_instance_id() else {
            return Task::none();
        };
        if !self.launch_form.bound_to(&instance) {
            // 表单还没对着这个实例（首帧还没播种）：不接编辑，免得改到别的实例上。
            return Task::none();
        }

        match message {
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
    fn selected_instance_id(&self) -> Option<String> {
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
