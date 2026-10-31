//! `list_accounts`：账户清单（**凭据永不出口**）。
//!
//! 这个工具存在的理由很具体：用户常问「我有哪几个号 / 这局用的是哪个号」。
//! 但 D4/D9 的凭据红线是硬的——工具结果会回灌模型、也可能被发往远端 LLM，
//! 所以：
//! 1. 端口只回 [`crate::port::AccountSummary`]（`AccountMeta` 的公开字段）；
//! 2. 本文件里**没有任何**能表达 token / 密码的类型（连字段名都不出现）；
//! 3. 工具结果再过一遍 [`crate::redact`]，兜住实现方把凭据塞进 `profile_name` 的意外。

use std::sync::Arc;

use serde_json::{json, Value};

use crate::port::AccountPort;
use crate::registry::schema::object;
use crate::registry::{
    RegistryError, ToolCtx, ToolError, ToolFn, ToolOutput, ToolRegistry, ToolSpec,
};

/// 工具名。
pub const LIST_ACCOUNTS: &str = "list_accounts";

/// 注册 `list_accounts`。
///
/// # Errors
/// 工具名重复/schema 非法时返回 [`RegistryError`]。
pub fn register(
    registry: &mut ToolRegistry,
    port: Arc<dyn AccountPort>,
) -> Result<(), RegistryError> {
    let handler: ToolFn = Arc::new(move |_ctx: &ToolCtx, _args: &Value| {
        let accounts = port.list_accounts().map_err(ToolError::Port)?;
        let count = accounts.len();
        let display: Vec<String> = accounts
            .iter()
            .map(|a| {
                if a.profile_name.trim().is_empty() {
                    format!("{}（{}）", a.id, a.kind)
                } else {
                    format!("{}（{}）", a.profile_name, a.kind)
                }
            })
            .collect();
        let mut content = json!({
            "count": count,
            "accounts": accounts,
            "credential_policy": "账户凭据（access token / 刷新令牌 / authlib 密码）永不经由 AI 工具出口",
        });
        // 兜底脱敏：端口实现若把敏感串塞进了公开字段，这里也不会带出去。
        content = crate::redact::redact_texts(&content);
        Ok(ToolOutput::ok(
            content,
            if count == 0 {
                "还没有任何账户".to_string()
            } else {
                format!("账户 {count} 个：{}", display.join("、"))
            },
        ))
    });

    registry.register(ToolSpec::read_only(
        LIST_ACCOUNTS,
        "列出启动器里已登录的账户（展示名 / 轨道 offline|microsoft|authlib / 档案 UUID / 创建时间）。**永不返回任何凭据**：令牌与密码不在工具结果的类型里，需要登录态请让用户在启动器界面查看。",
        object(&[], &[]),
        handler,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MockAccounts;

    fn registry(port: Arc<dyn AccountPort>) -> ToolRegistry {
        let mut registry = ToolRegistry::new();
        register(&mut registry, port).expect("注册应成功");
        registry
    }

    fn call(registry: &ToolRegistry) -> Value {
        let spec = registry.get(LIST_ACCOUNTS).expect("工具存在").clone();
        let ctx = ToolCtx {
            agent_session_id: "s1".to_string(),
            confirmed: false,
        };
        (spec.handler)(&ctx, &json!({})).expect("应成功").content
    }

    #[test]
    fn lists_accounts_without_credentials() {
        let content = call(&registry(Arc::new(MockAccounts::new())));
        assert_eq!(content["count"], json!(1));
        assert_eq!(content["accounts"][0]["profile_name"], json!("Steve"));
        // 只扫账户数据本身：`credential_policy` 那句说明文字里本来就写着 "token"。
        let fields = content["accounts"].to_string().to_lowercase();
        for forbidden in ["token", "password", "refresh", "secret"] {
            assert!(
                !fields.contains(forbidden),
                "账户字段含 `{forbidden}`：{fields}"
            );
        }
    }

    #[test]
    fn empty_list_is_not_an_error() {
        let content = call(&registry(Arc::new(MockAccounts::empty())));
        assert_eq!(content["count"], json!(0));
        assert_eq!(content["accounts"], json!([]));
    }
}
