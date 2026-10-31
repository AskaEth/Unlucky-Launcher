//! `ull launch` 的账户选择：三轨账户 → [`AuthPlaceholders`]（唯一的身份出口）。
//!
//! 选择规则（headless 没有交互，必须确定）：
//! - `--account <name>`：按玩家名 / 登录名 / 条目 id 精确找（大小写不敏感）；
//! - 不给 `--account`：**唯一账户自动选中**；零账户回落离线占位（`--username` 或 `Player`，
//!   与 M21 的行为一致，保证没有账户时 `ull launch` 照旧能跑）；多账户报错并列出候选
//!   （猜一个等于让用户拿错身份进服务器，宁可让他选）。
//!
//! 令牌策略：`--dry-run` **不刷新、不写盘**（M30 的 dry-run 纯读语义），只对过期状态给告警；
//! 真启动前对微软轨做一次 `ensure_fresh`（静默续期），刷新成功立刻回写凭据
//! （refresh_token 轮换后旧值就作废了，不能丢）。

use unlucky_core::auth::account::{AccountKind, AccountMeta, Credentials};
use unlucky_core::auth::microsoft::MicrosoftAuth;
use unlucky_core::auth::offline::offline_login;
use unlucky_core::auth::AccountStore;
use unlucky_core::download::blocking::block_on;
use unlucky_core::game::ProfileId;
use unlucky_core::launch::request::AuthPlaceholders;

use crate::context::Ctx;
use crate::error::CliError;

/// 解析结果。
pub struct Identity {
    /// 喂给 launch 的占位符集合。
    pub placeholders: AuthPlaceholders,
    /// 选中的账户（`None` = 离线占位，没有账户）。
    pub account: Option<AccountMeta>,
    /// 需要展示给用户的说明（令牌过期、authlib 注入未落地等）。
    pub notes: Vec<String>,
}

/// 解析本次启动的身份。
///
/// # Errors
///
/// 账户不存在/重名冲突/多账户未指定/凭据过期且刷新失败/凭据文件缺条目。
pub fn resolve(
    ctx: &Ctx,
    account: Option<&str>,
    username: Option<&str>,
    dry_run: bool,
) -> Result<Identity, CliError> {
    let store = AccountStore::open(ctx.auth_data());
    let chosen: Option<AccountMeta> = match account {
        Some(key) => Some(
            store
                .find(key)
                .ok_or_else(|| unknown_account(key, &store))?
                .clone(),
        ),
        None => match store.accounts() {
            [] => None,
            [only] => Some(only.clone()),
            many => {
                return Err(CliError::user(
                    "account_ambiguous",
                    format!("有 {} 个账户，需要指定用哪个", many.len()),
                )
                .with_hint(format!(
                    "ull launch <instance> --account <名字>；候选：{}",
                    names_of(&store)
                )))
            }
        },
    };

    let Some(meta) = chosen else {
        // 零账户：回落离线占位（保留 M21 行为）。
        return Ok(offline_fallback(username));
    };
    if username.is_some() {
        return Err(CliError::usage(format!(
            "已选中账户 {}，`--username` 只用于没有账户时的离线占位",
            meta.display_name()
        ))
        .with_hint("去掉 --username，或用 `ull accounts add offline <名字>` 建账户"));
    }

    let mut notes = Vec::new();
    let placeholders = match meta.kind {
        AccountKind::Offline => offline_login(&meta).map_err(super::map_auth_error)?,
        AccountKind::Microsoft => microsoft_identity(ctx, &store, &meta, dry_run, &mut notes)?,
        AccountKind::AuthlibInjector => authlib_identity(&store, &meta, &mut notes)?,
    };
    Ok(Identity {
        placeholders,
        account: Some(meta),
        notes,
    })
}

/// 零账户时的离线占位（uuid 随机，userType 用 HMCL 的 `legacy` 占位口径不变）。
fn offline_fallback(username: Option<&str>) -> Identity {
    Identity {
        placeholders: AuthPlaceholders {
            player_name: username.unwrap_or("Player").to_string(),
            uuid: ProfileId::generate().as_str().replace('-', ""),
            user_type: "legacy".to_string(),
            user_properties: "{}".to_string(),
            ..AuthPlaceholders::default()
        },
        account: None,
        notes: vec![
            "没有账户，按离线占位启动（ull accounts add offline <名字> 可建账户）".to_string(),
        ],
    }
}

fn microsoft_identity(
    ctx: &Ctx,
    store: &AccountStore,
    meta: &AccountMeta,
    dry_run: bool,
    notes: &mut Vec<String>,
) -> Result<AuthPlaceholders, CliError> {
    let Some(Credentials::Microsoft(session)) = store.credentials(&meta.id) else {
        return Err(CliError::user(
            "account_credentials",
            format!("账户 {} 的微软凭据缺失或类型不符", meta.display_name()),
        )
        .with_hint("重新登录：ull accounts add msa"));
    };

    if dry_run {
        // dry-run 不刷新：令牌过期只提示，命令行里的 token 反正会被脱敏。
        if session.is_expired(unlucky_core::auth::microsoft::now_ms()) {
            notes.push("微软令牌已过期（--dry-run 不刷新，真启动会自动续期）".to_string());
        }
        return session.to_placeholders().map_err(super::map_auth_error);
    }

    let auth = MicrosoftAuth::from_env().map_err(super::map_auth_error)?;
    let (refreshed, reason) =
        block_on(auth.ensure_fresh(session)).map_err(super::map_auth_error)?;
    if let Some(reason) = reason {
        notes.push(format!("{}（已静默续期）", reason.message()));
        persist_refresh(ctx, meta, &refreshed)?;
    }
    refreshed.to_placeholders().map_err(super::map_auth_error)
}

/// 刷新成功后立刻回写凭据（refresh_token 已轮换，旧值作废）。
fn persist_refresh(
    ctx: &Ctx,
    meta: &AccountMeta,
    session: &unlucky_core::auth::MicrosoftSession,
) -> Result<(), CliError> {
    let mut store = AccountStore::open(ctx.auth_data());
    store
        .update_credentials(&meta.id, Credentials::Microsoft(session.clone()))
        .map_err(super::map_auth_error)?;
    let mut updated = meta.clone();
    // profile_name 属于可变的私有缓存，刷新后顺手把元数据里的展示名补上。
    if !session.profile_name.trim().is_empty() {
        updated.profile_name = session.profile_name.clone();
        store.update_meta(updated).map_err(super::map_auth_error)?;
    }
    store.save().map_err(super::map_auth_error)
}

fn authlib_identity(
    store: &AccountStore,
    meta: &AccountMeta,
    notes: &mut Vec<String>,
) -> Result<AuthPlaceholders, CliError> {
    let Some(Credentials::AuthlibInjector(session)) = store.credentials(&meta.id) else {
        return Err(CliError::user(
            "account_credentials",
            format!("账户 {} 的 authlib 凭据缺失或类型不符", meta.display_name()),
        ));
    };
    if !session.has_profile() {
        return Err(CliError::user(
            "account_credentials",
            format!("账户 {} 还没有选中角色", meta.display_name()),
        )
        .with_hint("多角色服务器要在图形界面里选角色"));
    }
    notes.push("authlib-injector 的 -javaagent 参数属后续 mission，本次启动不会注入".to_string());
    session.to_placeholders().map_err(super::map_auth_error)
}

fn names_of(store: &AccountStore) -> String {
    store
        .accounts()
        .iter()
        .map(|meta| meta.display_name().to_string())
        .collect::<Vec<_>>()
        .join("、")
}

fn unknown_account(key: &str, store: &AccountStore) -> CliError {
    let hint = if store.is_empty() {
        "还没有账户：ull accounts add offline <名字> 或 ull accounts add msa".to_string()
    } else {
        format!("可用账户：{}", names_of(store))
    };
    CliError::user("not_found", format!("找不到账户 `{key}`")).with_hint(hint)
}

#[cfg(test)]
mod tests {
    use super::offline_fallback;

    #[test]
    fn fallback_uses_username_or_player() {
        let identity = offline_fallback(Some("Alex"));
        assert_eq!(identity.placeholders.player_name, "Alex");
        assert!(identity.account.is_none());
        assert_eq!(identity.placeholders.user_type, "legacy");
        assert_eq!(identity.placeholders.uuid.len(), 32);
        assert_eq!(offline_fallback(None).placeholders.player_name, "Player");
    }
}
