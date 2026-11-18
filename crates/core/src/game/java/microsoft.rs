//! Microsoft OpenJDK：`aka.ms` 模板下载 + 成对 `.sha256sum.txt` 校验。
//!
//! 分发形态与 Temurin / Oracle 都不同（research/06 §3.2）：
//!
//! ```text
//! 下载   https://aka.ms/download-jdk/microsoft-jdk-{version}-{os}-{arch}.zip
//!        → 301 → https://download.visualstudio.microsoft.com/download/pr/{guid}/{hash}/{同名}
//! 校验   https://aka.ms/download-jdk/<同一文件名>.sha256sum.txt   # "<hash>  <filename>"
//! ```
//!
//! 三条硬事实（都影响实现）：
//!
//! 1. **版本是四段/五段**（`21.0.13.0.2` = JDK 21.0.13+2；11 线是 `11.0.32.1`），
//!    不能拿 Temurin 的三段号直接拼——所以这里有自己的 [`parse_version`] 与内置快照表；
//! 2. **没有 8**（微软已停发，官方页面让去下 Temurin 8），`supports()` 也这么判；
//! 3. **GitHub 侧没有任何资产**（`microsoft/openjdk` 的 release `assets` 全空），
//!    所以 gh-proxy 对这条链毫无用处——`domestic_mirrors` 实测为空数组。

use super::catalog::{matches_version, version_pin_error, VersionListScope};
use super::error::{JavaError, JavaErrorKind};
use super::net;
use super::release::{parse_sha256_text, JdkArchive, JdkRelease};
use super::target::AdoptiumTarget;
use super::vendor::JdkVendor;
use crate::download::MsOpenJdkTable;

/// 内置版本快照（`learn.microsoft.com/java/openjdk/download` 2026-10-30 的表，research/06 §1.3）。
///
/// 官方只有人读 HTML 表、没有 JSON API，逐版本探测又要 `aka.ms` 301（0.3–0.8 s/次），
/// 所以按 research/06 §3.3 的建议 A 内置快照；`size` 与实测字节数一致，用于长度对拍。
pub const SNAPSHOT: &[(u32, &str, u64)] = &[
    (25, "25.0.5.0.2", 220_869_659),
    (21, "21.0.13.0.2", 201_450_315),
    (17, "17.0.21.0.2", 187_236_595),
    (11, "11.0.32.1", 191_250_397),
];

/// 四段/五段版本号（`21.0.13.0.2` / `11.0.32.1`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsVersion {
    /// 大版本（`21`）。
    pub major: u32,
    /// 各数字段（长度 3..=5）。
    pub segments: Vec<u32>,
}

impl MsVersion {
    /// 原样的版本串。
    #[must_use]
    pub fn text(&self) -> String {
        self.segments
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// 解析 MS 的版本串；段数必须 3..=5（MS 的语义：`major.update.patch[.build[.rev]]`）。
#[must_use]
pub fn parse_version(text: &str) -> Option<MsVersion> {
    let mut segments = Vec::new();
    for segment in text.trim().split('.') {
        segments.push(segment.parse::<u32>().ok()?);
    }
    if segments.len() < 3 || segments.len() > 5 {
        return None;
    }
    Some(MsVersion {
        major: segments[0],
        segments,
    })
}

/// MS 的 os 段（`windows` / `macos` / `linux`）。
#[must_use]
pub fn ms_os(target: AdoptiumTarget) -> &'static str {
    match target.os {
        "mac" => "macos",
        other => other,
    }
}

/// MS 的 arch 段（`x64` / `aarch64`）。
#[must_use]
pub fn ms_arch(target: AdoptiumTarget) -> &'static str {
    target.arch
}

/// 归档文件名（模板：`microsoft-jdk-{version}-{os}-{arch}.zip`，全平台都是 zip）。
#[must_use]
pub fn archive_name(version: &str, target: AdoptiumTarget) -> String {
    format!(
        "microsoft-jdk-{version}-{}-{}.zip",
        ms_os(target),
        ms_arch(target)
    )
}

/// 把模板里的 `{version}` / `{os}` / `{arch}` 换掉。
#[must_use]
pub fn expand_template(template: &str, version: &str, target: AdoptiumTarget) -> String {
    template
        .replace("{version}", version)
        .replace("{os}", ms_os(target))
        .replace("{arch}", ms_arch(target))
}

/// 快照里该大版本的版本串（没有 → `None`）。
#[must_use]
pub fn snapshot_version(major: u32) -> Option<&'static str> {
    SNAPSHOT
        .iter()
        .find(|(known, _, _)| *known == major)
        .map(|(_, version, _)| *version)
}

/// 快照里该大版本的字节数。
#[must_use]
pub fn snapshot_size(major: u32) -> Option<u64> {
    SNAPSHOT
        .iter()
        .find(|(known, _, _)| *known == major)
        .map(|(_, _, size)| *size)
}

/// 查一次「某大版本的最新 MS OpenJDK」。
///
/// 两跳：内置快照挑版本 → 取 `.sha256sum.txt` 拿哈希。校验文件取不到就整家失败
/// （宁可降级到 Zulu，也不下一个没法验证的 200 MB 归档）。
///
/// # Errors
///
/// 表里没配模板、该大版本微软不发（含 8）、校验文件取不到或格式不对时返回错误。
pub async fn fetch_latest(
    client: &reqwest::Client,
    table: &MsOpenJdkTable,
    major: u32,
    target: AdoptiumTarget,
) -> Result<JdkRelease, JavaError> {
    let version = snapshot_version(major).ok_or_else(|| missing_major(major))?;
    let filename = archive_name(version, target);
    let sha_url = expand_template(&table.sha256_template, version, target);
    let sha_text = net::get_text(client, &sha_url, "Microsoft OpenJDK 校验文件").await?;
    let sha256 = parse_sha256_text(&sha_text).ok_or_else(|| {
        JavaError::new(
            JavaErrorKind::Integrity,
            format!("{filename} 的 .sha256sum.txt 里没有 64 位哈希"),
        )
    })?;
    Ok(JdkRelease {
        vendor: JdkVendor::Microsoft,
        major,
        version: version.to_string(),
        semver: None,
        release_name: version.to_string(),
        archive: JdkArchive {
            name: filename.clone(),
            url: expand_template(&table.template, version, target),
            sha256,
            size: snapshot_size(major).unwrap_or(0),
        },
    })
}

/// 查一次「某大版本的指定版本 MS OpenJDK」。
///
/// 微软没有「按版本查」的接口（官方只有人读 HTML 表），[`SNAPSHOT`] 里每个大版本只有**一个**
/// 版本号（`size` 也是按它实测的），所以只认快照里那一个：别的版本号**明确报错**
/// （文案在 `catalog::version_pin_error`：说清这家不能自选版本、可用的是哪一个），
/// 绝不回落到 latest 假装装成了。
///
/// # Errors
///
/// 该大版本微软不发（含 8）、以及请求的版本号不是快照里那一个时返回错误。
pub async fn fetch_version(
    client: &reqwest::Client,
    table: &MsOpenJdkTable,
    major: u32,
    target: AdoptiumTarget,
    wanted: &str,
) -> Result<JdkRelease, JavaError> {
    let known = snapshot_version(major).ok_or_else(|| missing_major(major))?;
    if !matches_version(wanted, known) {
        return Err(version_pin_error(
            JdkVendor::Microsoft,
            major,
            wanted,
            &[known.to_string()],
            VersionListScope::Complete,
        ));
    }
    fetch_latest(client, table, major, target).await
}

/// 「微软不发这个 Java 大版本」（含 8）。
fn missing_major(major: u32) -> JavaError {
    JavaError::new(
        JavaErrorKind::Metadata,
        format!(
            "Microsoft OpenJDK 没有 {major} 的 GA（只发 {}）",
            majors_text()
        ),
    )
    .with_hint("8 用 temurin；其它大版本用 oracle / zulu")
}

fn majors_text() -> String {
    SNAPSHOT
        .iter()
        .map(|(major, _, _)| major.to_string())
        .collect::<Vec<_>>()
        .join(" / ")
}

#[cfg(test)]
mod tests {
    use super::{
        archive_name, expand_template, fetch_version, ms_os, parse_version, snapshot_size,
        snapshot_version, SNAPSHOT,
    };
    use crate::download::MirrorTable;
    use crate::game::java::error::JavaErrorKind;
    use crate::game::java::target::AdoptiumTarget;

    const WINDOWS: AdoptiumTarget = AdoptiumTarget::new("windows", "x64");

    /// 指定版本：除了快照里那一个版本号，别的一律**报错**（不回落 latest），
    /// 而且错误里要说清「只有这份快照」与「可用的是哪个」。
    ///
    /// 变异证明：把 `fetch_version` 里的版本校验删掉（直接 `fetch_latest`）⇒ 本用例红。
    #[tokio::test]
    async fn pinned_version_outside_the_snapshot_is_refused_not_fallen_back() {
        let table = MirrorTable::builtin();
        let microsoft = table.microsoft_openjdk().expect("内置表应带 MS 分发");
        let client = reqwest::Client::new();
        // 错误在取校验文件之前就返回，所以这条用例一个请求都不发。
        let error = fetch_version(&client, microsoft, 21, WINDOWS, "21.0.5")
            .await
            .expect_err("快照里没有 21.0.5");
        assert_eq!(error.kind(), JavaErrorKind::Metadata);
        assert!(
            error.message().contains("21.0.13.0.2"),
            "错误里要说清这家有哪个版本：{}",
            error.message()
        );
        assert!(
            error.message().contains("不能自选版本"),
            "要把「这家不能自选版本」说明白：{}",
            error.message()
        );
    }

    #[test]
    fn snapshot_covers_the_four_majors_microsoft_ships() {
        assert_eq!(snapshot_version(21), Some("21.0.13.0.2"));
        assert_eq!(snapshot_version(11), Some("11.0.32.1"));
        assert_eq!(snapshot_version(8), None, "微软已停发 8");
        assert_eq!(snapshot_size(17), Some(187_236_595));
        assert_eq!(
            SNAPSHOT
                .iter()
                .map(|(major, _, _)| *major)
                .collect::<Vec<_>>(),
            vec![25, 21, 17, 11]
        );
    }

    #[test]
    fn four_and_five_segment_versions_have_their_own_semantics() {
        // 21.0.13.0.2 = JDK 21.0.13+2（五段）；11 线只有四段。
        let ms = parse_version("21.0.13.0.2").expect("五段应解析");
        assert_eq!(ms.major, 21);
        assert_eq!(ms.segments.len(), 5);
        assert_eq!(ms.text(), "21.0.13.0.2");
        let eleven = parse_version("11.0.32.1").expect("四段应解析");
        assert_eq!(eleven.major, 11);
        assert_eq!(eleven.segments.len(), 4);
        // Temurin 的三段号形态在这里同样成立，但语义由快照表定义，不共用 Temurin 的解析。
        assert_eq!(
            parse_version("17.0.21").map(|version| version.major),
            Some(17)
        );
        assert_eq!(parse_version("21"), None);
        assert_eq!(parse_version("21.0"), None);
        assert_eq!(parse_version("21.0.13.0.2.1"), None);
        assert_eq!(parse_version("21.0.x.2"), None);
    }

    #[test]
    fn archive_names_and_templates_expand_per_platform() {
        assert_eq!(
            archive_name("21.0.13.0.2", WINDOWS),
            "microsoft-jdk-21.0.13.0.2-windows-x64.zip"
        );
        let template = "https://aka.ms/download-jdk/microsoft-jdk-{version}-{os}-{arch}.zip";
        assert_eq!(
            expand_template(template, "17.0.21.0.2", WINDOWS),
            "https://aka.ms/download-jdk/microsoft-jdk-17.0.21.0.2-windows-x64.zip"
        );
        assert_eq!(
            expand_template(
                "https://aka.ms/download-jdk/microsoft-jdk-{version}-{os}-{arch}.zip.sha256sum.txt",
                "21.0.13.0.2",
                AdoptiumTarget::new("linux", "aarch64")
            ),
            "https://aka.ms/download-jdk/microsoft-jdk-21.0.13.0.2-linux-aarch64.zip.sha256sum.txt"
        );
        assert_eq!(ms_os(AdoptiumTarget::new("mac", "x64")), "macos");
    }

    #[test]
    fn builtin_table_uses_the_official_templates() {
        let table = crate::download::MirrorTable::builtin();
        let microsoft = table.microsoft_openjdk().expect("内置表应带 MS 分发");
        assert!(microsoft
            .template
            .starts_with("https://aka.ms/download-jdk/"));
        assert!(microsoft.sha256_template.ends_with(".sha256sum.txt"));
        assert!(microsoft.domestic_mirrors.is_empty(), "MS 无国内镜像");
        assert_eq!(microsoft.majors, vec![25, 21, 17, 11]);
    }
}
