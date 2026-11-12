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

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use unlucky_core::auth::microsoft::{client_id_status, CLIENT_ID_ENV};
use unlucky_core::auth::AuthError;

use super::*;

use super::super::job::{HostTask, JobSender};
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

/// **M178 复审 r1 P1-1 的守卫**：生产路径上，「用浏览器登录」这颗按钮必须真的把
/// `browser_first` 置位，设备码那颗必须不置位。
///
/// 修复前实测过的形态：`start_browser_login` 用共享的 `microsoft_auth(&env)`（默认
/// `browser_first=false`），于是 `browser_then_device_login` 里整段预检/回退编排不执行——
/// 按钮静默退化成设备码、连一句说明都不出，`preflight` / `BrowserFallbackReason` /
/// `BrowserLoginNote` / 三条 i18n 文案全成不可达代码。
///
/// 抓得住（本用例就是为此写的）：把 `msa_auth(&env, MsaEntry::Browser)` 改回
/// `MsaEntry::DeviceCode`（或删掉 `.with_browser_first(...)` 那一跳以外的模式选择），
/// 记录会变成 `false` ⇒ 红。
/// 抓不住：真的去连微软（无 client id 的测试构建里两条入口都在网络之前早退）。
#[test]
fn the_browser_button_actually_turns_on_browser_first() {
    let dir = std::env::temp_dir().join(format!("ull-msa-entry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = HostEnv::at_root(&dir);
    std::fs::create_dir_all(env.layout.auth_data()).expect("auth 目录");

    // 设备码入口：不许碰浏览器（也不许碰回环预检）。
    let device_job = LoginJob::new();
    let (tx, _rx) = super::super::job::channel();
    start_login(env.clone(), tx, device_job.clone());
    assert_eq!(
        wait_for_entry(&device_job),
        Some(false),
        "设备码入口把 browser_first 置成了 true：默认路会去开监听口/弹浏览器"
    );

    // 浏览器入口：**必须**置位，否则预检与回退整段不可达。
    let browser_job = LoginJob::new();
    let (tx, _rx) = super::super::job::channel();
    start_browser_login(env, tx, browser_job.clone());
    assert_eq!(
        wait_for_entry(&browser_job),
        Some(true),
        "「用浏览器登录」没有置位 browser_first：按钮会静默退化成设备码（复审 r1 P1-1 的洞）"
    );
}

/// 等登录线程把「这个 job 的入口模式」记下来（线程是 `spawn` 出去的，最多等这么久）。
///
/// 按 job 分桶而不是读「最后一次」：同一进程里别的用例也在调这两个入口（实测会串台假红）。
fn wait_for_entry(job: &LoginJob) -> Option<bool> {
    for _ in 0..200 {
        if let Some(flag) = super::test_seam::mode_for(job) {
            return Some(flag);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

/// **M236（台账「真开着的测试债」之一）：测试缝的身份不许是会复用的堆地址。**
///
/// 构造：两个**不同**的 `LoginJob` 共用同一个 `cancelled` 控制块地址（`Arc::clone` 不换地址，
/// `job_around` 各自给一个新身份）。这正是 `Arc::as_ptr` 做 key 时那个假设——「地址相同 ⇒
/// 同一个 job」——失效的形态；真实世界里它由「job A 被 drop 后控制块立刻分给 job B」触发。
///
/// 反向证据（实测过）：把身份退回 `Arc::as_ptr` ⇒ 本用例红（`mode_for(&younger)` 读到
/// `Some(true)`）；那个读串台正是并行跑时「设备码入口读到别人的 browser_first=true」的假红。
/// 抓不住：两条入口真的去连微软（无 client id 的测试构建里它们都在网络之前早退）。
#[test]
fn the_test_seam_does_not_reuse_a_heap_address_as_identity() {
    let shared = Arc::new(AtomicBool::new(false));
    let older = super::test_seam::job_around(&shared);
    let younger = super::test_seam::job_around(&shared);
    assert_eq!(
        Arc::as_ptr(&older.cancelled),
        Arc::as_ptr(&younger.cancelled),
        "前提：两个 job 必须共用同一个取消槽地址（否则本用例什么也没构造出来）"
    );
    assert_eq!(
        super::test_seam::mode_for(&younger),
        None,
        "前提：这两个 job 都还没请求过"
    );
    super::test_seam::record(&older, MsaEntry::Browser);
    assert_eq!(
        super::test_seam::mode_for(&older),
        Some(true),
        "前提：记录本身要能按 job 读回来"
    );
    assert_eq!(
        super::test_seam::mode_for(&younger),
        None,
        "同一个堆地址、但确定是另一个 job：读到 older 的记录就是把「会被复用的地址」当身份"
    );
}

/// **M236**：测试缝的书写规则——每 job 恒一条（覆盖成最后一次）、死 job 的条目被 `retain` 收走。
///
/// 改之前 `ENTRIES` 只 push 不 clear/retain：条目随登录次数无上限增长；`mode_for` 又用「第一条」
/// 回答，同一个 job 记两次（重试）时会永远读到第一次的模式。
/// 这里直接对纯函数 `remember` 造活/死条目，不碰全局表——并行用例不会互相污染。
#[test]
fn the_test_seam_keeps_the_latest_mode_per_job_and_forgets_dropped_ones() {
    let alive = Arc::new(AtomicBool::new(false));
    // 一个已经 drop 的 job：`Entry` 里的 `Weak` 把控制块钉住 ⇒ `strong_count() == 0` 读得准。
    let dead = {
        let gone = Arc::new(AtomicBool::new(false));
        super::test_seam::Entry::new(JobId(1), &gone, true)
    };
    assert!(!dead.is_alive(), "构造失败：这条应当已经是死条目");

    let mut entries = vec![dead, super::test_seam::Entry::new(JobId(2), &alive, false)];
    super::test_seam::remember(
        &mut entries,
        super::test_seam::Entry::new(JobId(2), &alive, true),
    );

    assert_eq!(
        entries.len(),
        1,
        "死 job 的条目必须被收走（表只随活 job 增长）"
    );
    assert_eq!(entries[0].job(), JobId(2));
    assert!(
        entries[0].browser_first(),
        "同一个 job 记两次必须取最后一次"
    );
}

/// 置位不是「记了个数」：`msa_auth` 必须把模式真的落到 `MicrosoftAuth::browser_first` 上。
///
/// 前半段（入口 → 模式）由上一个用例守；这里守后半段（模式 → 实例字段）。两者缺一，
/// 就会出现「日志说置位了、实际没置位」这类假绿。
#[test]
fn the_entry_mode_lands_on_the_auth_instance() {
    assert!(!MsaEntry::DeviceCode.browser_first());
    assert!(MsaEntry::Browser.browser_first());
    // 无 client id 时 `from_env` 早退，所以这里用显式 id 走同一个 builder 链。
    let auth = unlucky_core::auth::microsoft::MicrosoftAuth::new("0000-entry-test")
        .expect("构造")
        .with_browser_first(MsaEntry::Browser.browser_first());
    assert!(
        auth.browser_first(),
        "with_browser_first 没把字段置上（core 里 browser_then_device_login 会整段跳过）"
    );
}

/// **M178**：core 的回退原因码 → 页面的说明码。两边的 ascii 码必须逐字相同——
/// 日志里看到的码就是页面上 `Code:` 那一行，对不上就没法用用户回报的码定位。
///
/// 抓得住：core 加/改一个回退原因而 presenter 忘了接（`browser_note` 是 `match`，
/// 新增变体会编译失败；这里再把**码的字符串**钉住，防的是两侧各改各的）。
#[test]
fn browser_fallback_reasons_map_to_the_page_notes_with_the_same_codes() {
    for (reason, note) in [
        (
            BrowserFallbackReason::RedirectNotRegistered,
            BrowserLoginNote::RedirectNotRegistered,
        ),
        (
            BrowserFallbackReason::PreflightUnavailable,
            BrowserLoginNote::PreflightUnavailable,
        ),
        (
            BrowserFallbackReason::AttemptFailed,
            BrowserLoginNote::AttemptFailed,
        ),
    ] {
        assert_eq!(browser_note(reason), note);
        assert_eq!(note.code(), reason.code());
    }
    // 「没登记回环」是唯一带「怎么修」的那一条，码也必须稳定（排障脚本按它 grep 日志）。
    assert_eq!(
        BrowserLoginNote::RedirectNotRegistered.code(),
        "redirect_not_registered"
    );
}
