//! 启动身份：选中账户 → `AuthPlaceholders`（**凭据只在这里读，读完即丢**）+ 收尾结局映射。
//!
//! 与 CLI `commands/identity.rs` 同源：微软轨过期就静默续期并回写凭据（refresh_token 轮换，
//! 旧值作废）；零账户回落离线占位，保证没有账户时照样能启动。

use unlucky_core::auth::account::{AccountKind, Credentials};
use unlucky_core::auth::microsoft::MicrosoftAuth;
use unlucky_core::auth::offline::offline_login;
use unlucky_core::auth::AccountStore;
use unlucky_core::download::blocking::block_on;
use unlucky_core::game::ProfileId;
use unlucky_core::launch::request::AuthPlaceholders;

use crate::dto::{LaunchFailureStage, LaunchFailureView, LogOutcome};

use super::env::HostEnv;

/// 把选中账户翻成启动占位符（**凭据只在这里读，读完即丢**）。
///
/// 微软轨：过期就静默续期并回写凭据（refresh_token 轮换，旧值作废）。
///
/// # Errors
///
/// 账户不存在/凭据类型不符/令牌过期且刷新失败时返回用户可读文案。
pub fn resolve(
    env: &HostEnv,
    account: Option<&str>,
) -> Result<(AuthPlaceholders, String), LaunchFailureView> {
    let store = AccountStore::open(env.layout.auth_data());
    let chosen = match account {
        Some(key) => store
            .find(key)
            .cloned()
            .ok_or_else(|| identity_failure(format!("找不到账户 `{key}`")))?,
        None => match store.accounts() {
            [] => {
                let uuid = ProfileId::generate().as_str().replace('-', "");
                return Ok((
                    AuthPlaceholders {
                        player_name: "Player".to_string(),
                        uuid,
                        user_type: "legacy".to_string(),
                        user_properties: "{}".to_string(),
                        ..AuthPlaceholders::default()
                    },
                    "未登录账户，按离线占位启动".to_string(),
                ));
            }
            [only] => only.clone(),
            many => {
                return Err(identity_failure(format!(
                    "有 {} 个账户，请在账户页选一个再启动",
                    many.len()
                )))
            }
        },
    };
    let line = format!("账户 {}（{}）", chosen.display_name(), chosen.kind.label());
    match chosen.kind {
        AccountKind::Offline => {
            let placeholders = offline_login(&chosen)
                .map_err(|error| identity_failure(format!("离线账户解析失败：{error}")))?;
            Ok((placeholders, line))
        }
        AccountKind::Microsoft => {
            let Some(Credentials::Microsoft(session)) = store.credentials(&chosen.id) else {
                return Err(identity_failure(format!(
                    "账户 {} 的微软凭据缺失或类型不符，请重新登录",
                    chosen.display_name()
                )));
            };
            let auth = MicrosoftAuth::from_env()
                .map_err(|error| identity_failure(format!("微软登录不可用：{error}")))?;
            let (fresh, _) = block_on(auth.ensure_fresh(session))
                .map_err(|error| identity_failure(format!("微软令牌刷新失败：{error}")))?;
            if &fresh != session {
                // refresh_token 已轮换：立刻回写，旧值作废。
                let mut store = AccountStore::open(env.layout.auth_data());
                let _ = store.update_credentials(&chosen.id, Credentials::Microsoft(fresh.clone()));
                let _ = store.save();
            }
            let placeholders = fresh
                .to_placeholders()
                .map_err(|error| identity_failure(format!("微软身份转换失败：{error}")))?;
            Ok((placeholders, line))
        }
        AccountKind::AuthlibInjector => {
            let Some(Credentials::AuthlibInjector(session)) = store.credentials(&chosen.id) else {
                return Err(identity_failure(format!(
                    "账户 {} 的第三方凭据缺失或类型不符",
                    chosen.display_name()
                )));
            };
            let placeholders = session
                .to_placeholders()
                .map_err(|error| identity_failure(format!("第三方身份转换失败：{error}")))?;
            Ok((placeholders, line))
        }
    }
}

/// 账户/身份这一步的失败（阶段固定 `Identity`；`detail` 保留原始文案与本机错误）。
fn identity_failure(detail: String) -> LaunchFailureView {
    LaunchFailureView::new(LaunchFailureStage::Identity, detail)
}

/// `SessionHandle` → 日志页的三种结局。
pub fn outcome_of(handle: &unlucky_core::launch::SessionHandle) -> LogOutcome {
    if handle.exit.silent_failure.is_some() {
        LogOutcome::SilentFail
    } else if handle.succeeded() {
        LogOutcome::Ok
    } else {
        LogOutcome::Crashed
    }
}
