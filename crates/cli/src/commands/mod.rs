//! 命令分发：命令模型 → 具体实现，并统一挂上环境提示。

pub mod accounts;
pub mod agent;
pub mod config;
pub mod identity;
pub mod install;
pub mod instances;
pub mod java;
pub mod java_launch;
pub mod java_vendors;
pub mod launch;
pub mod meta;
pub mod sessions;
pub mod snapshot;

use crate::cli::{Command, Globals, Invocation};
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{Outcome, Report};

/// 执行一条已解析的命令。
///
/// # Errors
///
/// 用法/数据根/目标不存在等用户错误，或序列化失配等内部错误。
pub fn run(invocation: &Invocation) -> Result<Outcome, CliError> {
    match &invocation.command {
        // 这三条不需要数据根（`--version` 在不达基线的机器上也要能跑，用于诊断）。
        Command::Welcome => Ok(Outcome::new(meta::welcome())),
        Command::Version => Ok(Outcome::new(meta::version())),
        Command::Help(topic) => Ok(Outcome::new(meta::help(topic.as_deref()))),
        Command::InstancesList => with_ctx(&invocation.globals, instances::list),
        Command::Install(args) => with_ctx(&invocation.globals, |ctx| install::run(ctx, args)),
        Command::Launch(args) => with_ctx(&invocation.globals, |ctx| launch::run(ctx, args)),
        Command::SessionsList { limit } => {
            with_ctx(&invocation.globals, |ctx| sessions::list(ctx, *limit))
        }
        Command::Logs(args) => with_ctx(&invocation.globals, |ctx| sessions::logs(ctx, args)),
        Command::Diag(args) => with_ctx(&invocation.globals, |ctx| sessions::diag(ctx, args)),
        Command::ConfigGet(args) => with_ctx(&invocation.globals, |ctx| {
            config::get(ctx, args.key.as_deref(), args.show_secret)
        }),
        Command::ConfigSet(args) => with_ctx(&invocation.globals, |ctx| {
            config::set(ctx, &args.key, &args.value)
        }),
        Command::JavaList => with_ctx(&invocation.globals, java::list),
        Command::JavaInstall { major, vendor } => with_ctx(&invocation.globals, |ctx| {
            java::install(ctx, *major, *vendor)
        }),
        Command::JavaVendors => with_ctx(&invocation.globals, java::vendors),
        Command::AccountsList => with_ctx(&invocation.globals, accounts::list),
        Command::AccountsAdd(args) => with_ctx(&invocation.globals, |ctx| accounts::add(ctx, args)),
        Command::AccountsRemove { name } => {
            with_ctx(&invocation.globals, |ctx| accounts::remove(ctx, name))
        }
        // `AgentServe` 由 `runner::run` 提前分流（stdio 形态下 stdout 就是协议流，
        // 不能走「先返回 Report 再打印」的路径）；这里只兜住「有人直接调 commands::run」。
        Command::AgentServe(_) => Err(CliError::usage(
            "agent serve 必须在进程入口直接运行（runner 会提前分流）",
        )),
        Command::AgentConfig { port } => {
            with_ctx(&invocation.globals, |ctx| agent::config_report(ctx, *port))
        }
    }
}

/// `core::auth` 的错误 → CLI 错误（保留稳定机读码；文案已是人话，这里只补「下一步」）。
///
/// 集中一处的原因：账户相关命令（`accounts *`）与 `launch` 的身份解析要为**同一批**
/// 错误给出一致的码与提示——两处各写一份必然会漂。
pub(crate) fn map_auth_error(error: unlucky_core::auth::AuthError) -> CliError {
    use unlucky_core::auth::body;
    use unlucky_core::auth::AuthError;
    let hint = match &error {
        AuthError::Canceled => Some("登录被取消，可以重跑一次".to_string()),
        AuthError::Timeout { .. } => {
            Some("重新执行 `ull accounts add msa` 拿新的用户码".to_string())
        }
        AuthError::CredentialExpired => Some("重新登录：ull accounts add msa".to_string()),
        AuthError::Unsupported(_) => {
            Some("微软登录需要设置 ULL_MSA_CLIENT_ID（Azure 公共客户端 id）".to_string())
        }
        AuthError::Conflict { .. } => Some("用 `ull accounts list` 查看已有账户".to_string()),
        AuthError::NotFound { .. } => Some("用 `ull accounts list` 查看账户名".to_string()),
        AuthError::Denied { code, .. } if code == "no_profile" || code == "no_license" => {
            Some("去 minecraft.net 检查该账号的 Java 版拥有权与档案".to_string())
        }
        AuthError::Xbox { .. } => {
            Some("去 xbox.com / 家庭组设置里解决该账号的 Xbox 限制".to_string())
        }
        // M38 的两类分流落到 hint 行：拦截页（HTML/纯文本/空体）先查网络，
        // JSON 拒绝按步骤查许可与区域。`code()` 仍是 "denied"（M31 兼容），
        // 细分结论由 `detail_code()` 带在 hint 里，排障脚本照样能机读分流。
        AuthError::Response(detail) if detail.is_interception() => Some(format!(
            "检查网络或代理（关掉/换掉代理后再试，必要时用镜像方案）；诊断码 {}",
            detail.detail_code()
        )),
        AuthError::Response(detail) => Some(format!(
            "{}；诊断码 {}",
            body::advice(&detail.step, detail.kind),
            detail.detail_code()
        )),
        _ => None,
    };
    let cli = CliError::user(error.code(), error.message());
    match hint {
        Some(hint) => cli.with_hint(hint),
        None => cli,
    }
}

/// 需要数据根的命令：解析上下文 → D19 支持级别闸门 → 执行 → 挂环境提示。
fn with_ctx<F>(globals: &Globals, action: F) -> Result<Outcome, CliError>
where
    F: FnOnce(&Ctx) -> Result<Report, CliError>,
{
    let ctx = Ctx::resolve(globals)?;
    ctx.ensure_supported()?;
    let report = action(&ctx)?;
    Ok(Outcome::with_warnings(report, ctx.notes()))
}

#[cfg(test)]
mod tests {
    use super::map_auth_error;
    use unlucky_core::auth::{AuthError, BodyKind, ResponseDetail};

    fn response(step: &str, kind: BodyKind) -> AuthError {
        AuthError::Response(Box::new(ResponseDetail {
            step: step.to_string(),
            what: "Minecraft 登录被拒".to_string(),
            status: 403,
            content_type: None,
            kind,
            server_message: None,
            digest: String::new(),
        }))
    }

    #[test]
    fn response_hints_split_network_from_license_and_carry_detail_code() {
        let blocked = map_auth_error(response("login_with_xbox", BodyKind::Html));
        assert_eq!(blocked.code(), "denied", "code() 仍是 M31 的 denied");
        let hint = blocked.hint().expect("拦截页要有 hint").to_string();
        assert!(hint.contains("代理"), "{hint}");
        assert!(hint.contains("login_with_xbox_non_json"), "{hint}");

        let denied = map_auth_error(response("login_with_xbox", BodyKind::Json));
        let hint = denied.hint().expect("JSON 拒绝要有 hint").to_string();
        assert!(hint.contains("许可"), "{hint}");
        assert!(!hint.contains("代理"), "{hint}");
        assert!(hint.contains("login_with_xbox_403"), "{hint}");
    }
}
