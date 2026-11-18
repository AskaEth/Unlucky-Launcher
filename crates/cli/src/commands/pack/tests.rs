//! `pack import` 失败收尾的三档文案（M161 复审「不计入但该记」第一条：CLI 也曾把
//! 预检失败说成「已整包回滚」）。
//!
//! 抓得住：三档塌成两句（预检 → 已回滚）；`is_complete()` 被丢掉；`status != Failed` 也出话。

use std::path::PathBuf;
use std::sync::Mutex;

use unlucky_core::addon::import::{ImportError, ImportReport, ImportStatus, RollbackOutcome};

use super::lines::{rollback_stage, RollbackStage};
use super::*;

fn failed() -> ImportReport {
    let mut report = ImportReport::empty(PathBuf::from("C:/tmp/.minecraft"));
    report.fail(ImportError::integrity("sha1 mismatch"));
    report
}

/// 三档三句，**互不相同**（同族的谎就是这个 mission 要消灭的那类）。
#[test]
fn the_three_rollback_stages_get_three_different_sentences() {
    // ① 预检没过：一个字节都没写。
    let mut preflight = failed();
    preflight.rolled_back = true;
    assert_eq!(rollback_stage(&preflight), Some(RollbackStage::NotStarted));
    let not_started = rollback_line(&preflight);
    assert!(not_started.contains("没写"), "{not_started}");
    assert!(
        !not_started.contains("回滚：目标目录"),
        "预检失败不许说「已回滚」：{not_started}"
    );
    assert!(status_label(&preflight).contains("没写"), "预检失败那句");

    // ② 写过又完整回滚。
    let mut rolled_back = failed();
    rolled_back.rolled_back = true;
    rolled_back.rollback = Some(RollbackOutcome {
        attempted: 2,
        failed: Vec::new(),
    });
    assert_eq!(
        rollback_stage(&rolled_back),
        Some(RollbackStage::RolledBack)
    );
    assert!(rollback_line(&rolled_back).contains("已整包回滚"));
    assert!(status_label(&rolled_back).contains("已整包回滚"));

    // ③ 回滚没做完。
    let mut incomplete = failed();
    incomplete.rolled_back = false;
    incomplete.rollback = Some(RollbackOutcome {
        attempted: 2,
        failed: vec!["mods/a.jar".to_string()],
    });
    assert_eq!(rollback_stage(&incomplete), Some(RollbackStage::Incomplete));
    assert!(rollback_line(&incomplete).contains("1 项没回去"));
    assert!(status_label(&incomplete).contains("没做完"));

    // 三句必须互不相同（把任何两档并成一句，这里就红）。
    let lines = [
        rollback_line(&preflight),
        rollback_line(&rolled_back),
        rollback_line(&incomplete),
    ];
    assert_eq!(
        lines.iter().collect::<std::collections::HashSet<_>>().len(),
        3,
        "{lines:?}"
    );
}

/// 不是致命失败 ⇒ 没有回滚那一段话（成功/部分成功不该出现「已回滚」）。
#[test]
fn a_non_fatal_report_says_nothing_about_rollback() {
    let mut report = ImportReport::empty(PathBuf::from("C:/tmp"));
    report.status = ImportStatus::Partial;
    assert_eq!(rollback_stage(&report), None);
    assert_eq!(rollback_line(&report), "");
    assert!(status_label(&report).contains("缺口"), "部分成功 = 有缺口");
}

// ------------------------------------- M318：CF Key 真的接进了 CLI 的导入链（判据唯一在 core）
// 支撑件（`TempDir` / `KeyEnv` / `ctx_at` / `write_key` / `fetcher_debug`）搬去了
// `tests/support.rs`（M346 r3：本文件贴着 400 行硬线）；下面的用例照旧按名字取用。
#[path = "tests/support.rs"]
mod support;

pub(crate) use support::*;

/// 有 Key（设置那一档）⇒ CLI 造出来的 fetcher **真的带上了**它。
///
/// M318 之前这里恒是 `DownloadEngineFetcher::new(&engine)`：CLI 导入 CF 包时 CF 条目一律记「缺 Key」，
/// 而 GUI 那边设置里的 Key 是好用的——同一份配置两条路径两个结论。
#[test]
fn the_cli_fetcher_carries_a_key_configured_in_the_settings() {
    let _lock = KEY_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _env = KeyEnv::set(None);
    let dir = TempDir::new("settings");
    let ctx = ctx_at(dir.path());
    write_key(&ctx, "settings-only-key");

    let debug = fetcher_debug(&ctx);
    assert!(
        debug.contains("curse_key: Some(ApiKey(<redacted>))"),
        "设置里填了 Key，CLI 的 fetcher 必须带上它：{debug}"
    );
    assert!(
        !debug.contains("settings-only-key"),
        "Key 的明文不许出现在 Debug 里：{debug}"
    );
}

/// 环境变量只是兜底：设置没写时它仍然生效（老用户手上的 `ULL_CF_API_KEY` 不许失效）。
#[test]
fn the_cli_fetcher_falls_back_to_the_env_var() {
    let _lock = KEY_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _env = KeyEnv::set(Some("env-only-key"));
    let dir = TempDir::new("env");
    let ctx = ctx_at(dir.path());

    let debug = fetcher_debug(&ctx);
    assert!(
        debug.contains("curse_key: Some(ApiKey(<redacted>))"),
        "环境变量是兜底档，CLI 也得认：{debug}"
    );
    assert!(!debug.contains("env-only-key"), "{debug}");
}

/// 两档都没有（含空串 / 空白）⇒ **不硬塞一把空 Key**：fetcher 不带 Key，core 的 `curse_file` 于是
/// 在发请求**之前**回具名的 `NotConfigured`（那一条进 `missing`，其余条目照装、整包 `Partial`——
/// 那份行为在 core 的 `import_curseforge_key` 用例里钉着，本 crate 只钉「递进去的是 `None`」）。
#[test]
fn the_cli_fetcher_stays_keyless_when_nothing_is_configured() {
    let _lock = KEY_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _env = KeyEnv::set(None);
    let dir = TempDir::new("none");
    let ctx = ctx_at(dir.path());

    let debug = fetcher_debug(&ctx);
    assert!(
        debug.contains("curse_key: None"),
        "没配 Key 时不许往 core 塞一把空的：{debug}"
    );

    // 空串 / 纯空白同样是「没配」（M316 栽过这条）。
    write_key(&ctx, "   ");
    let blank = fetcher_debug(&ctx);
    assert!(
        blank.contains("curse_key: None"),
        "空白 Key 不是 Key：{blank}"
    );

    {
        let _blank_env = KeyEnv::set(Some("  "));
        let env_blank = fetcher_debug(&ctx);
        assert!(
            env_blank.contains("curse_key: None"),
            "空白环境变量不是 Key：{env_blank}"
        );
    }
}

/// 内存传输层：记下每次请求的请求头，回一份空正文（**不开 socket**，解析失败无所谓：
/// 这条用例只看「递出去的 Key 是哪一把」）。
struct SpyTransport {
    seen: Mutex<Vec<Vec<(String, String)>>>,
}

impl SpyTransport {
    fn new() -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
        }
    }

    /// 第一次请求里的 `x-api-key`（大小写不敏感）。
    fn sent_key(&self) -> Option<String> {
        self.seen
            .lock()
            .expect("seen 锁")
            .first()
            .and_then(|headers| {
                headers
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case("x-api-key"))
            })
            .map(|(_, value)| value.clone())
    }
}

impl unlucky_core::addon::curseforge::CfTransport for SpyTransport {
    fn get<'a>(
        &'a self,
        _url: &'a str,
        headers: &'a [(String, String)],
    ) -> unlucky_core::addon::curseforge::CfFuture<
        'a,
        Result<
            unlucky_core::addon::curseforge::CfResponse,
            unlucky_core::addon::curseforge::CfError,
        >,
    > {
        Box::pin(async move {
            self.seen.lock().expect("seen 锁").push(headers.to_vec());
            Ok(unlucky_core::addon::curseforge::CfResponse::new(
                200,
                None,
                String::new(),
            ))
        })
    }
}

/// 两档都有值时，CLI 递给 CurseForge 客户端的是**设置那一把**（离线假传输层，零 socket）。
///
/// 这条是「优先级改反」在 **CLI 侧**的判据：`ApiKey` 没有公开取值路，可用性也看不出谁赢，
/// 所以让 `curseforge_key` 的结果走一次真实客户端调用——值只能在请求头里被观测。
/// 与 presenter 的 `key_source::the_setting_key_is_the_one_that_reaches_the_wire` 同源同一份判据。
#[test]
fn the_setting_key_is_the_one_the_cli_hands_to_the_curseforge_client() {
    let _lock = KEY_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _env = KeyEnv::set(Some("env-wire-key"));
    let dir = TempDir::new("wire");
    let ctx = ctx_at(dir.path());
    write_key(&ctx, "settings-wire-key");

    let key = curseforge_key(&ctx).expect("两档都有值 ⇒ 必须有 Key");
    let spy = SpyTransport::new();
    let client = unlucky_core::addon::curseforge::CurseForge::with_key(&spy, &key);
    let query = unlucky_core::addon::curseforge::ModQuery {
        text: "sodium".to_string(),
        ..unlucky_core::addon::curseforge::ModQuery::default()
    };
    let _ = unlucky_core::download::blocking::block_on(client.search(&query));

    assert_eq!(
        spy.sent_key().as_deref(),
        Some("settings-wire-key"),
        "两档都有值时请求头里必须是设置那一把"
    );
}

// ------------------------------------- M319：人读输出不许泄漏 `%prog%` 占位符

use crate::cli::program::PLACEHOLDER;
use unlucky_core::addon::import::LoaderTarget;
use unlucky_core::addon::LoaderKind;

#[path = "tests/m321.rs"]
mod m321;

/// `pack import` 的人读行必须**渲染过**程序名。
///
/// M318 真包盘查 §4.3：`pack.rs` 最后一行用 `Report { data, human }` 结构体字面量返回，绕过了
/// `Report::new` 里的 `fill_program` ⇒ 用户原样看到「启动前先跑 `%prog% install` 装它」。
/// 抓得住：改回结构体字面量 ⇒ 占位符留在 `human` 里 ⇒ 本用例红。
#[test]
fn the_human_output_renders_the_program_placeholder() {
    let dir = TempDir::new("prog");
    let ctx = ctx_at(dir.path());
    let mut report = ImportReport::empty(PathBuf::from("C:/tmp/.minecraft"));
    // 那句 `%prog% install` 只在「清单声明了加载器、这次没装、本机也没有那个版本」时出现。
    report.loader = Some(LoaderTarget {
        kind: LoaderKind::Fabric,
        mc_version: "1.21.5".to_string(),
        loader_version: "0.16.14".to_string(),
        raw: "fabric-0.16.14".to_string(),
        version_id: "1.21.5-fabric-0.16.14".to_string(),
    });
    report.version_id = Some("1.21.5-fabric-0.16.14".to_string());
    let args = PackImportArgs {
        path: PathBuf::from("pack.mrpack"),
        target: None,
        name: None,
        overwrite: false,
        no_register: false,
    };

    let outcome = outcome(
        &ctx,
        &report,
        &PathBuf::from("C:/tmp/instance"),
        &[],
        &args,
        None,
    );

    let hint = outcome
        .report
        .human
        .iter()
        .find(|line| line.contains("install"))
        .unwrap_or_else(|| panic!("那句 install 提示没出现：{:?}", outcome.report.human));
    assert!(
        !hint.contains(PLACEHOLDER),
        "人读行里泄漏了程序名占位符：{hint}"
    );
    assert!(
        outcome
            .report
            .human
            .iter()
            .all(|line| !line.contains(PLACEHOLDER)),
        "整份人读输出都不许留占位符：{:?}",
        outcome.report.human
    );
}

#[path = "tests/m323.rs"]
mod m323;
#[path = "tests/m346.rs"]
mod m346;
