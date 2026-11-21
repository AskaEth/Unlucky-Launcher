//! **M361 r2（P2-2）**：`Select` 必须派一次滚动（「点一条结果 ⇒ 详情进视野」的接线判据）。
//!
//! ## 为什么判据挂在 `units()` 上
//!
//! `Task::units()`（`iced_runtime::task`）是任务条数：`Task::none()` 是 0，`Task::widget(..)`
//! 是 1。修前 `Select` 落到函数末尾的 `Task::none()` ⇒ **0 条**；修后派 `snap_to_detail()` ⇒ 1 条。
//! 这条抓得住「把那行 `return` 删掉」（回到 0 = 点结果什么都不动，详情留在一屏之外）。
//!
//! **抓不住**：这一条验证不了「滚动真的把详情带进视野」——那是 iced 运行期的事，单元测试里没有
//! 渲染器与布局（`Element::operate` 要 `&Renderer`）。视野的**算式**判据在
//! `pages::content::tests::m361::selecting_a_result_puts_the_detail_in_view`；像素证据见报告 §4。

use std::cell::RefCell;
use std::rc::Rc;

use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::dto::{AppCommand, AppEvent, Backend};
use unlucky_presenter::router::Route;
use unlucky_presenter::settings_bridge::SettingsBridge;

use crate::app::state::App;
use crate::cli::Options;
use crate::pages::content::Message as ContentMessage;
use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 只记命令、永不回事件的后端（本条不关心网络）。
#[derive(Debug, Default)]
struct Recorder {
    dispatched: Rc<RefCell<Vec<AppCommand>>>,
}

impl Backend for Recorder {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        Vec::new()
    }

    fn dispatch(&mut self, command: AppCommand) {
        self.dispatched.borrow_mut().push(command);
    }
}

/// 造一个停在内容库页的 App（与 `app/update/instances/scan_phase.rs` 同一个夹具口径）。
fn app() -> App {
    let mut app = App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(Recorder::default()),
        SettingsBridge::memory_only(),
    );
    app.state.navigate(Route::Content);
    app.refresh_view();
    app
}

/// 修前是 `Task::none()`（0 条）⇒ 点结果不会动滚动位置，详情留在列表末尾（可能几个视口之外）。
#[test]
fn selecting_a_result_dispatches_the_scroll_to_the_detail_tail() {
    let mut app = app();
    let task = app.on_content(ContentMessage::Select("sodium-0".to_string()));

    assert_eq!(
        task.units(),
        1,
        "Select 必须派一次滚动操作（`parts::snap_to_detail`），否则详情尾块进不了视野"
    );
    // 选中态照旧落到页面模型（滚动不该顶替 M162 的状态变更）
    assert_eq!(app.content.selected.as_deref(), Some("sodium-0"));
}

/// 其余意图**不**该顺带滚动：滚动是 `Select` 独有的动作。
#[test]
fn only_selecting_scrolls() {
    let mut app = app();
    for message in [
        ContentMessage::SetTarget("survival".to_string()),
        ContentMessage::Search,
        ContentMessage::LoadMore,
        ContentMessage::Retry,
    ] {
        assert_eq!(
            app.on_content(message).units(),
            0,
            "只有 Select 该派滚动，其它意图顺手滚一下就是「点哪都跳」"
        );
    }
}
