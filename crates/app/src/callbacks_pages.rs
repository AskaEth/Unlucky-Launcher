//! `PageState` global 的回调接线：账户 / Java / 设置 / 通知 / 向导 / 崩溃导入 / 日志刷新。
//!
//! 与 `callbacks.rs` 同一纪律：每个回调 = 「改状态 + 发 AppCommand + 同步 UI」，一处看全。
//! 用 global 而不是逐层转发属性（见 `ui/page_state.slint` 顶部的取舍说明）。

use std::rc::Weak;

use slint::ComponentHandle;

use crate::conv;
use crate::dto::{AppCommand, CreateRequest, LoaderKind};
use crate::ui_bind::Inner;

/// 接全部页面状态回调。
pub(crate) fn wire_pages(weak: Weak<Inner>) {
    let Some(inner) = weak.upgrade() else { return };
    let page = inner.ui.global::<crate::PageState<'_>>();

    // ── 账户 ──────────────────────────────────────────────────
    {
        let weak = weak.clone();
        page.on_account_add_offline(move |name| {
            let name = name.to_string();
            if name.trim().is_empty() {
                return;
            }
            let Some(this) = weak.upgrade() else { return };
            this.dispatch(AppCommand::AddOfflineAccount(name));
            this.ui
                .global::<crate::PageState<'_>>()
                .set_offline_name("".into());
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_account_remove(move |id| {
            let Some(this) = weak.upgrade() else { return };
            this.dispatch(AppCommand::RemoveAccount(id.to_string()));
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_account_switch(move |id| {
            let Some(this) = weak.upgrade() else { return };
            this.dispatch(AppCommand::SwitchAccount(id.to_string()));
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_msa_begin(move || {
            let Some(this) = weak.upgrade() else { return };
            this.dispatch(AppCommand::StartDeviceLogin);
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_msa_cancel(move || {
            let Some(this) = weak.upgrade() else { return };
            this.dispatch(AppCommand::CancelDeviceLogin);
            this.sync();
        });
    }

    // ── Java ──────────────────────────────────────────────────
    {
        let weak = weak.clone();
        page.on_java_install(move |vendor| {
            let Some(this) = weak.upgrade() else { return };
            this.dispatch(AppCommand::InstallJava(vendor.to_string()));
            this.sync();
        });
    }

    // ── 设置 ──────────────────────────────────────────────────
    {
        let weak = weak.clone();
        page.on_setting_changed(move |key, value| {
            let Some(this) = weak.upgrade() else { return };
            this.dispatch(AppCommand::SetSetting {
                key: key.to_string(),
                value: value.to_string(),
            });
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_setting_action(move |action| {
            let Some(this) = weak.upgrade() else { return };
            let action = action.to_string();
            match action.as_str() {
                // 清 24h 地区缓存 → 下次下载重新判定
                "detect_region" => this.dispatch(AppCommand::SetSetting {
                    key: "download.action.detect_region".to_string(),
                    value: String::new(),
                }),
                // Java 分组的「安装/管理」→ 按回退链装一个托管 JDK
                "install_java" => this.dispatch(AppCommand::InstallJava(String::new())),
                _ => {}
            }
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_settings_tab_changed(move |tab| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().settings_tab = tab.to_string();
            this.sync();
        });
    }

    // ── 通知 ──────────────────────────────────────────────────
    {
        let weak = weak.clone();
        page.on_notice_dismiss(move |id| {
            let Some(this) = weak.upgrade() else { return };
            this.dispatch(AppCommand::DismissNotice(id.to_string()));
            this.sync();
        });
    }

    // ── 向导 ──────────────────────────────────────────────────
    {
        let weak = weak.clone();
        page.on_wizard_open(move || {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().wizard = crate::wizard::WizardState::open();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_wizard_close(move || {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().wizard.open = false;
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_wizard_select_version(move |version| {
            let Some(this) = weak.upgrade() else { return };
            this.state
                .borrow_mut()
                .wizard
                .select_version(version.as_ref());
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_wizard_select_loader(move |index| {
            let Some(this) = weak.upgrade() else { return };
            let loader = match index {
                1 => LoaderKind::Fabric,
                2 => LoaderKind::Quilt,
                3 => LoaderKind::Forge,
                4 => LoaderKind::NeoForge,
                _ => LoaderKind::Vanilla,
            };
            this.state.borrow_mut().wizard.select_loader(loader);
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_wizard_name_changed(move |name| {
            let Some(this) = weak.upgrade() else { return };
            let mut state = this.state.borrow_mut();
            state.wizard.name = name.to_string();
            state.wizard.error.clear();
            drop(state);
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_wizard_next(move || {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().wizard.advance();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_wizard_back(move || {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().wizard.step_back();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_wizard_confirm(move || {
            let Some(this) = weak.upgrade() else { return };
            let request = {
                let state = this.state.borrow();
                let wizard = &state.wizard;
                wizard.ready_to_submit().map(|()| CreateRequest {
                    version: wizard.version.trim().to_string(),
                    loader: wizard.loader,
                    loader_version: wizard.loader_version.trim().to_string(),
                    name: wizard.name.trim().to_string(),
                })
            };
            match request {
                Ok(request) => {
                    this.dispatch(AppCommand::CreateInstance(request));
                    this.state.borrow_mut().wizard.submitted();
                }
                Err(error) => {
                    this.state.borrow_mut().wizard.error = error;
                }
            }
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_install_cancel(move || {
            let Some(this) = weak.upgrade() else { return };
            this.dispatch(AppCommand::CancelInstall);
            this.sync();
        });
    }

    // ── 日志页 ────────────────────────────────────────────────
    {
        let weak = weak.clone();
        page.on_crash_import(move |path| {
            let Some(this) = weak.upgrade() else { return };
            let path = path.to_string();
            if path.trim().is_empty() {
                return;
            }
            this.dispatch(AppCommand::ImportCrashLog(path));
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        page.on_logs_refresh(move || {
            let Some(this) = weak.upgrade() else { return };
            this.dispatch(AppCommand::RefreshLogSessions);
            this.sync();
        });
    }

    // 设置页分组切换同时刷新一遍分组快照（外部改配置后手点分组即最新）
    let _ = conv::setting_groups;
}
