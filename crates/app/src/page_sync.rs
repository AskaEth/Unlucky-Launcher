//! `PageState` global 的状态灌入（从 `ui_bind` 拆出，守 400 行上限）。
//!
//! 一个函数只做「AppState → Slint global 属性」的搬运，同步发生在每一拍末尾
//! （`ui_bind::Inner::sync`）。分文件而不是砍内容：这些字段每一行都对应一个页面元素。

use crate::conv;
use crate::state::AppState;
use crate::ui_bind::model;
use crate::{MainWindow, PageState};
use slint::ComponentHandle;

/// 把页面状态灌进 `PageState`。
pub(crate) fn apply(ui: &MainWindow, state: &AppState) {
    // 导航底部账户 + PageState 全量页面状态（账户/Java/安装/设置/通知/向导）
    ui.set_account_name(conv::active_account_name(state).into());
    let page = ui.global::<PageState<'_>>();
    page.set_accounts(model(conv::accounts(state)));
    page.set_active_account(conv::active_account_name(state).into());
    page.set_device(conv::device(state));
    page.set_java_vendors(model(conv::java_vendors(state)));
    page.set_java_chain(model(conv::java_chain(state)));
    page.set_java_runtimes(model(conv::java_runtimes(state)));
    page.set_java_order(state.java.order_text.clone().into());
    page.set_java_region(state.java.region.clone().into());
    page.set_java_priority_source(state.java.priority_source.clone().into());
    page.set_java_message(state.java.message.clone().into());
    page.set_java_busy(state.java.busy);
    let major = state.java.majors().last().copied().unwrap_or(17);
    page.set_java_target_major(i32::try_from(major).unwrap_or(17));
    page.set_install(conv::install(state));
    page.set_setting_groups(model(conv::setting_groups(state)));
    page.set_setting_tab(state.settings_tab.clone().into());
    page.set_notices(model(conv::notices(state)));
    page.set_wizard(conv::wizard(state));
    page.set_wizard_versions(model(conv::wizard_versions(state)));
}
