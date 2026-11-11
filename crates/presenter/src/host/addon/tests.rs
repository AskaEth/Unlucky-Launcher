//! 内容库宿主的守门测试（M109）：编码、文件名穿越、退避判定、两个站点的方言映射。

use std::time::Duration;

use super::http::{encode, retry_after, retryable};
use super::install::safe_file_name;
use super::sources::{all_kinds, curseforge_loader, date_part, failure_of};
use crate::dto::{ContentFailureKind, ContentKind};
use unlucky_core::addon::online::{OnlineError, OnlineErrorKind};

#[test]
fn percent_encoding_keeps_the_query_readable_and_escapes_the_rest() {
    assert_eq!(encode("1.20.1"), "1.20.1");
    assert_eq!(
        encode("[\"versions:1.20.1\"]"),
        "%5B%22versions%3A1.20.1%22%5D"
    );
    assert_eq!(encode("中文"), "%E4%B8%AD%E6%96%87");
    assert_eq!(encode("a b"), "a%20b");
}

#[test]
fn file_names_never_carry_a_path() {
    // 抓得住：站点给的 `../../evil.jar` 落到实例目录之外（穿越）。
    assert_eq!(safe_file_name("sodium.jar"), "sodium.jar");
    assert_eq!(safe_file_name("../../evil.jar"), "evil.jar");
    assert_eq!(safe_file_name("mods\\jei.jar"), "jei.jar");
    assert_eq!(safe_file_name(".."), "download.bin");
    assert_eq!(safe_file_name(""), "download.bin");
}

#[test]
fn dates_are_sliced_not_guessed() {
    assert_eq!(date_part("2024-05-01T12:00:00Z"), "2024-05-01");
    assert_eq!(date_part("2024-05"), "2024-05");
    assert_eq!(date_part(""), "");
}

#[test]
fn only_rate_limited_and_transient_failures_are_retried() {
    assert!(retryable("HTTP 429 (retry_after=Some(3))"));
    assert!(retryable("HTTP 503 (retry_after=None)"));
    assert!(!retryable("HTTP 404 (retry_after=None)"));
    assert!(
        !retryable("HTTP 401 (retry_after=None)"),
        "密钥错重试只会更糟"
    );
}

#[test]
fn retry_after_prefers_the_server_and_falls_back_to_exponential() {
    assert_eq!(
        retry_after("HTTP 429 (retry_after=Some(7))", 0),
        Duration::from_secs(7)
    );
    assert_eq!(
        retry_after("HTTP 429 (retry_after=None)", 0),
        Duration::from_secs(1)
    );
    assert_eq!(
        retry_after("HTTP 429 (retry_after=None)", 2),
        Duration::from_secs(4)
    );
    // 服务端要得再多也不等超过 30s（交互与截图不能被一次退避挂死）
    assert_eq!(
        retry_after("HTTP 429 (retry_after=Some(900))", 0),
        Duration::from_secs(30)
    );
}

#[test]
fn curseforge_loader_ids_follow_the_official_enum() {
    assert_eq!(curseforge_loader(1), Some("forge"));
    assert_eq!(curseforge_loader(4), Some("fabric"));
    assert_eq!(curseforge_loader(5), Some("quilt"));
    assert_eq!(curseforge_loader(6), Some("neoforge"));
    assert_eq!(curseforge_loader(99), None);
}

#[test]
fn the_four_content_kinds_are_the_four_side_entries() {
    assert_eq!(
        all_kinds().map(ContentKind::slug),
        ["modpack", "mod", "resourcepack", "shader"]
    );
}

// ------------------------------------------------ M162：失败分类（core → DTO）

/// core 的 `OnlineErrorKind` → DTO 的 `ContentFailureKind`：**一一映射，不重新判**。
///
/// 「没联网」是单独一类：页面据此画离线态（说「检查网络」），而不是画「换个关键词」。
#[test]
fn core_errors_map_to_dto_failure_kinds() {
    let cases = [
        (OnlineErrorKind::NotConnected, ContentFailureKind::Offline),
        (OnlineErrorKind::NotFound, ContentFailureKind::NotFound),
        (OnlineErrorKind::Server, ContentFailureKind::Server),
        (
            OnlineErrorKind::Unsupported,
            ContentFailureKind::Unsupported,
        ),
        (OnlineErrorKind::Malformed, ContentFailureKind::Other),
        (OnlineErrorKind::Io, ContentFailureKind::Other),
        (OnlineErrorKind::Integrity, ContentFailureKind::Other),
    ];
    for (core, expected) in cases {
        let failure = failure_of(OnlineError::new(core, "x"));
        assert_eq!(failure.kind, expected, "{} 映射错了", core.as_str());
    }
    assert!(failure_of(OnlineError::offline("connect timed out")).is_offline());
    // 详情/版本列表的文案要带出 core 的原话（不是二次包装的「出错了」）
    assert_eq!(
        failure_of(OnlineError::offline("connect timed out")).detail,
        "connect timed out"
    );
}

// ------------------------------------------------ M186：前置解析的两档失败与可重试名单

use super::install::finish;
use super::plan::{failure_of_plan, summary};
use crate::dto::{InstallProgressView, InstallStage};
use unlucky_core::addon::online::{
    DependencyFailure, DependencyPlan, FailureKind, PlannedNode, ResolveRequest, SkipReason,
    SkippedNode, UnresolvedRequired, VersionFile,
};

fn node(project: &str, file: &str, depth: u8, required_by: &[&str]) -> PlannedNode {
    PlannedNode {
        project: project.to_string(),
        requested_as: project.to_string(),
        version_id: format!("ver-{project}"),
        version_name: project.to_string(),
        version_number: "1.0".to_string(),
        file: VersionFile {
            url: format!("https://cdn.modrinth.com/data/{project}/1.0/{file}"),
            filename: file.to_string(),
            size: 1024,
            ..VersionFile::default()
        },
        depth,
        required_by: required_by
            .iter()
            .map(|parent| (*parent).to_string())
            .collect(),
    }
}

fn failure(
    project: &str,
    kind: FailureKind,
    cause: Option<OnlineErrorKind>,
    message: &str,
) -> DependencyFailure {
    DependencyFailure {
        requested_as: project.to_string(),
        required_by: vec!["AANobbMI".to_string()],
        depth: 1,
        kind,
        cause,
        message: message.to_string(),
    }
}

fn plan(
    failures: Vec<DependencyFailure>,
    nodes: Vec<PlannedNode>,
    skipped: Vec<SkippedNode>,
) -> DependencyPlan {
    DependencyPlan {
        request: ResolveRequest::single("sodium"),
        nodes,
        skipped,
        failures,
        truncated: Vec::new(),
        unresolved_required: Vec::new(),
    }
}

/// 全是「不兼容」时：说换版本/加载器，而不是「下载失败」，并且照样给重试名单。
#[test]
fn an_incompatible_only_plan_is_not_reported_as_a_download_failure() {
    let plan = plan(
        vec![failure(
            "P7dR8mSH",
            FailureKind::Incompatible,
            None,
            "没有匹配所选游戏版本/加载器（1.20.1 / fabric）的版本",
        )],
        vec![node("AANobbMI", "sodium.jar", 0, &[])],
        Vec::new(),
    );
    let failure = failure_of_plan(&plan, 0).expect("有失败就该给一条");
    // M191：「不兼容」有了自己的大类（以前借 `Unsupported`，与「缺密钥」共用一档）
    assert_eq!(failure.kind, ContentFailureKind::Incompatible);
    assert!(failure.kind.is_incompatible());
    assert!(!failure.is_offline());
    assert!(failure.detail.contains("不兼容所选版本/加载器 1 个"));
    assert!(failure.detail.contains("可只重试这 1 个：P7dR8mSH"));
    assert!(!failure.detail.contains("下载失败"), "{}", failure.detail);
    assert!(
        !failure.detail.contains("取不到"),
        "两档不许混：{}",
        failure.detail
    );
}

/// 两档同时出现时：两段都写，大类按「用户接下来该做什么」选（查无此项 > 不兼容）。
#[test]
fn a_mixed_plan_keeps_both_buckets_and_picks_the_actionable_kind() {
    let plan = plan(
        vec![
            failure(
                "P7dR8mSH",
                FailureKind::Incompatible,
                None,
                "没有匹配的版本",
            ),
            failure(
                "1eAoo2KR",
                FailureKind::Unreachable,
                Some(OnlineErrorKind::NotFound),
                "取版本失败（online_not_found）：HTTP 404",
            ),
        ],
        Vec::new(),
        Vec::new(),
    );
    let failure = failure_of_plan(&plan, 0).expect("有失败就该给一条");
    assert_eq!(failure.kind, ContentFailureKind::NotFound);
    assert!(failure.detail.contains("不兼容所选版本/加载器 1 个"));
    assert!(failure.detail.contains("取不到（网络 / 项目不存在）1 个"));
    assert!(failure.detail.contains("可只重试这 2 个"));
}

/// 没网那一档：大类是离线（页面据此画「检查网络」而不是「换个关键词」）。
#[test]
fn an_offline_jump_wins_over_everything_else() {
    let plan = plan(
        vec![
            failure(
                "P7dR8mSH",
                FailureKind::Incompatible,
                None,
                "没有匹配的版本",
            ),
            failure(
                "1eAoo2KR",
                FailureKind::Unreachable,
                Some(OnlineErrorKind::NotConnected),
                "连不上 api.modrinth.com",
            ),
        ],
        Vec::new(),
        Vec::new(),
    );
    let failure = failure_of_plan(&plan, 0).expect("有失败就该给一条");
    assert_eq!(failure.kind, ContentFailureKind::Offline);
    assert!(failure.is_offline());
}

/// 全装成的那句话要点出「含 N 个前置」与「跳过 M 个已存在」。
#[test]
fn the_success_message_counts_dependencies_and_skipped_files() {
    let plan = plan(
        Vec::new(),
        vec![
            node("AANobbMI", "sodium.jar", 0, &[]),
            node("P7dR8mSH", "fabric-api.jar", 1, &["AANobbMI"]),
        ],
        vec![SkippedNode {
            project: "1eAoo2KR".to_string(),
            requested_as: "1eAoo2KR".to_string(),
            file_name: "yacl.jar".to_string(),
            depth: 2,
            required_by: vec!["P7dR8mSH".to_string()],
            reason: SkipReason::AlreadyPresent,
        }],
    );
    assert!(failure_of_plan(&plan, 2).is_none());
    let installed = plan.nodes.clone();
    let message = summary(&plan, &installed);
    assert!(
        message.contains("装了 2 个文件（含 1 个前置）"),
        "{message}"
    );
    // 「为什么装」：前置那句要带出要求它的项目
    assert!(
        message.contains("fabric-api.jar（前置，被 AANobbMI 要求）"),
        "{message}"
    );
    assert!(message.contains("跳过 1 个（目录里已存在，没有重复下载）：yacl.jar"));
}

/// 扫目录只认文件、不认子目录（子目录不是 jar，混进来会让去重误判）。
#[test]
fn present_files_lists_only_files_in_the_target_directory() {
    let dir = std::env::temp_dir().join(format!("ull-m186-present-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("nested")).expect("建临时目录");
    std::fs::write(dir.join("sodium.jar"), b"x").expect("写文件");
    std::fs::write(dir.join("yacl.jar.disabled"), b"x").expect("写文件");
    let mut names: Vec<String> = super::plan::present_files(&dir)
        .into_iter()
        .map(|file| file.name)
        .collect();
    names.sort();
    assert_eq!(names, ["sodium.jar", "yacl.jar.disabled"]);
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------ M186 r1：完成态进度、下载失败上下文、无地址前置

/// 完成态不许把进度倒回 `1/N`（r1 P2-3：这里曾写死 `files_done = 1`）。
#[test]
fn finishing_aligns_the_terminal_progress_with_the_total() {
    let mut progress = InstallProgressView {
        files_total: 3,
        files_done: 3,
        stage: InstallStage::Downloading,
        ..InstallProgressView::new()
    };
    finish(&mut progress);
    assert_eq!(progress.stage, InstallStage::Done);
    assert_eq!(progress.files_done, progress.files_total);
    assert_eq!(progress.ratio(), 1.0, "装完了条必须满");

    // 全部跳过（total = 0）时也不许变成 1/0
    let mut skipped_all = InstallProgressView {
        files_total: 0,
        ..InstallProgressView::new()
    };
    finish(&mut skipped_all);
    assert_eq!(skipped_all.files_done, 0);
    assert_eq!(skipped_all.stage, InstallStage::Done);
}

/// 下载阶段的失败也要带上下文：已装几个 + 缺的是谁的前置 + 可只重试谁。
#[test]
fn a_download_failure_keeps_the_already_installed_context() {
    let plan = plan(
        vec![failure(
            "P7dR8mSH",
            FailureKind::Unreachable,
            Some(OnlineErrorKind::NotConnected),
            "下载 fabric-api.jar 失败（online_not_connected）：连不上 cdn.modrinth.com",
        )],
        vec![node("AANobbMI", "sodium.jar", 0, &[])],
        Vec::new(),
    );
    let failure = failure_of_plan(&plan, 1).expect("有失败就该给一条");
    assert_eq!(failure.kind, ContentFailureKind::Offline);
    assert!(
        failure.detail.contains("已装入 1 个文件（已下好的不回滚）"),
        "{}",
        failure.detail
    );
    assert!(failure.detail.contains("取不到（网络 / 项目不存在）1 个"));
    assert!(failure.detail.contains("可只重试这 1 个：P7dR8mSH"));
}

/// 只有版本 id、没有项目 id 的必备前置：既装不了，也不许静默吞掉。
#[test]
fn a_required_dependency_without_a_project_id_is_reported() {
    let mut plan = plan(
        Vec::new(),
        vec![node("AANobbMI", "sodium.jar", 0, &[])],
        Vec::new(),
    );
    plan.unresolved_required = vec![UnresolvedRequired {
        parent: "AANobbMI".to_string(),
        version_id: Some("ver-embedded".to_string()),
        file_name: Some("embedded-lib.jar".to_string()),
    }];
    let installed = plan.nodes.clone();
    let message = summary(&plan, &installed);
    assert!(
        message.contains("有 1 条必备前置只给了版本/文件名、没有项目 id，无法自动解析：ver-embedded（被 AANobbMI 要求）"),
        "{message}"
    );
}

// M191：「只重试这些」的名单与重试路径（按主题分卷，父文件已贴 400 行硬线）
mod retry;
