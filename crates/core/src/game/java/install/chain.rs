//! `install` 回退链的骨架：一次尝试的判据（取到 / 版本不符 / 失败）与「按顺序试、命中即停」。
//!
//! 单独成模块有两个理由：
//!
//! 1. `install.rs` 贴着 400 行硬线（M280 r1 加了这些守卫后到 440+）；
//! 2. 这段是**纯判断**——真正的「取版本」由闭包注入，所以「版本不符 ⇒ 记失败继续走链」
//!    与「钉死一家时只试它一家」都能**离线**测。复审的变异 A（把
//!    `catalog::version_mismatch` 改成恒 `None`）就是靠这里的用例变红的：没有它，
//!    「绝不把 latest 当用户要的版本装下去」只有代码、没有守卫。

use std::future::Future;

use super::super::catalog;
use super::super::error::JavaError;
use super::super::release::JdkRelease;
use super::super::vendor::JdkVendor;

/// 一次尝试的结果：`Ok` = 这一版就是用户要的；`Err` = 一句话原因（进降级记录）。
pub(super) type Attempt = Result<JdkRelease, String>;

/// 一次尝试的**判据**：取不到 → 原因；取到了但版本不是用户点名的那一版 → 拒绝。
///
/// `fetch` 由调用方给（真跑是 `source::fetch`，单测是夹具），所以这道闸离线可测。
/// `wanted = None`（没钉版本）时恒放行——默认路径不受任何影响。
pub(super) async fn resolve_one<F, Fut>(wanted: Option<&str>, fetch: F) -> Attempt
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<JdkRelease, JavaError>>,
{
    match fetch().await {
        Err(error) => Err(error.message().to_string()),
        Ok(release) => match catalog::version_mismatch(wanted, &release.version) {
            Some(reason) => Err(reason),
            None => Ok(release),
        },
    }
}

/// 回退链骨架：按顺序试，第一个通过的即命中；失败与版本不符都变成一条原因继续下一家。
///
/// 返回 `(命中的发行版与版本, 每家失败的原因)`。`plan`（`--dry-run`）用它；`install` 还要记
/// `ChainLog` 与「不支持该大版本」的跳过原因，所以自己驱动循环，但**唯一入口仍是
/// [`resolve_one`]**——两边的判据只有一份。
pub(super) async fn chain_fetch<F, Fut>(
    vendors: &[JdkVendor],
    wanted: Option<&str>,
    mut fetch: F,
) -> (Option<(JdkVendor, JdkRelease)>, Vec<(JdkVendor, String)>)
where
    F: FnMut(JdkVendor) -> Fut,
    Fut: Future<Output = Result<JdkRelease, JavaError>>,
{
    let mut failures: Vec<(JdkVendor, String)> = Vec::new();
    for candidate in vendors {
        let vendor = *candidate;
        match resolve_one(wanted, || fetch(vendor)).await {
            Ok(release) => return (Some((vendor, release)), failures),
            Err(reason) => failures.push((vendor, reason)),
        }
    }
    (None, failures)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::download::RegionProfile;
    use crate::download::{
        ContentCache, DownloadEngine, EngineConfig, HealthRegistry, MirrorTable,
    };
    use crate::game::java::error::JavaError;
    use crate::game::java::install::{self, InstallRequest, JavaInstallRoot};
    use crate::game::java::release::{JdkArchive, JdkRelease};
    use crate::game::java::vendor::JdkVendor;

    use super::{chain_fetch, resolve_one};

    fn release(version: &str) -> JdkRelease {
        JdkRelease {
            vendor: JdkVendor::Temurin,
            major: 21,
            version: version.to_string(),
            semver: None,
            release_name: format!("jdk-{version}"),
            archive: JdkArchive {
                name: format!("jdk-{version}.zip"),
                url: "https://example.invalid/x.zip".to_string(),
                sha256: "a".repeat(64),
                size: 1,
            },
        }
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ull-javapin-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    /// **P2-2 守卫（兜底闸）**：拿到的版本不是点名的那一版 ⇒ 拒绝，原因里带双方版本号。
    ///
    /// 变异证明（复审的变异 A）：把 `catalog::version_mismatch` 改成恒 `None`
    /// ⇒ 本用例第一条红（`expect_err` 拿到 `Ok`）。
    #[tokio::test]
    async fn a_release_that_is_not_the_requested_version_is_rejected() {
        let refused = resolve_one(Some("21.0.12"), || async { Ok(release("21.0.13+9")) })
            .await
            .expect_err("要 21.0.12、供应商回 latest 21.0.13+9 ⇒ 必须拒绝");
        assert!(refused.contains("21.0.13+9"), "{refused}");
        assert!(refused.contains("不是请求的 21.0.12"), "{refused}");

        // 相符 ⇒ 放行（公共部分一致也算相符）。
        assert!(
            resolve_one(Some("21.0.12"), || async { Ok(release("21.0.12+8")) })
                .await
                .is_ok()
        );
        // 没钉版本 ⇒ 恒放行（默认路径不许误报）。
        assert!(resolve_one(None, || async { Ok(release("21.0.13+9")) })
            .await
            .is_ok());
        // 真正取不到 ⇒ 原因原样带出来。
        let failed = resolve_one(Some("21.0.12"), || async {
            Err::<JdkRelease, JavaError>(JavaError::new(
                crate::game::java::JavaErrorKind::Network,
                "连不上",
            ))
        })
        .await
        .expect_err("取不到就得报错");
        assert_eq!(failed, "连不上");
    }

    /// **P2-2 守卫（继续走链）**：第一家回落的 latest 必须被记成失败，第二家（真给了那一版）命中。
    ///
    /// 变异证明：把 `chain_fetch` 的 `Err(reason) => failures.push(..)` 改成直接返回
    /// ⇒ 本用例红（命中变成第一家那个不符的版本）。
    #[tokio::test]
    async fn a_mismatch_records_a_failure_and_the_chain_moves_on() {
        let vendors = [JdkVendor::Temurin, JdkVendor::Oracle];
        let (hit, failures) = chain_fetch(&vendors, Some("21.0.12"), |vendor| async move {
            match vendor {
                // 第一家没按版本取（就是复审担心的那种 bug：静默回落 latest）。
                JdkVendor::Temurin => Ok(release("21.0.13+9")),
                _ => Ok(release("21.0.12+8")),
            }
        })
        .await;

        let (vendor, picked) = hit.expect("第二家应命中");
        assert_eq!(vendor, JdkVendor::Oracle, "命中不许是第一家");
        assert_eq!(picked.version, "21.0.12+8");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, JdkVendor::Temurin);
        assert!(failures[0].1.contains("不是请求的 21.0.12"));
    }

    /// 全都不符 ⇒ 没有命中，每家一条原因（命令据此退出非 0 并列出全部原因）。
    #[tokio::test]
    async fn nothing_matching_means_no_hit_and_every_reason_is_kept() {
        let vendors = [JdkVendor::Temurin, JdkVendor::Oracle];
        let (hit, failures) = chain_fetch(&vendors, Some("21.0.12"), |_vendor| async {
            Ok(release("21.0.13+9"))
        })
        .await;
        assert!(hit.is_none(), "一家都没给对版本 ⇒ 不许命中");
        assert_eq!(failures.len(), 2);
        assert!(failures
            .iter()
            .all(|(_, reason)| reason.contains("不是请求的")));
    }

    /// **P2-2 守卫（不换家）**：钉死一家、而这家不支持指定版本 ⇒ `install` 只试它一家
    /// （错误里的「试过 N」必须是 1），一个请求都不发。
    ///
    /// 变异证明：把 `source::pinned` 里 Zulu 的拒绝改成回落 `fetch_latest`
    /// ⇒ 本用例红（错误里不再有 `latest=true`，且「试过」不再是 1 条原因）。
    #[tokio::test]
    async fn a_pinned_vendor_that_cannot_pin_is_tried_alone() {
        let dir = temp_dir("pinned-alone");
        let cache = Arc::new(ContentCache::open(dir.join("cache")));
        let engine = DownloadEngine::new(
            EngineConfig::default(),
            Arc::new(MirrorTable::builtin()),
            Arc::new(RegionProfile::direct(0)),
            Arc::new(HealthRegistry::in_memory()),
            cache,
        );
        let location = JavaInstallRoot::custom(dir.join("java-managed"));
        let request = InstallRequest::with_vendor(21, JdkVendor::Zulu)
            .with_version(Some("21.0.13".to_string()));

        let error = install::install(&engine, &location, request)
            .await
            .expect_err("zulu 上游只有 latest ⇒ 必须报错");
        assert!(
            error.message().contains("试过 1"),
            "只许试点名的那一家：{}",
            error.message()
        );
        assert!(
            error.message().contains("latest=true"),
            "{}",
            error.message()
        );
        assert!(
            error
                .hint()
                .unwrap_or_default()
                .contains("--vendor temurin"),
            "要给出能做的下一步：{:?}",
            error.hint()
        );
        // 一个字节都没落盘（连目录都不该建出来）。
        assert!(!dir.join("java-managed").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
