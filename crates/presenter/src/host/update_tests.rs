//! 自更新宿主通道的单测（M159）。
//!
//! 全部离线：清单与产物都在临时目录里造（[`Source::Dir`]），**没有一个用例发公网请求**。
//! 覆盖 mission 的验收条目：
//!
//! | 用例 | 守什么 |
//! |---|---|
//! | [`version_matrix_decides_who_gets_prompted`] | 版本更高才提示；同版本/更低一律不提示；`v0.2.3` 这类真实 tag 形态 |
//! | [`a_broken_manifest_degrades_to_failure_not_up_to_date`] | 清单坏 JSON → 「检查失败」，**不是**「已是最新」 |
//! | [`a_higher_version_without_our_platform_is_not_up_to_date`] | 有新版但没本平台产物 → 单独的变体 |
//! | [`sha256_mismatch_refuses_the_apply_and_leaves_no_temp_file`] | 摘要不符拒绝应用、暂存文件不残留、目标没被换掉 |
//! | [`a_successful_apply_leaves_the_new_file_and_a_backup`] | 换上去的是新字节、旧字节留在备份里、回报是「就位」 |
//! | [`skipping_a_version_does_not_silence_the_next_one`] | 忽略的语义是**那一个版本**：同版本不再弹、更高版本照弹；手动检查清掉忽略 |
//! | [`auto_check_off_means_no_request_on_start`] | 关掉开关后启动不派发检查（结构性 + 状态两层） |
//! | [`the_gui_and_the_cli_share_one_kernel_path`] | GUI/CLI 走同一个 `Job`（源码锚点，不是口号） |

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

/// 造一个本地更新源目录（`dist/stable.json` + `dist/asset.exe`）。
fn dir_source(tmp: &TempDir, manifest: &str, payload: &[u8]) {
    tmp.write("dist/stable.json", manifest.as_bytes());
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

/// 坏 JSON / 缺字段 ⇒ **检查失败**，绝不是「已是最新」。
///
/// 两个变体的文案与后果完全不同：说成「已是最新」会让用户以为网络通、版本新，实际一个字节都没取到。
#[test]
fn a_broken_manifest_degrades_to_failure_not_up_to_date() {
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
        tmp.write("dist/stable.json", manifest.as_bytes());
        let outcome = outcome_of(job(&tmp).check(&version("0.2.2"), Channel::Stable));
        assert!(
            matches!(outcome, UpdateOutcome::Failed(_)),
            "{label}：坏清单必须降级成「检查失败」，实际 {outcome:?}"
        );
        let failure = outcome.failure().expect("失败带原因");
        assert!(
            !failure.message.trim().is_empty(),
            "{label}：失败要有人话原因"
        );
    }
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
        tmp.write("dist/stable.json", manifest.as_bytes());
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
    tmp.write(
        "dist/stable.json",
        format!(
            r#"{{"channel":"stable","releases":[{{"version":"0.9.0","channel":"stable","artifacts":[{{"target":"{}","format":"portable-exe","url":"asset.exe","size":3,"sha256":"{}"}}]}}]}}"#,
            // 故意给一个不可能匹配本机的目标（`unknown` 架构永远对不上）
            "linux-x86",
            sha256(b"abc")
        )
        .as_bytes(),
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
