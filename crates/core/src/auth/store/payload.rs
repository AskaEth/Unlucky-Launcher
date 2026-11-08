//! 落盘细节：payload 形状、宽容解析、凭据文件权限。
//!
//! 两个文件的形状（都带 `schemaVersion`，便于以后演进）：
//!
//! ```json
//! // accounts.json —— 可公开
//! { "schemaVersion": 1, "accounts": [ { "id": "account:…", "kind": "offline",
//!   "profileId": "…", "profileName": "Steve", "createdAtMs": 1700000000000 } ] }
//!
//! // credentials.dat —— 凭据
//! { "schemaVersion": 1, "protection": "plain-v1",
//!   "entries": [ { "accountId": "account:…", "data": { "kind": "microsoft", … } } ] }
//! ```
//!
//! `entries[].data` 里是 [`Credentials`] 的序列化结果（由 serde 自己定义形状），
//! 本模块只负责「哪条属于哪个账户」。

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::auth::account::{AccountMeta, Credentials};
use crate::auth::error::{AuthError, AuthResult};
use crate::platform::atomic::write_atomic;

use super::{AuthPaths, AUTH_SCHEMA_VERSION, PROTECTION_PLAIN_V1};

/// 写两处文件：**先凭据、后元数据**。
pub(super) fn write(
    paths: &AuthPaths,
    accounts: &[AccountMeta],
    credentials: &BTreeMap<String, Credentials>,
) -> AuthResult<()> {
    let credentials_payload = credentials_payload(accounts, credentials);
    let path = paths.credentials_file();
    write_atomic(&path, credentials_payload.as_bytes())
        .map_err(|error| AuthError::store(path.display(), error.to_string()))?;
    restrict(&path)?;

    let accounts_payload = accounts_payload(accounts);
    let path = paths.accounts_file();
    write_atomic(&path, accounts_payload.as_bytes())
        .map_err(|error| AuthError::store(path.display(), error.to_string()))?;
    Ok(())
}

fn credentials_payload(
    accounts: &[AccountMeta],
    credentials: &BTreeMap<String, Credentials>,
) -> String {
    let mut entries = Vec::new();
    for meta in accounts {
        // 孤儿凭据（元数据已删）不写回。
        let Some(data) = credentials.get(meta.id.as_str()) else {
            continue;
        };
        entries.push(json!({
            "accountId": meta.id.as_str(),
            "data": data,
        }));
    }
    let payload = json!({
        "schemaVersion": AUTH_SCHEMA_VERSION,
        "protection": PROTECTION_PLAIN_V1,
        "entries": entries,
    });
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
}

fn accounts_payload(accounts: &[AccountMeta]) -> String {
    let payload = json!({
        "schemaVersion": AUTH_SCHEMA_VERSION,
        "accounts": accounts,
    });
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
}

/// 读元数据（坏记录跳过并记 warning）。
pub(super) fn read_accounts(path: &Path, warnings: &mut Vec<String>) -> Vec<AccountMeta> {
    let mut accounts = Vec::new();
    let Some(text) = read_to_string(path, warnings) else {
        return accounts;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        warnings.push(format!("{} 不是合法 JSON，按空账户库处理", path.display()));
        return accounts;
    };
    let Some(items) = value.get("accounts").and_then(Value::as_array) else {
        warnings.push(format!("{} 缺 accounts 数组", path.display()));
        return accounts;
    };
    for item in items {
        match AccountMeta::from_value(item) {
            Ok(meta) => accounts.push(meta),
            Err(error) => warnings.push(format!("跳过一条账户元数据：{}", error.message())),
        }
    }
    accounts
}

/// 读凭据（只保留 `known` 里存在的账户，孤儿丢弃并记 warning）。
pub(super) fn read_credentials(
    path: &Path,
    known: &[String],
    warnings: &mut Vec<String>,
) -> BTreeMap<String, Credentials> {
    let mut credentials = BTreeMap::new();
    let Some(text) = read_to_string(path, warnings) else {
        return credentials;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        warnings.push(format!(
            "{} 不是合法 JSON，按「无凭据」处理（账户列表仍可用）",
            path.display()
        ));
        return credentials;
    };
    if let Some(protection) = value.get("protection").and_then(Value::as_str) {
        if protection != PROTECTION_PLAIN_V1 {
            warnings.push(format!(
                "{} 的保护级别是 `{protection}`，本期只认 `{PROTECTION_PLAIN_V1}`",
                path.display()
            ));
        }
    }
    let Some(entries) = value.get("entries").and_then(Value::as_array) else {
        return credentials;
    };
    for entry in entries {
        let Some(account_id) = entry.get("accountId").and_then(Value::as_str) else {
            continue;
        };
        let account_id = account_id.to_ascii_lowercase();
        if !known.contains(&account_id) {
            warnings.push(format!("凭据 {account_id} 没有对应账户，已忽略"));
            continue;
        }
        let Some(data) = entry.get("data") else {
            continue;
        };
        match serde_json::from_value::<Credentials>(data.clone()) {
            Ok(parsed) => {
                credentials.insert(account_id, parsed);
            }
            Err(error) => warnings.push(format!("跳过一条凭据（{account_id}）：{error}")),
        }
    }
    credentials
}

fn read_to_string(path: &Path, warnings: &mut Vec<String>) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            warnings.push(format!("读 {} 失败：{error}", path.display()));
            None
        }
    }
}

/// 把凭据文件收紧到「仅本人可读写」。
///
/// POSIX 侧显式 `0600`；Windows 侧 `set_permissions` 只能表达只读位，凭据保护依赖
/// 用户目录默认 ACL（`data/auth/` 位于用户数据根下）。**这是已知缺口**：
/// 补一个 `platform::atomic::write_atomic_private` 比在 auth 里散写 cfg 更对，
/// 已作为 finding 报给 tower（见完成报告）。
fn restrict(path: &Path) -> AuthResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, permissions)
            .map_err(|error| AuthError::store(path.display(), error.to_string()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{accounts_payload, credentials_payload};
    use crate::auth::account::{AccountKind, AccountMeta, Credentials};
    use crate::auth::microsoft::MicrosoftSession;
    use crate::auth::secret::Secret;

    fn microsoft_meta() -> (AccountMeta, Credentials) {
        let meta = AccountMeta::new(
            AccountKind::Microsoft,
            "0f8fad5bd9cb469fa16570867728950e",
            "Steve",
            7,
        );
        let session = MicrosoftSession::new(
            "0f8fad5bd9cb469fa16570867728950e",
            "Steve",
            Secret::new("mc-access-token"),
            Secret::new("live-refresh-token"),
            3600,
            1_000,
            None,
        );
        (meta, Credentials::Microsoft(session))
    }

    #[test]
    fn metadata_payload_never_contains_tokens() {
        let (meta, creds) = microsoft_meta();
        let payload = accounts_payload(std::slice::from_ref(&meta));
        assert!(payload.contains("0f8fad5bd9cb469fa16570867728950e"));
        assert!(payload.contains("\"kind\": \"microsoft\""));
        assert!(!payload.contains("mc-access-token"));
        assert!(!payload.contains("live-refresh-token"));
        // 凭据对象的 Debug 也是脱敏的，别把 token 带进 payload。
        assert!(!format!("{creds:?}").contains("mc-access-token"));
    }

    #[test]
    fn credentials_payload_carries_protection_and_entries() {
        let (meta, creds) = microsoft_meta();
        let mut map = BTreeMap::new();
        map.insert(meta.id.as_str().to_string(), creds);
        let payload = credentials_payload(std::slice::from_ref(&meta), &map);
        assert!(payload.contains("\"protection\": \"plain-v1\""));
        assert!(payload.contains(meta.id.as_str()));
        assert!(payload.contains("mc-access-token"));
    }

    #[test]
    fn orphan_credentials_are_not_written() {
        let (meta, creds) = microsoft_meta();
        let mut map = BTreeMap::new();
        map.insert(
            "account:deadbeefdeadbeefdeadbeefdeadbeef".to_string(),
            creds,
        );
        let payload = credentials_payload(std::slice::from_ref(&meta), &map);
        assert!(!payload.contains("mc-access-token"));
        assert!(payload.contains("\"entries\": []"));
    }
}
