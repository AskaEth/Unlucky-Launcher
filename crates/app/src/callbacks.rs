//! 回调接线：把 `ui/main.slint` 暴露的每个回调翻译成「状态变更 + AppCommand + 同步」。
//!
//! 逐条列出而不做通用转发，是为了让「用户能做的每一件事」在这里一目了然；
//! 新增交互时只改这一处 + `.slint` 的 callback 声明。

use std::rc::Weak;

use slint::ComponentHandle;

use crate::conv;
use crate::ui_bind::Inner;
use unlucky_presenter::dto::AppCommand;
use unlucky_presenter::router::Route;
use unlucky_presenter::toast::ToastKind;

/// 接全部回调；闭包持弱引用，避免 `MainWindow` 与闭包互相持有导致窗口永不释放。
pub(crate) fn wire(weak: Weak<Inner>) {
    let Some(inner) = weak.upgrade() else { return };
    // 只在注册期间持有强引用；闭包内一律持弱引用（否则窗口与闭包互相持有、永不释放）。
    let ui = &inner.ui;

    wire_window(ui, &weak);
    wire_navigation(ui, &weak);
    wire_instances(ui, &weak);
    wire_download(ui, &weak);
    wire_logs(ui, &weak);
    wire_settings(ui, &weak);
    wire_overlays(ui, &weak);
}

fn wire_window(ui: &crate::MainWindow, weak: &Weak<Inner>) {
    {
        let weak = weak.clone();
        ui.on_toggle_maximize(move || {
            let Some(this) = weak.upgrade() else { return };
            let window = this.ui.window();
            window.set_maximized(!window.is_maximized());
        });
    }
    {
        let weak = weak.clone();
        ui.on_minimize(move || {
            if let Some(this) = weak.upgrade() {
                this.ui.window().set_minimized(true);
            }
        });
    }
    {
        let weak = weak.clone();
        ui.on_request_close(move || {
            if let Some(this) = weak.upgrade() {
                let _ = this.ui.hide();
            }
            let _ = slint::quit_event_loop();
        });
    }
}

fn wire_navigation(ui: &crate::MainWindow, weak: &Weak<Inner>) {
    {
        let weak = weak.clone();
        ui.on_navigate(move |route| {
            let Some(this) = weak.upgrade() else { return };
            if let Some(route) = conv::route_from(route) {
                this.state.borrow_mut().navigate(route);
                this.sync();
            }
        });
    }
    {
        let weak = weak.clone();
        ui.on_toggle_nav(move || {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().toggle_nav();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_toggle_ai(move || {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().toggle_ai();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_expand_ai(move || {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().navigate(Route::Agent);
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_dock_ai(move || {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().back();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_toggle_yolo(move || {
            let Some(this) = weak.upgrade() else { return };
            let enabled = {
                let mut state = this.state.borrow_mut();
                state.ai.yolo = !state.ai.yolo;
                state.ai.yolo
            };
            this.dispatch(AppCommand::SetYolo(enabled));
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_ai_send(move |text| {
            let Some(this) = weak.upgrade() else { return };
            let text = text.to_string();
            if text.trim().is_empty() {
                return;
            }
            // 不做本地回显：user 消息由 host 的 MessageAppended 事件带回（契约 §3）
            this.state.borrow_mut().ai.draft.clear();
            this.ui.set_ai_draft("".into());
            this.ask_agent(&text);
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_ai_stop(move || {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().ai.finish_stream();
            this.dispatch(AppCommand::StopAgent);
            this.sync();
        });
    }
}

fn wire_instances(ui: &crate::MainWindow, weak: &Weak<Inner>) {
    {
        let weak = weak.clone();
        ui.on_instance_launch(move |id| {
            let Some(this) = weak.upgrade() else { return };
            this.dispatch(AppCommand::LaunchInstance(id.to_string()));
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_instance_configure(move |id| {
            let Some(this) = weak.upgrade() else { return };
            // 齿轮键进实例详情（G4 占位页），实例级设置后续挂在该页 tab 上
            this.state
                .borrow_mut()
                .navigate(Route::Detail(id.to_string()));
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_instance_stop(move |id| {
            let Some(this) = weak.upgrade() else { return };
            this.dispatch(AppCommand::StopInstance(id.to_string()));
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_instance_open(move |id| {
            let Some(this) = weak.upgrade() else { return };
            this.state
                .borrow_mut()
                .navigate(Route::Detail(id.to_string()));
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_new_instance(move || {
            let Some(this) = weak.upgrade() else { return };
            // G5：打开向导（版本 → 加载器 → 名字 → 确认 → 安装进度）
            this.state.borrow_mut().wizard = unlucky_presenter::wizard::WizardState::open();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_instance_view_changed(move |view| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().prefs.instance_view = conv::instance_view_from(view);
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_tile_size_changed(move |size| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().prefs.tile_size = conv::tile_size_from(size);
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_instance_query_changed(move |query| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().instance_query = query.to_string();
            this.sync();
        });
    }
}

fn wire_download(ui: &crate::MainWindow, weak: &Weak<Inner>) {
    {
        let weak = weak.clone();
        ui.on_download_category_changed(move |category| {
            let Some(this) = weak.upgrade() else { return };
            let category = conv::version_category(category);
            this.state.borrow_mut().prefs.download_category = category;
            this.dispatch(AppCommand::ListVersions(category));
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_download_install(move |id| {
            let Some(this) = weak.upgrade() else { return };
            this.dispatch(AppCommand::InstallVersion(id.to_string()));
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_installed_only_changed(move |only| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().installed_only = only;
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_download_query_changed(move |query| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().download_query = query.to_string();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_ancient_help(move || {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().toasts.push(
                ToastKind::Info,
                "远古版（classic/alpha/beta）需旧版 Java 与旧启动参数；完整说明页 P1 接入",
            );
            this.sync();
        });
    }
}

fn wire_logs(ui: &crate::MainWindow, weak: &Weak<Inner>) {
    {
        let weak = weak.clone();
        ui.on_log_select(move |id| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().selected_session = Some(id.to_string());
            this.dispatch(AppCommand::OpenLogSession(id.to_string()));
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_log_ask_ai(move |id| {
            let Some(this) = weak.upgrade() else { return };
            let question = format!("帮我看下会话 {id} 为什么崩溃");
            this.state.borrow_mut().navigate(Route::Agent);
            this.ask_agent(&question);
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_log_apply_fix(move || {
            let Some(this) = weak.upgrade() else { return };
            this.state
                .borrow_mut()
                .toasts
                .push(ToastKind::Warn, "一键修复在 P1 接 diagnose 的 fix_action");
            this.sync();
        });
    }
}

fn wire_settings(ui: &crate::MainWindow, weak: &Weak<Inner>) {
    // 主题三态 + 外观四件在 appearance_bind.rs（拆文件守 400 行）
    crate::appearance_bind::wire(ui, weak);
    {
        let weak = weak.clone();
        ui.on_yolo_changed(move |enabled| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().ai.yolo = enabled;
            this.dispatch(AppCommand::SetYolo(enabled));
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_mcp_changed(move |_enabled| {
            let Some(this) = weak.upgrade() else { return };
            this.state
                .borrow_mut()
                .toasts
                .push(ToastKind::Info, "MCP server 开关在 P1 接 agent");
            this.sync();
        });
    }
    // P0 只保留入口：设置分组与 AI 端点暂不落状态（P1 接 setting schema）。
    ui.on_settings_section_changed(|_section| {});
    ui.on_ai_endpoint_changed(|_index| {});
}

fn wire_overlays(ui: &crate::MainWindow, weak: &Weak<Inner>) {
    {
        let weak = weak.clone();
        ui.on_dialog_primary(move || close_current_dialog(&weak));
    }
    {
        let weak = weak.clone();
        ui.on_dialog_secondary(move || close_current_dialog(&weak));
    }
    {
        let weak = weak.clone();
        ui.on_dialog_dismissed(move || close_current_dialog(&weak));
    }
    {
        let weak = weak.clone();
        ui.on_toast_dismissed(move |id| {
            let Some(this) = weak.upgrade() else { return };
            if let Ok(id) = id.parse::<u64>() {
                this.state.borrow_mut().toasts.dismiss(id);
                this.sync();
            }
        });
    }
}

fn close_current_dialog(weak: &Weak<Inner>) {
    let Some(this) = weak.upgrade() else { return };
    this.state.borrow_mut().dialogs.close_current();
    this.sync();
}
