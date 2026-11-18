//! Adoptium（Eclipse Temurin）的**指定版本**查询：先枚举该大版本的 GA，把用户给的版本串对上
//! 一条 release，再用 `assets/release_name` 取那一份归档。
//!
//! 为什么是两跳（2026-11-12 逐个端点 curl 验过，见 mission M280 报告）：
//!
//! 1. `/v3/assets/version/{version}` 在 `/q/openapi` 里有这条路由，但**部署上的服务回 404 空体**
//!    （`21.0.13+9` 与 `21.0.13%2B9` 都一样），所以不用它；
//! 2. `GET /v3/assets/release_name/{vendor}/{release_name}`（如 `eclipse/jdk-21.0.13+9`）回 200，
//!    体是 `{release_name, version_data{…}, binaries[]{os,architecture,image_type,package{…}}}` ——
//!    这就是「指定版本」那一跳；
//! 3. 但 `release_name` 要完整名（`jdk-21.0.13+9`），用户给的常是公共版本号（`21.0.13`），
//!    所以先打 `GET /v3/assets/feature_releases/{major}/ga?…&sort_order=DESC&page_size=20`
//!    （openapi 里 `page_size` 的 `maximum` 就是 20）把前者翻成后者。同一份响应的 release 名
//!    列表同时是「这个大版本有哪些版本」的唯一来源——版本不存在时就靠它列给人看。
//!
//! **必须翻页**（M280 r1）：`page_size` 上限 20 是上游硬约束，而 8 / 11 / 17 线各有 26 条 GA
//! （2026-11-12 实测：`17/ga` page=0 回 20 条、page=1 回 6 条）。只读第一页会把
//! `17.0.4+8` 这种「上游有、第一页没有」的版本报成「没有这个版本」——文案还会把窗口当总数。
//! 所以这里按 `page` 翻到「本页不满一页」为止，上限 [`MAX_PAGES`] 页；真到上限就把清单标成
//! [`VersionListScope::Truncated`]，错误文案里如实写「只查了前 N 条」。
//!
//! 这里**只有取**：匹配用 [`matches_version`]，匹配不上时把可用清单交给
//! [`version_pin_error`] 生成错误。**不换家、不回落 latest**（那是 `install` / `source` 的事）。

use serde::Deserialize;

use crate::download::http;
use crate::download::stall::Deadlines;

use super::{ApiBinary, ApiVersion, ADOPTIUM_API_BASE};
use crate::game::java::catalog::{
    canonical_version, matches_version, version_pin_error, VersionListScope,
};
use crate::game::java::error::{JavaError, JavaErrorKind};
use crate::game::java::net;
use crate::game::java::release::JdkRelease;
use crate::game::java::target::AdoptiumTarget;
use crate::game::java::vendor::JdkVendor;

/// `feature_releases` 的每页条数（Adoptium 允许的上限）。
pub const PAGE_SIZE: usize = 20;

/// 最多翻几页（正常最多 2 页；5 页 = 100 条，纯防御，别让上游的异常响应把内存吃掉）。
pub const MAX_PAGES: usize = 5;

/// 某大版本第 `page` 页的 GA 清单地址（新 → 旧，`page` 从 0 起）。
#[must_use]
pub fn releases_url(major: u32, target: AdoptiumTarget, page: usize) -> String {
    format!(
        "{ADOPTIUM_API_BASE}/assets/feature_releases/{major}/ga?os={}&arch={}&image_type=jdk&page={page}&page_size={PAGE_SIZE}&sort_order=DESC&vendor=eclipse",
        target.os, target.arch
    )
}

/// 指定 release 名的产物地址。
#[must_use]
pub fn release_url(release_name: &str, target: AdoptiumTarget) -> String {
    format!(
        "{ADOPTIUM_API_BASE}/assets/release_name/eclipse/{release_name}?os={}&arch={}&image_type=jdk&vendor=eclipse",
        target.os, target.arch
    )
}

/// `feature_releases` / `release_name` 响应里的一条（只有这几项用得上）。
#[derive(Debug, Deserialize)]
struct ApiRelease {
    #[serde(default)]
    release_name: String,
    #[serde(default)]
    version_data: Option<ApiVersion>,
    #[serde(default)]
    binaries: Vec<ApiBinary>,
}

/// GA 清单响应 → release 名列表（`jdk-21.0.13+9`，上游给的顺序即新 → 旧）。
///
/// # Errors
///
/// 响应不是 JSON 时返回 [`JavaErrorKind::Metadata`]（调用方据此降级到下一家发行版）。
pub fn parse_release_list(json: &str) -> Result<Vec<String>, JavaError> {
    let releases: Vec<ApiRelease> = serde_json::from_str(json).map_err(|error| {
        JavaError::new(
            JavaErrorKind::Metadata,
            format!("Adoptium GA 清单不是预期 JSON：{error}"),
        )
    })?;
    Ok(releases
        .into_iter()
        .map(|release| release.release_name)
        .filter(|name| !canonical_version(name).is_empty())
        .collect())
}

/// 指定版本的响应 → 一份可下载归档（挑出目标平台的 jdk 那一条）。
///
/// # Errors
///
/// 响应不是 JSON、没有目标平台产物、sha256 不合法时返回错误。
pub fn parse_release(
    json: &str,
    expected: &str,
    target: AdoptiumTarget,
) -> Result<JdkRelease, JavaError> {
    let release: ApiRelease = serde_json::from_str(json).map_err(|error| {
        JavaError::new(
            JavaErrorKind::Metadata,
            format!("Adoptium 指定版本响应不是预期 JSON：{error}"),
        )
    })?;
    let name = if release.release_name.is_empty() {
        expected.to_string()
    } else {
        release.release_name
    };
    let binary = release
        .binaries
        .into_iter()
        .find(|binary| binary.matches(target))
        .ok_or_else(|| {
            JavaError::new(
                JavaErrorKind::Metadata,
                format!(
                    "Adoptium 的 {name} 没有 {}/{} 的 jdk 产物",
                    target.os, target.arch
                ),
            )
        })?;
    binary.into_release(name, release.version_data)
}

/// 翻页把该大版本的 GA release 名全部取回来（按页拼，直到「这一页不存在」/「本页不满一页」/ 到 [`MAX_PAGES`]）。
///
/// `fetch_page(page)` 由调用方给：真跑时是一发 HTTP（见 [`fetch_release_page`]），单测里是夹具闭包
/// ——所以「翻几页 / 什么时候停 / 到上限了没有」这段判断**完全离线可测**。
///
/// 返回 `Ok(None)` 的语义是**「上游明确说没有这一页」**（404/410 或 200+空体），
/// 与「这一页读不懂 / 网络坏了」是两件事：
///
/// * `Ok(None)` 在**第一页** ⇒ 「这个大版本真没有 GA 清单」，仍然 `Err`（与 r2 之前同口径）；
/// * `Ok(None)` 在**第二页及以后** ⇒「翻到尾巴」，把已取到的交出去，`truncated = false`；
/// * `Err(..)` ⇒ **一律照抛**（含第二页及以后）。读不懂的响应一旦被当成「没有更多」，
///   用户会看到「这个 major 没有版本 X」，而 X 其实在上游——一句可证伪的假话。
///
/// 返回值第二项 = 是否**因为到上限**而停（还有没看到的页）。错误文案要用它决定说
/// 「上游就这些」还是「只查了前 N 条」。
///
/// # Errors
///
/// 任何一页**读不懂**（解析失败 / 非 2xx 非 404 / 网络）都照抛，不吞。
pub async fn collect_releases<F, Fut>(mut fetch_page: F) -> Result<(Vec<String>, bool), JavaError>
where
    F: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = Result<Option<Vec<String>>, JavaError>>,
{
    let mut all: Vec<String> = Vec::new();
    for page in 0..MAX_PAGES {
        let Some(names) = fetch_page(page).await? else {
            if page == 0 {
                // 第一页就不存在 = 这个大版本真没有 GA 清单（不是「翻到尾巴」）。
                // 与 r2 之前的行为一致：那一条仍然报错，只是判据从「错误的种类」换成了
                // 「哪一页不存在」。
                return Err(JavaError::new(
                    JavaErrorKind::Metadata,
                    "Adoptium 没有该大版本的 GA 清单（HTTP 404 / 空响应）",
                ));
            }
            // 「这一页不存在」= 翻到尾巴。上游对越界页回 404（2026-11-12 实测），
            // 某条 GA 线的条数恰好是 20 / 40 / 60 时就会撞上这一发；此时已取到的前几页必须交出去。
            return Ok((all, false));
        };
        let short_page = names.len() < PAGE_SIZE;
        all.extend(names);
        if short_page {
            return Ok((all, false));
        }
    }
    Ok((all, true))
}

/// 一页响应的判定：`Ok(None)` = 「上游明确说没有这一页」，`Err` = 读不懂或网络坏。
///
/// **判据是语义的**（状态码 + 体的形状），不匹配任何错误文案：
///
/// | 上游情形 | 结果 |
/// | --- | --- |
/// | 404 / 410（越界页实测就是这个） | `Ok(None)` |
/// | 200 + 空体 | `Ok(None)`（等价于「没有这一页」） |
/// | 其它非 2xx（5xx / 403 / 429…） | `Err(Network)` |
/// | 200 + 不是预期 JSON | `Err(Metadata)` —— **不许**被当成「没有更多」 |
/// | 200 + 正常清单 | `Ok(Some(names))` |
///
/// # Errors
///
/// 非 2xx 且不是 404/410 时是 [`JavaErrorKind::Network`]；体读不懂时是
/// [`JavaErrorKind::Metadata`]。
pub fn page_outcome(status: u16, body: &str) -> Result<Option<Vec<String>>, JavaError> {
    if status == 404 || status == 410 {
        return Ok(None);
    }
    if !(200..300).contains(&status) {
        return Err(JavaError::new(
            JavaErrorKind::Network,
            format!("Adoptium GA 清单返回 HTTP {status}"),
        ));
    }
    if body.trim().is_empty() {
        return Ok(None);
    }
    parse_release_list(body).map(Some)
}

/// 取一页 GA 清单（HTTP 那一跳）。判定全交给 [`page_outcome`]，这里只负责把响应取回来。
///
/// 两处 `await` 都有界（M290 r1 p3-1：这一处原先既无上界也无内存上限，是「上游接了连不回话就
/// 永久挂」那一族的最后一份）：等响应头走 [`http::send_bounded`]、正文走 [`http::text_bounded`]，
/// 与 `net::get_text` 同一套构造点，失败时 `message` 里带**阶段**。界是参数只为让用例压到
/// 毫秒级，生产走 [`http::default_deadlines`]。
async fn fetch_release_page(
    client: &reqwest::Client,
    url: &str,
    deadlines: Deadlines,
) -> Result<Option<Vec<String>>, JavaError> {
    let response = http::send_bounded(client.get(url), url, deadlines, None)
        .await
        .map_err(|error| {
            JavaError::new(
                JavaErrorKind::Network,
                format!("取 Adoptium GA 清单失败：{error}"),
            )
            .with_hint("检查网络/代理；已装的 JDK 仍可用 `ull java list` 看到")
        })?;
    let status = response.status().as_u16();
    let body = http::text_bounded(response, url, deadlines)
        .await
        .map_err(|error| {
            JavaError::new(
                JavaErrorKind::Network,
                format!("读 Adoptium GA 清单响应失败：{error}"),
            )
        })?;
    page_outcome(status, &body)
}

/// 在枚举出来的 release 名里挑用户要的那一版（**不取最新、不取第一条**）。
#[must_use]
pub fn pick_release<'a>(releases: &'a [String], wanted: &str) -> Option<&'a String> {
    releases.iter().find(|name| matches_version(wanted, name))
}

/// 查一次「某大版本的指定版本」。
///
/// # Errors
///
/// 清单不可达、该大版本没有版本（错误里列出可用版本）、目标平台无产物、sha256 缺失时返回错误。
pub async fn fetch_version(
    client: &reqwest::Client,
    major: u32,
    target: AdoptiumTarget,
    wanted: &str,
) -> Result<JdkRelease, JavaError> {
    let (releases, truncated) = collect_releases(|page| {
        let url = releases_url(major, target, page);
        async move { fetch_release_page(client, &url, http::default_deadlines()).await }
    })
    .await?;
    let Some(name) = pick_release(&releases, wanted).cloned() else {
        let versions = releases
            .iter()
            .map(|name| canonical_version(name).to_string())
            .collect::<Vec<_>>();
        let scope = if truncated {
            VersionListScope::Truncated
        } else {
            VersionListScope::Complete
        };
        return Err(version_pin_error(
            JdkVendor::Temurin,
            major,
            wanted,
            &versions,
            scope,
        ));
    };
    let body = net::get_text(client, &release_url(&name, target), "Adoptium 指定版本产物").await?;
    let release = parse_release(&body, &name, target)?;
    if release.major != major {
        return Err(JavaError::new(
            JavaErrorKind::Metadata,
            format!(
                "Adoptium 的 {name} 属于 Java {}，不是请求的 {major}",
                release.major
            ),
        ));
    }
    Ok(release)
}

#[cfg(test)]
mod tests;

/// `fetch_release_page` 的两处上界 + `http::text_bounded` 的两道内存闸；夹具与 `net::probe` 同源。
#[cfg(test)]
mod bounded_probes {
    use std::time::Duration;

    use super::fetch_release_page;
    use crate::download::bytes::MAX_IN_MEMORY_BYTES;
    use crate::download::error::{DownloadError, DownloadErrorKind};
    use crate::download::http::text_bounded;
    use crate::game::java::error::JavaErrorKind;
    use crate::game::java::net::probe::{bounded, tight, Hanging, Mode};

    async fn fetch_body(url: &str) -> Result<String, DownloadError> {
        let client = reqwest::Client::new();
        let response = client.get(url).send().await.expect("响应头应到手");
        text_bounded(response, url, tight()).await
    }

    /// 服务端接了连接、一个字不回：取 GA 清单必须自己返回，并说清卡在**等响应头**。
    /// 变异：把 `fetch_release_page` 的 `send_bounded` 退回裸 `send()` ⇒ 护栏 5s panic，红。
    #[tokio::test]
    async fn a_silent_release_list_is_bounded_and_the_stage_is_named() {
        let url = Hanging::start(Mode::Silent).url("/ga");
        let (result, elapsed) = bounded(
            "取 GA 清单的 send() 又无界了",
            fetch_release_page(&reqwest::Client::new(), &url, tight()),
        )
        .await;
        assert!(
            elapsed < Duration::from_secs(2),
            "必须自己返回：{elapsed:?}"
        );
        let error = result.expect_err("silent 服务端必须失败");
        assert_eq!(error.kind(), JavaErrorKind::Network);
        assert!(
            error.message().contains("等响应头") && error.message().contains("await_headers"),
            "{}",
            error.message()
        );
    }

    /// 头到手、正文挤出几个字节就停住：阶段是**等字节**（变异：换回 `text()` ⇒ 5s panic，红）。
    #[tokio::test]
    async fn a_stalled_release_body_says_await_bytes() {
        let mode = Mode::Declared {
            declared: 4096,
            sent: 8,
        };
        let url = Hanging::start(mode).url("/ga");
        let (result, _) = bounded(
            "正文读取又无界了",
            fetch_release_page(&reqwest::Client::new(), &url, tight()),
        )
        .await;
        let error = result.expect_err("说到一半停住必须失败");
        assert!(
            error.message().contains("等字节") && error.message().contains("await_bytes"),
            "{}",
            error.message()
        );
    }

    /// 头里离谱的 `Content-Length`：**有界失败**，绝不拿它预分配（复审 r1 p2-1）。
    /// 变异：删掉提前拒绝并保留 `Vec::with_capacity(declared)` ⇒ 测试进程 abort（报告里贴了）；
    /// 只删提前拒绝（预分配是 `Vec::new()`）⇒ 本用例红（错误从「声明的长度」变成「等字节」）。
    #[tokio::test]
    async fn a_lying_content_length_is_a_bounded_error_not_an_abort() {
        let mode = Mode::Declared {
            declared: 999_999_999_999,
            sent: 0,
        };
        let url = Hanging::start(mode).url("/lying.json");
        let (result, elapsed) = bounded("离谱的声明长度没人管", fetch_body(&url)).await;
        assert!(
            elapsed < Duration::from_secs(2),
            "读完头就该返回：{elapsed:?}"
        );
        let error = result.expect_err("离谱的声明长度必须失败");
        assert_eq!(error.kind, DownloadErrorKind::InvalidRequest);
        assert!(
            error.message.contains("声明的长度") && error.message.contains("999999999999"),
            "要看出是「声明的长度超上限」而不是超时：{}",
            error.message
        );
    }

    /// 声明缺失但正文是洪流：读循环逐块卡 [`MAX_IN_MEMORY_BYTES`]（变异：删掉那道卡口 ⇒ 本
    /// 用例拿到 `Ok`，红）。
    #[tokio::test]
    async fn a_flooding_body_is_cut_at_the_memory_cap() {
        let url = Hanging::start(Mode::Flood(MAX_IN_MEMORY_BYTES + 1024 * 1024)).url("/flood.txt");
        let (result, _) = bounded("正文没有内存上限", fetch_body(&url)).await;
        let error = result.expect_err("洪流必须在接近上限处被拒");
        assert_eq!(error.kind, DownloadErrorKind::InvalidRequest);
        assert!(error.message.contains("内存上限"), "{}", error.message);
    }
}
