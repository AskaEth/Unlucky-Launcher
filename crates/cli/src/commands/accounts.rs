//! `ull accounts list|add|remove`：账户元数据操作，**凭据永不打印**。
//!
//! 设计约束（D9/Q21 + D4 凭据红线）：
//! - 只开四条路径：`list` / `add offline <name>` / `add msa` / `remove <name>`；
//!   authlib 的密码登录**故意不开 CLI 通道**（密码进 argv 会直接进 shell 历史与
//!   `ps` 输出，破坏红线；留给 GUI 输入框）；
//! - 输出只经 `AccountMeta`（可公开字段），`Report`/`--json` 里没有任何 token 字段；
//! - device-code 的 `user_code`/URL 打到 **stderr**（不管 `--quiet` 都要打：不打用户没法完成登录）。

use serde_json::json;
use unlucky_core::auth::account::{AccountKind, AccountMeta, Credentials};
use unlucky_core::auth::microsoft::{now_ms, MicrosoftAuth};
use unlucky_core::auth::offline::offline_meta;
use unlucky_core::auth::{AccountStore, AuthJournal};

use crate::cli::{AccountsAddArgs, AccountsAddKind};
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{progress_line, Report};

/// `accounts list`：列出账户（名称/轨道/UUID/创建时间）。
///
/// # Errors
///
/// 数据根解析失败（在 `Ctx`）或读盘 IO 失败。
pub fn list(ctx: &Ctx) -> Result<Report, CliError> {
    let store = open_store(ctx);
    let mut human = Vec::new();
    if store.is_empty() {
        human.push("还没有账户。".to_string());
        human.push("  离线：ull accounts add offline <玩家名>".to_string());
        human.push("  微软：ull accounts add msa（设备码交互式登录）".to_string());
    } else {
        human.push(format!("账户（{} 个）：", store.accounts().len()));
        for meta in store.accounts() {
            human.push(format!(
                "  {} · {} · {}",
                meta.display_name(),
                meta.kind.label(),
                meta.profile_id
            ));
        }
    }
    for warning in store.warnings() {
        human.push(format!("提示: {warning}"));
    }
    let accounts: Vec<serde_json::Value> = store.accounts().iter().map(account_json).collect();
    let data = json!({
        "accounts": accounts,
        "count": accounts.len(),
        "auth_dir": store.paths().root().display().to_string(),
        "warnings": store.warnings(),
    });
    Ok(Report::new(data, human))
}

/// `accounts add offline <name>`。
///
/// # Errors
///
/// 名字不合法、同名账户已存在、落盘失败。
pub fn add(ctx: &Ctx, args: &AccountsAddArgs) -> Result<Report, CliError> {
    match args.kind {
        AccountsAddKind::Offline => add_offline(ctx, args.name.as_deref().unwrap_or_default()),
        AccountsAddKind::Microsoft => add_microsoft(ctx),
    }
}

fn add_offline(ctx: &Ctx, name: &str) -> Result<Report, CliError> {
    let meta = offline_meta(name, now_ms()).map_err(super::map_auth_error)?;
    let mut store = open_store(ctx);
    store
        .insert(meta.clone(), Credentials::Offline)
        .map_err(super::map_auth_error)?;
    store.save().map_err(super::map_auth_error)?;
    let human = vec![
        format!(
            "已添加离线账户 {}（HMCL 兼容 UUID {}）",
            meta.display_name(),
            meta.profile_id
        ),
        format!("账户文件：{}", store.paths().accounts_file().display()),
    ];
    Ok(Report::new(added_json(&meta), human))
}

/// `accounts add msa`：device-code 交互式登录。
fn add_microsoft(ctx: &Ctx) -> Result<Report, CliError> {
    // 失败日志落**本次命令的实际数据根**（`ctx.layout().logs()`）：`MicrosoftAuth::from_env()`
    // 默认挂的是便携根（exe 同级 `.ULL/logs`），`--data-dir` / launcher.json 的 standard 模式下
    // 那是别人的目录，排障时找不到 auth.jsonl（M38 遗留 finding）。
    let auth = MicrosoftAuth::from_env()
        .map_err(super::map_auth_error)?
        .with_journal(Some(AuthJournal::in_logs_dir(ctx.layout().logs())));
    // 设备码要先给用户看，再开始等：core 的 `present` 回调正好卡在这个时间点。
    let session = unlucky_core::download::blocking::block_on(auth.device_code_login(|prompt| {
        let (headline, code) = prompt.lines();
        progress_line(&headline);
        progress_line(&code);
    }))
    .map_err(super::map_auth_error)?;

    let meta = AccountMeta::new(
        AccountKind::Microsoft,
        session.profile_id.clone(),
        session.profile_name.clone(),
        now_ms(),
    );
    let mut store = open_store(ctx);
    store
        .insert(meta.clone(), Credentials::Microsoft(session))
        .map_err(super::map_auth_error)?;
    store.save().map_err(super::map_auth_error)?;
    let human = vec![
        format!(
            "已添加微软账户 {}（档案 UUID {}）",
            meta.display_name(),
            meta.profile_id
        ),
        format!("账户文件：{}", store.paths().accounts_file().display()),
    ];
    Ok(Report::new(added_json(&meta), human))
}

/// `accounts remove <name>`。
///
/// # Errors
///
/// 找不到该账户、落盘失败。
pub fn remove(ctx: &Ctx, name: &str) -> Result<Report, CliError> {
    let mut store = open_store(ctx);
    let meta = store.remove(name).map_err(super::map_auth_error)?;
    store.save().map_err(super::map_auth_error)?;
    let human = vec![format!(
        "已删除账户 {}（{}）",
        meta.display_name(),
        meta.kind.label()
    )];
    let data = json!({
        "removed": account_json(&meta),
        "count": store.accounts().len(),
    });
    Ok(Report::new(data, human))
}

fn open_store(ctx: &Ctx) -> AccountStore {
    AccountStore::open(ctx.auth_data())
}

fn added_json(meta: &AccountMeta) -> serde_json::Value {
    json!({
        "account": account_json(meta),
        "account_id": meta.id.as_str(),
    })
}

/// 账户元数据 → JSON（**只含可公开字段**）。
fn account_json(meta: &AccountMeta) -> serde_json::Value {
    json!({
        "name": meta.display_name(),
        "profile_name": meta.profile_name,
        "id": meta.id.as_str(),
        "kind": meta.kind.as_str(),
        "profile_id": meta.profile_id,
        "login_name": meta.login_name,
        "server_url": meta.server_url,
        "created_at_ms": meta.created_at_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::account_json;
    use unlucky_core::auth::account::{AccountKind, AccountMeta};

    #[test]
    fn account_json_has_no_credential_fields() {
        let mut meta = AccountMeta::new(
            AccountKind::Microsoft,
            "0f8fad5bd9cb469fa16570867728950e",
            "Steve",
            7,
        );
        meta.login_name = Some("me@example.com".to_string());
        let value = account_json(&meta);
        let keys: Vec<&str> = value
            .as_object()
            .expect("对象")
            .keys()
            .map(String::as_str)
            .collect();
        for forbidden in ["access_token", "refresh_token", "token", "password"] {
            assert!(!keys.contains(&forbidden), "不该出现 {forbidden}");
        }
        assert_eq!(value["name"], "Steve");
        assert_eq!(value["kind"], "microsoft");
    }
}
