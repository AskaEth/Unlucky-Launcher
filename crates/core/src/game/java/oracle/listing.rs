//! Oracle OpenJDK 的**纯**那一半：目录页解析、版本枚举、版本选择。
//!
//! 从 `oracle.rs` 拆出来只为守 400 行硬线（M280 的指定版本选择让那边到 418 行）；
//! 语义未变——这里一个请求都不发，全是「HTML 文本 → 名字列表 / 版本串」的纯函数：
//!
//! ```text
//! parse_links    一级目录页 → 条目名（去重保序，跳过 ../ 与绝对链接）
//! version_key    版本串 → 可比较的数字段（27.0.1.0.3 > 27.0.1）
//! versions_for   目录页里属于某 major 的版本串（降序）
//! pick_version   该线最新（= versions_for 的第一个）
//! select_version 在目录页里挑版本：`None` = 最新，`Some` = 必须是这个版本（不近似）
//! find_archive   目录页里是否真有这个文件名（**唯一**的「存在」判据，不信状态码）
//! ```
//!
//! 「存在不信状态码」的理由见 `oracle.rs` 头：华为云对任意不存在的路径都回 200 + 门户 HTML。

use super::super::catalog::{matches_version, version_pin_error, VersionListScope};
use super::super::error::{JavaError, JavaErrorKind};
use super::super::vendor::JdkVendor;

/// 目录页 → 本级条目名列表（去掉尾部 `/`、跳过 `../` 与绝对链接，去重保序）。
#[must_use]
pub fn parse_links(html: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("href=\"") {
        let after = &rest[start + "href=\"".len()..];
        let Some(end) = after.find('"') else {
            break;
        };
        let value = after[..end].trim();
        rest = &after[end..];
        if value.is_empty()
            || value.starts_with("http")
            || value.starts_with("//")
            || value.starts_with('?')
            || value.contains('#')
        {
            continue;
        }
        let name = value.trim_end_matches('/');
        let Some(name) = name.rsplit('/').next() else {
            continue;
        };
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        if !out.iter().any(|known| known == name) {
            out.push(name.to_string());
        }
    }
    out
}

/// 版本串 → 可比较的数字段（`21.0.2` → `[21, 0, 2]`，`27.0.1.0.3` → `[27,0,1,0,3]`）。
#[must_use]
pub fn version_key(version: &str) -> Option<Vec<u64>> {
    let mut parts = Vec::new();
    for segment in version.split('.') {
        parts.push(segment.parse::<u64>().ok()?);
    }
    (!parts.is_empty()).then_some(parts)
}

/// 目录页里属于 `major` 的版本串，按版本**降序**（最新在前）。
#[must_use]
pub fn versions_for(html: &str, major: u32) -> Vec<String> {
    let mut found: Vec<(Vec<u64>, String)> = parse_links(html)
        .into_iter()
        .filter_map(|name| {
            let key = version_key(&name)?;
            (key.first() == Some(&u64::from(major))).then_some((key, name))
        })
        .collect();
    found.sort_by(|left, right| right.0.cmp(&left.0));
    found.into_iter().map(|(_, name)| name).collect()
}

/// 取 `major` 线的最新 GA 版本串。
#[must_use]
pub fn pick_version(html: &str, major: u32) -> Option<String> {
    versions_for(html, major).into_iter().next()
}

/// 目录页里是否有这个文件（**唯一**的「存在」判据：不信状态码）。
#[must_use]
pub fn find_archive(html: &str, filename: &str) -> bool {
    parse_links(html).iter().any(|name| name == filename)
}

/// 在目录页里挑版本：`None` = 该线最新，`Some` = 必须是这个版本（不做前缀近似）。
pub(super) fn select_version(
    html: &str,
    major: u32,
    wanted: Option<&str>,
) -> Result<String, JavaError> {
    let Some(wanted) = wanted else {
        return pick_version(html, major).ok_or_else(|| missing_major(html, major));
    };
    let available = versions_for(html, major);
    available
        .iter()
        .find(|version| matches_version(wanted, version))
        .cloned()
        .ok_or_else(|| {
            version_pin_error(
                JdkVendor::Oracle,
                major,
                wanted,
                &available,
                VersionListScope::Complete,
            )
        })
}

/// 「目录页里没有这个大版本」。
pub(super) fn missing_major(html: &str, major: u32) -> JavaError {
    JavaError::new(
        JavaErrorKind::Metadata,
        format!(
            "镜像目录里没有 {major} 线的 Oracle OpenJDK（目录页 {} 项）",
            parse_links(html).len()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::{parse_links, pick_version, select_version, version_key, versions_for};

    /// 华为云 `/openjdk/` 的真实目录页片段（2026-10-30 抓取，`<pre>` 列表）。
    const INDEX: &str = r#"<html><body><pre>Name         Last modified     Size</pre><hr><pre>
<a href="10/" onclick="handleLinkUrl(this)">10/</a>          22-Aug-2021 15:18 -
<a href="17.0.2/" onclick="handleLinkUrl(this)">17.0.2/</a>      22-Aug-2021 15:19 -
<a href="21/" onclick="handleLinkUrl(this)">21/</a>          22-Aug-2021 15:19 -
<a href="21.0.1/" onclick="handleLinkUrl(this)">21.0.1/</a>      22-Aug-2021 15:19 -
<a href="21.0.2/" onclick="handleLinkUrl(this)">21.0.2/</a>      06-Jan-2024 15:26 -
<a href="27.0.1.0.3/" onclick="handleLinkUrl(this)">27.0.1.0.3/</a> 06-Jan-2026 15:26 -
<a href="../" onclick="handleLinkUrl(this)">../</a>
</pre></body></html>"#;

    #[test]
    fn directory_links_are_scraped_without_any_hash_segment() {
        let links = parse_links(INDEX);
        assert!(links.contains(&"21.0.2".to_string()));
        assert!(!links.iter().any(|name| name == ".."));
        // Oracle 直链里的构建哈希（f2283984656d49d69e91c558476027ac）在镜像目录里根本不存在：
        // 目录层级只有 `{ver}/`，所以实现里没有任何拼哈希的位置。
        assert!(!links.iter().any(|name| name.len() == 32));
    }

    #[test]
    fn newest_version_of_a_line_wins_and_ordering_is_numeric() {
        assert_eq!(
            versions_for(INDEX, 21),
            vec!["21.0.2".to_string(), "21.0.1".to_string(), "21".to_string()]
        );
        assert_eq!(pick_version(INDEX, 21).as_deref(), Some("21.0.2"));
        // 27.0.1.0.3 是四段+，比 27.0.1 新（字符串比较会得出相反结论）。
        assert_eq!(version_key("27.0.1.0.3"), Some(vec![27, 0, 1, 0, 3]));
        assert_eq!(version_key("27.0.1"), Some(vec![27, 0, 1]));
        assert!(version_key("27.0.1") < version_key("27.0.1.0.3"));
        assert_eq!(pick_version(INDEX, 8), None);
    }

    /// 指定版本走同一条枚举路径：命中就用那一版，**不是** `pick_version` 的最新；
    /// 版本不存在时错误里列出该 major 真正有的版本（不许只回一句 404）。
    ///
    /// 变异证明：把 `select_version` 的 `Some` 分支改回 `pick_version` ⇒ 第一条断言红。
    #[test]
    fn pinned_version_is_taken_from_the_listing_not_the_newest() {
        assert_eq!(
            select_version(INDEX, 21, Some("21.0.1")).expect("目录页里有 21.0.1"),
            "21.0.1"
        );
        assert_eq!(
            select_version(INDEX, 21, Some("21.0.2")).expect("目录页里有 21.0.2"),
            "21.0.2"
        );
        // 不给版本 = 今天的行为（最新）。
        assert_eq!(select_version(INDEX, 21, None).expect("最新"), "21.0.2");
        let error = select_version(INDEX, 21, Some("21.0.9")).expect_err("目录页里没有 21.0.9");
        assert!(error.message().contains("21.0.9"), "{}", error.message());
        assert!(
            error.message().contains("21.0.2") && error.message().contains("21.0.1"),
            "错误里要列出可用版本：{}",
            error.message()
        );
    }
}
