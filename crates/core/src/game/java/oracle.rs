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

mod listing;

pub use listing::{find_archive, parse_links, pick_version, version_key, versions_for};

use listing::select_version;

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

/// 期望文件名（Oracle 的命名是**模板**，哈希只在目录层级上，不在文件名里）。
///
/// 目录页解析与版本枚举（`parse_links` / `versions_for` / `select_version`）在
/// [`listing`] 里，这里只留「拿到版本串之后」的那一段。
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
    try_mirrors(client, table, major, target, None).await
}

/// 查一次「某大版本的指定版本 Oracle OpenJDK」。
///
/// 版本串来自镜像目录页（[`versions_for`] 本来就能枚举某 major 的全部版本），
/// 选定就用它，**不再取 [`pick_version`] 的最新**；目录页里没有这个版本时
/// 由 `catalog::version_pin_error` 报错并列出可用版本。
///
/// # Errors
///
/// 所有镜像都没有这个版本（或版本目录页 / `.sha256` 取不到）时返回错误。
pub async fn fetch_version(
    client: &reqwest::Client,
    table: &MirrorTable,
    major: u32,
    target: AdoptiumTarget,
    wanted: &str,
) -> Result<JdkRelease, JavaError> {
    try_mirrors(client, table, major, target, Some(wanted)).await
}

/// 逐个镜像试（`wanted` 决定是取最新还是取指定版本）。
async fn try_mirrors(
    client: &reqwest::Client,
    table: &MirrorTable,
    major: u32,
    target: AdoptiumTarget,
    wanted: Option<&str>,
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
        match fetch_from(client, mirror.base(), major, target, wanted).await {
            Ok(release) => return Ok(release),
            Err(error) => reasons.push(format!("{}：{}", mirror.name, error.message())),
        }
    }
    let subject = match wanted {
        None => format!("Oracle OpenJDK {major}"),
        Some(wanted) => format!("oracle 的 Java {major} 版本 {wanted}"),
    };
    Err(JavaError::new(
        JavaErrorKind::Network,
        format!("{subject} 在所有镜像上都取不到：{}", reasons.join("；")),
    )
    .with_hint("Oracle 的 21 线只到 21.0.2（只发首个 GA + ≤2 update）；换 17/25 或改用 temurin"))
}

/// 单个镜像的完整三跳查询。
async fn fetch_from(
    client: &reqwest::Client,
    base: &str,
    major: u32,
    target: AdoptiumTarget,
    wanted: Option<&str>,
) -> Result<JdkRelease, JavaError> {
    let index = net::get_text(
        client,
        &format!("{}/", base.trim_end_matches('/')),
        "Oracle 镜像根目录",
    )
    .await?;
    let version = select_version(&index, major, wanted)?;
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
    use super::listing::find_archive;
    use super::{archive_name, archive_url, checksum_url, oracle_os};
    use crate::game::java::target::AdoptiumTarget;

    /// 华为云 `/openjdk/21.0.2/` 的真实文件列表片段。
    const LISTING: &str = r#"<html><body><pre><a href="../">../</a>
<a href="openjdk-21.0.2_linux-x64_bin.tar.gz" onclick="handleLinkUrl(this)">openjdk-21.0.2_linux-x64_bin.tar.gz</a> 194.18 MB
<a href="openjdk-21.0.2_windows-x64_bin.zip" onclick="handleLinkUrl(this)">openjdk-21.0.2_windows-x64_bin.zip</a> 192 MB
</pre></body></html>"#;

    const WINDOWS: AdoptiumTarget = AdoptiumTarget::new("windows", "x64");

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
