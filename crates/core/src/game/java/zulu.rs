//! Azul Zulu：**第四备选**（D23）。API 元数据 + CDN 直链，实现刻意简化。
//!
//! 为什么排最后：国内没有任何 Zulu 镜像（三家云 + 四家校园源全无 `zulu`/`azul` 目录），
//! 只能吃 `cdn.azul.com` 的 3.3 MB/s（211 MB ≈ 64 s）；而前三家在国内都有更好的路。
//! 留这一家的价值是「三家全挂时的最后一条活路」，以及 Temurin/Oracle/MS 都缺某版本时的兜底。
//!
//! 两跳（research/06 §4.3）：
//!
//! ```text
//! GET {api}?java_version=21&os=windows&arch=x64&java_package_type=jdk&archive_type=zip&latest=true
//!     → [{ package_uuid, name, java_version, download_url }]      # 注意：不含 sha256
//! GET {detail（{uuid} 占位）} → { sha256_hash, size（不可信，差 133 B） }
//! ```
//!
//! 三条实测约束：①`latest=true` 的响应**没有** `sha256_hash`，必须再取一次详情；
//! ②`size` 字段与真实字节数差 133 B → 一律不信，只填 0；③文件名必须照抄 API 返回值
//! （`zulu21.54.109-ca-jdk21.0.13.0.3-win_x64.zip` 里混着三段与四段两种写法，拼不出来）。

use serde::Deserialize;

use super::error::{JavaError, JavaErrorKind};
use super::net;
use super::release::{JdkArchive, JdkRelease};
use super::target::AdoptiumTarget;
use super::vendor::JdkVendor;
use crate::download::ZuluTable;

/// Zulu 的 os 段（`windows` / `macos` / `linux`）。
#[must_use]
pub fn zulu_os(target: AdoptiumTarget) -> &'static str {
    match target.os {
        "mac" => "macos",
        other => other,
    }
}

/// 包列表查询地址（`latest=true` 只返回最新一个）。
#[must_use]
pub fn api_url(table: &ZuluTable, major: u32, target: AdoptiumTarget) -> String {
    format!(
        "{}?java_version={major}&os={}&arch={}&java_package_type=jdk&archive_type=zip&latest=true",
        table.api,
        zulu_os(target),
        target.arch
    )
}

/// 包详情地址（`{uuid}` 占位替换）。
#[must_use]
pub fn detail_url(table: &ZuluTable, uuid: &str) -> String {
    table.detail.replace("{uuid}", uuid)
}

/// 列表响应里我们关心的字段。
///
/// ⚠️ 真机形态（2026-10-30 实测）：`latest=true` **不止返回一条**，而且 `java_version` 是
/// **数字数组**（`[21,0,13,0,3]`）而不是字符串；同一批里还混着 `-crac-`（CRaC）与
/// `-fx-`（JavaFX）变体。所以这里按「字段宽进 + 只挑纯 JDK 包」处理，见 [`parse_packages`]。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ZuluPackage {
    /// 包 id（取详情用）。
    pub package_uuid: String,
    /// 归档文件名（照抄，不拼）。
    pub name: String,
    /// JDK 版本（`[21,0,13,0,3]`，Azul 用数组给）。
    #[serde(default)]
    pub java_version: Vec<u32>,
    /// CDN 直链。
    pub download_url: String,
}

impl ZuluPackage {
    /// 版本串（`[21,0,13,0,3]` → `21.0.13.0.3`；空数组时退回文件名）。
    #[must_use]
    pub fn version_text(&self) -> String {
        if self.java_version.is_empty() {
            return self.name.clone();
        }
        self.java_version
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// 是否是「纯 JDK」包（排除 CRaC / JavaFX / JRE 变体）。
#[must_use]
pub fn is_plain_jdk(name: &str) -> bool {
    name.contains("-jdk")
        && !name.contains("-jre")
        && !name.contains("crac")
        && !name.contains("-fx")
}

/// 列表响应 → 纯 JDK 包（`latest=true` 会给多条，必须自己挑）。
///
/// # Errors
///
/// 响应不是 JSON、列表为空时返回错误。
pub fn parse_packages(json: &str) -> Result<ZuluPackage, JavaError> {
    let packages: Vec<ZuluPackage> = serde_json::from_str(json).map_err(|error| {
        JavaError::new(
            JavaErrorKind::Metadata,
            format!("Zulu 列表响应不是预期 JSON：{error}"),
        )
    })?;
    let picked = packages
        .iter()
        .find(|package| is_plain_jdk(&package.name))
        .or_else(|| packages.first())
        .ok_or_else(|| {
            JavaError::new(
                JavaErrorKind::Metadata,
                "Zulu 没有该版本/平台的 JDK 包（列表为空）",
            )
        })?;
    Ok(picked.clone())
}

/// 详情响应里的 `sha256_hash`（只信这一个字段，`size` 不可对拍）。
///
/// # Errors
///
/// 响应不是 JSON 或缺可用哈希时返回错误。
pub fn parse_detail_sha256(json: &str) -> Result<String, JavaError> {
    #[derive(Deserialize)]
    struct Detail {
        #[serde(default)]
        sha256_hash: Option<String>,
    }
    let detail: Detail = serde_json::from_str(json).map_err(|error| {
        JavaError::new(
            JavaErrorKind::Metadata,
            format!("Zulu 详情响应不是预期 JSON：{error}"),
        )
    })?;
    detail
        .sha256_hash
        .map(|hex| hex.trim().to_ascii_lowercase())
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| {
            JavaError::new(
                JavaErrorKind::Integrity,
                "Zulu 详情里没有可用的 sha256_hash",
            )
        })
}

/// 查一次「某大版本的最新 Zulu JDK」。
///
/// # Errors
///
/// 表里没配 Zulu、API 不可达/无包、详情缺哈希时返回错误（回退链到此为止，报总失败）。
pub async fn fetch_latest(
    client: &reqwest::Client,
    table: &ZuluTable,
    major: u32,
    target: AdoptiumTarget,
) -> Result<JdkRelease, JavaError> {
    let list = net::get_text(client, &api_url(table, major, target), "Zulu 包列表").await?;
    let package = parse_packages(&list)?;
    let detail = net::get_text(
        client,
        &detail_url(table, &package.package_uuid),
        "Zulu 包详情",
    )
    .await?;
    let sha256 = parse_detail_sha256(&detail)?;
    // Zulu 的版本串用 `java_version` 数组拼（真机是数组不是字符串）；文件名照抄，
    // 因为文件名里 `jdk21.0.13.0.3` 这类四段/五段写法拼不出来。
    Ok(JdkRelease {
        vendor: JdkVendor::Zulu,
        major,
        version: package.version_text(),
        semver: None,
        release_name: package.name.clone(),
        archive: JdkArchive {
            name: package.name,
            url: package.download_url,
            sha256,
            // size 字段不可信（实测差 133 B）→ 填 0，交给 sha256 校验。
            size: 0,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{api_url, detail_url, is_plain_jdk, parse_detail_sha256, parse_packages, zulu_os};
    use crate::download::MirrorTable;
    use crate::game::java::target::AdoptiumTarget;

    const WINDOWS: AdoptiumTarget = AdoptiumTarget::new("windows", "x64");

    fn table() -> crate::download::ZuluTable {
        MirrorTable::builtin()
            .zulu()
            .expect("内置表应带 Zulu")
            .clone()
    }

    #[test]
    fn urls_carry_every_filter_the_api_needs() {
        let table = table();
        let url = api_url(&table, 21, WINDOWS);
        assert!(url.starts_with("https://api.azul.com/metadata/v1/zulu/packages/?"));
        for needle in [
            "java_version=21",
            "os=windows",
            "arch=x64",
            "java_package_type=jdk",
            "archive_type=zip",
            "latest=true",
        ] {
            assert!(url.contains(needle), "{needle} 应在 {url}");
        }
        assert_eq!(
            detail_url(&table, "abc-123"),
            "https://api.azul.com/metadata/v1/zulu/packages/abc-123"
        );
        assert_eq!(zulu_os(AdoptiumTarget::new("mac", "aarch64")), "macos");
    }

    #[test]
    fn latest_listing_gives_the_uuid_and_the_exact_filename() {
        // 真机响应（2026-10-30）：`latest=true` 返回多条，`java_version` 是数组，
        // 并且混着 `-crac-` / `-fx-` 变体——必须挑出纯 JDK 那条。
        let json = r#"[
          {"package_uuid":"4ee30e3a-b26a-40d3-9d04-2182eae4075e",
           "name":"zulu21.54.109-ca-jdk21.0.13.0.3-win_x64.zip",
           "java_version":[21,0,13,0,3],
           "download_url":"https://cdn.azul.com/zulu/bin/zulu21.54.109-ca-jdk21.0.13.0.3-win_x64.zip"},
          {"package_uuid":"2781fb21-9401-40d3-a88e-cf8f36541e3c",
           "name":"zulu21.52.203-ca-crac-jdk21.0.12.1-win_x64.zip",
           "java_version":[21,0,12,1],
           "download_url":"https://cdn.azul.com/zulu/bin/zulu21.52.203-ca-crac-jdk21.0.12.1-win_x64.zip"},
          {"package_uuid":"49851aa2-4fd2-4e54-a903-323989cdf711",
           "name":"zulu21.52.203-ca-fx-jdk21.0.12.1-win_x64.zip",
           "java_version":[21,0,12,1],
           "download_url":"https://cdn.azul.com/zulu/bin/zulu21.52.203-ca-fx-jdk21.0.12.1-win_x64.zip"}
        ]"#;
        let package = parse_packages(json).expect("应解析");
        assert_eq!(package.package_uuid, "4ee30e3a-b26a-40d3-9d04-2182eae4075e");
        assert_eq!(
            package.name, "zulu21.54.109-ca-jdk21.0.13.0.3-win_x64.zip",
            "文件名必须照抄 API（三段/四段混写，拼不出来）"
        );
        assert_eq!(package.version_text(), "21.0.13.0.3");
        assert!(is_plain_jdk(&package.name));
        assert!(!is_plain_jdk(
            "zulu21.52.203-ca-crac-jdk21.0.12.1-win_x64.zip"
        ));
        assert!(!is_plain_jdk(
            "zulu21.52.203-ca-fx-jdk21.0.12.1-win_x64.zip"
        ));
        assert!(
            parse_packages("[]").is_err(),
            "空列表要报错而不是造一个空包"
        );
        assert!(parse_packages("<html>502</html>").is_err());
    }

    #[test]
    fn detail_gives_a_trustworthy_sha256_only() {
        let json = r#"{"package_uuid":"x","size":211151000,
            "sha256_hash":"6B0E2B0F4C1D2A3E4F5061728394A5B6C7D8E9F0A1B2C3D4E5F60718293A4B5C"}"#;
        assert_eq!(
            parse_detail_sha256(json).expect("大写哈希应被接受并转小写"),
            "6b0e2b0f4c1d2a3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f60718293a4b5c"
        );
        assert!(parse_detail_sha256(r#"{"size":1}"#).is_err());
        assert!(parse_detail_sha256("not json").is_err());
    }
}
