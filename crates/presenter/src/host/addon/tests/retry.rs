//! M191：「只重试这些」的两档名单 + 真重试路径（`super` 的用例按主题分卷，这一卷专管重试）。
//!
//! 三条判据对应的变异守门在各自的文档里写清了「改了什么这条会红」。

use super::{failure, node, plan};
use crate::dto::{InstallFailureKind, InstallFailureView};
use crate::host::addon::plan::{
    failure_of_plan, forget, names_for_incompatible, remember, remembered, retry_items, retry_plan,
    retry_request, Root,
};
use unlucky_core::addon::online::{DependencyPlan, FailureKind, OnlineErrorKind, PresentFile};

/// 一份「根装好了、两个前置失败」的计划（重试路径的典型输入）。
fn plan_with_two_tier_failures() -> DependencyPlan {
    plan(
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
        vec![node("AANobbMI", "sodium.jar", 0, &[])],
        Vec::new(),
    )
}

/// 可重试名单：**两档分在类型上**，每条的 project 就是重试会发出去的那个键。
///
/// 抓得住：把 `Incompatible` 并进 `Unreachable`（两档合成一档）⇒ 这条立刻红。
#[test]
fn the_retry_list_keeps_the_two_tiers_apart() {
    let items = retry_items(
        &plan_with_two_tier_failures(),
        Root {
            id: "AANobbMI",
            title: "Sodium",
        },
        &Vec::new(),
    );
    assert_eq!(items.len(), 2);
    assert_eq!(
        items
            .iter()
            .map(|item| (item.project.as_str(), item.kind))
            .collect::<Vec<_>>(),
        [
            ("P7dR8mSH", InstallFailureKind::Incompatible),
            ("1eAoo2KR", InstallFailureKind::Unreachable),
        ]
    );
    // 两档的名字**必须不同**（界面拿它分组、也是日志/用例里的稳定 id）
    assert_ne!(
        InstallFailureKind::Incompatible.as_str(),
        InstallFailureKind::Unreachable.as_str()
    );
}

/// 全成功 ⇒ 名单是空的（界面据此**不渲染**那条入口，而不是画一个灰按钮）。
#[test]
fn a_clean_plan_offers_nothing_to_retry() {
    let clean = plan(
        Vec::new(),
        vec![node("AANobbMI", "sodium.jar", 0, &[])],
        Vec::new(),
    );
    assert!(retry_items(
        &clean,
        Root {
            id: "AANobbMI",
            title: "Sodium",
        },
        &Vec::new(),
    )
    .is_empty());
    assert!(failure_of_plan(&clean, 1).is_none());
}

/// 「只重试这些」**只请求失败的那几个项目**：根项目不再发一次，解析只跑一次。
///
/// 这条是 M191 的变异守门①：把 `retry_request()` 换成「重新装根」（`ResolveRequest::single(root)`
/// 或 `plan.request.clone()`）⇒ `roots` 里会出现 `AANobbMI`，这条红。
#[test]
fn retrying_asks_for_the_failed_projects_only() {
    let plan = plan_with_two_tier_failures();
    let present = vec![PresentFile::new("sodium.jar")];
    let mut seen: Vec<unlucky_core::addon::online::ResolveRequest> = Vec::new();
    let _ = retry_plan(&plan, present, |request| {
        seen.push(request.clone());
        // 假解析器：把请求的根原样回成节点（用例只关心「请求了谁、请求了几次」）
        DependencyPlan {
            request: request.clone(),
            nodes: request
                .roots
                .iter()
                .map(|root| node(root, "x.jar", 0, &[]))
                .collect(),
            skipped: Vec::new(),
            failures: Vec::new(),
            truncated: Vec::new(),
            unresolved_required: Vec::new(),
        }
    });

    assert_eq!(seen.len(), 1, "重试只发一次解析，请求数不许放大");
    let roots = &seen[0].roots;
    assert_eq!(roots, &["P7dR8mSH".to_string(), "1eAoo2KR".to_string()]);
    assert!(
        !roots.contains(&"AANobbMI".to_string()),
        "根项目已装好，重试不许再解它：{roots:?}"
    );
    // `present` 重新扫过（这一轮新落盘的文件要在里面，否则会被当成「还没有」再下一份）
    assert_eq!(seen[0].present, vec![PresentFile::new("sodium.jar")]);
    // 过滤条件沿用上次那份
    assert_eq!(seen[0].game_version, plan.request.game_version);
}

/// 计划被记住 / 忘掉：键是「host + 实例」，成功之后不该留可重试的记录。
#[test]
fn the_last_failed_plan_is_remembered_per_instance() {
    let root = std::env::temp_dir().join(format!("ull-m191-retry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let env = crate::host::HostEnv::at_root(&root);
    assert!(remembered(&env, "survival").is_none());

    let plan = plan_with_two_tier_failures();
    let dir = root.join(".minecraft/mods");
    remember(&env, "survival", &dir, "AANobbMI", "Sodium", &plan);
    let saved = remembered(&env, "survival").expect("记住了这一次");
    assert_eq!(saved.dir, dir);
    // 原始根 id + 显示名跟着计划一起记住：重试时面板头用它，判「是不是根」也用它（r2 P2-2）
    assert_eq!(saved.root, "AANobbMI");
    assert_eq!(saved.title, "Sodium");
    assert_eq!(saved.plan.failures.len(), 2);
    // 别的实例不受影响（同一台机器上装过两个实例）
    assert!(remembered(&env, "modded").is_none());

    // 重试请求从「记住的计划」里取：仍然只含失败项
    let request = retry_request(&saved.plan, Vec::new());
    assert_eq!(request.roots, vec!["P7dR8mSH", "1eAoo2KR"]);

    forget(&env, "survival");
    assert!(remembered(&env, "survival").is_none());
    let _ = std::fs::remove_dir_all(&root);
}

/// DTO 名单与「真重试请求」是**同一份**：界面画的 N 个 = 重试会请求的 N 个。
///
/// 抓得住：界面名单自己另拼一份（多列/少列一个项目）⇒ 这条红。
#[test]
fn the_dto_list_is_exactly_what_the_retry_request_asks_for() {
    let plan = plan_with_two_tier_failures();
    let items: Vec<InstallFailureView> = retry_items(
        &plan,
        Root {
            id: "AANobbMI",
            title: "Sodium",
        },
        &Vec::new(),
    );
    let request = retry_request(&plan, Vec::new());
    assert_eq!(
        items
            .iter()
            .map(|item| item.project.clone())
            .collect::<Vec<_>>(),
        request.roots
    );
}

/// **P2-1 的守门**：面板上写的是可读名，不是裸项目 id。
///
/// 三个来源各一条（`plan::label_of` 的三种情况），而且都必须**语言中立**（站点标题 / 文件名，
/// 不是 core 的中文散文）：
///
/// | 项 | 情况 | 期望那一行 |
/// |---|---|---|
/// | 根项目 | 它自己失败 | `Sodium (AANobbMI)` —— 名字来自请求的 `title` |
/// | 前置 | 下载失败（计划里有节点） | `fabric-api.jar (P7dR8mSH)` —— 名字来自文件名 |
/// | 前置 | 解析阶段 404（没有节点） | `1eAoo2KR` —— 拿不到名，只画 id（合理兜底） |
///
/// 变异守门：把 `label_of` 的标签置空（或 `retry_items` 传空串）⇒ 这条红。
#[test]
fn the_retry_list_shows_a_readable_name_not_just_the_project_id() {
    let mut plan = plan(
        vec![
            // 根项目自己失败（下载失败）：requested_as 与 roots[0] 相同
            failure(
                "AANobbMI",
                FailureKind::Unreachable,
                Some(OnlineErrorKind::NotConnected),
                "下载 sodium.jar 失败（online_not_connected）：连不上 cdn.modrinth.com",
            ),
            // 前置下载失败：计划里有它的节点 ⇒ 用文件名
            failure(
                "P7dR8mSH",
                FailureKind::Unreachable,
                Some(OnlineErrorKind::NotConnected),
                "下载 fabric-api.jar 失败（online_not_connected）：连不上 cdn.modrinth.com",
            ),
            // 前置解析 404：没有节点、也不是根 ⇒ 没有名字可显示
            failure(
                "1eAoo2KR",
                FailureKind::Unreachable,
                Some(OnlineErrorKind::NotFound),
                "取版本失败（online_not_found）：HTTP 404",
            ),
        ],
        vec![
            node("AANobbMI", "sodium-fabric-0.6.0.jar", 0, &[]),
            node("P7dR8mSH", "fabric-api.jar", 1, &["AANobbMI"]),
        ],
        Vec::new(),
    );
    // 根必须是请求里的那个键（`plan::request` 只放一个根）
    plan.request = unlucky_core::addon::online::ResolveRequest::single("AANobbMI");

    let items = retry_items(
        &plan,
        Root {
            id: "AANobbMI",
            title: "Sodium",
        },
        &Vec::new(),
    );
    let lines: Vec<String> = items.iter().map(InstallFailureView::line).collect();
    assert_eq!(
        lines,
        vec![
            "Sodium (AANobbMI)".to_string(),
            "fabric-api.jar (P7dR8mSH)".to_string(),
            // 拿不到名的那一档：只画 id，不编造、也不拿 core 的中文 message 顶上
            "1eAoo2KR".to_string(),
        ]
    );
    // 标签是语言中立的：站点标题/文件名，不是 core 的中文散文
    assert!(!items[0].label.contains("下载"), "{}", items[0].label);
    assert!(!items[1].label.contains("失败"), "{}", items[1].label);
}

/// **r2 P2-2 的守门**：重试那一轮，第一条失败项**不许**被贴成原根项目的名字。
///
/// 形状就是复审给的那个：重试轮的 plan 来自 `retry_request()`，它的 `roots` 是上一轮的失败名单
/// （第一个是**前置**），而 `root` 记的是原始根 `AANobbMI / Sodium`。老实现拿 `roots.first()`
/// 判「是不是根」⇒ 画出 `Sodium (P7dR8mSH)`；现在按 id 比 ⇒ 第一条是它自己的名字（或 id）。
///
/// 变异守门：把 `label_of` 的判据换回 `plan.request.roots.first()` ⇒ 这条红。
#[test]
fn the_retry_round_does_not_paste_the_root_title_onto_a_dependency() {
    let mut plan = plan_with_two_tier_failures();
    // 重试请求：roots = 上一轮的失败名单
    plan.request = unlucky_core::addon::online::ResolveRequest {
        roots: vec!["P7dR8mSH".to_string(), "1eAoo2KR".to_string()],
        ..plan.request.clone()
    };
    // 重试轮里没有被安装的节点（上一轮装好的不会再进计划），于是标签只能来自 id
    let items = retry_items(
        &plan,
        Root {
            id: "AANobbMI",
            title: "Sodium",
        },
        &Vec::new(),
    );
    let lines: Vec<String> = items.iter().map(InstallFailureView::line).collect();
    assert_eq!(lines, vec!["P7dR8mSH".to_string(), "1eAoo2KR".to_string()]);
    assert!(
        !lines.iter().any(|line| line.contains("Sodium")),
        "重试轮把根的标题贴到前置上了：{lines:?}"
    );
}

/// **r2 P2-1 续的守门**：不兼容档要显示项目名，不是 id。
///
/// 那一档的项目**确实存在**（站点只是没有匹配目标版本/加载器的条目），所以失败路径上值得
/// 现取一次名字。变异守门：把这次获取去掉（`names` 传空）⇒ 第一条退回 `P7dR8mSH`，这条红。
#[test]
fn an_incompatible_item_shows_the_project_name_once_it_is_known() {
    let plan = plan_with_two_tier_failures();
    let root = Root {
        id: "AANobbMI",
        title: "Sodium",
    };
    // 生产路径：`names_for_incompatible` 用注入的查询取名字（这里模拟"站点有回应"）
    let names = names_for_incompatible(&plan, |id| {
        (id == "P7dR8mSH").then(|| "Entity Culling".to_string())
    });
    assert_eq!(
        names,
        vec![("P7dR8mSH".to_string(), "Entity Culling".to_string())],
        "只给不兼容档取名字（404 那条不发请求）"
    );

    let items = retry_items(&plan, root, &names);
    assert_eq!(items[0].line(), "Entity Culling (P7dR8mSH)");
    // 「取不到」那档不受影响：没有节点、没有名字 ⇒ 只画 id
    assert_eq!(items[1].line(), "1eAoo2KR");
}

/// **降级**：取名字本身失败，不许把安装结果搞坏，也不许编名字——照旧画 id。
///
/// 变异守门：让 `project_title` 的错误往外冒（`?` / `expect`）⇒ 这条连编译都过不了，
/// 或者（退回成「拿不到就 panic」）在这里红。
#[test]
fn a_failed_name_lookup_falls_back_to_the_id_without_breaking_anything() {
    let plan = plan_with_two_tier_failures();
    // 站点 404 / 没网 / 坏响应：`lookup` 全是 `None`
    let names = names_for_incompatible(&plan, |_| None);
    assert!(names.is_empty(), "{names:?}");
    let items = retry_items(
        &plan,
        Root {
            id: "AANobbMI",
            title: "Sodium",
        },
        &names,
    );
    assert_eq!(items[0].line(), "P7dR8mSH", "取不到名字就画 id，不编造");
    // 拿到空串（站点给了个空标题）同样不算名字
    let blank = names_for_incompatible(&plan, |_| Some("   ".to_string()));
    assert!(blank.is_empty(), "{blank:?}");
}

// r3 复审 p2：生产接线与降级守卫（按主题分卷，父文件已贴 400 行硬线）
mod wire;
