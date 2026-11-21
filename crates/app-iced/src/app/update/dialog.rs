//! 弹窗消息的落点：关闭 / 展开 / 导出 / 打开文件夹 / 复制路径。
//!
//! 只有两类副作用：**剪贴板**（返回 `Task`，必须交回 runtime）与**弹 Toast**（就地推）。
//! 导出与打开文件夹是同步的本地动作（写一个 zip / 拉起一个 `explorer`），耗时都在毫秒级，
//! 不为它们单开线程——真要慢（网络盘）也是用户能感知的一次点击。

use iced::Task;
use unlucky_presenter::toast::ToastKind;

use crate::app::components::motion;
use crate::app::dialog::{export, GuardInstall, Message as DialogMessage};
use crate::app::state::App;

/// 关掉当前这个框（**唯一出口**，M256）。
///
/// 逻辑状态是**当场**关的（`close_dialog()` 立刻清 `state.dialogs`）——延迟关闭会让队列里那个
/// 框在退场期间仍然"活着"（`current()` 还是它），所有按"有没有框"判断的代码都会看到一个已经
/// 点过「确定」的框。所以退场只发生在**画面上**：`refresh_view` 在 `motion::modal_closing()`
/// 期间保留旧框的视图模型，`App::sync_motion` 在退场走完那一拍把它置 `None`。
///
/// 关着动效（`MotionScale::NONE` / 截图通道）时 `begin_modal_close()` 返回 `true`，
/// `modal_closing()` 恒假 ⇒ 当场清，退回改造前的行为。
fn close(app: &mut App) {
    let _ = motion::begin_modal_close();
    app.close_dialog();
    // 立即重画：空闲态只有 1 Hz 心跳，只置 `view_dirty` 会让弹窗再留最多 1 秒
    // （r1 复审 P3-1）。退场的第一帧必须在这一拍画出来。
    app.refresh_view();
}

pub(super) fn on_dialog(app: &mut App, message: DialogMessage) -> Task<crate::app::Message> {
    // M256：退场动画期间**吞掉**弹窗消息。框正在缩小，这时再点一次「删除世界」不该再生效
    // （主按钮的 `take_confirm` 已经在上一次点击里核销过目标，但用户看到的仍是"框还在"）。
    if motion::modal_closing() {
        return Task::none();
    }
    match message {
        // M331：Java 不符的**启动前**框，主按钮 = 去设置改 Java（推荐出口）。
        // 认框不认人：只有当前这条队列请求绑着 guard 卡片时才走这里（别人的框一字不动）。
        DialogMessage::Primary if app.guard_for_current().is_some() => {
            close(app);
            app.state
                .navigate(unlucky_presenter::router::Route::Settings);
            app.refresh_view();
            Task::none()
        }
        DialogMessage::Primary => {
            // M247：删除存档的二次确认——主按钮就是**真删**（这是本页唯一不可逆的动作）。
            // 目标记在 presenter 的状态里（`SavesState::confirm`）：模态队列只画框，不记"要删谁"。
            //
            // **认框不认人**（复审 r1 P1-1）：只核销"当前这个框"登记的目标。哪怕某条关闭路径漏清，
            // 崩溃卡片的「导出诊断包」也拿不走它——否则就是一次静默删世界。
            let current = app.state.dialogs.current().map(|request| request.id);
            if let Some(dir) = current.and_then(|id| app.state.saves.take_confirm(id)) {
                // M256：走统一出口（逻辑当拍关，画面多留 150ms 退场）。
                // 目标已经在上一行核销 ⇒ 退场期间再点一次主按钮不会删第二个世界
                // （那一拍的消息还会被 `modal_closing` 那道闸门吞掉）。
                close(app);
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
                return Task::none();
            }
            // 崩溃卡片的主按钮是「导出诊断包」；普通确认框的主按钮就是「关掉」。
            let Some(card) = app.dialog_card.clone() else {
                close(app);
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
        // M331：次按钮 = 「仍要启动」：等价于 CLI 的 `--java`（显式指定要试这个 Java）。
        // 只放过这一次——host 不写任何设置，下一次点启动照旧会被拦。
        // M349 r3：钉过实例 Java 的建议框里，「去实例启动参数」是**唯一**能改对的地方。
        // 全局设置页对这条实例无效（实例层 `launch.javaPath` 优先级更高），所以这里跳详情页，
        // 不跳 `Route::Settings`——用例 `m349.rs::the_pinned_advisory_*` 就守着这一条。
        DialogMessage::InstanceLaunchArgs if app.guard_for_current().is_some() => {
            let id = app
                .guard_for_current()
                .map(|card| card.view.id.clone())
                .unwrap_or_default();
            close(app);
            app.state
                .navigate(unlucky_presenter::router::Route::Detail(id));
            app.refresh_view();
            Task::none()
        }
        DialogMessage::Secondary if app.guard_for_current().is_some() => {
            let id = app
                .guard_for_current()
                .map(|card| card.view.id.clone())
                .unwrap_or_default();
            close(app);
            app.dispatch(unlucky_presenter::dto::AppCommand::LaunchInstanceAnyway(id));
            Task::none()
        }
        // M334：第三个出口。三档：没装过 ⇒ 派 `InstallJavaFor { major }`；正在装 ⇒ 吞掉
        // （不重复派，一个框只该有一次安装）；装好了 ⇒ 直接再启动一次（这一次会过闸，
        // 因为托管目录里已经有那个大版本了）。
        DialogMessage::Tertiary if app.guard_for_current().is_some() => {
            let Some(card) = app.guard_for_current() else {
                return Task::none();
            };
            // 装机出口只在 `install_major` 有值时才画（M349 r3）：钉过实例 Java 的建议框没有它，
            // 下面这行是**不可达**的防御——真到了这里就什么都不做，绝不拿 0 去装一个野生版本。
            let Some(major) = card.view.install_major else {
                return Task::none();
            };
            match card.install {
                GuardInstall::Requested | GuardInstall::Running => Task::none(),
                GuardInstall::Done => {
                    let id = card.view.id.clone();
                    close(app);
                    app.dispatch(unlucky_presenter::dto::AppCommand::LaunchInstance(id));
                    Task::none()
                }
                GuardInstall::Idle | GuardInstall::Failed { .. } => {
                    app.mark_guard_install_requested();
                    app.dispatch(unlucky_presenter::dto::AppCommand::InstallJavaFor { major });
                    // 状态改了要立刻重画：进展行（「正在下载 Java 8…」）就是这一拍出现的，
                    // 不置脏的话要等下一个事件才看得见（`dialog_modal` 由 `refresh_view` 重建）。
                    app.view_dirty = true;
                    app.refresh_view();
                    Task::none()
                }
            }
        }
        // 没有 guard 卡片时（别的框误派这一枚）：只关框，绝不跳页。
        DialogMessage::InstanceLaunchArgs => {
            close(app);
            Task::none()
        }
        DialogMessage::Secondary | DialogMessage::Dismiss => {
            // M247：取消/关掉删除确认框 ⇒ 忘掉那个目标（不然下一次点确定会删上一个）。
            // **只核销当前这个框的目标**（复审 r2 加固）：别人的框关掉时不该动我们的。
            if let Some(current) = app.state.dialogs.current().map(|request| request.id) {
                app.state.saves.close_confirm(current);
            }
            // M256：统一出口（有动效时先退场）。「立即重画」这条要求由 `close` 里的
            // `refresh_view` 满足：退场的第一帧必须在这一拍画出来。
            close(app);
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
            // M298：提示有了渲染点之后，「推了不置脏」= 要等下一次事件才出现（拷贝成功尤其
            // 刺眼：用户盯着屏幕等一句"已复制"）。两条拷贝路径都在这里补上。
            app.view_dirty = true;
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
            // M298：同 `CopyPaths`（推了必须置脏，否则这一条要等下一次事件）。
            app.view_dirty = true;
            iced::clipboard::write(path.display().to_string())
        }
        // 第三个出口只在「Java 不符」那个框上画（上面那条臂带 `guard_for_current` 守卫）。
        // 别的框收到它就是空消息——不关框、不启动、不做任何事。
        DialogMessage::Tertiary => Task::none(),
    }
}

/// `Esc` 关弹窗（有弹窗时**先吃这一条**，不让它落到 AI 栏的 Esc 收起上）。
pub(super) fn escape(app: &mut App) -> bool {
    // M256：退场窗口里 Esc **也算被吃掉**。`on_dialog` 那道闸门管不到这里（Esc 走的是
    // `Message::Agent(Key)` 这条独立路径），而逻辑状态已经关掉 ⇒ 不加这一行，Esc 会落到
    // AI 栏去收起面板，用户看到的是「框还在缩、AI 栏先没了」。
    if motion::modal_closing() {
        return true;
    }
    let Some(current) = app.state.dialogs.current().map(|request| request.id) else {
        return false;
    };
    // M247（复审 r1 P1-1）：**三条关闭路径都要清**——次按钮/遮罩清了，`Esc` 这条以前漏了，
    // 于是"删存档 → Esc → 崩溃卡片 → 导出诊断包"会把那个世界静默删掉。
    // 现在还有 id 绑定兜一道（`take_confirm(current)`），这里是纵深防御的第二层。
    // 复审 r2 加固：清的时候**只清这个框自己的目标**（`close_confirm`），不碰别人的。
    app.state.saves.close_confirm(current);
    // M256：统一出口（Esc 也走退场动画，而不是硬跳）。
    close(app);
    true
}

#[cfg(test)]
mod tests;
