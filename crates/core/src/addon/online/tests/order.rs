//! 顺序确定性（r1 P2-4）：同层 ≥2 个前置、夹具数组故意逆序、多根乱序，结果必须一样。
//!
//! # 这条用例守的是什么
//!
//! `resolve` 在每层处理前把队列按**项目键字典序**排一遍。把那一行删掉，原来那 11 条用例
//! （以及 core 的全部用例）**仍然全绿**——因为三跳链每层只有一个前置、扇入用例里两个同层
//! 节点的先后恰好等于夹具数组顺序。也就是说「顺序由数据决定」这条性质当时没有守门。
//!
//! # 归因（实测过，别写成「都靠这两条守门」）
//!
//! 删掉 `resolve` 里那行 `layer.sort_by` ⇒
//! [`prerequisites_from_two_roots_are_ordered_by_project_key`] 与
//! [`multiple_roots_are_processed_in_key_order_whatever_the_argument_order`] 红，
//! [`same_root_prerequisites_follow_key_order_not_payload_order`] 仍绿——因为那两个前置
//! 是**同一个根**入队的，顺序已经被 `Version::required_project_ids()` 内部的排序排过了。
//! 反过来，只删 `required_project_ids()` 的排序时三条都绿：`layer.sort_by` 会把结果重排一遍。
//!
//! 也就是说：跨层顺序由 `layer.sort_by` 兜底（这两条用例钉它），同一个根内的顺序**两层保护**
//! 冗余存在；`required_project_ids()` 自己的「排序去重」契约由 `tests/regressions.rs` 的
//! `required_project_ids_are_sorted_and_deduplicated_on_their_own` 单独钉（那条只删排序就会红）。

use crate::addon::online::dependency::{DependencyPlan, ResolveRequest};
use crate::addon::online::{resolve, Modrinth};
use crate::download::blocking::block_on;

use super::support::{version_json, FixtureTransport};

const API: &str = "https://api.modrinth.com/v2/project";

fn request(roots: &[&str]) -> ResolveRequest {
    ResolveRequest {
        roots: roots.iter().map(|root| (*root).to_string()).collect(),
        game_version: Some("1.20.1".to_string()),
        loader: Some("fabric".to_string()),
        ..ResolveRequest::default()
    }
}

fn plan_of(transport: &FixtureTransport, roots: &[&str]) -> DependencyPlan {
    let client = Modrinth::new(transport);
    block_on(resolve(&client, &request(roots)))
}

fn projects(plan: &DependencyPlan) -> Vec<&str> {
    plan.nodes
        .iter()
        .map(|node| node.project.as_str())
        .collect()
}

#[test]
fn same_root_prerequisites_follow_key_order_not_payload_order() {
    // 夹具里 ZEBRA01 在前、ALPHA01 在后（站点原文顺序反过来是常态）。
    // 这一条守的是 `Version::required_project_ids()` 里的排序：靠「入队顺序」展开的话，
    // 站点换个数组顺序，安装顺序就跟着变。
    let transport = FixtureTransport::new()
        .with(
            &format!("{API}/order-root/version"),
            &version_json(
                "order-root",
                "order-root.jar",
                &[("ZEBRA01", "required"), ("ALPHA01", "required")],
            ),
        )
        .with(
            &format!("{API}/ALPHA01/version"),
            &version_json("ALPHA01", "alpha.jar", &[]),
        )
        .with(
            &format!("{API}/ZEBRA01/version"),
            &version_json("ZEBRA01", "zebra.jar", &[]),
        );

    let plan = plan_of(&transport, &["order-root"]);
    assert!(plan.is_clean(), "{:?}", plan.failures);
    assert_eq!(
        projects(&plan),
        ["order-root", "ALPHA01", "ZEBRA01"],
        "同层按项目键字典序，与 `dependencies` 数组顺序无关"
    );
}

/// 两个根各带一个前置 ⇒ 它们在**同一层**相遇，且项目键的顺序与根的先后相反。
///
/// 这一条守的是 `resolve` 里每层的 `sort_by`：不排的话，层内顺序就是「谁先被入队」——
/// 也就是参数顺序与站点响应顺序的混合，删掉那行 `sort_by` 这条会红
/// （实测：`left: ["zebra-branch","alpha-branch"]` 之类的参数序）。
#[test]
fn prerequisites_from_two_roots_are_ordered_by_project_key() {
    let transport = FixtureTransport::new()
        .with(
            &format!("{API}/branch-a/version"),
            &version_json("branch-a", "branch-a.jar", &[("ZEBRA01", "required")]),
        )
        .with(
            &format!("{API}/branch-b/version"),
            &version_json("branch-b", "branch-b.jar", &[("ALPHA01", "required")]),
        )
        .with(
            &format!("{API}/ALPHA01/version"),
            &version_json("ALPHA01", "alpha.jar", &[]),
        )
        .with(
            &format!("{API}/ZEBRA01/version"),
            &version_json("ZEBRA01", "zebra.jar", &[]),
        );

    let plan = plan_of(&transport, &["branch-a", "branch-b"]);
    assert!(plan.is_clean(), "{:?}", plan.failures);
    assert_eq!(
        projects(&plan),
        ["branch-a", "branch-b", "ALPHA01", "ZEBRA01"],
        "第二层两个前置按项目键排，不按「谁先入队」"
    );
}

#[test]
fn multiple_roots_are_processed_in_key_order_whatever_the_argument_order() {
    let transport = FixtureTransport::new()
        .with(
            &format!("{API}/zebra-root/version"),
            &version_json("zebra-root", "zebra-root.jar", &[]),
        )
        .with(
            &format!("{API}/alpha-root/version"),
            &version_json("alpha-root", "alpha-root.jar", &[]),
        );

    let forward = plan_of(&transport, &["zebra-root", "alpha-root"]);
    let backward = plan_of(&transport, &["alpha-root", "zebra-root"]);
    assert_eq!(
        projects(&forward),
        ["alpha-root", "zebra-root"],
        "参数逆序也要按项目键处理"
    );
    assert_eq!(projects(&backward), projects(&forward), "两次结果必须一样");
}
