//! 自更新宿主通道的单测（M159）。
//!
//! 全部离线：清单与产物都在临时目录里造（[`Source::Dir`]），**没有一个用例发公网请求**。
//! 覆盖 mission 的验收条目：
//!
//! | 用例 | 守什么 |
//! |---|---|
//! | [`version_matrix_decides_who_gets_prompted`] | 版本更高才提示；同版本/更低一律不提示；`v0.2.3` 这类真实 tag 形态 |
//! | [`a_broken_manifest_degrades_to_failure_not_up_to_date`] | 坏 JSON → `invalid_*`、签名被拒 → `signature_*`；两种都是「检查失败」，**不是**「已是最新」 |
//! | [`a_higher_version_without_our_platform_is_not_up_to_date`] | 有新版但没本平台产物 → 单独的变体 |
//! | [`sha256_mismatch_refuses_the_apply_and_leaves_no_temp_file`] | 摘要不符拒绝应用、暂存文件不残留、目标没被换掉 |
//! | [`a_successful_apply_leaves_the_new_file_and_a_backup`] | 换上去的是新字节、旧字节留在备份里、回报是「就位」 |
//! | [`skipping_a_version_does_not_silence_the_next_one`] | 忽略的语义是**那一个版本**：同版本不再弹、更高版本照弹；手动检查清掉忽略 |
//! | [`auto_check_off_means_no_request_on_start`] | 关掉开关后启动不派发检查（结构性 + 状态两层） |
//! | [`the_gui_and_the_cli_share_one_kernel_path`] | GUI/CLI 走同一个 `Job`（源码锚点，不是口号） |

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use unlucky_core::download::region::{RegionMode, RegionProfile};
use unlucky_core::download::verify::digest_bytes;
use unlucky_core::download::HashAlgo;
use unlucky_core::update::{Applied, Channel, Target, Version};

use crate::dto::{UpdateBanner, UpdateOutcome};

use super::super::env::HostEnv;
use super::super::update::{outcome_of, ApplyReport, Factory, Report};
use super::Runtime;

/// 一次性临时目录。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("ull-m159-{}-{tag}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("建临时目录");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("建父目录");
        }
        std::fs::write(&path, bytes).expect("写文件");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn sha256(bytes: &[u8]) -> String {
    digest_bytes(HashAlgo::Sha256, bytes)
}

/// 本平台在清单里的 `target` 写法。
fn here() -> String {
    Target::current().as_str().to_string()
}

/// 一份「本平台有产物」的清单（摘要按 `payload` 真实值算）。
fn manifest_for(version: &str, payload: &[u8], notes: &str) -> String {
    format!(
        r#"{{"channel":"stable","releases":[{{"version":"{version}","channel":"stable","notes":"{notes}",
        "artifacts":[{{"target":"{}","format":"portable-exe","url":"asset.exe","size":{},"sha256":"{}"}}]}}]}}"#,
        here(),
        payload.len(),
        sha256(payload)
    )
}

/// 测试签名钥的 32 字节种子。
///
/// **与 `crates/core/src/update/tests/support_sign.rs` 同源**（同一个种子、同一个 `key_id`、
/// 同一个信封形状）：那边是 core 单测的夹具，这里是 presenter 侧的**等价薄夹具**——presenter 不能
/// 依赖 core 的 `#[cfg(test)]` 资产，也不该把测试资产导出成生产 API。改一处就要改两处；
/// 对不上时用例会以「签名验不过 / 不认识的钥」红，而不是静默变绿。
const TEST_SIGNING_SEED: &[u8; 32] = b"ull-m190-test-key-seed-000000001";
/// 测试钥的 `key_id`（与 `crates/core/src/update/trust.rs::TEST_ONLY` 一致）。
const TEST_KEY_ID: &str = "test-only-key";

/// 用测试钥给清单签一份 `manifest.sig` 信封（M190 的信封形状：多签名数组 + 冗余摘要）。
///
/// 只有 `[dev-dependencies]` 打开了 core 的 `update-test-key`，所以这把公开的测试公钥在**测试构建**
/// 的信任表里；发布构建里没有它，也就装不了用公开种子签出来的清单。
fn signature_envelope(manifest: &str) -> String {
    use ed25519_dalek::{Signer, SigningKey};
    let signature = SigningKey::from_bytes(TEST_SIGNING_SEED)
        .sign(manifest.as_bytes())
        .to_bytes();
    let hex: String = signature.iter().map(|byte| format!("{byte:02x}")).collect();
    let digest = sha256(manifest.as_bytes());
    format!(
        r#"{{"format":1,"signatures":[{{"alg":"ed25519","key_id":"{TEST_KEY_ID}","sig":"{hex}","manifest_sha256":"{digest}"}}]}}"#
    )
}

/// 写一份清单**连它的签名**（`dist/stable.json` + `dist/stable.json.sig`）。
///
/// 真发布必须带 `.sig`（core 的 `verified_manifest` 先验签再解析），所以夹具一律成对落盘；
/// 「坏清单」「空清单」这些用例也照签——它们要证的是「签名对、内容坏/为空」时的行为，
/// 而不是绕过签名。
fn write_manifest(tmp: &TempDir, manifest: &str) {
    tmp.write("dist/stable.json", manifest.as_bytes());
    tmp.write(
        "dist/stable.json.sig",
        signature_envelope(manifest).as_bytes(),
    );
}

/// 造一个本地更新源目录（`dist/stable.json` + 它的签名 + `dist/asset.exe`）。
fn dir_source(tmp: &TempDir, manifest: &str, payload: &[u8]) {
    write_manifest(tmp, manifest);
    tmp.write("dist/asset.exe", payload);
}

/// 造一份装配：本地源 + 临时目标 + 临时暂存。
fn factory(tmp: &TempDir) -> Factory {
    Factory::for_env(&HostEnv::at_root(tmp.join("root")))
        .with_dir(tmp.join("dist"))
        .with_dest(tmp.join("bin/unlucky-launcher.exe"))
        .with_staging(tmp.join("staging"))
}

/// 一份 job（走同一份装配）。
fn job(tmp: &TempDir) -> unlucky_core::update::Job {
    factory(tmp).job()
}

fn version(text: &str) -> Version {
    Version::parse(text).expect("合法版本")
}

// ------------------------------------------------------------------ 版本矩阵

/// 版本更高才提示；同版本 / 更低一律不提示；真实 tag 形态（`v0.2.3`）照常解析。
///
/// 这条是 mission 第二条任务的核心判据，**判定本身在 core**（`UpdateChecker`），这里守的是
/// 「host 把它翻成 DTO 之后仍然如此」——两处任何一处把 `<=` 写成 `<` 都会红。
#[test]
fn version_matrix_decides_who_gets_prompted() {
    // (当前版本, 清单里的版本, 期望「有更新」)
    let cases = [
        ("0.2.2", "0.2.3", true),
        ("0.2.2", "v0.2.3", true),
        ("0.2.2", "0.2.2", false),
        ("0.2.2", "v0.2.2", false),
        ("0.2.2", "0.2.1", false),
        ("0.2.2", "0.10.0", true),
        ("0.2.2", "0.2.10", true),
        ("0.2.2", "0.2.2-rc.1", false),
        ("0.2.2-rc.1", "0.2.2", true),
    ];
    for (current, listed, expected) in cases {
        let tmp = TempDir::new("matrix");
        let payload = b"new";
        dir_source(&tmp, &manifest_for(listed, payload, ""), payload);
        let outcome = outcome_of(job(&tmp).check(&version(current), Channel::Stable));
        assert_eq!(
            outcome.candidate().is_some(),
            expected,
            "当前 {current} vs 清单 {listed}：期望 available={expected}，实际 {outcome:?}"
        );
        if !expected {
            assert!(
                matches!(outcome, UpdateOutcome::UpToDate { .. }),
                "不提示的时候必须是「已是最新」，实际 {outcome:?}"
            );
        }
    }
}

// ------------------------------------------------------------- 清单契约降级

/// 坏 JSON / 缺字段 / 签名被拒 ⇒ **检查失败**，绝不是「已是最新」；而且两种坏法**分类不同**。
///
/// M199 复审 P3-2：旧版只断言「结果是 Failed」，而它喂的清单都**签对了**——验签一旦 fail-open，
/// 坏签名清单会一路走到解析，坏 JSON 照样解析失败，断言全绿。现在内容坏必须落 `invalid_*`，
/// 签名坏必须落 `signature_*` 且不可重试。
/// 抓得住：验签被跳过/放行（② 会拿到 `Available 0.9.0`）、签名与解析互相误分类、失败被折叠成「已是最新」；
/// 抓不住：签名信封字段与钥表（在 core 的 `signature_cases.rs`），以及 `manifest_sha256` 那条冗余校验
/// （需「钥已钉住 + 方程验过 + 声明 sha 不同」，**全仓无用例**，已立 finding）。
#[test]
fn a_broken_manifest_degrades_to_failure_not_up_to_date() {
    // ① 签名是对的（`write_manifest` 成对落盘），内容坏：分类只能是「清单结构 / 版本号不合法」。
    for (label, manifest) in [
        ("not json", "{ this is not json".to_string()),
        (
            "wrong field type",
            r#"{"channel":"stable","releases":42}"#.to_string(),
        ),
        (
            "bad digest",
            format!(
                r#"{{"channel":"stable","releases":[{{"version":"0.9.0","channel":"stable","artifacts":[{{"target":"{}","format":"portable-exe","url":"a.exe","size":3,"sha256":"zz"}}]}}]}}"#,
                here()
            ),
        ),
        (
            "missing sha256",
            format!(
                r#"{{"channel":"stable","releases":[{{"version":"0.9.0","channel":"stable","artifacts":[{{"target":"{}","format":"portable-exe","url":"asset.exe","size":3}}]}}]}}"#,
                here()
            ),
        ),
        ("bad version", manifest_for("not-a-version", b"abc", "")),
    ] {
        let tmp = TempDir::new("badmanifest");
        write_manifest(&tmp, &manifest);
        let outcome = outcome_of(job(&tmp).check(&version("0.2.2"), Channel::Stable));
        let Some(failure) = outcome.failure() else {
            panic!("{label}：坏清单必须降级成「检查失败」，实际 {outcome:?}");
        };
        let kind = failure.kind.as_str();
        assert!(
            matches!(kind, "invalid_manifest" | "invalid_version"),
            "{label}：坏内容分类是 {kind}"
        );
        assert!(!failure.message.trim().is_empty(), "{label}：失败没原因");
    }
    // ② 清单字节是好的（放行就会报「有更新 0.9.0」），只有签名对不上：必须落 `signature_invalid`。
    let tmp = TempDir::new("badsignature");
    tmp.write(
        "dist/stable.json",
        manifest_for("0.9.0", b"new", "").as_bytes(),
    );
    tmp.write(
        "dist/stable.json.sig",
        signature_envelope("another").as_bytes(),
    );
    let outcome = outcome_of(job(&tmp).check(&version("0.2.2"), Channel::Stable));
    let Some(failure) = outcome.failure() else {
        panic!("签名验不过必须拒装（放行就会报「有更新 0.9.0」），实际 {outcome:?}");
    };
    assert_eq!(failure.kind, "signature_invalid", "签名被拒的分类");
    assert!(!failure.retryable, "签名问题不可重试");
}

/// 清单里**一条发布都没有**（`{"channel":"stable"}` / 空 `releases`）⇒ 已是最新，不是失败。
///
/// 这是 core 的既定口径（「该渠道还没发过版」不是错误），与上面那条「坏清单」要分得清清楚楚：
/// 少一个字段是坏文件，空列表是「还没发」。两者都被报成同一种状态的话，发布脚本第一次上线
/// 之前所有人都会看到红字。
#[test]
fn an_empty_manifest_means_nothing_published_yet() {
    for manifest in [
        r#"{"channel":"stable"}"#.to_string(),
        r#"{"channel":"stable","releases":[]}"#.to_string(),
    ] {
        let tmp = TempDir::new("empty-manifest");
        write_manifest(&tmp, &manifest);
        let outcome = outcome_of(job(&tmp).check(&version("0.2.2"), Channel::Stable));
        assert!(
            matches!(&outcome, UpdateOutcome::UpToDate { latest: None }),
            "空清单 = 还没发过版，实际 {outcome:?}"
        );
    }
}

/// 有更高的版本、但里面没有本平台的产物 ⇒ 单独的变体（不是「已是最新」，也不是「有更新」）。
#[test]
fn a_higher_version_without_our_platform_is_not_up_to_date() {
    let tmp = TempDir::new("missing-artifact");
    write_manifest(
        &tmp,
        &format!(
            r#"{{"channel":"stable","releases":[{{"version":"0.9.0","channel":"stable","artifacts":[{{"target":"{}","format":"portable-exe","url":"asset.exe","size":3,"sha256":"{}"}}]}}]}}"#,
            // 故意给一个不可能匹配本机的目标（`unknown` 架构永远对不上）
            "linux-x86",
            sha256(b"abc")
        ),
    );
    let outcome = outcome_of(job(&tmp).check(&version("0.2.2"), Channel::Stable));
    assert!(
        matches!(&outcome, UpdateOutcome::MissingArtifact { version } if version == "0.9.0"),
        "缺本平台产物要单独报出来，实际 {outcome:?}"
    );
    assert_eq!(outcome.as_str(), "missing_artifact");
}

// 两个子模块各守一段（拆开只为守 400 行硬线；夹具在本模块，子模块 `use super::*` 共用，
// 不重复抄一份）。
#[path = "update_apply_tests.rs"]
mod apply;
#[path = "update_session_tests.rs"]
mod session;

// ---------------------------------------------------------------- M165 复审 P2-1
//
// 这两条走的是**真实的命令路径**（`AppCommand::SetSetting` → `dispatch` → `settings::set`），
// 不是直接调内部函数：要守的正是「动作路径上有没有把该做的事做全」。

/// 「重新检测地区」之后，**自更新手里的引擎也要换**。
///
/// 自更新的 `Factory` 从 `CoreHost::build` 起就把引擎存在字段里（常驻到进程结束）；
/// `download::shared::forget` 只让登记处不再持有那一份，**换不掉**它。于是只 forget 的话，
/// 自更新会继续按**旧地区档案**展开候选，直到重启——M165 复审 P2-1 的那个洞。
///
/// 抓得住的变异：删掉 `host/dispatch.rs` 的 `SetSetting` 成功分支里那句
/// `self.refresh_update_factory()` ⇒ 本用例红在 `Arc::ptr_eq`（Factory 还是淘汰前那一份）。
#[test]
fn detect_region_rebuilds_the_update_factory_engine() {
    use std::sync::Arc;
    use unlucky_core::download::shared;

    let tmp = TempDir::new("region-refresh");
    let env = HostEnv::at_root(tmp.path());
    std::fs::create_dir_all(env.layout.cache()).expect("建缓存目录");
    // 钉一份确定的地区档案：第一次构造不依赖本机时区/语言，断言才有确定的起点。
    let region_path = env.layout.cache().join("region.json");
    unlucky_core::download::region::store_cached(
        &region_path,
        &RegionProfile::pinned(RegionMode::Direct, unlucky_core::download::cache::now_ms()),
    )
    .expect("写 region.json");

    let mut host = crate::host::CoreHost::headless(env.clone());
    let cache_root = env.layout.cache();
    assert_eq!(
        shared::constructions(&cache_root),
        1,
        "装配时只该造一个引擎"
    );
    let before = host
        .update
        .factory_engine()
        .expect("生产装配（Default）应有引擎")
        .clone();
    assert_eq!(before.region().mode, RegionMode::Direct);

    host.dispatch_command(crate::dto::AppCommand::SetSetting {
        key: "download.action.detect_region".to_string(),
        value: String::new(),
    });

    let after = host
        .update
        .factory_engine()
        .expect("重建后仍该有引擎")
        .clone();
    assert!(
        !Arc::ptr_eq(&before, &after),
        "「重新检测地区」必须换掉自更新用的引擎（否则它按旧地区档案跑到进程结束）"
    );
    assert!(
        Arc::ptr_eq(&after, &crate::host::engine::engine(&env)),
        "自更新必须指向登记处的当前引擎，而不是被淘汰的那一份"
    );
    assert_eq!(
        shared::constructions(&cache_root),
        2,
        "淘汰一次 = 多构造一次（既不多也不少）"
    );
}

/// 重建 `Factory` **不打断**在飞的检查/应用：在跑的那条 `Job` 派发时就已经把装配克隆走了。
#[test]
fn rebuilding_the_factory_does_not_touch_a_running_job() {
    let tmp = TempDir::new("factory-rebuild");
    dir_source(&tmp, &manifest_for("0.9.0", b"new", "notes"), b"new");
    let env = HostEnv::at_root(tmp.path());
    let mut runtime = Runtime::with_factory(&env, factory(&tmp));

    // 派发一条（模拟「正在查」），再重建装配：
    let job = runtime.begin_check(true).expect("应能派发");
    assert!(runtime.inflight());
    runtime.rebuild_factory(&env);
    assert!(runtime.inflight(), "重建装配不该清掉在飞标记");
    assert!(
        matches!(runtime.view().outcome, UpdateOutcome::Checking),
        "重建装配不该改状态快照"
    );
    // 在飞那条 Job 仍然可用（它拿着自己那份装配），能正常跑完并回报。
    let outcome = outcome_of(job.check(&version("0.2.2"), Channel::Stable));
    assert_eq!(outcome.as_str(), "available");
}
