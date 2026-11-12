//! 一次版本查询的统一产物：一个可下载的 JDK 归档（四家发行版共用形态）。
//!
//! 各家的**版本号语义不同**（research/06 §5.3 第 3 条），所以这里只存「vendor 原生的版本串」
//! 而不是硬拆成 semver 四段：
//!
//! | 发行版 | 版本串 | 出处 |
//! | --- | --- | --- |
//! | Temurin | `21.0.13+9`（三段+构建号） | Adoptium `release_name` |
//! | Oracle OpenJDK | `21.0.2`（三段，只到 GA + ≤2 update） | 镜像目录名 |
//! | Microsoft OpenJDK | `21.0.13.0.2`（四段/五段），11 线是 `11.0.32.1` | `aka.ms` 文件名 |
//! | Azul Zulu | `21.0.13+3`（三段+构建号，文件名里还有 `21.0.13.0.3` 另一种写法） | Zulu API |
//!
//! 目录名、戳记、`--json` 都直接用这个串（`location::sanitize_version` 只做字符白名单）。

use super::vendor::JdkVendor;

/// 一个可下载的 JDK 归档。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JdkArchive {
    /// 产物文件名（`OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip`）。
    pub name: String,
    /// 官方下载链接（镜像展开的输入；Oracle / MS / Zulu 已经是最终链接）。
    pub url: String,
    /// SHA-256（小写十六进制）。
    pub sha256: String,
    /// 字节数（`0` = 上游没给可信大小，引擎跳过长度对拍只验哈希）。
    pub size: u64,
}

/// 一次版本查询的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JdkRelease {
    /// 发行版。
    pub vendor: JdkVendor,
    /// 大版本号（`21`）。
    pub major: u32,
    /// vendor 原生的完整版本串（目录名与展示都用它）。
    pub version: String,
    /// Temurin 系才有的 semver（其它家为 `None`）。
    pub semver: Option<String>,
    /// release 名（Temurin `jdk-21.0.13+9`；其它家用版本串）。
    pub release_name: String,
    /// 归档。
    pub archive: JdkArchive,
}

impl JdkRelease {
    /// 是否就是这个发行版 + 大版本的产物。
    #[must_use]
    pub fn matches(&self, vendor: JdkVendor, major: u32) -> bool {
        self.vendor == vendor && self.major == major
    }

    /// sha256（幂等判据用）。
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.archive.sha256
    }

    /// 人读一行（`Temurin 21.0.13+9 · OpenJDK21U-…zip`）。
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} {} · {}",
            self.vendor.display(),
            self.version,
            self.archive.name
        )
    }

    /// `--json` 的归档块（四家共用一个形状）。
    #[must_use]
    pub fn archive_json(&self) -> serde_json::Value {
        serde_json::json!({
            "vendor": self.vendor.id(),
            "version": self.version,
            "name": self.archive.name,
            "url": self.archive.url,
            "sha256": self.archive.sha256,
            "size": self.archive.size,
        })
    }
}

/// 从 `release_name`（Temurin 的 `jdk-21.0.13+9`）取版本串。
#[must_use]
pub fn version_from_release_name(release_name: &str) -> Option<String> {
    let trimmed = release_name.trim();
    let stripped = trimmed.strip_prefix("jdk-").unwrap_or(trimmed);
    (!stripped.is_empty()).then(|| stripped.to_string())
}

/// 解析 sha256 文本（`.sha256` / `.sha256sum.txt` / 裸哈希都吃），校验 64 位十六进制。
#[must_use]
pub fn parse_sha256_text(text: &str) -> Option<String> {
    for token in text.split_whitespace() {
        let lowered = token.trim().to_ascii_lowercase();
        let candidate = lowered.strip_prefix('*').unwrap_or(lowered.as_str());
        if candidate.len() == 64 && candidate.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Some(candidate.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{parse_sha256_text, version_from_release_name, JdkArchive, JdkRelease};
    use crate::game::java::vendor::JdkVendor;

    fn release() -> JdkRelease {
        JdkRelease {
            vendor: JdkVendor::Oracle,
            major: 21,
            version: "21.0.2".to_string(),
            semver: None,
            release_name: "21.0.2".to_string(),
            archive: JdkArchive {
                name: "openjdk-21.0.2_windows-x64_bin.zip".to_string(),
                url: "https://mirrors.huaweicloud.com/openjdk/21.0.2/openjdk-21.0.2_windows-x64_bin.zip"
                    .to_string(),
                sha256: "b6c17e747ae78cdd6de4d7532b3164b277daee97c007d3eaa2b39cca99882664"
                    .to_string(),
                size: 0,
            },
        }
    }

    #[test]
    fn release_carries_vendor_and_native_version() {
        let item = release();
        assert!(item.matches(JdkVendor::Oracle, 21));
        assert!(!item.matches(JdkVendor::Microsoft, 21));
        assert!(item.summary().contains("Oracle OpenJDK 21.0.2"));
        assert_eq!(item.archive_json()["vendor"], "oracle");
    }

    #[test]
    fn sha256_texts_from_every_vendor_parse_the_same_way() {
        // Huawei 的 .sha256：只有 64 位哈希，没有文件名。
        assert_eq!(
            parse_sha256_text("b6c17e747ae78cdd6de4d7532b3164b277daee97c007d3eaa2b39cca99882664\n")
                .as_deref(),
            Some("b6c17e747ae78cdd6de4d7532b3164b277daee97c007d3eaa2b39cca99882664")
        );
        // MS 的 .sha256sum.txt：`<hash>  <filename>`，并且带大写与 CRLF。
        assert_eq!(
            parse_sha256_text(
                "6ECFA864AE0A1B2C3D4E5F60718293A4B5C6D7E8F90A1B2C3D4E5F60718293A4  microsoft-jdk-21.0.13-windows-x64.zip\r\n"
            )
            .as_deref(),
            Some("6ecfa864ae0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4")
        );
        assert_eq!(parse_sha256_text("<html>404</html>"), None);
        assert_eq!(parse_sha256_text("deadbeef"), None);
    }

    #[test]
    fn version_falls_back_to_the_release_name() {
        assert_eq!(
            version_from_release_name("jdk-21.0.13+9").as_deref(),
            Some("21.0.13+9")
        );
        assert_eq!(
            version_from_release_name("21.0.13+9").as_deref(),
            Some("21.0.13+9")
        );
        assert_eq!(version_from_release_name(""), None);
    }
}
