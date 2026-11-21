//! 设置页「AI 助手」分组（M397 从 `host/settings.rs` 整段搬来，函数体逐字不动）。

use crate::dto::{SettingGroupView, SettingRowView};

use super::super::env::HostEnv;
use super::{flag, selected, text};

/// AI（D4）：开关/端点/模型/密钥状态。
/// **选项表直接取 schema 的 `AI_PROVIDERS`**（M174）：原先手写三项、漏了 `disabled`，于是被别的
/// 写者设成 `disabled` 的机器在这一页会显示成 `openai_compatible`（`selected()` 找不到就回 0）。
/// **密钥只显示状态不显示值**：`ai.api_key` 是 `locked` 凭据，进 DTO 就等于进事件流/日志。
///
/// ⚠️ 本组这五行在**可编的 iced 壳**里没有任何渲染路径（M390 核过）：`AppState::settings_groups`
/// 在 iced 侧只有 download 组按 schema 键读（`pages/settings/download.rs`），AI 分组是自定义表单
/// （`pages/settings/ai/`），它现读 `launcher.json`。所以这里的 `ai.yolo` 行已删（真控件在
/// `pages/settings/ai/toggles.rs::yolo_row`）。**别读成全仓**：退役的 Slint 树 `crates/app`（已 exclude、不可编）的
/// `ui/pages/settings.slint` 仍把 "ai" 组画成设置页、RowView 会把 toggle 转成
/// `PageState.setting-changed`；那棵树若复活，这一行会重新变成能点的控件。
pub(super) fn ai_group(env: &HostEnv) -> SettingGroupView {
    let providers = unlucky_core::setting::keys::AI_PROVIDERS;
    let provider = text(env, "ai.provider");
    let chosen = if provider.is_empty() {
        "openai_compatible"
    } else {
        provider.as_str()
    };
    let key_state = match text(env, "ai.api_key").trim().chars().count() {
        0 => "未配置（在 iced 设置页填写）".to_string(),
        chars => format!("已配置（{chars} 个字符）"),
    };
    SettingGroupView::new("ai", "AI 助手（D4）")
        .with(SettingRowView::toggle(
            "ai.enabled",
            "启用 AI 助手",
            "关闭后规则引擎诊断仍可用",
            flag(env, "ai.enabled", true),
        ))
        .with(SettingRowView::choice(
            "ai.provider",
            "模型端点",
            "OpenAI 兼容（含本地 llama.cpp）/ Anthropic / Ollama",
            providers,
            selected(providers, chosen),
        ))
        .with(SettingRowView::text(
            "ai.base_url",
            "端点 base_url",
            "如 http://127.0.0.1:8080/v1",
            &text(env, "ai.base_url"),
        ))
        .with(SettingRowView::text(
            "ai.model",
            "模型名",
            "如 deepseek-chat",
            &text(env, "ai.model"),
        ))
        .with(SettingRowView::read_only("API 密钥", &key_state))
}
