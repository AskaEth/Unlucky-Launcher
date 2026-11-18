//! 依赖解析的离线用例（录制 JSON 夹具，**不打真实网络**）。
//!
//! 三条守门对应任务书的三条：
//! 1. [`two_nested_levels_are_pulled_in_a_predictable_order`] —— 嵌套两层能拉全，顺序确定；
//! 2. [`a_dependency_cycle_terminates_and_each_project_appears_once`] —— 环不无限递归；
//! 3. [`an_incompatible_dependency_lands_in_the_retry_list`] —— 不兼容进重试名单，不静默丢。
//!
//! 每条都配过一次「把实现改坏就该红」的变异（过程写在 M186 报告里），所以断言不是装饰。

use crate::addon::online::dependency::{
    present_key, DependencyPlan, FailureKind, PresentFile, ResolveRequest, SkipReason,
};
use crate::addon::online::model::parse_versions;
use crate::addon::online::{resolve, Modrinth, OnlineErrorKind, MODRINTH_API};
use crate::download::blocking::block_on;

use super::fixtures_deps as deps;
use super::support::FixtureTransport;

const SODIUM_VERSIONS: &str = "https://api.modrinth.com/v2/project/sodium/version";
const FABRIC_API_VERSIONS: &str = "https://api.modrinth.com/v2/project/P7dR8mSH/version";
const YACL_VERSIONS: &str = "https://api.modrinth.com/v2/project/1eAoo2KR/version";

fn request(root: &str) -> ResolveRequest {
    ResolveRequest {
        roots: vec![root.to_string()],
        game_version: Some("1.20.1".to_string()),
        loader: Some("fabric".to_string()),
        ..ResolveRequest::default()
    }
}

/// 三层链的传输层（sodium → fabric-api → yacl）。
fn chain_transport() -> FixtureTransport {
    FixtureTransport::new()
        .with(SODIUM_VERSIONS, deps::SODIUM_WITH_DEPENDENCIES)
        .with(FABRIC_API_VERSIONS, deps::FABRIC_API_WITH_DEPENDENCIES)
        .with(YACL_VERSIONS, deps::YACL)
}

fn resolve_with(transport: &FixtureTransport, request: &ResolveRequest) -> DependencyPlan {
    let client = Modrinth::new(transport);
    block_on(resolve(&client, request))
}

// ---------------------------------------------------------------- ① 嵌套两层

#[test]
fn two_nested_levels_are_pulled_in_a_predictable_order() {
    let transport = chain_transport();
    let plan = resolve_with(&transport, &request("sodium"));

    assert!(plan.is_clean(), "不该有失败：{:?}", plan.failures);
    // 顺序 = BFS 层序，同层按项目键字典序：根 → fabric-api → yacl
    let order: Vec<(&str, u8)> = plan
        .nodes
        .iter()
        .map(|node| (node.project.as_str(), node.depth))
        .collect();
    assert_eq!(
        order,
        vec![("AANobbMI", 0), ("P7dR8mSH", 1), ("1eAoo2KR", 2)]
    );

    // 「为什么装」：父子关系逐条对得上
    assert!(plan.required_by("AANobbMI").expect("根在计划里").is_empty());
    assert_eq!(
        plan.required_by("P7dR8mSH").expect("fabric-api 在计划里"),
        ["AANobbMI".to_string()].as_slice()
    );
    assert_eq!(
        plan.required_by("1eAoo2KR").expect("yacl 在计划里"),
        ["P7dR8mSH".to_string()].as_slice()
    );

    // 只跟 required：optional / incompatible / embedded 三个 id 一次请求都没发
    let seen = transport.seen();
    assert_eq!(seen.len(), 3, "只该有 3 次版本请求：{seen:?}");
    for quiet in ["OCS3z7f6", "AAAABBBB", "embedded-lib"] {
        assert!(
            !seen.iter().any(|url| url.contains(quiet)),
            "{quiet} 不该被跟：{seen:?}"
        );
    }

    // 主文件取 primary（第一个是 sources.jar）
    assert_eq!(plan.nodes[0].file.filename, "sodium-fabric-0.6.0.jar");
    assert_eq!(plan.nodes[1].file.filename, "fabric-api-0.92.2+1.20.1.jar");
}

#[test]
fn only_required_dependencies_are_followed() {
    // 模型层：四值都解析得出来，`required_project_ids` 只吐 required 那一条。
    let versions = parse_versions(deps::SODIUM_WITH_DEPENDENCIES).expect("夹具该解析成功");
    let version = &versions[0];
    assert_eq!(version.dependencies.len(), 4);
    let kinds: Vec<&str> = version
        .dependencies
        .iter()
        .map(|dependency| dependency.dependency_type.as_str())
        .collect();
    assert_eq!(kinds, ["required", "optional", "incompatible", "embedded"]);
    assert_eq!(version.required_project_ids(), ["P7dR8mSH"]);
    // embedded 那条只有 file_name、没有 project_id，不该冒出一个空 id
    assert!(version.dependencies[3].project_id.is_none());
}

#[test]
fn the_depth_limit_is_reported_instead_of_silently_cutting_the_chain() {
    let transport = chain_transport();
    let mut request = request("sodium");
    request.max_depth = 1;
    let plan = resolve_with(&transport, &request);

    assert_eq!(plan.nodes.len(), 2, "根 + 第一层");
    assert_eq!(plan.truncated, ["1eAoo2KR"], "截断必须写在计划里");
}

// ---------------------------------------------------------------- ② 环

#[test]
fn a_dependency_cycle_terminates_and_each_project_appears_once() {
    let transport = FixtureTransport::new()
        .with(
            "https://api.modrinth.com/v2/project/cycle-a/version",
            deps::CYCLE_A,
        )
        .with(
            "https://api.modrinth.com/v2/project/cycBid01/version",
            deps::CYCLE_B,
        );
    let plan = resolve_with(&transport, &request("cycle-a"));

    let projects: Vec<&str> = plan
        .nodes
        .iter()
        .map(|node| node.project.as_str())
        .collect();
    assert_eq!(projects, ["cycAid01", "cycBid01"], "每个项目只出现一次");
    assert_eq!(transport.seen().len(), 2, "环不许把请求数放大");
    // B 是被 A 要来的；A 又被 B 要（环）——这条边折回已有节点，不新建节点
    assert_eq!(
        plan.required_by("cycBid01").expect("B 在计划里"),
        ["cycAid01".to_string()].as_slice()
    );
    assert_eq!(
        plan.required_by("cycAid01").expect("A 在计划里"),
        ["cycBid01".to_string()].as_slice()
    );
    assert!(plan.nodes[0].is_root(), "根判据是 depth，不是 required_by");
    assert!(!plan.nodes[1].is_root());
}

// ---------------------------------------------------------------- ③ 两档失败

#[test]
fn an_incompatible_dependency_lands_in_the_retry_list() {
    // fabric-api 在 1.20.1/fabric 下没有条目（站点回空数组）
    let transport = FixtureTransport::new()
        .with(SODIUM_VERSIONS, deps::SODIUM_WITH_DEPENDENCIES)
        .with(FABRIC_API_VERSIONS, "[]");
    let plan = resolve_with(&transport, &request("sodium"));

    // 根还在计划里（失败不连坐），缺的那条单独立案
    assert_eq!(plan.nodes.len(), 1);
    assert_eq!(plan.failures.len(), 1, "不许静默丢：{:?}", plan.failures);
    let failure = &plan.failures[0];
    assert_eq!(failure.kind, FailureKind::Incompatible);
    assert_eq!(failure.requested_as, "P7dR8mSH");
    assert_eq!(failure.depth, 1);
    assert_eq!(failure.required_by, ["AANobbMI".to_string()]);
    assert!(failure.cause.is_none(), "这一档不是网络成因");
    assert!(failure.message.contains("1.20.1"), "文案要带过滤条件");
    assert!(plan.failures_of(FailureKind::Unreachable).next().is_none());

    // 可重试名单只含失败项，过滤条件沿用
    let retry = plan.retry_request();
    assert_eq!(retry.roots, ["P7dR8mSH"]);
    assert_eq!(retry.game_version.as_deref(), Some("1.20.1"));
    assert_eq!(retry.loader.as_deref(), Some("fabric"));
}

#[test]
fn a_missing_project_is_unreachable_and_keeps_its_leaf_cause() {
    let transport = FixtureTransport::new()
        .with(SODIUM_VERSIONS, deps::SODIUM_WITH_DEPENDENCIES)
        .missing(FABRIC_API_VERSIONS);
    let plan = resolve_with(&transport, &request("sodium"));

    let failure = &plan.failures[0];
    assert_eq!(failure.kind, FailureKind::Unreachable);
    assert_eq!(failure.cause, Some(OnlineErrorKind::NotFound));
    // 两档从类型上就分得开：同一个计划里两种成因不会挤进一条
    assert_eq!(plan.failures_of(FailureKind::Incompatible).count(), 0);
}

#[test]
fn a_version_without_a_usable_file_is_incompatible_not_a_network_error() {
    let transport = FixtureTransport::new()
        .with(SODIUM_VERSIONS, deps::SODIUM_WITH_DEPENDENCIES)
        .with(
            FABRIC_API_VERSIONS,
            r#"[{"id": "ver-empty", "project_id": "P7dR8mSH",
                 "version_number": "0.0", "files": []}]"#,
        );
    let plan = resolve_with(&transport, &request("sodium"));
    assert_eq!(plan.failures[0].kind, FailureKind::Incompatible);
    assert!(plan.failures[0].message.contains("没有可下载的主文件"));
}

// ---------------------------------------------------------------- 两道去重

#[test]
fn one_project_required_by_two_parents_is_planned_once_with_both_parents() {
    let transport = FixtureTransport::new()
        .with(
            "https://api.modrinth.com/v2/project/fanin-root/version",
            deps::FANIN_ROOT,
        )
        .with(
            "https://api.modrinth.com/v2/project/BRANCHAA/version",
            deps::FANIN_A,
        )
        .with(
            "https://api.modrinth.com/v2/project/BRANCHBB/version",
            deps::FANIN_B,
        )
        .with(
            "https://api.modrinth.com/v2/project/SHARED01/version",
            deps::SHARED_LIB,
        );
    let plan = resolve_with(&transport, &request("fanin-root"));

    let projects: Vec<&str> = plan
        .nodes
        .iter()
        .map(|node| node.project.as_str())
        .collect();
    assert_eq!(
        projects,
        ["ROOT0001", "BRANCHAA", "BRANCHBB", "SHARED01"],
        "同层按项目键字典序，公共前置只出现一次"
    );
    assert_eq!(
        plan.required_by("SHARED01").expect("公共前置在计划里"),
        ["BRANCHAA".to_string(), "BRANCHBB".to_string()].as_slice(),
        "两条依赖路径都要留下"
    );
    let shared_requests = transport
        .seen()
        .iter()
        .filter(|url| url.contains("SHARED01"))
        .count();
    assert_eq!(shared_requests, 1, "一个项目只解一次版本");
}

#[test]
fn a_file_that_already_exists_is_skipped_but_its_dependencies_are_still_walked() {
    let mut request = request("sodium");
    // 禁用中的 jar 也算「装过」（本仓用 `.disabled` 后缀表示禁用）
    request.present = vec![PresentFile::new("fabric-api-0.92.2+1.20.1.jar.disabled")];
    let transport = chain_transport();
    let plan = resolve_with(&transport, &request);

    assert_eq!(plan.skipped.len(), 1);
    let skipped = &plan.skipped[0];
    assert_eq!(skipped.project, "P7dR8mSH");
    assert_eq!(skipped.reason, SkipReason::AlreadyPresent);
    assert_eq!(skipped.file_name, "fabric-api-0.92.2+1.20.1.jar");
    // 跳过的是**下载**，不是图：fabric-api 的前置照样解析出来
    let projects: Vec<&str> = plan
        .nodes
        .iter()
        .map(|node| node.project.as_str())
        .collect();
    assert_eq!(projects, ["AANobbMI", "1eAoo2KR"]);
}

#[test]
fn present_keys_normalize_paths_case_and_the_disabled_suffix() {
    assert_eq!(present_key("mods/Sodium-0.6.0.jar"), "sodium-0.6.0.jar");
    assert_eq!(present_key("a\\b\\x.jar.disabled"), "x.jar");
    assert_eq!(present_key("  y.jar  "), "y.jar");
    assert_eq!(present_key(""), "");
}

// ---------------------------------------------------------------- 光影过滤器

#[test]
fn shader_loaders_are_recognised_in_search_hits() {
    let transport =
        FixtureTransport::new().with(&format!("{MODRINTH_API}/search"), deps::SEARCH_SHADER);
    let client = Modrinth::new(&transport);
    let page = block_on(client.search(&crate::addon::online::SearchQuery {
        text: String::new(),
        project_type: Some(crate::addon::online::ContentType::Shader),
        limit: 1,
        ..crate::addon::online::SearchQuery::default()
    }))
    .expect("解析应当成功");
    assert_eq!(
        page.hits[0].loaders,
        vec!["iris".to_string(), "optifine".to_string()],
        "光影的加载器不该被当成分类标签丢掉"
    );
}
