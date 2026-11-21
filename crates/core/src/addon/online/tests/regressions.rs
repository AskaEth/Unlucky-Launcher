//! M186 复审（r1）补的用例：失败去重、null 载荷、版本级去重。
//!
//! 三条都对应一轮真实缺陷或真实缺口，而不是「补覆盖率」：
//! 1. [`the_same_failing_project_is_recorded_once_however_many_paths_ask_for_it`] — P2-1：
//!    失败分支以前直接 `continue`，同一项目被两条路径要求时取两次版本、记两条失败、
//!    重试名单里写两遍同一个名字；
//! 2. [`a_null_dependency_type_does_not_poison_the_whole_version_list`] — P2-2：
//!    `"dependency_type": null` 曾让整份 `/version` 报 Malformed，于是整个项目都装不上；
//! 3. [`another_version_of_the_same_mod_in_the_directory_still_downloads`] — nit③：
//!    归一化一旦改松，同 mod 的另一个版本会被误判成「已装」而跳过。

use crate::addon::online::dependency::{present_key, DependencyPlan, ResolveRequest};
use crate::addon::online::model::parse_versions;
use crate::addon::online::{resolve, DependencyType, Modrinth, PresentFile, UnresolvedRequired};
use crate::download::blocking::block_on;

use super::fixtures_deps as deps;
use super::support::{version_json, FixtureTransport};

const API: &str = "https://api.modrinth.com/v2/project";
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

fn resolve_with(transport: &FixtureTransport, request: &ResolveRequest) -> DependencyPlan {
    let client = Modrinth::new(transport);
    block_on(resolve(&client, request))
}

fn chain_transport() -> FixtureTransport {
    FixtureTransport::new()
        .with(SODIUM_VERSIONS, deps::SODIUM_WITH_DEPENDENCIES)
        .with(FABRIC_API_VERSIONS, deps::FABRIC_API_WITH_DEPENDENCIES)
        .with(YACL_VERSIONS, deps::YACL)
}

/// 同一个项目被两条路径要求、且它 404：**只请求一次、只记一条失败**，父边合起来。
///
/// r1 P2-1 的探针：`ROOT0001 → {BRANCH01, MIDDLE01}`、`BRANCH01 → GONE0001`(404)、
/// `MIDDLE01 → LATE0001 → GONE0001`。修之前 GONE0001 被请求 2 次、失败 2 条、
/// 重试名单是 `["GONE0001","GONE0001"]`（用户会看到「可只重试这 2 个：GONE0001、GONE0001」）。
#[test]
fn the_same_failing_project_is_recorded_once_however_many_paths_ask_for_it() {
    let transport = FixtureTransport::new()
        .with(
            &format!("{API}/root/version"),
            &version_json(
                "ROOT0001",
                "root.jar",
                &[("BRANCH01", "required"), ("MIDDLE01", "required")],
            ),
        )
        .with(
            &format!("{API}/BRANCH01/version"),
            &version_json("BRANCH01", "branch.jar", &[("GONE0001", "required")]),
        )
        .with(
            &format!("{API}/MIDDLE01/version"),
            &version_json("MIDDLE01", "middle.jar", &[("LATE0001", "required")]),
        )
        .with(
            &format!("{API}/LATE0001/version"),
            &version_json("LATE0001", "late.jar", &[("GONE0001", "required")]),
        )
        .missing(&format!("{API}/GONE0001/version"));

    let plan = resolve_with(&transport, &request("root"));

    let gone_requests = transport
        .seen()
        .iter()
        .filter(|url| url.contains("GONE0001"))
        .count();
    assert_eq!(
        gone_requests,
        1,
        "同一个失败项目不许取两次：{:?}",
        transport.seen()
    );
    assert_eq!(plan.failures.len(), 1, "{:?}", plan.failures);
    assert_eq!(plan.failures[0].requested_as, "GONE0001");
    assert_eq!(
        plan.failures[0].required_by,
        ["BRANCH01".to_string(), "LATE0001".to_string()],
        "两条路径的父边都要留下"
    );
    assert_eq!(plan.retry_request().roots, ["GONE0001"], "名单不许重复");
}

/// `"dependency_type": null` 不许把整份版本响应打成 Malformed（r1 P2-2 的回归）。
///
/// 以前 `RawDependency.dependency_type` 是 `String`：`#[serde(default)]` 只兜「字段缺失」，
/// 兜不住 null ⇒ 整份 `/version` 解析失败 ⇒ 根项目记 `Malformed`、`nodes` 为空 ⇒ 一个都装不上。
#[test]
fn a_null_dependency_type_does_not_poison_the_whole_version_list() {
    let body = r#"[{"id": "ver-null", "project_id": "NULLTYPE", "version_number": "1.0",
        "files": [{"url": "https://cdn.modrinth.com/data/NULLTYPE/1.0/null-type.jar",
                   "filename": "null-type.jar", "size": 16, "primary": true}],
        "dependencies": [{"project_id": "X1", "dependency_type": null}]}]"#;

    // 解析层：null 与缺失一样落 `Unknown`
    let versions = parse_versions(body).expect("null 不该让整份响应失败");
    assert_eq!(
        versions[0].dependencies[0].dependency_type,
        DependencyType::Unknown
    );
    assert_eq!(versions[0].required_project_ids(), Vec::<&str>::new());

    // 解析流程：根照样装得上，X1 一次请求都不发
    let transport = FixtureTransport::new().with(&format!("{API}/null-type/version"), body);
    let plan = resolve_with(&transport, &request("null-type"));
    assert!(plan.is_clean(), "{:?}", plan.failures);
    assert_eq!(plan.nodes.len(), 1, "根项目要还在计划里");
    assert_eq!(transport.seen().len(), 1, "未知类型不许递归");
}

/// 目录里是**同 mod 的另一个版本** ⇒ 文件名不同，仍然要下载（r1 nit③ 的负向用例）。
///
/// 归一化如果改松（例如「砍到第一个 `-` 前 + 砍扩展名」），`sodium-fabric-0.5.11.jar` 与
/// `sodium-fabric-0.6.0.jar` 会被判成同一个文件，该装的被跳过。
#[test]
fn another_version_of_the_same_mod_in_the_directory_still_downloads() {
    let mut request = request("sodium");
    request.present = vec![PresentFile::new("sodium-fabric-0.5.11.jar")];
    let transport = chain_transport();
    let plan = resolve_with(&transport, &request);

    assert!(plan.skipped.is_empty(), "{:?}", plan.skipped);
    assert_eq!(plan.nodes[0].file.filename, "sodium-fabric-0.6.0.jar");
    assert_ne!(
        present_key("sodium-fabric-0.5.11.jar"),
        present_key("sodium-fabric-0.6.0.jar"),
        "不同版本的文件名归一化后不许相等"
    );
}

/// 未知/大小写不同的取值一律 `Unknown`，**绝不**猜成 required（这条设计不许被 r1 的修法改坏）。
#[test]
fn unknown_dependency_types_never_trigger_recursion() {
    for raw in [
        "required_by_default",
        "Required ",
        "Recommended",
        "",
        "REQUIRED",
        "req",
    ] {
        assert_eq!(
            DependencyType::parse(raw),
            DependencyType::Unknown,
            "{raw:?} 不该被当成 required"
        );
    }
    assert_eq!(DependencyType::parse("required"), DependencyType::Required);
}

/// `required_project_ids()` 的「排序 + 去重」契约单独钉住。
///
/// 为什么要单独钉：resolver 里 `layer.sort_by` 会把它重排一遍、`wave_in` 也会按同键合并，
/// 于是**这条契约在端到端用例里被兜住了**（删掉这里的 `sort_unstable` 三条顺序用例仍绿）。
/// 但 `dedup()` 只去相邻重复，不排序就留下 `["ZEBRA01","ALPHA01","ZEBRA01"]`——
/// 公开 API 说好了「排序去重」，就得有自己的一条。
#[test]
fn required_project_ids_are_sorted_and_deduplicated_on_their_own() {
    let body = version_json(
        "dup-root",
        "dup-root.jar",
        &[
            ("ZEBRA01", "required"),
            ("ALPHA01", "required"),
            ("ZEBRA01", "required"),
        ],
    );
    let versions = parse_versions(&body).expect("夹具该解析成功");
    assert_eq!(
        versions[0].required_project_ids(),
        ["ALPHA01", "ZEBRA01"],
        "既要去重（非相邻）也要排序"
    );
}

/// 只有 `version_id` 的必备前置：**core 自己**要把它们收进 `unresolved_required`（r2 nit①）。
///
/// presenter 那条用例是直接赋值 `plan.unresolved_required`，绕过了 core；这里从夹具走一遍解析，
/// 顺带钉住「别为了收集把正常的 required 也吞掉」——同一版本里带 `project_id` 的那条仍要被跟。
#[test]
fn a_required_dependency_with_only_a_version_id_is_collected_by_the_resolver() {
    let body = r#"[{"id": "ver-mixed", "project_id": "MIXED001", "version_number": "1.0",
        "files": [{"url": "https://cdn.modrinth.com/data/MIXED001/1.0/mixed.jar",
                   "filename": "mixed.jar", "size": 16, "primary": true}],
        "dependencies": [
          {"version_id": "vid-only", "dependency_type": "required"},
          {"project_id": "FOLLOW01", "dependency_type": "required"}
        ]}]"#;
    let transport = FixtureTransport::new()
        .with(&format!("{API}/mixed/version"), body)
        .with(
            &format!("{API}/FOLLOW01/version"),
            &version_json("FOLLOW01", "follow.jar", &[]),
        );

    let plan = resolve_with(&transport, &request("mixed"));

    assert_eq!(
        plan.unresolved_required,
        [UnresolvedRequired {
            parent: "MIXED001".to_string(),
            version_id: Some("vid-only".to_string()),
            file_name: None,
        }],
        "只给版本 id 的 required 必须被记下来"
    );
    let projects: Vec<&str> = plan
        .nodes
        .iter()
        .map(|node| node.project.as_str())
        .collect();
    assert_eq!(
        projects,
        ["MIXED001", "FOLLOW01"],
        "同一版本里带 project_id 的 required 仍要被跟"
    );
}
