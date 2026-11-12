//! 下载阶段的**可测那一段**：把计划里的文件逐个下下来，失败进名单而不中断。
//!
//! # 为什么单独一个文件
//!
//! 这段逻辑以前整块躺在 `install::run` 里，而 `run` 需要实例目录 + 真引擎 ⇒ 没有用例能驱动它，
//! 于是「下载失败记进两档名单」这条改动**整段删掉也不会红一条**（M186 r2 复审实测：
//! 整段包进 `if false` 后 `cargo test -p unlucky-presenter --lib host::addon` 仍 36 passed）。
//! 抽成只依赖 [`ArtifactFetcher`] 的函数之后，假下载器就能驱动它。
//!
//! # 三条不变量（每条都有用例）
//!
//! 1. **失败不中断**：某个文件下载失败 ⇒ 记进 [`DependencyPlan::record_failure`]，继续下一个；
//! 2. **已装的不回滚**：成功的文件留在盘上，并出现在返回的「已装」名单里；
//! 3. **两处取消点**：进入本阶段先查一次（`run` 解析完就调它 ⇒ 解析阶段的取消生效，
//!    全部文件都「已存在」而 `nodes` 为空时也不会漏掉取消），循环每次迭代再查一次。

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use unlucky_core::addon::online::{
    download_verified, ArtifactFetcher, DependencyFailure, DependencyPlan, FailureKind,
    OnlineError, PlannedNode,
};
use unlucky_core::download::blocking::block_on;

use crate::dto::{ContentFailure, InstallProgressView};

use super::super::job::{HostTask, JobSender};
use super::install::{local, node_label, safe_file_name};

/// 下载失败 → 两档失败里的一条（`Unreachable` + 叶子成因 `cause`）。
///
/// 为什么归 `Unreachable`：`download_verified` 的失败就是「取不到这份字节」——没网 / 站点错 /
/// 内容与声明不符 / 本地 IO，叶子成因留在 `cause` 上，上层照 `sources::failure_of` 映射成
/// DTO 大类；**不是** `Incompatible`（换版本/加载器也救不了一次断网）。
#[must_use]
pub(super) fn download_failure(node: &PlannedNode, error: &OnlineError) -> DependencyFailure {
    DependencyFailure {
        requested_as: node.requested_as.clone(),
        required_by: node.required_by.clone(),
        depth: node.depth,
        kind: FailureKind::Unreachable,
        cause: Some(error.kind()),
        message: format!(
            "下载 {} 失败（{}）：{}",
            node.file.filename,
            error.kind().as_str(),
            error.message()
        ),
    }
}

/// 下载阶段：先查一次取消，再逐个下（失败进名单、不中断）。
///
/// 返回**真落盘**的节点（失败的不在里面），调用方据此拼「装了 N 个」。
///
/// # Errors
/// 用户取消时返回一条 `ContentFailure`（detail 以「已取消」开头，`install()` 据此画 `Cancelled`）。
pub(super) fn download_stage(
    plan: &mut DependencyPlan,
    target_dir: &Path,
    fetcher: &dyn ArtifactFetcher,
    cancel: &AtomicBool,
    progress: &mut InstallProgressView,
    tx: &JobSender,
    epoch: u64,
) -> Result<Vec<PlannedNode>, ContentFailure> {
    // 解析完立刻查一次：解析最多要跑 6 层版本请求，此时取消不该先下一份文件再说
    // （main 的 `resolve_file` 之后就有这条检查，r2 复审指出我们把它丢了）。
    if cancel.load(Ordering::SeqCst) {
        return Err(local("已取消（下载开始前）"));
    }
    let mut installed: Vec<PlannedNode> = Vec::new();
    for index in 0..plan.nodes.len() {
        if cancel.load(Ordering::SeqCst) {
            return Err(local("已取消（下载中途）"));
        }
        let node = plan.nodes[index].clone();
        progress.message = node_label(&node);
        let _ = tx.send(HostTask::Install {
            progress: Box::new(progress.clone()),
            epoch,
        });
        // M162：走 `online::download_verified`——sha1 交给引擎流式校验、sha512 落盘后复核，
        // 失败重试且不留半截文件（内容库安装与整合包导入用**同一段**校验，不各写一份）。
        let dest = target_dir.join(safe_file_name(&node.file.filename));
        let outcome = block_on(download_verified(
            fetcher,
            &node.file.url,
            node.file.sha1(),
            node.file.sha512(),
            (node.file.size > 0).then_some(node.file.size),
            &dest,
        ));
        match outcome {
            Ok(_) => installed.push(node),
            Err(error) => {
                // 下载阶段的失败**也记进同一套两档结构**（Unreachable + 叶子成因）：
                // 已下好的不回滚、失败项进「可只重试」，整份计划的上下文（缺的是谁的前置）不丢。
                plan.record_failure(download_failure(&node, &error));
                continue;
            }
        }
        progress.files_done += 1;
        let _ = tx.send(HostTask::Install {
            progress: Box::new(progress.clone()),
            epoch,
        });
    }
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex;

    use unlucky_core::addon::online::{
        ArtifactFetcher, DependencyPlan, FailureKind, FetchArgs, OnlineError, OnlineErrorKind,
        OnlineFuture, PlannedNode, ResolveRequest, VersionFile,
    };

    use crate::dto::InstallProgressView;

    use super::{download_failure, download_stage};

    /// 假下载器：`fail_on` 里的文件名一律「没联网」，其余写一行字节（**不校验摘要**）。
    struct FakeFetcher {
        fail_on: Vec<&'static str>,
        attempts: Mutex<Vec<String>>,
    }

    impl FakeFetcher {
        fn new(fail_on: &[&'static str]) -> Self {
            Self {
                fail_on: fail_on.to_vec(),
                attempts: Mutex::new(Vec::new()),
            }
        }

        fn attempts(&self) -> Vec<String> {
            self.attempts.lock().expect("锁没中毒").clone()
        }
    }

    impl ArtifactFetcher for FakeFetcher {
        fn fetch<'a>(&'a self, args: FetchArgs<'a>) -> OnlineFuture<'a, Result<u64, OnlineError>> {
            Box::pin(async move {
                let name = args
                    .dest
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.attempts.lock().expect("锁没中毒").push(name.clone());
                if self.fail_on.contains(&name.as_str()) {
                    return Err(OnlineError::offline("假装的断网"));
                }
                std::fs::write(args.dest, b"jar").map_err(|error| {
                    OnlineError::new(OnlineErrorKind::Io, format!("写盘失败：{error}"))
                })?;
                Ok(4)
            })
        }
    }

    fn node(project: &str, file: &str, required_by: &[&str]) -> PlannedNode {
        PlannedNode {
            project: project.to_string(),
            requested_as: project.to_string(),
            version_id: format!("ver-{project}"),
            version_name: project.to_string(),
            version_number: "1.0".to_string(),
            file: VersionFile {
                url: format!("https://cdn.modrinth.com/data/{project}/1.0/{file}"),
                filename: file.to_string(),
                size: 0,
                ..VersionFile::default()
            },
            depth: u8::from(!required_by.is_empty()),
            required_by: required_by
                .iter()
                .map(|parent| (*parent).to_string())
                .collect(),
        }
    }

    fn plan(nodes: Vec<PlannedNode>) -> DependencyPlan {
        DependencyPlan {
            request: ResolveRequest::single("AANobbMI"),
            nodes,
            skipped: Vec::new(),
            failures: Vec::new(),
            truncated: Vec::new(),
            unresolved_required: Vec::new(),
        }
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ull-m186-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    /// 失败的那一条**进名单**、父边与叶子成因都带着（守的是 [`download_failure`] 的映射）。
    #[test]
    fn a_download_error_becomes_an_unreachable_failure_with_its_leaf_cause() {
        let node = node("P7dR8mSH", "fabric-api.jar", &["AANobbMI"]);
        let failure = download_failure(&node, &OnlineError::offline("连不上 cdn.modrinth.com"));

        assert_eq!(failure.kind, FailureKind::Unreachable);
        assert_eq!(failure.cause, Some(OnlineErrorKind::NotConnected));
        assert_eq!(failure.requested_as, "P7dR8mSH");
        assert_eq!(failure.required_by, ["AANobbMI".to_string()]);
        assert_eq!(failure.depth, 1);
        assert!(
            failure.message.contains("fabric-api.jar"),
            "{}",
            failure.message
        );
        assert!(
            failure.message.contains("online_not_connected"),
            "成因要带 code：{}",
            failure.message
        );
    }

    /// 第 2 个文件失败：**不中断**、已装的不回滚、失败进名单、第 3 个仍会被尝试。
    #[test]
    fn a_failed_file_does_not_stop_the_rest_and_keeps_what_already_landed() {
        let dir = temp_dir("download-partial");
        let mut plan = plan(vec![
            node("AANobbMI", "sodium.jar", &[]),
            node("P7dR8mSH", "fabric-api.jar", &["AANobbMI"]),
            node("1eAoo2KR", "yacl.jar", &["P7dR8mSH"]),
        ]);
        let fetcher = FakeFetcher::new(&["fabric-api.jar"]);
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut progress = InstallProgressView {
            files_total: 3,
            ..InstallProgressView::new()
        };

        let installed = download_stage(
            &mut plan,
            &dir,
            &fetcher,
            &AtomicBool::new(false),
            &mut progress,
            &tx,
            7,
        )
        .expect("没取消就不该回 Err");

        let names: Vec<&str> = installed
            .iter()
            .map(|node| node.file.filename.as_str())
            .collect();
        assert_eq!(
            names,
            ["sodium.jar", "yacl.jar"],
            "成功的不回滚、后面的照下"
        );
        // 失败的那个会被 `download_verified` 重试 `RETRY_ATTEMPTS` 次（这里 3 次），
        // 关键是**它之后**还会去试第 3 个文件。
        let attempts = fetcher.attempts();
        assert_eq!(attempts.first().map(String::as_str), Some("sodium.jar"));
        assert_eq!(attempts.last().map(String::as_str), Some("yacl.jar"));
        assert_eq!(
            attempts
                .iter()
                .filter(|name| *name == "fabric-api.jar")
                .count(),
            3,
            "失败的尝试 + 重试都发生在第 3 个文件之前：{attempts:?}"
        );
        assert_eq!(plan.failures.len(), 1, "{:?}", plan.failures);
        assert_eq!(plan.failures[0].requested_as, "P7dR8mSH");
        assert!(plan.failures[0].message.contains("fabric-api.jar"));
        assert_eq!(progress.files_done, 2, "进度只数真落盘的");
        assert!(dir.join("sodium.jar").exists(), "已下好的文件不许删");
        assert!(!dir.join("fabric-api.jar").exists());
        assert!(dir.join("yacl.jar").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 解析完就取消 ⇒ 一条文件都不下（r2 nit②：main 有这条检查，我们曾把它丢了）。
    #[test]
    fn cancelling_right_after_resolution_touches_nothing() {
        let dir = temp_dir("download-cancelled");
        let mut plan = plan(vec![
            node("AANobbMI", "sodium.jar", &[]),
            node("P7dR8mSH", "fabric-api.jar", &["AANobbMI"]),
        ]);
        let fetcher = FakeFetcher::new(&[]);
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut progress = InstallProgressView::new();

        let error = download_stage(
            &mut plan,
            &dir,
            &fetcher,
            &AtomicBool::new(true),
            &mut progress,
            &tx,
            7,
        )
        .expect_err("取消必须回 Err");

        assert!(error.detail.contains("已取消"), "{}", error.detail);
        assert!(fetcher.attempts().is_empty(), "一份都不许下");
        assert!(plan.failures.is_empty(), "取消不是失败，不进名单");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
