//! `api::version` 的单测：端点形状、两页夹具下的翻页、按版本挑而不是挑最新。
//!
//! 从 `version.rs` 拆出来只为守 400 行硬线（M280 r1 加了 P2-1/P2-2 的守卫后那边到 417 行）；
//! **纯搬移**，既有断言一字未改。

use super::{
    collect_releases, page_outcome, parse_release, parse_release_list, pick_release, release_url,
    releases_url, MAX_PAGES, PAGE_SIZE,
};
use crate::game::java::error::{JavaError, JavaErrorKind};
use crate::game::java::target::AdoptiumTarget;

const WINDOWS: AdoptiumTarget = AdoptiumTarget::new("windows", "x64");

/// 真机 `feature_releases/21/ga` 响应的裁剪（2026-11-12，只留两条与用到的字段）。
fn list_body() -> String {
    r#"[
      {"release_name":"jdk-21.0.13+9","version_data":{"major":21,"semver":"21.0.13+9.0.LTS"},"binaries":[]},
      {"release_name":"jdk-21.0.12+8","version_data":{"major":21,"semver":"21.0.12+8.0.LTS"},"binaries":[]}
    ]"#
    .to_string()
}

/// 真机 `assets/release_name/eclipse/jdk-21.0.13+9` 响应的裁剪。
fn release_body() -> String {
    r#"{
      "release_name": "jdk-21.0.13+9",
      "version_data": {"major": 21, "semver": "21.0.13+9.0.LTS", "build": 9},
      "binaries": [
        {
          "os": "windows", "architecture": "x64", "image_type": "jdk", "jvm_impl": "hotspot",
          "package": {
            "name": "OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip",
            "link": "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13%2B9/OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip",
            "checksum": "a4dbaa05fca4bd89d84584e58c459bd732efebb1722af80ab13c6516832004f9",
            "size": 205421281
          }
        }
      ]
    }"#
    .to_string()
}

#[test]
fn urls_target_the_versioned_endpoints_never_latest() {
    let releases = releases_url(21, WINDOWS, 0);
    assert!(releases.starts_with("https://api.adoptium.net/v3/assets/feature_releases/21/ga?"));
    assert!(releases.contains("page_size=20"));
    assert!(releases.contains("page=0"));
    // 页码真的进 URL（翻页不是装饰）。
    assert!(releases_url(21, WINDOWS, 1).contains("page=1"));
    assert!(releases.contains("sort_order=DESC"));
    assert!(!releases.contains("latest"));
    let release = release_url("jdk-21.0.13+9", WINDOWS);
    assert!(release
        .starts_with("https://api.adoptium.net/v3/assets/release_name/eclipse/jdk-21.0.13+9?"));
    assert!(release.contains("os=windows") && release.contains("arch=x64"));
    assert!(!release.contains("latest"));
}

#[test]
fn release_list_keeps_the_upstream_order_and_names() {
    assert_eq!(
        parse_release_list(&list_body()).expect("应解析"),
        vec!["jdk-21.0.13+9".to_string(), "jdk-21.0.12+8".to_string()]
    );
    let error = parse_release_list("<html>502</html>").expect_err("HTML 应被拒");
    assert_eq!(error.kind(), JavaErrorKind::Metadata);
}

#[test]
fn release_body_becomes_the_pinned_archive() {
    let release = parse_release(&release_body(), "jdk-21.0.13+9", WINDOWS).expect("应解析");
    assert_eq!(release.version, "21.0.13+9", "版本串取自 release 名");
    assert_eq!(release.semver.as_deref(), Some("21.0.13+9.0.LTS"));
    assert_eq!(release.major, 21);
    assert_eq!(
        release.archive.name,
        "OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip"
    );
    assert_eq!(release.archive.size, 205_421_281);
    assert!(release.archive.url.contains("temurin21-binaries"));
}

#[test]
fn other_platform_has_no_pinned_archive() {
    let error = parse_release(
        &release_body(),
        "jdk-21.0.13+9",
        AdoptiumTarget::new("windows", "x86-32"),
    )
    .expect_err("x86-32 应没有条目");
    assert_eq!(error.kind(), JavaErrorKind::Metadata);
}

/// 真机 `17/ga` 的两页形状（2026-11-12 实测 26 条：page=0 满 20 条、page=1 只有 6 条）。
/// 目标版本 `jdk-17.0.4+8` 在**第二页**，第一页 20 条里没有它。
fn page_zero() -> Vec<String> {
    // 17.0.21 … 17.0.6 共 16 条 + 4 条补丁变体，凑满一页 20 条；
    // 这一页里**没有** 17.0.4（它只在第二页）。
    let mut names: Vec<String> = (0..16)
        .map(|index| format!("jdk-17.0.{}+9", 21 - index))
        .collect();
    for filler in ["17.0.5.1+1", "17.0.5+11", "17.0.6.1+1", "17.0.10.1+1"] {
        names.push(format!("jdk-{filler}"));
    }
    assert_eq!(names.len(), PAGE_SIZE);
    assert!(
        pick_release(&names, "17.0.4").is_none(),
        "第一页不许有 17.0.4"
    );
    names
}

fn page_one() -> Vec<String> {
    vec![
        "jdk-17.0.4.1+1".to_string(),
        "jdk-17.0.4+8".to_string(),
        "jdk-17.0.3+7".to_string(),
        "jdk-17+35".to_string(),
        "jdk-16.0.2+7".to_string(),
        "jdk-16+36".to_string(),
    ]
}

/// 假的上游：按 page 返回夹具（`pages` 用完之后一律空页）。
fn two_pages(page: usize) -> Result<Option<Vec<String>>, JavaError> {
    match page {
        0 => Ok(Some(page_zero())),
        1 => Ok(Some(page_one())),
        // 越界页：上游说「没有这一页」（404/410 或 200+空体）
        _ => Ok(None),
    }
}

/// **P2-1 守卫**：`17.0.4` 只在第二页出现，必须翻到第二页才命中。
///
/// 变异证明（复审实测过的那个）：把 `collect_releases` 的循环改成「只取第一页」
/// ⇒ 本用例红（`pick_release` 找不到 `jdk-17.0.4+8`）。
#[tokio::test]
async fn a_version_on_the_second_page_is_still_found() {
    let pages: std::cell::RefCell<Vec<usize>> = std::cell::RefCell::new(Vec::new());
    let (releases, truncated) = collect_releases(|page| {
        pages.borrow_mut().push(page);
        async move { two_pages(page) }
    })
    .await
    .expect("两页都应取到");

    assert_eq!(*pages.borrow(), vec![0, 1], "应该正好翻两页");
    assert_eq!(releases.len(), PAGE_SIZE + page_one().len());
    assert!(!truncated, "第二页不满一页 ⇒ 清单是完整的");
    assert_eq!(
        pick_release(&releases, "17.0.4").map(String::as_str),
        Some("jdk-17.0.4+8"),
        "第二页那一版必须能被选中"
    );
    assert_eq!(
        pick_release(&releases, "17.0.4.1").map(String::as_str),
        Some("jdk-17.0.4.1+1")
    );
}

/// **P2-2 守卫 1**：列表里同时有目标与更新的版本时，必须选**目标**，不许取第一条/最新。
///
/// 变异证明（复审的变异 B）：把 `pick_release` 换成 `releases.first()`
/// ⇒ 本用例第一条红（会拿到 `jdk-17.0.21+9`）。
#[test]
fn the_pinned_version_wins_over_the_newest_entry() {
    let releases = vec![
        "jdk-17.0.21+9".to_string(),
        "jdk-17.0.12+7".to_string(),
        "jdk-17.0.4+8".to_string(),
    ];
    assert_eq!(
        pick_release(&releases, "17.0.4").map(String::as_str),
        Some("jdk-17.0.4+8"),
        "点名 17.0.4 时不许拿最新的 17.0.21+9"
    );
    assert_eq!(
        pick_release(&releases, "17.0.12+7").map(String::as_str),
        Some("jdk-17.0.12+7")
    );
    assert_eq!(pick_release(&releases, "17.0.99"), None);
}

/// 上限兜底：上游每页都满（异常情况）时，翻到 [`MAX_PAGES`] 页就停，并如实标「还有更多」。
///
/// 变异证明：把 `collect_releases` 的 `for page in 0..MAX_PAGES` 改成 `loop`
/// ⇒ 夹具永远满页，本用例挂死/超时（红）。
#[tokio::test]
async fn a_never_ending_catalog_stops_at_the_page_cap_and_says_so() {
    let calls = std::cell::Cell::new(0usize);
    let (releases, truncated) = collect_releases(|page| {
        calls.set(calls.get() + 1);
        async move {
            let _ = page;
            Ok(Some(
                (0..PAGE_SIZE)
                    .map(|index| format!("jdk-17.0.{}+1", index + 1))
                    .collect::<Vec<_>>(),
            ))
        }
    })
    .await
    .expect("每页都成功");
    assert_eq!(calls.get(), MAX_PAGES, "翻满上限就停");
    assert_eq!(releases.len(), MAX_PAGES * PAGE_SIZE);
    assert!(truncated, "到上限时必须标成截断，文案才能如实说");
}

/// **判据 1（别退回去）**：某条 GA 线的条数恰好是 20 的整数倍时，最后一页是满页 ⇒ 会多要一页
/// ⇒ 上游对那一页说「没有这一页」（404/410 或 200+空体，见 [`page_outcome`]）⇒
/// 必须把**已经取到的**交出去，且 `truncated = false`。
///
/// 它真正钉住的是「`collect_releases` 收到『这一页不存在』时要把已取到的交出去」；
/// **不是**「上游越界页就是 404」——后者只能靠 curl 实测，夹具手写不出来
/// （r3 复审 P3-1 的更正）。
///
/// 变异证明：把 `collect_releases` 里 `let Some(names) = … else { return Ok((all, false)) }`
/// 的 `None` 分支改成 `return Err(…)`（即「没有这一页」也当失败）⇒ 本用例红。
#[tokio::test]
async fn a_missing_page_after_a_full_page_ends_the_tail_cleanly() {
    let calls = std::cell::Cell::new(0usize);
    let result = collect_releases(|page| {
        calls.set(calls.get() + 1);
        async move {
            match page {
                0 => Ok(Some(page_zero())),
                // 越界页的语义信号：上游明确说「没有这一页」。
                _ => Ok(None),
            }
        }
    })
    .await;

    let (releases, truncated) = result.expect("第一页拿到了就是拿到了，越界页不该作废它");
    assert_eq!(releases.len(), PAGE_SIZE, "正好一页，条数要精确");
    assert_eq!(releases, page_zero(), "逐条相等（不是「大概 20 个」）");
    assert!(!truncated, "这是「翻到尾巴」，不是「到上限」");
    assert_eq!(
        calls.get(),
        2,
        "确实多要了第二页（这正是上游说 404 的那一发）"
    );
}

/// **判据 1 的语义底座**：`page_outcome` 把「这一页不存在」与「这一页读不懂」分开，
/// 判据是状态码 + 体的形状，**不匹配任何错误文案**。
///
/// 变异证明：把 404/410 那一支删掉 ⇒ 第一条红；把空体那一支删掉 ⇒ 第三条红；
/// 把解析失败并进 `Ok(None)` ⇒ 最后一条红。
#[test]
fn a_missing_page_is_404_410_or_an_empty_body_and_nothing_else() {
    assert_eq!(page_outcome(404, "").expect("404 = 没有这一页"), None);
    assert_eq!(
        page_outcome(410, "<html>gone</html>").expect("410 = 没有这一页"),
        None
    );
    assert_eq!(
        page_outcome(200, "   \n\t ").expect("200 + 空体 = 没有这一页"),
        None
    );
    // 正常清单：真的解析出名字（顺序即上游给的顺序）。
    assert_eq!(
        page_outcome(200, &list_body()).expect("应解析"),
        Some(vec![
            "jdk-21.0.13+9".to_string(),
            "jdk-21.0.12+8".to_string()
        ])
    );
    // 读不懂 = 错误，**不许**被当成「没有这一页」。
    let parse_failure = page_outcome(200, "<html>502 Bad Gateway</html>").expect_err("HTML 应被拒");
    assert_eq!(parse_failure.kind(), JavaErrorKind::Metadata);
    // 其它非 2xx 是网络侧，同样不许被当成「没有这一页」。
    for status in [500u16, 403, 429, 502] {
        let error = page_outcome(status, "").expect_err("非 2xx 应报错");
        assert_eq!(error.kind(), JavaErrorKind::Network, "status={status}");
    }
}

/// **判据 2（r3 的 p2 本体）**：第二页**读不懂**（解析失败）⇒ 必须 `Err`，绝不许 `Ok`。
///
/// 夹具用的是**真的** `parse_release_list` 产出的错误（不是手写字符串），所以它跟着解析实现走。
/// 返回类型是 `Result<(Vec<String>, bool), JavaError>`，`Err` 分支在类型上就**不可能**带出
/// 部分结果——「不能既 Err 又把 all 交出去」由签名保证，用例断言的是「不许变成 Ok」。
///
/// 变异证明：在 `collect_releases` 里把 `Metadata` 错误重新并进「翻到尾巴」的分支
/// （`Err(e) if e.kind() == Metadata => return Ok((all, false))`）⇒ 本用例红。
#[tokio::test]
async fn a_parse_failure_on_a_later_page_is_an_error_not_the_end_of_the_list() {
    let parse_failure = parse_release_list("<html>502 Bad Gateway</html>")
        .expect_err("HTML 不是 GA 清单，必须报错");
    assert_eq!(
        parse_failure.kind(),
        JavaErrorKind::Metadata,
        "先确认这一类错误确实落在 Metadata 桶里（这正是 r3 那条 p2 的入口）"
    );

    let result = collect_releases(|page| {
        let error = parse_failure.clone();
        async move {
            match page {
                0 => Ok(Some(page_zero())),
                _ => Err(error),
            }
        }
    })
    .await;

    let error = result.expect_err("第二页读不懂 ⇒ 必须报错，不许静默截断");
    assert_eq!(error.kind(), JavaErrorKind::Metadata);
    assert!(
        error.message().contains("不是预期 JSON"),
        "报的必须是解析失败本身，不是一句无关的 404：{}",
        error.message()
    );
}

/// **判据 3**：错误落在**最后一页**（`page == MAX_PAGES - 1`）时同样不许报「清单完整」——
/// 那是被错误截断，不是「上游就这些」。
///
/// 变异证明：同判据 2 的那个变异（把 `Metadata` 并进吞掉的分支）⇒ 本用例红
/// （会拿到 `Ok((…, false))`，即文案会走 `Complete`）。
#[tokio::test]
async fn an_error_on_the_last_allowed_page_is_never_reported_as_complete() {
    let parse_failure =
        parse_release_list("<html>502</html>").expect_err("HTML 不是 GA 清单，必须报错");
    let full_page: Vec<String> = (0..PAGE_SIZE)
        .map(|index| format!("jdk-17.0.{}+1", index + 1))
        .collect();

    let result = collect_releases(|page| {
        let error = parse_failure.clone();
        let full_page = full_page.clone();
        async move {
            if page + 1 == MAX_PAGES {
                Err(error)
            } else {
                Ok(Some(full_page))
            }
        }
    })
    .await;

    let error = result.expect_err("最后一页读不懂也必须报错，不许报「完整」");
    assert_eq!(error.kind(), JavaErrorKind::Metadata);
    assert!(
        error.message().contains("不是预期 JSON"),
        "{}",
        error.message()
    );
}

/// 网络错误在任何一页都不许被当成「翻到尾巴」——那是真坏了。
///
/// 变异证明：把 `fetch_page` 的错误改成一律 `Ok(None)` ⇒ 本用例红。
#[tokio::test]
async fn a_network_error_on_a_later_page_is_not_swallowed() {
    let result = collect_releases(|page| async move {
        match page {
            0 => Ok(Some(page_zero())),
            _ => Err(JavaError::new(
                JavaErrorKind::Network,
                "连不上 Adoptium API：connection reset",
            )),
        }
    })
    .await;
    let error = result.expect_err("网络错误要冒泡");
    assert_eq!(error.kind(), JavaErrorKind::Network);
}

/// **第一页就不存在**：那是「这个大版本真没有 GA 清单」，仍然报错（与 r2 之前同口径），
/// 不许被当成「翻到尾巴」而返回空清单——否则用户会看到
/// 「没有版本 X（上游一个都没列出来）」这种什么都说明不了的文案。
///
/// 变异证明：把 `collect_releases` 里 `if page == 0` 那一支删掉 ⇒ 本用例红。
#[tokio::test]
async fn a_missing_first_page_is_still_an_error() {
    let result = collect_releases(|_page| async { Ok::<_, JavaError>(None) }).await;
    let error = result.expect_err("第一页就不存在 = 这个 major 没有 GA 清单");
    assert_eq!(error.kind(), JavaErrorKind::Metadata);
    assert!(
        error.message().contains("没有该大版本的 GA 清单"),
        "{}",
        error.message()
    );
}
