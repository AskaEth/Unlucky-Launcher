//! 「导入已有版本」向导的落点（M223 GUIADOPT）。
//!
//! ## 四段各自发什么、等什么
//!
//! | 用户动作 | 发什么 | 等什么回程 |
//! |---|---|---|
//! | 页头「导入」/ [`AdoptMessage::Open`] | `AdoptCommand::Scan` | `AdoptEvent::Scan` |
//! | 选目标 + 点「预检」/ [`AdoptMessage::Check`] | `AdoptCommand::Preview` | `Planned` / `Failed` |
//! | 点「确认导入」/ [`AdoptMessage::Confirm`] | `AdoptCommand::Confirm` | `Done` / `Failed` |
//! | 「返回」/ [`AdoptMessage::Back`] | `AdoptCommand::Scan`（重扫一次，顺手清掉预检/失败） | `Scan` |
//! | 「关闭」/ [`AdoptMessage::Close`] | `AdoptCommand::Dismiss` | `Cleared` |
//!
//! ## 三段纪律
//!
//! 1. **不在本地判「能不能导入」**：候选可不可用、落点在哪、有哪些问题全部来自 presenter 的回程
//!    （`AdoptState`）。这里只把「用户点了什么」翻成命令；
//! 2. **确认只发用户填过的那份请求**（版本 + 名字 + 目标档），预检与确认**同一份**——
//!    于是「预检 A、写 B」这种错位不可能发生；
//! 3. **在等回程时把主按钮并掉**（`Form::busy`）：连点两次「确认」不会写两条实例记录。

use iced::Task;
use unlucky_presenter::dto::{AdoptCommand, AdoptRequestView, AppCommand};

use crate::app::adopt::{AdoptMessage, Busy};

use super::super::message::Message;
use super::super::state::App;

impl App {
    /// 向导的一条消息。
    pub(super) fn on_adopt(&mut self, message: AdoptMessage) -> Task<Message> {
        match message {
            AdoptMessage::Open => {
                self.adopt_form.open();
                self.dispatch_adopt(AdoptCommand::Scan);
            }
            AdoptMessage::Close => {
                // 登记在飞时不吃「关闭」：core 那条线程已经开跑了，写了盘却不回报 Done
                // 就会出现「实例已存在但界面什么都没说」——等它交卷（几十毫秒级）。
                if self.adopt_form.busy == Busy::Importing {
                    return Task::none();
                }
                self.adopt_form.close();
                self.dispatch_adopt(AdoptCommand::Dismiss);
            }
            AdoptMessage::Select(version_id) => {
                self.adopt_form.selected = Some(version_id);
            }
            AdoptMessage::Name(name) => self.adopt_form.name = name,
            AdoptMessage::Target(target) => {
                // 登记在飞时不吃「换档」：它会派一条零写盘的预览把 `adopt_epoch` +1，
                // 于是在飞的 Confirm 交卷时 Done 被代次丢掉——实例写进去了，界面什么都没说。
                // 连本地档位也不改：界面上显示 A 档、盘上写的是 B 档，是最糟的一种不一致。
                if self.adopt_form.busy == Busy::Importing {
                    return Task::none();
                }
                self.adopt_form.target = target;
                // 换了目标档 ⇒ 上一档的预检作废（确认按钮不该继续亮着）
                self.adopt_form.checked = false;
                // 共享档 / 版本目录档会写进**别人的目录**：一被点选就把后果原文拿出来
                // （零写盘的预检，见 `Form::stage` 的收窄：这一次结果落在 Pick 段当提示读）。
                if target.touches_foreign_dir() {
                    if let Some(request) = self.adopt_request() {
                        self.adopt_form.busy = Busy::Checking;
                        self.dispatch_adopt(AdoptCommand::Preview(Box::new(request)));
                    }
                }
            }
            AdoptMessage::Check => {
                if let Some(request) = self.adopt_request() {
                    self.adopt_form.checked = true;
                    self.adopt_form.busy = Busy::Checking;
                    self.dispatch_adopt(AdoptCommand::Preview(Box::new(request)));
                }
            }
            AdoptMessage::Confirm => {
                if let Some(request) = self.adopt_request() {
                    self.adopt_form.busy = Busy::Importing;
                    self.dispatch_adopt(AdoptCommand::Confirm(Box::new(request)));
                }
            }
            AdoptMessage::Back => {
                // 同 Close / Target：登记在飞时「返回」会把 `adopt_epoch` +1（它派 Scan），
                // 在飞 Confirm 的 Done 就再也到不了界面。
                if self.adopt_form.busy == Busy::Importing {
                    return Task::none();
                }
                // 退回候选列表 = 重新扫一次：`scan` 会把上一轮的预检/失败一起作废，
                // 于是界面回到 Pick 段，不需要再给 core 加一个「清失败」的命令。
                self.adopt_form.checked = false;
                self.adopt_form.busy = Busy::Scanning;
                self.dispatch_adopt(AdoptCommand::Scan);
            }
        }
        Task::none()
    }

    /// 回程到了：这一条命令已经交卷（`pump` 收到 `CoreEvent::Adopt` 时调，见 `update/clock.rs`）。
    /// 出图通道的种子（`ULL_ADOPT_STATE`，与 `ULL_JAVA_STATE` / `ULL_AI_STATE` 同一族）。
    ///
    /// **只在 `--screenshot` 下生效**：交互模式读这个环境变量没有意义（也没有人会设它）。
    /// 不认识的名字一律忽略，退回正常启动路径（向导保持关闭）。
    pub(in crate::app) fn hydrate_adopt_state(&mut self) {
        if self.options.screenshot.is_none() {
            return;
        }
        let Ok(raw) = std::env::var(crate::app::adopt::shot::ENV) else {
            return;
        };
        if let Some((form, state)) = crate::app::adopt::shot::seed(&raw) {
            self.adopt_form = form;
            self.state.adopt = state;
        }
    }

    pub(super) fn settle_adopt(&mut self) {
        self.adopt_form.settled();
    }

    /// 当前表单能拼出的请求（没选版本就没有请求——按钮在那边也是禁用的）。
    fn adopt_request(&self) -> Option<AdoptRequestView> {
        let version_id = self.adopt_form.selected.clone()?;
        let name = self.adopt_form.name.trim();
        Some(AdoptRequestView {
            version_id,
            name: (!name.is_empty()).then(|| name.to_string()),
            target: self.adopt_form.target,
        })
    }

    fn dispatch_adopt(&mut self, command: AdoptCommand) {
        self.dispatch(AppCommand::Adopt(Box::new(command)));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use unlucky_design::color::SchemeFlavor;
    use unlucky_presenter::dto::{
        AdoptCommand, AdoptEvent, AdoptTargetView, AppCommand, AppEvent, Backend,
    };
    use unlucky_presenter::mock::MockBackend;
    use unlucky_presenter::settings_bridge::SettingsBridge;

    use crate::app::adopt::{AdoptMessage, Busy};
    use crate::app::state::App;
    use crate::cli::Options;
    use crate::theme::Families;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    fn headless() -> App {
        App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(MockBackend::new()),
            SettingsBridge::memory_only(),
        )
    }

    /// 记账后端：把 `dispatch` 收到的命令留在台账里（`MockBackend` 只认自己那几条，
    /// 「有没有派出去」根本看不见——而 P2-1 的判据正是「不许派」）。
    #[derive(Debug, Default)]
    struct RecordingBackend {
        dispatched: Rc<RefCell<Vec<AppCommand>>>,
    }

    impl Backend for RecordingBackend {
        fn drain_events(&mut self) -> Vec<AppEvent> {
            Vec::new()
        }

        fn dispatch(&mut self, command: AppCommand) {
            self.dispatched.borrow_mut().push(command);
        }
    }

    /// 带台账的 App（返回台账句柄，测试直接查「派了什么」）。
    fn recording() -> (App, Rc<RefCell<Vec<AppCommand>>>) {
        let dispatched: Rc<RefCell<Vec<AppCommand>>> = Rc::new(RefCell::new(Vec::new()));
        let app = App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(RecordingBackend {
                dispatched: Rc::clone(&dispatched),
            }),
            SettingsBridge::memory_only(),
        );
        (app, dispatched)
    }

    /// 台账里有没有派过某条 adopt 命令（只认命令种类，不看载荷内容）。
    ///
    /// 写法是 `let ... else` + `matches!`：内层用 `match` 会被 clippy 判成
    /// `match expression looks like matches! macro`（deny 级，CI 的 `-D warnings` 直接红）。
    fn dispatched_adopt(ledger: &Rc<RefCell<Vec<AppCommand>>>, want: &str) -> bool {
        ledger.borrow().iter().any(|command| {
            let AppCommand::Adopt(command) = command else {
                return false;
            };
            matches!(
                (&**command, want),
                (AdoptCommand::Scan, "scan")
                    | (AdoptCommand::Preview(_), "preview")
                    | (AdoptCommand::Confirm(_), "confirm")
                    | (AdoptCommand::Dismiss, "dismiss")
            )
        })
    }

    /// 打开向导 = 表单进入「在等扫描」+ 派一条 `Scan`。
    ///
    /// 抓得住：`Open` 没派命令（面板永远停在「正在扫描」）、或忘了置 `Busy::Scanning`（没有等待态）。
    /// 抓不住：扫描真的返回了什么（那在 presenter 的 `host/adopt` 用例里）。
    #[test]
    fn opening_the_wizard_asks_for_a_scan() {
        let mut app = headless();
        let _ = app.on_adopt(AdoptMessage::Open);
        assert!(app.adopt_form.open);
        assert_eq!(app.adopt_form.busy, Busy::Scanning);
        assert_eq!(
            app.adopt_form.target,
            AdoptTargetView::InstanceDir,
            "缺省不是 isolated"
        );
    }

    /// 没选版本时「预检 / 确认」不派命令（否则会发出一条 `version_id` 为空的请求）。
    ///
    /// 抓得住：`adopt_request()` 在没有选中项时仍拼出请求（空版本 id 打到 core 换一句
    /// `adopt_version_not_found`，用户看到莫名其妙的失败）。
    #[test]
    fn no_request_without_a_selected_version() {
        let mut app = headless();
        let _ = app.on_adopt(AdoptMessage::Open);
        let _ = app.on_adopt(AdoptMessage::Check);
        assert_eq!(app.adopt_form.busy, Busy::Scanning, "Check 不该改等待态");
        let _ = app.on_adopt(AdoptMessage::Confirm);
        assert_eq!(app.adopt_form.busy, Busy::Scanning);
    }

    /// 选中 + 预检 = 一条 Preview 命令，且请求里的名字是**去过空白**的用户输入。
    #[test]
    fn check_sends_the_form_as_a_preview_request() {
        let mut app = headless();
        let _ = app.on_adopt(AdoptMessage::Open);
        let _ = app.on_adopt(AdoptMessage::Select("1.20.1".to_string()));
        let _ = app.on_adopt(AdoptMessage::Name("  生存  ".to_string()));
        let _ = app.on_adopt(AdoptMessage::Target(AdoptTargetView::Shared));
        let _ = app.on_adopt(AdoptMessage::Check);
        assert_eq!(app.adopt_form.busy, Busy::Checking);
        // 请求本体不在 App 上留副本：能证明意图的只有等待态与目标档
        assert_eq!(app.adopt_form.target, AdoptTargetView::Shared);
    }

    /// 回程到了就解除等待（`settle_adopt` 由 `pump` 调）。
    #[test]
    fn an_incoming_event_settles_the_wait() {
        let mut app = headless();
        let _ = app.on_adopt(AdoptMessage::Open);
        assert!(app.adopt_form.busy.waiting());
        app.settle_adopt();
        assert_eq!(app.adopt_form.busy, Busy::Idle);
        // 事件真相进了 presenter 的状态（界面从它取词）
        app.state.apply(unlucky_presenter::dto::AppEvent::Core(
            unlucky_presenter::dto::CoreEvent::Adopt(Box::new(AdoptEvent::Cleared)),
        ));
        assert!(app.state.adopt.scan.is_none());
    }

    /// 共享档一被点选就发一条**预检**（零写盘），但**不**置 `checked`——确认页要用户点「预检」。
    ///
    /// 抓得住：`Target` 分支忘了发预检（后果原文永远不出现，契约 §5.2 失守）、
    /// 或者顺手把 `checked` 也置上（点一下档位就跳到确认页）。
    #[test]
    fn choosing_a_foreign_target_previews_but_waits_for_the_user() {
        let mut app = headless();
        let _ = app.on_adopt(AdoptMessage::Open);
        let _ = app.on_adopt(AdoptMessage::Select("1.20.1".to_string()));
        let _ = app.on_adopt(AdoptMessage::Target(AdoptTargetView::Shared));
        assert_eq!(app.adopt_form.busy, Busy::Checking);
        assert!(!app.adopt_form.checked, "确认页要等用户点「预检」");

        // 缺省档不触发自动预检（它不会写进别人的目录）
        let _ = app.on_adopt(AdoptMessage::Target(AdoptTargetView::InstanceDir));
        assert!(!app.adopt_form.checked);
    }

    /// 登记在飞时「关闭」被忽略：core 已经在写了，关掉面板只会让 Done 无处可去。
    ///
    /// 抓得住：Close 无条件清表单并 `Dismiss`（实例写成功了，界面却什么都没说）。
    #[test]
    fn closing_is_ignored_while_the_registration_is_in_flight() {
        let mut app = headless();
        let _ = app.on_adopt(AdoptMessage::Open);
        let _ = app.on_adopt(AdoptMessage::Select("1.20.1".to_string()));
        let _ = app.on_adopt(AdoptMessage::Check);
        app.adopt_form.busy = Busy::Importing;
        let _ = app.on_adopt(AdoptMessage::Close);
        assert!(app.adopt_form.open, "登记在飞时不许把面板关掉");
    }

    /// **P2-1**：登记在飞时，除了 Close，「返回」与「换目标档」也必须早退。
    ///
    /// 三条都是同一条病：它们都会派一条新命令把 `adopt_epoch` +1，于是在飞 Confirm 的
    /// `Done` 回来时被代次丢掉 —— 实例**写进去了**，界面却什么都没说（Close 那条早就守住了，
    /// 唯独这两支漏了）。
    ///
    /// 抓得住：`Back` 派了 `Scan`（旧 `Done` 被丢）、`Target` 派了 `Preview`（同上），
    /// 以及「守卫写了但写在改本地状态之后」（表单被改成新档位，而盘上写的是旧档位）。
    /// 抓不住：host 侧代次比较的真实时序（那在 presenter 的 `host/adopt` 那层）。
    #[test]
    fn back_and_target_are_ignored_while_the_registration_is_in_flight() {
        let (mut app, ledger) = recording();
        let _ = app.on_adopt(AdoptMessage::Open);
        let _ = app.on_adopt(AdoptMessage::Select("1.20.1".to_string()));
        let _ = app.on_adopt(AdoptMessage::Target(AdoptTargetView::InstanceDir));
        // 走到「确认」那一步：真的派一条 Confirm 出去
        let _ = app.on_adopt(AdoptMessage::Check);
        let _ = app.on_adopt(AdoptMessage::Confirm);
        assert!(
            dispatched_adopt(&ledger, "confirm"),
            "前提：确认要真的派出去"
        );
        assert_eq!(app.adopt_form.busy, Busy::Importing);
        ledger.borrow_mut().clear();

        // 在飞期间点「返回」：不许派 Scan
        let _ = app.on_adopt(AdoptMessage::Back);
        assert!(
            !dispatched_adopt(&ledger, "scan"),
            "登记在飞时 Back 不许派 Scan（会把 Done 的代次顶掉）"
        );
        assert_eq!(app.adopt_form.busy, Busy::Importing, "等待态不许被改掉");

        // 在飞期间换目标档：不许派 Preview，也不许改本地档位
        let _ = app.on_adopt(AdoptMessage::Target(AdoptTargetView::Shared));
        assert!(
            !dispatched_adopt(&ledger, "preview"),
            "登记在飞时换档不许派 Preview（会把 Done 的代次顶掉）"
        );
        assert_eq!(
            app.adopt_form.target,
            AdoptTargetView::InstanceDir,
            "挡下来的点击不该改出界面与盘上不一致的档位"
        );
        assert_eq!(app.adopt_form.busy, Busy::Importing);
    }
}
