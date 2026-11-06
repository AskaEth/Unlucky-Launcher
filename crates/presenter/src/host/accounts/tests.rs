//! 账户 host 的判据：入库/删除/名字规则、失败分类，以及**缺 client id 的上屏链路**（M132）。
//!
//! ## 为什么这个文件要跟 `accounts.rs` 分开
//!
//! `accounts.rs` 顶着 400 行硬线，而 M132 的锚必须**真的走一遍登录入口**（`start_login` /
//! `start_browser_login`）才能钉住「入口 → 原因码」那一行——只测 `auth_failure` 的话，把调用点
//! 改回 `LoginFailure::Other` 仍然全绿（复审 r1 P2-1 实测）。测试拆出来是为了留出这段空间。
//!
//! 拆法照抄仓库既有先例（`dto/accounts.rs`、`host/install.rs` 等）：父文件里留
//! `#[cfg(test)] mod tests;`，本文件 `use super::*;` 照旧拿到父模块的一切。

use std::time::Duration;

use unlucky_core::auth::microsoft::{client_id_status, CLIENT_ID_ENV};
use unlucky_core::auth::AuthError;

use super::*;

use super::failure::mentions_whitelist;

#[test]
fn time_display_matches_known_timestamps() {
    assert_eq!(display_ms(0), "");
    // 1_769_000_000_000 ms = 2026-01-21T12:53:20Z（取整到分钟）
    assert_eq!(display_ms(1_769_000_000_000), "2026-01-21 12:53");
}

#[test]
fn offline_add_list_remove_round_trips() {
    let dir = std::env::temp_dir().join(format!("ull-host-accounts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = HostEnv::at_root(&dir);
    std::fs::create_dir_all(env.layout.auth_data()).expect("auth 目录");

    let name = add_offline(&env, "Steve").expect("加账户");
    assert_eq!(name, "Steve");
    let list = accounts(&env, None);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].kind, AccountKind::Offline);
    assert!(!list[0].active);

    let active = accounts(&env, Some(list[0].id.as_str()));
    assert!(active[0].active);

    let removed = remove(&env, "Steve").expect("删账户");
    assert_eq!(removed, "Steve");
    assert!(accounts(&env, None).is_empty());
    assert!(add_offline(&env, "  ").is_err());
    std::fs::remove_dir_all(&dir).ok();
}

fn denied(code: &str) -> AuthError {
    AuthError::Denied {
        code: code.to_string(),
        description: "bad".to_string(),
    }
}

fn network() -> AuthError {
    AuthError::Network {
        url: "https://x".to_string(),
        detail: "dns".to_string(),
    }
}

#[test]
fn failure_codes_are_classified_from_variants_not_chinese_text() {
    assert_eq!(classify(&network()), LoginFailure::Unreachable);
    let whitelist = classify(&denied("AADSTS7000218"));
    assert_eq!(whitelist, LoginFailure::NotWhitelisted);
    assert_eq!(classify(&denied("invalid_grant")), LoginFailure::Denied);
    assert_eq!(classify(&AuthError::Canceled), LoginFailure::Other);
    assert!(mentions_whitelist("AADSTS7000218"));
    assert!(!mentions_whitelist("invalid_grant"));
}

/// **M132**：只有「缺 client id」变成「需要配置」，网络/凭据失败不许跟着变（否则成噪音）。
#[test]
fn only_a_missing_client_id_becomes_the_config_gap_code() {
    let missing = auth_failure(&AuthError::Unsupported(String::new()));
    assert_eq!(missing, LoginFailure::MsaNotConfigured);
    assert_eq!(auth_failure(&network()), LoginFailure::Unreachable);
    assert_eq!(auth_failure(&denied("invalid_grant")), LoginFailure::Denied);
}

/// **M132 的入口锚（复审 r1 P2-1）**：缺 client id 时，**两条登录流**回给页面的那条事件里
/// 必须带 `MsaNotConfigured`。
///
/// 抓得住（实测过的变异）：把 `start_login` / `start_browser_login` 里的
/// `auth_failure(&error)` 改回 `LoginFailure::Other`——那是页面退回「完整原因在启动器日志里」
/// 的唯一开关，而 `auth_failure` 自己的单测对它一声不响。
/// 零网络：`MicrosoftAuth::from_env` 在 id 判定处就地早退，连 HTTP 客户端都不建，所以这里
/// 不需要任何桩，也不会因为没网而红。
///
/// **它要求一个「两处都没有 client id」的构建**：构建期烘焙了 id（`option_env!`）的构建里它
/// `panic!` 而不是跳过——跳过就等于让这条锚悄悄失效（r2 复审实测过那种形态），红着喊人才
/// 逼得人做决定（不带 id 构建，或给 `resolve_client_id` 加接缝后重写这条锚）。
#[test]
fn a_missing_client_id_reaches_the_login_panel_as_a_config_gap() {
    // 环境变量是**进程级**的：动它的用例必须串行（core 侧同一套写法），否则并行跑互相污染。
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let saved = std::env::var(CLIENT_ID_ENV).ok();
    std::env::remove_var(CLIENT_ID_ENV);
    // panic 也要还原：不还原的话后面所有用例都会在一个「没配 id」的进程里跑。
    struct Restore(Option<String>);
    impl Drop for Restore {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => std::env::set_var(CLIENT_ID_ENV, value),
                None => std::env::remove_var(CLIENT_ID_ENV),
            }
        }
    }
    let _restore = Restore(saved);

    // 环境变量已经摘掉了，此时若仍报告「已配置」，来源只可能是构建期注入
    // （`ULL_MSA_CLIENT_ID=<id> cargo build` ⇒ `option_env!`），或者 `resolve_client_id` 的
    // 实现变了。**这种构建里必须红**：这条锚验证不了缺 id 路径，静默 `return` 就是让锚悄悄失效
    // （r2 复审实测：注入 id + 同一变异 → 全绿 22 passed，正是塔台最怕的「守门空跑」形态）。
    if let Some(source) = client_id_status().source() {
        panic!(
            "这条锚要求「两处都没有 client id」的构建，但二进制里 client id 已被报告为已配置\
             （source={source:?}，环境变量 {CLIENT_ID_ENV} 已摘掉）。要么不带 \
             ULL_MSA_CLIENT_ID 构建，要么给 `unlucky_core::auth::microsoft::resolve_client_id` \
             加一个可注入的接缝，再按接缝重写这条锚。"
        );
    }

    assert_config_gap("device", start_login);
    assert_config_gap("authcode", start_browser_login);
}

/// 起一条登录流、等它回的第一条登录事件，并断言那是一条「缺配置」而不是通用失败。
///
/// 缺 id 时这条路径**零网络**（`from_env` 在 id 判定处早退），所以事件立刻到，不需要桩。
fn assert_config_gap(name: &str, start: fn(HostEnv, JobSender, LoginJob)) {
    let dir = std::env::temp_dir().join(format!("ull-msa-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (tx, rx) = super::super::job::channel();
    start(HostEnv::at_root(&dir), tx, LoginJob::new());
    let view = match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(HostTask::Login(view)) => view,
        Ok(_) => panic!("{name}：缺 id 时第一条事件就该是登录失败"),
        Err(error) => panic!("{name}：等不到登录事件（{error}）"),
    };
    assert_eq!(view.failure, Some(LoginFailure::MsaNotConfigured), "{name}");
    // 原文（含要设的变量名）仍然只在 `message` 里：它只进日志，页面画不出来。
    assert!(
        view.message.contains(CLIENT_ID_ENV),
        "{name}: {}",
        view.message
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn offline_name_rule_matches_the_ui_contract() {
    assert!(valid_offline_name("Steve"));
    assert!(valid_offline_name("a_1"));
    assert!(!valid_offline_name("ab"), "太短");
    assert!(!valid_offline_name("中文名字"), "非 ascii");
    assert!(!valid_offline_name("has space"));
    assert!(!valid_offline_name("x"));
}

#[test]
fn a_bad_offline_name_is_classified_not_stringified() {
    let dir = std::env::temp_dir().join(format!("ull-name-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = HostEnv::at_root(&dir);
    std::fs::create_dir_all(env.layout.auth_data()).expect("auth 目录");
    let failure = add_offline(&env, "ab").expect_err("太短要报错");
    assert_eq!(failure.code, AccountNoticeCode::OfflineNameInvalid);
    assert!(!failure.detail.is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn login_job_cancel_flag_is_shared() {
    let job = LoginJob::new();
    let clone = job.clone();
    assert!(!clone.is_cancelled());
    job.cancel();
    assert!(clone.is_cancelled());
}
