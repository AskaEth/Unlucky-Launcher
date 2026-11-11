//! 自更新动作的落点（M159）：`pages::Message::Update(UpdateAction)` → presenter 指令 / 重启。
//!
//! 三条纪律：
//!
//! 1. **不做乐观回显**：点了「下载并安装」不改本地状态，等 `CoreEvent::Update` 回来才变
//!    （与启动/安装同一条：本地状态与事件流打架时，界面会自说一套）；
//! 2. 更新的**判断**（要不要提示、失败算什么、版本比较）全在 core/presenter，这里只转发；
//! 3. 「立即重启」是本文件唯一碰进程的地方：先按同样的命令行拉起新进程，再退出自己——
//!    正因为替换已经完成（新 exe 就在原路径上），拉起来的就是新版本。

use crate::app::diag::diag;
use iced::Task;
use unlucky_presenter::dto::AppCommand;

use crate::pages::UpdateAction;

use super::super::message::Message;
use super::super::state::App;

impl App {
    /// 一条更新动作。
    pub(super) fn on_update_action(&mut self, action: UpdateAction) -> Task<Message> {
        match action {
            UpdateAction::CheckNow => {
                if self.update_busy() {
                    return self.refuse_busy("立即检查");
                }
                self.dispatch(AppCommand::CheckUpdate)
            }
            UpdateAction::Apply => {
                if self.update_busy() {
                    return self.refuse_busy("下载并安装");
                }
                self.dispatch(AppCommand::ApplyUpdate)
            }
            UpdateAction::Dismiss => self.dispatch(AppCommand::DismissUpdate),
            UpdateAction::SetAutoCheck(on) => self.dispatch(AppCommand::SetUpdateAutoCheck(on)),
            UpdateAction::RestartNow => return self.restart_now(),
        }
        Task::none()
    }

    /// 有没有一条检查/下载正在跑。
    ///
    /// 与 `pages::Update::busy()` **同一个判据**（`applying` 或 `status == Checking`）：
    /// presenter 的 `begin_check` / `begin_apply` 在 inflight 时静默 `return None`，
    /// 所以「忙碌」这件事必须在按下之前就挡住——否则用户点了没反应，还以为按钮坏了（M161）。
    ///
    /// 这一层是**最后一道**：按钮已经画成禁用（`pages/settings/update.rs` 的 `can_check` /
    /// `can_apply`），但快捷键/未来新增的入口仍会走到这里。
    fn update_busy(&self) -> bool {
        let view = &self.state.update;
        view.applying || view.outcome.is_checking()
    }

    /// 忙碌时把这条动作丢掉（不派发、不乐观改状态），并在日志里留一行取证。
    fn refuse_busy(&self, what: &str) -> Task<Message> {
        diag!("[update] 已有一条检查/下载在跑，忽略重复的「{what}」");
        Task::none()
    }

    /// 立即重启：按同样的命令行拉起自己，然后退出。
    ///
    /// 截图通道（`--screenshot`）**不重启**：那条路每跑一次就该落一张图然后退出，重启会覆盖同一
    /// 文件、并把取证进程拖成死循环。
    fn restart_now(&mut self) -> Task<Message> {
        if self.options.screenshot.is_some() {
            diag!("[update] 截图通道不重启（忽略「立即重启」）");
            return Task::none();
        }
        match std::env::current_exe() {
            Ok(exe) => {
                let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
                // 子进程是本 GUI exe 自己（Subsystem=2，不会分到控制台）；走 `quiet` 是为了让
                // 「app-iced 里除 opener 外不留裸构造」这条规则整体可被守卫钉住（M217）。
                let spawned = unlucky_core::platform::child::quiet(&exe)
                    .args(&args)
                    .spawn();
                match spawned {
                    Ok(child) => diag!(
                        "[update] 已拉起新进程 pid={}：{}",
                        child.id(),
                        exe.display()
                    ),
                    Err(error) => {
                        diag!("[update] 拉起新进程失败：{error}（请手动重启启动器）");
                        return Task::none();
                    }
                }
            }
            Err(error) => {
                diag!("[update] 取 current_exe 失败：{error}");
                return Task::none();
            }
        }
        iced::exit()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use iced::Task;
    use unlucky_design::color::SchemeFlavor;
    use unlucky_presenter::dto::{AppCommand, AppEvent, Backend};
    use unlucky_presenter::settings_bridge::SettingsBridge;

    use crate::app::state::App;
    use crate::cli::Options;
    use crate::pages::UpdateAction;
    use crate::theme::Families;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    /// 记录命令流的假后端：把 `dispatch` 收到的每一条抄进共享表（动作接线断没断靠它判）。
    struct Recorder {
        seen: Arc<Mutex<Vec<AppCommand>>>,
    }

    impl Backend for Recorder {
        fn drain_events(&mut self) -> Vec<AppEvent> {
            Vec::new()
        }

        fn dispatch(&mut self, command: AppCommand) {
            self.seen
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(command);
        }

        fn tick(&mut self, _elapsed: std::time::Duration) {}
    }

    fn app() -> (App, Arc<Mutex<Vec<AppCommand>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let app = App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(Recorder {
                seen: Arc::clone(&seen),
            }),
            SettingsBridge::memory_only(),
        );
        (app, seen)
    }

    fn last(seen: &Arc<Mutex<Vec<AppCommand>>>) -> AppCommand {
        seen.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .last()
            .cloned()
            .expect("应当派发过一条命令")
    }

    /// 四个转发动作各自交给后端**对应的**指令（串线会红）。
    #[test]
    fn every_update_action_reaches_the_backend() {
        let (mut app, seen) = app();

        let _: Task<_> = app.on_update_action(UpdateAction::CheckNow);
        assert_eq!(last(&seen), AppCommand::CheckUpdate);

        let _ = app.on_update_action(UpdateAction::Apply);
        assert_eq!(last(&seen), AppCommand::ApplyUpdate);

        let _ = app.on_update_action(UpdateAction::Dismiss);
        assert_eq!(last(&seen), AppCommand::DismissUpdate);

        let _ = app.on_update_action(UpdateAction::SetAutoCheck(false));
        assert_eq!(last(&seen), AppCommand::SetUpdateAutoCheck(false));
    }

    /// **忙碌时按下去不发命令**（M161：此前点了没反应，因为 presenter 在 inflight 时静默返回）。
    ///
    /// 三条都要钉住：①`Checking` 时「立即检查」不派发；②`applying` 时「下载并安装」不派发；
    /// ③忙碌结束后**恢回可点**（失败/超时不该把按钮永久钉死）。
    #[test]
    fn pressing_while_busy_dispatches_nothing() {
        let (mut app, seen) = app();

        app.state.update.outcome = unlucky_presenter::dto::UpdateOutcome::Checking;
        let _ = app.on_update_action(UpdateAction::CheckNow);
        assert!(
            seen.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_empty(),
            "正在检查时再按「立即检查」不许派发第二条命令"
        );

        // 检查回来（失败也算回来）：不忙了，能再点。
        app.state.update.outcome = unlucky_presenter::dto::UpdateOutcome::Failed(
            unlucky_presenter::dto::UpdateFailureView::new("network", "连不上", true),
        );
        let _ = app.on_update_action(UpdateAction::CheckNow);
        assert_eq!(last(&seen), AppCommand::CheckUpdate, "失败之后要能重试");

        // 下载/校验/替换期间：`Apply` 与 `CheckNow` 都不许派发。
        let before = seen.lock().unwrap_or_else(|p| p.into_inner()).len();
        app.state.update.applying = true;
        let _ = app.on_update_action(UpdateAction::Apply);
        let _ = app.on_update_action(UpdateAction::CheckNow);
        assert_eq!(
            seen.lock().unwrap_or_else(|p| p.into_inner()).len(),
            before,
            "applying 期间不许派发任何新的检查/安装"
        );

        // 应用结束：恢复可点。
        app.state.update.applying = false;
        let _ = app.on_update_action(UpdateAction::Apply);
        assert_eq!(last(&seen), AppCommand::ApplyUpdate, "失败/完成后恢回可点");
    }

    /// 截图通道不重启：取证进程不许把自己重启成死循环。
    #[test]
    fn the_screenshot_channel_never_restarts() {
        let (mut app, seen) = app();
        app.options.screenshot = Some(std::path::PathBuf::from("C:/Temp/shot.png"));
        let _ = app.on_update_action(UpdateAction::RestartNow);
        assert!(
            seen.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_empty(),
            "重启不是一条后端指令（不该派发任何命令）"
        );
    }
}
