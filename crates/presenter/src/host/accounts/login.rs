//! 微软登录的两条流（设备码默认 / 浏览器优先失败回退设备码）+ 会话入库。
//!
//! 从 `accounts.rs` 拆出来（M236）：父文件顶着 400 行硬线，而本轮要给测试缝换一个**稳定身份**
//! （`Arc::as_ptr` 会被堆地址复用骗到，见父模块的 `test_seam`），拆出子模块才有地方落。
//!
//! **纯搬移**：下面每个函数体、每一行签名都与搬移前逐行相同（没有改一个字的可见性）；原来的
//! 公开路径靠父模块的 `pub use` 保住（`host::accounts::start_login` / `start_browser_login`）。
//!
//! 两条流除了「怎么拿到 MS access_token」之外完全共用：同一个取消槽位（父模块的 `LoginJob`）、
//! 同一段入库（`store_session`）、同一个收尾（`finish`）——所以它们必须住在同一个文件里。

use unlucky_core::auth::account::Credentials;
use unlucky_core::auth::microsoft::{now_ms, MicrosoftAuth, MicrosoftSession};
use unlucky_core::auth::{AccountStore, AuthResult};
use unlucky_core::download::blocking::block_on;

use crate::dto::{DeviceLoginView, LoginFailure};

use super::super::env::HostEnv;
use super::super::job::{spawn, HostTask, JobSender};
use super::super::opener;
use super::failure::classify;
#[cfg(test)]
use super::test_seam;
use super::{accounts, auth_failure, browser_note, LoginJob, MsaEntry};

/// 起一次微软登录（M178：**GUI 的默认路**——设备码）。
///
/// 设备码流不依赖任何应用注册项，是唯一「在官方产物上开箱可用」的路；它**一个本地监听口都不开**，
/// 也就不会撞上 `redirect_uri is not valid`（守卫断言 `listeners_opened == 0`，在 bind 现场加）。
pub fn start_login(env: HostEnv, tx: JobSender, job: LoginJob) {
    spawn("msa-login", move || {
        let auth = match msa_auth(&env, MsaEntry::DeviceCode, &job) {
            Ok(auth) => auth,
            Err(error) => return fail(&tx, auth_failure(&error), error.to_string()),
        };
        let tx_prompt = tx.clone();
        let result = block_on(auth.device_code_login(|prompt| {
            let _ = tx_prompt.send(HostTask::Login(Box::new(DeviceLoginView::waiting(
                &prompt.user_code,
                &prompt.verification_uri,
                prompt.expires_in.as_secs(),
                &format!("{}#{}", prompt.verification_uri, prompt.user_code),
            ))));
        }));
        finish(&env, &tx, &job, result);
    });
}

/// 起一次「浏览器优先、失败自动回退设备码」的登录（M178：可选加速路）。
///
/// 只在用户**显式要求**时才走到这里（账户页的「用浏览器登录」）。即使如此也不盲试：`core` 先做
/// 回环注册预检，没登记就**不绑端口、不弹浏览器**，直接设备码；试了但失败同样回退并给一句说明。
pub fn start_browser_login(env: HostEnv, tx: JobSender, job: LoginJob) {
    spawn("msa-browser", move || {
        // **P1-1 的那一行**：这里是「用户点了用浏览器登录」，必须把 browser_first 置位，
        // 否则下面这段预检/回退编排整段不执行（复审 r1 实测的洞）。
        let auth = match msa_auth(&env, MsaEntry::Browser, &job) {
            Ok(auth) => auth,
            Err(error) => return fail(&tx, auth_failure(&error), error.to_string()),
        };
        let tx_browser = tx.clone();
        let tx_device = tx.clone();
        let result = block_on(auth.browser_then_device_login(
            |prompt| {
                // 浏览器拉不起来不是致命错误：面板里还有可复制的授权链接。
                let opened = opener::open_url(&prompt.authorization_url);
                let _ =
                    tx_browser.send(HostTask::Login(Box::new(DeviceLoginView::browser_waiting(
                        &prompt.authorization_url,
                        prompt.timeout.as_secs(),
                        opened.err().as_deref(),
                    ))));
            },
            |prompt, fallback| {
                let view = DeviceLoginView::waiting(
                    &prompt.user_code,
                    &prompt.verification_uri,
                    prompt.expires_in.as_secs(),
                    &format!("{}#{}", prompt.verification_uri, prompt.user_code),
                );
                let view = match fallback {
                    Some(fallback) => {
                        // 回退原文只进日志；面板只看码。
                        eprintln!(
                            "[auth] 浏览器登录未走成（{}），已回退设备码：{}",
                            fallback.reason.code(),
                            fallback.detail
                        );
                        view.with_browser_note(browser_note(fallback.reason))
                    }
                    None => view,
                };
                let _ = tx_device.send(HostTask::Login(Box::new(view)));
            },
        ));
        finish(&env, &tx, &job, result.map(|outcome| outcome.session));
    });
}

/// 生产入口的 `MicrosoftAuth`（client id 取环境/构建期，失败日志落 `<数据根>/logs/`）。
///
/// **`browser_first` 必须在这里按入口置位**（复审 r1 P1-1）：只构造默认 `MicrosoftAuth` 的话，
/// 浏览器按钮会静默退化成设备码——预检/回退码/说明全不可达。
fn msa_auth(env: &HostEnv, entry: MsaEntry, _job: &LoginJob) -> AuthResult<MicrosoftAuth> {
    #[cfg(test)]
    test_seam::record(_job, entry);
    MicrosoftAuth::from_env().map(|auth| {
        auth.with_browser_first(entry.browser_first())
            .with_journal(Some(unlucky_core::auth::AuthJournal::in_logs_dir(
                env.layout.logs(),
            )))
    })
}

/// 失败收尾（带**原因码**）：原文进日志，页面据码出 i18n（复审 r1 P1-1）。
fn fail(tx: &JobSender, failure: LoginFailure, detail: impl Into<String>) {
    let detail = detail.into();
    eprintln!("[auth] 微软登录失败（{}）：{detail}", failure.code());
    let _ = tx.send(HostTask::Login(Box::new(DeviceLoginView::failed_as(
        failure, detail,
    ))));
}

/// 收尾：取消就吞掉结果；成功入库；失败带 core 的诊断文案（M38 的 403 也走这条）。
fn finish(env: &HostEnv, tx: &JobSender, job: &LoginJob, result: AuthResult<MicrosoftSession>) {
    if job.is_cancelled() {
        let _ = tx.send(HostTask::Login(Box::default()));
        return;
    }
    match result {
        Ok(session) => match store_session(env, session) {
            Ok(name) => {
                let _ = tx.send(HostTask::Login(Box::new(DeviceLoginView::succeeded(&name))));
                let _ = tx.send(HostTask::Accounts(accounts(env, None)));
            }
            Err(message) => fail(tx, LoginFailure::StoreFailed, message),
        },
        Err(error) => {
            let failure = classify(&error);
            fail(tx, failure, error.to_string());
        }
    }
}

/// 微软会话入库（元数据 + 凭据分文件，见 `core::auth::AccountStore`）。
fn store_session(env: &HostEnv, session: MicrosoftSession) -> Result<String, String> {
    let meta = unlucky_core::auth::account::AccountMeta::new(
        unlucky_core::auth::account::AccountKind::Microsoft,
        session.profile_id.clone(),
        session.profile_name.clone(),
        now_ms(),
    );
    let name = meta.display_name().to_string();
    let mut store = AccountStore::open(env.layout.auth_data());
    store
        .insert(meta, Credentials::Microsoft(session))
        .and_then(|()| store.save())
        .map_err(|error| format!("账户入库失败：{error}"))?;
    Ok(name)
}
