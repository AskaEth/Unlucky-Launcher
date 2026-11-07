//! Oracle OpenJDK（`download.java.net` GA）：**枚举与下载都走国内镜像目录页**。
//!
//! 为什么不能像 Temurin 那样「先拿官方链接、再反解镜像路径」：Oracle 的直链里有一整段
//! 构建哈希（`/GA/jdk21.0.2/f2283984656d49d69e91c558476027ac/13/GPL/openjdk-21.0.2_windows-x64_bin.zip`），
//! 那段**不可推导**（research/06 §4.1 实测）。于是本模块的纪律是：
//!
//! > 候选 URL 的 `{ver}` 段与文件名段都必须来自对镜像目录页的解析（`parse_links`），
//! > 代码里没有任何一处把哈希段拼出来。
//!
//! 目录形状（华为云 / 南大与官方完全同构，§4.2）：
//!
//! ```text
//! {base}/                      → Apache 列表页：21/ 21.0.1/ 21.0.2/ 27.0.1.0.3/ …
//! {base}/{ver}/                → 文件列表：openjdk-21.0.2_windows-x64_bin.zip …
//! {base}/{ver}/{filename}      → 归档本体（大小与官方逐字节一致）
//! {base}/{ver}/{filename}.sha256 → 64 位哈希纯文本（成对提供）
//! ```
//!
//! 三处实测坑（都写进了实现）：
//!
//! 1. **华为云对任意不存在的路径都回 200 + 12 434 B 门户 HTML**（假阳性）——所以本模块只信
//!    「目录页里真的出现了这个文件名」，不信状态码；下载侧另有 `download::http::html_mirror_reason`
//!    兜一层（Content-Type 非 HTML + Content-Length 对拍）。
//! 2. **只发「首个 GA + ≤2 个 update」**：21 线最高只到 21.0.2，别当补丁源用。
//! 3. **目录页给的大小是四舍五入的**（`192 MB`）→ 不可当 `expected_size`，本模块一律填 0，
//!    校验交给成对的 `.sha256`。

use super::error::{JavaError, JavaErrorKind};
use super::net;
use super::release::{parse_sha256_text, JdkArchive, JdkRelease};
use super::target::AdoptiumTarget;
use super::vendor::JdkVendor;
use crate::download::MirrorTable;

/// Oracle 的 os 段（Adoptium 的 `mac` 在 Oracle 里叫 `macos`）。
#[must_use]
pub fn oracle_os(target: AdoptiumTarget) -> &'static str {
    match target.os {
        "mac" => "macos",
        other => other,
    }
}

/// Oracle 的 arch 段（`x86-32` → `x86`）。
#[must_use]
pub fn oracle_arch(target: AdoptiumTarget) -> &'static str {
    match target.arch {
        "x86-32" => "x86",
        other => other,
    }
}

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

/// 期望文件名（Oracle 的命名是**模板**，哈希只在目录层级上，不在文件名里）。
#[must_use]
pub fn archive_name(version: &str, target: AdoptiumTarget) -> String {
    format!(
        "openjdk-{version}_{}-{}_bin.{}",
        oracle_os(target),
        oracle_arch(target),
        target.archive_ext()
    )
}

/// `{base}/{ver}/{filename}`。
#[must_use]
pub fn archive_url(base: &str, version: &str, filename: &str) -> String {
    format!("{}/{version}/{filename}", base.trim_end_matches('/'))
}

/// 归档旁的 `.sha256` 地址。
#[must_use]
pub fn checksum_url(base: &str, version: &str, filename: &str) -> String {
    format!("{}.sha256", archive_url(base, version, filename))
}

/// 目录页里是否有这个文件（**唯一**的「存在」判据：不信状态码）。
#[must_use]
pub fn find_archive(html: &str, filename: &str) -> bool {
    parse_links(html).iter().any(|name| name == filename)
}

/// 查一次「某大版本的最新 Oracle OpenJDK GA」。
///
/// 按 `mirrors.json` 里 `oracle_openjdk` 的启用顺序逐个镜像试（国内：华为云 → 南大），
/// 每个镜像三跳：目录页 → 版本目录页 → `.sha256`。任何一跳失败就记下原因换下一个镜像。
///
/// # Errors
///
/// 所有镜像都失败（没有该大版本 / 目录页不可解析 / 缺 sha256）时返回错误，
/// 由回退链决定降级到下一家发行版。
pub async fn fetch_latest(
    client: &reqwest::Client,
    table: &MirrorTable,
    major: u32,
    target: AdoptiumTarget,
) -> Result<JdkRelease, JavaError> {
    let mirrors: Vec<_> = table.oracle_mirrors().collect();
    if mirrors.is_empty() {
        return Err(JavaError::new(
            JavaErrorKind::Metadata,
            "镜像清单里没有启用的 oracle_openjdk 源",
        )
        .with_hint("`mirrors.json` 的 `oracle_openjdk` 表需要至少一个 enabled 条目"));
    }
    let mut reasons: Vec<String> = Vec::new();
    for mirror in mirrors {
        match fetch_from(client, mirror.base(), major, target).await {
            Ok(release) => return Ok(release),
            Err(error) => reasons.push(format!("{}：{}", mirror.name, error.message())),
        }
    }
    Err(JavaError::new(
        JavaErrorKind::Network,
        format!(
            "Oracle OpenJDK {major} 在所有镜像上都取不到：{}",
            reasons.join("；")
        ),
    )
    .with_hint("Oracle 的 21 线只到 21.0.2（只发首个 GA + ≤2 update）；换 17/25 或改用 temurin"))
}

/// 单个镜像的完整三跳查询。
async fn fetch_from(
    client: &reqwest::Client,
    base: &str,
    major: u32,
    target: AdoptiumTarget,
) -> Result<JdkRelease, JavaError> {
    let index = net::get_text(
        client,
        &format!("{}/", base.trim_end_matches('/')),
        "Oracle 镜像根目录",
    )
    .await?;
    let version = pick_version(&index, major).ok_or_else(|| {
        JavaError::new(
            JavaErrorKind::Metadata,
            format!(
                "镜像目录里没有 {major} 线的 Oracle OpenJDK（目录页 {} 项）",
                parse_links(&index).len()
            ),
        )
    })?;
    let filename = archive_name(&version, target);
    let listing = net::get_text(
        client,
        &format!("{}/{version}/", base.trim_end_matches('/')),
        "Oracle 版本目录",
    )
    .await?;
    if !find_archive(&listing, &filename) {
        return Err(JavaError::new(
            JavaErrorKind::Metadata,
            format!("{version} 目录里没有 {filename}（该平台没有产物，或目录页不可解析）"),
        ));
    }
    let sha_text = net::get_text(
        client,
        &checksum_url(base, &version, &filename),
        "Oracle 归档 sha256",
    )
    .await?;
    let sha256 = parse_sha256_text(&sha_text).ok_or_else(|| {
        JavaError::new(
            JavaErrorKind::Integrity,
            format!("{filename} 的 .sha256 不是 64 位十六进制"),
        )
    })?;
    Ok(JdkRelease {
        vendor: JdkVendor::Oracle,
        major,
        version: version.clone(),
        semver: None,
        release_name: version.clone(),
        archive: JdkArchive {
            name: filename.clone(),
            url: archive_url(base, &version, &filename),
            sha256,
            // 目录页给的是四舍五入的大小（192 MB），不能当 expected_size。
            size: 0,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{
        archive_name, archive_url, checksum_url, find_archive, oracle_os, parse_links,
        pick_version, version_key, versions_for,
    };
    use crate::game::java::target::AdoptiumTarget;

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

    /// 华为云 `/openjdk/21.0.2/` 的真实文件列表片段。
    const LISTING: &str = r#"<html><body><pre><a href="../">../</a>
<a href="openjdk-21.0.2_linux-x64_bin.tar.gz" onclick="handleLinkUrl(this)">openjdk-21.0.2_linux-x64_bin.tar.gz</a> 194.18 MB
<a href="openjdk-21.0.2_windows-x64_bin.zip" onclick="handleLinkUrl(this)">openjdk-21.0.2_windows-x64_bin.zip</a> 192 MB
</pre></body></html>"#;

    const WINDOWS: AdoptiumTarget = AdoptiumTarget::new("windows", "x64");

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

    #[test]
    fn filename_template_matches_the_archive_on_every_platform() {
        assert_eq!(
            archive_name("21.0.2", WINDOWS),
            "openjdk-21.0.2_windows-x64_bin.zip"
        );
        assert_eq!(
            archive_name("21.0.2", AdoptiumTarget::new("linux", "aarch64")),
            "openjdk-21.0.2_linux-aarch64_bin.tar.gz"
        );
        assert_eq!(
            archive_name("17.0.2", AdoptiumTarget::new("mac", "x64")),
            "openjdk-17.0.2_macos-x64_bin.tar.gz"
        );
        assert_eq!(oracle_os(AdoptiumTarget::new("mac", "x64")), "macos");
        assert_eq!(oracle_os(WINDOWS), "windows");
    }

    #[test]
    fn archive_is_only_published_when_it_really_appears_in_the_listing() {
        let filename = archive_name("21.0.2", WINDOWS);
        assert!(find_archive(LISTING, &filename));
        // 华为云的门户兜底页（任意路径 200 + HTML）里没有这个链接 → 必须判为不存在。
        assert!(!find_archive(
            "<html><body>mirrors 门户</body></html>",
            &filename
        ));
        assert_eq!(
            archive_url(
                "https://mirrors.huaweicloud.com/openjdk/",
                "21.0.2",
                &filename
            ),
            format!("https://mirrors.huaweicloud.com/openjdk/21.0.2/{filename}")
        );
        assert_eq!(
            checksum_url(
                "https://mirrors.huaweicloud.com/openjdk",
                "21.0.2",
                &filename
            ),
            format!("https://mirrors.huaweicloud.com/openjdk/21.0.2/{filename}.sha256")
        );
    }
}
