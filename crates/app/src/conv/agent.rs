//! AI 页与覆盖层：会话消息、对话框（一次一条）、Toast。

use crate::{ChatRole, DialogKind, UiChatMessage, UiDialog, UiToast};
use unlucky_presenter::dialogs as dlg;
use unlucky_presenter::dto::ChatRole as DtoChatRole;
use unlucky_presenter::state::AppState;
use unlucky_presenter::toast as toast_mod;

/// AI 会话消息。
pub fn chat(state: &AppState) -> Vec<UiChatMessage> {
    state
        .ai
        .messages
        .iter()
        .map(|message| UiChatMessage {
            role: match message.role {
                DtoChatRole::User => ChatRole::User,
                DtoChatRole::Assistant => ChatRole::Assistant,
                DtoChatRole::Tool => ChatRole::Tool,
                DtoChatRole::System => ChatRole::System,
            },
            text: message.text.clone().into(),
            tool_name: message
                .tool
                .as_ref()
                .map(|tool| tool.name.clone())
                .unwrap_or_default()
                .into(),
            tool_ok: message.tool.as_ref().is_none_or(|tool| tool.ok),
            streaming: message.streaming,
        })
        .collect()
}

/// 当前对话框（一次一条）+ 是否显示 + 队列剩余条数。
pub fn dialog(state: &AppState) -> (UiDialog, bool, i32) {
    let queued = i32::try_from(state.dialogs.pending_len()).unwrap_or(0);
    match state.dialogs.current() {
        None => (UiDialog::default(), false, queued),
        Some(request) => {
            let (kind, primary, secondary) = match &request.kind {
                dlg::DialogKind::Confirm { primary, secondary } => {
                    (DialogKind::Confirm, primary.clone(), secondary.clone())
                }
                dlg::DialogKind::Input { primary, .. } => {
                    (DialogKind::Input, primary.clone(), String::new())
                }
                dlg::DialogKind::Progress { .. } => {
                    (DialogKind::Progress, "取消".to_string(), String::new())
                }
                dlg::DialogKind::Error { primary, .. } => {
                    (DialogKind::Error, primary.clone(), "忽略".to_string())
                }
            };
            let progress = match &request.kind {
                dlg::DialogKind::Progress { progress, .. } => *progress,
                _ => 0.0,
            };
            let input_label = match &request.kind {
                dlg::DialogKind::Input { label, .. } => label.clone(),
                _ => String::new(),
            };
            let input_value = match &request.kind {
                dlg::DialogKind::Input { value, .. } => value.clone(),
                _ => String::new(),
            };
            (
                UiDialog {
                    id: request.id.to_string().into(),
                    kind,
                    title: request.title.clone().into(),
                    body: request.body.clone().into(),
                    primary: primary.into(),
                    secondary: secondary.into(),
                    progress,
                    input_label: input_label.into(),
                    input_value: input_value.into(),
                },
                true,
                queued,
            )
        }
    }
}

/// Toast 列表。
pub fn toasts(state: &AppState) -> Vec<UiToast> {
    state
        .toasts
        .items()
        .iter()
        .map(|toast| UiToast {
            id: toast.id.to_string().into(),
            kind: match toast.kind {
                toast_mod::ToastKind::Info => crate::ToastKind::Info,
                toast_mod::ToastKind::Success => crate::ToastKind::Success,
                toast_mod::ToastKind::Warn => crate::ToastKind::Warn,
                toast_mod::ToastKind::Error => crate::ToastKind::Error,
            },
            text: toast.text.clone().into(),
        })
        .collect()
}
