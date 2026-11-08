//! Adoptium（Eclipse Temurin）版本查询：`api.adoptium.net` → 一个可下载的 JDK 归档。
//!
//! 只用 v3 的 `assets/latest/{major}/hotspot` 这一条接口（D13：自动下载只装最新 GA）：
//!
//! ```text
//! GET https://api.adoptium.net/v3/assets/latest/21/hotspot?os=windows&arch=x64&image_type=jdk&vendor=eclipse
//!     → [ { release_name, version{major,semver}, binary{os,architecture,image_type,package{name,link,checksum,size}} } ]
//! ```
//!
//! 两个实测坑（都在 Windows x64 / Linux x64 上确认过）：
//!
//! 1. **`arch` 参数不参与过滤**：`os=windows&arch=x64` 也会把 `aarch64` 的条目一起返回，
//!    必须自己按 `binary.architecture` 再筛一遍，否则 ARM 包会被当成 x64 装下来；
//! 2. **`package.link` 是 GitHub release 直链**（`github.com/adoptium/temurin21-binaries/releases/download/…`），
//!    这正是 [`crate::download::adoptium`] 的镜像展开所认识的形态——所以院校镜像加速
//!    不需要本模块拼任何 URL。

use serde::Deserialize;

use super::error::{JavaError, JavaErrorKind};
use super::release::version_from_release_name;
use super::target::AdoptiumTarget;
use super::vendor::JdkVendor;

/// Adoptium API 根。
pub const ADOPTIUM_API_BASE: &str = "https://api.adoptium.net/v3";

/// 托管 JDK 的默认供应商标识（D13 v1 的目录名 `jdk-<major>-vendor` 沿用它）。
pub const JDK_VENDOR: &str = "temurin";

/// 归档与查询结果的结构定义在 [`super::release`]（四家发行版共用）；这里重导出以保持
/// `api::JdkRelease` / `api::JdkArchive` 这两个老路径可用。
pub use super::release::{JdkArchive, JdkRelease};

/// 拼 `latest/{major}` 的查询地址。
#[must_use]
pub fn latest_url(major: u32, target: AdoptiumTarget) -> String {
    format!(
        "{ADOPTIUM_API_BASE}/assets/latest/{major}/hotspot?os={}&arch={}&image_type=jdk&vendor=eclipse",
        target.os, target.arch
    )
}

/// 解析 `assets/latest` 的响应体，挑出与目标平台匹配的那一条。
///
/// # Errors
///
/// 响应体不是 JSON、列表为空、没有匹配 `os` + `arch` 的 jdk 条目、sha256 不合法时返回错误。
pub fn parse_latest(json: &str, target: AdoptiumTarget) -> Result<JdkRelease, JavaError> {
    let assets: Vec<ApiAsset> = serde_json::from_str(json).map_err(|error| {
        JavaError::new(
            JavaErrorKind::Metadata,
            format!("Adoptium 响应不是预期 JSON：{error}"),
        )
    })?;
    let matched = assets
        .into_iter()
        .find(|asset| asset.matches(target))
        .ok_or_else(|| {
            JavaError::new(
                JavaErrorKind::Metadata,
                format!(
                    "Adoptium 没有 {}/{} 的 jdk 产物（响应里 {} 条都不匹配）",
                    target.os, target.arch, "全部"
                ),
            )
            .with_hint("可能该大版本只有别的架构；用 `ull java list` 挑已有 JDK")
        })?;
    matched.into_release()
}

/// 请求一次「某大版本的最新 GA」。
///
/// # Errors
///
/// 网络不可达、HTTP 非 2xx、响应体不认识、没有对应平台产物时返回错误。
pub async fn fetch_latest(
    client: &reqwest::Client,
    major: u32,
    target: AdoptiumTarget,
) -> Result<JdkRelease, JavaError> {
    let url = latest_url(major, target);
    let response = client.get(&url).send().await.map_err(|error| {
        JavaError::new(
            JavaErrorKind::Network,
            format!("连不上 Adoptium API：{error}"),
        )
        .with_hint("检查网络/代理；已装的 JDK 仍可用 `ull java list` 看到")
    })?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(JavaError::new(
            JavaErrorKind::Metadata,
            format!("Adoptium 没有 {major} 的 GA 产物（HTTP 404）"),
        )
        .with_hint("可自动下载的一般是 8 / 11 / 17 / 21 / 25 这类 GA 大版本"));
    }
    if !status.is_success() {
        return Err(JavaError::new(
            JavaErrorKind::Network,
            format!("Adoptium API 返回 HTTP {status}（{url}）"),
        ));
    }
    let body = response.text().await.map_err(|error| {
        JavaError::new(
            JavaErrorKind::Network,
            format!("读 Adoptium 响应失败：{error}"),
        )
    })?;
    parse_latest(&body, target)
}

/// 响应里的 `package` 子对象。
#[derive(Debug, Deserialize)]
struct ApiPackage {
    name: String,
    link: String,
    #[serde(default)]
    checksum: Option<String>,
    #[serde(default)]
    size: u64,
}

/// 响应里的 `binary` 子对象。
#[derive(Debug, Deserialize)]
struct ApiBinary {
    #[serde(default)]
    os: Option<String>,
    #[serde(default)]
    architecture: Option<String>,
    #[serde(default)]
    image_type: Option<String>,
    #[serde(default)]
    package: Option<ApiPackage>,
}

/// 响应里的 `version` 子对象。
#[derive(Debug, Deserialize)]
struct ApiVersion {
    #[serde(default)]
    major: Option<u32>,
    #[serde(default)]
    semver: Option<String>,
}

/// 响应列表里的一条。
#[derive(Debug, Deserialize)]
struct ApiAsset {
    #[serde(default)]
    release_name: String,
    #[serde(default)]
    version: Option<ApiVersion>,
    #[serde(default)]
    binary: Option<ApiBinary>,
}

impl ApiAsset {
    /// 是否就是目标平台的 jdk 归档（`arch` 参数不过滤，必须自己判）。
    fn matches(&self, target: AdoptiumTarget) -> bool {
        let Some(binary) = &self.binary else {
            return false;
        };
        let same_os = binary.os.as_deref().is_some_and(|os| os == target.os);
        let same_arch = binary
            .architecture
            .as_deref()
            .is_some_and(|arch| arch == target.arch);
        let is_jdk = binary.image_type.as_deref() != Some("jre");
        same_os && same_arch && is_jdk && binary.package.is_some()
    }

    /// 转成对外结果（校验 sha256 形态）。
    fn into_release(self) -> Result<JdkRelease, JavaError> {
        let package = self
            .binary
            .and_then(|binary| binary.package)
            .ok_or_else(|| {
                JavaError::new(JavaErrorKind::Metadata, "Adoptium 条目缺少 package 子对象")
            })?;
        let sha256 = package
            .checksum
            .as_deref()
            .map(|hex| hex.trim().to_ascii_lowercase())
            .filter(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| {
                JavaError::new(
                    JavaErrorKind::Integrity,
                    format!("Adoptium 没给可用的 sha256（{}）", package.name),
                )
                .with_hint("换一个大版本，或手动装 JDK 后设 JAVA_HOME")
            })?;
        let version = self.version;
        let major = version
            .as_ref()
            .and_then(|version| version.major)
            .or_else(|| major_from_name(&package.name))
            .ok_or_else(|| {
                JavaError::new(
                    JavaErrorKind::Metadata,
                    format!("Adoptium 条目里读不出大版本号（{}）", package.name),
                )
            })?;
        let native_version = version_from_release_name(&self.release_name)
            .or_else(|| version.as_ref().and_then(|version| version.semver.clone()))
            .unwrap_or_else(|| major.to_string());
        Ok(JdkRelease {
            vendor: JdkVendor::Temurin,
            major,
            version: native_version,
            semver: version.and_then(|version| version.semver),
            release_name: self.release_name,
            archive: JdkArchive {
                name: package.name,
                url: package.link,
                sha256,
                size: package.size,
            },
        })
    }
}

/// 从产物名兜底读大版本（`OpenJDK21U-jdk_x64_windows_hotspot_…` → `21`）。
fn major_from_name(name: &str) -> Option<u32> {
    let rest = name.split_once("OpenJDK")?.1;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{latest_url, parse_latest, ApiAsset};
    use crate::game::java::error::JavaErrorKind;
    use crate::game::java::target::AdoptiumTarget;

    const WINDOWS: AdoptiumTarget = AdoptiumTarget::new("windows", "x64");

    fn sample() -> String {
        // 实测：`arch` 参数不过滤，响应里会混进别的架构。
        r#"[
          {
            "release_name": "jdk-21.0.13+9",
            "version": {"major": 21, "semver": "21.0.13+9.0.LTS"},
            "binary": {
              "os": "windows", "architecture": "aarch64", "image_type": "jdk",
              "package": {
                "name": "OpenJDK21U-jdk_aarch64_windows_hotspot_21.0.12.1_1.zip",
                "link": "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.12.1%2B1/OpenJDK21U-jdk_aarch64_windows_hotspot_21.0.12.1_1.zip",
                "checksum": "9dcbb8ebe24aa9ee5a47a7fab2106a7c8988d7acee461260fef107411df91dc5",
                "size": 179666944
              }
            }
          },
          {
            "release_name": "jdk-21.0.13+9",
            "version": {"major": 21, "semver": "21.0.13+9.0.LTS"},
            "binary": {
              "os": "windows", "architecture": "x64", "image_type": "jdk",
              "package": {
                "name": "OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip",
                "link": "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13%2B9/OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip",
                "checksum": "a4dbaa05fca4bd89d84584e58c459bd732efebb1722af80ab13c6516832004f9",
                "size": 190000000
              }
            }
          }
        ]"#
        .to_string()
    }

    #[test]
    fn picks_the_matching_arch_and_keeps_the_github_link() {
        let release = parse_latest(&sample(), WINDOWS).expect("应解析出 x64 条目");
        assert_eq!(release.major, 21);
        assert_eq!(
            release.vendor,
            crate::game::java::vendor::JdkVendor::Temurin
        );
        assert_eq!(release.version, "21.0.13+9", "版本串取自 release_name");
        assert_eq!(release.release_name, "jdk-21.0.13+9");
        assert_eq!(release.semver.as_deref(), Some("21.0.13+9.0.LTS"));
        assert_eq!(
            release.archive.name,
            "OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip"
        );
        assert_eq!(release.archive.size, 190_000_000);
        assert!(release.archive.url.contains("temurin21-binaries"));
        assert_eq!(
            crate::download::adoptium::parse_adoptium_artifact(&release.archive.url)
                .map(|artifact| (artifact.major, artifact.arch, artifact.os)),
            Some(("21".to_string(), "x64".to_string(), "windows".to_string()))
        );
    }

    #[test]
    fn missing_arch_is_an_error_not_a_guess() {
        let error = parse_latest(&sample(), AdoptiumTarget::new("windows", "x86-32"))
            .expect_err("x86-32 应没有条目");
        assert_eq!(error.kind(), JavaErrorKind::Metadata);
    }

    #[test]
    fn bad_checksum_is_rejected() {
        let body = sample().replace(
            "a4dbaa05fca4bd89d84584e58c459bd732efebb1722af80ab13c6516832004f9",
            "deadbeef",
        );
        let error = parse_latest(&body, WINDOWS).expect_err("短 sha256 应被拒");
        assert_eq!(error.kind(), JavaErrorKind::Integrity);
    }

    #[test]
    fn latest_url_carries_os_arch_and_flavor() {
        let url = latest_url(17, AdoptiumTarget::new("linux", "aarch64"));
        assert!(url.starts_with("https://api.adoptium.net/v3/assets/latest/17/hotspot?"));
        assert!(url.contains("os=linux"));
        assert!(url.contains("arch=aarch64"));
        assert!(url.contains("image_type=jdk"));
    }

    #[test]
    fn non_json_body_is_metadata_error() {
        let error = parse_latest("<html>502</html>", WINDOWS).expect_err("HTML 应被拒");
        assert_eq!(error.kind(), JavaErrorKind::Metadata);
    }

    #[test]
    fn major_falls_back_to_the_artifact_name() {
        let body = r#"[{"release_name":"jdk-17.0.13+11",
            "binary":{"os":"windows","architecture":"x64","image_type":"jdk",
            "package":{"name":"OpenJDK17U-jdk_x64_windows_hotspot_17.0.13_11.zip",
            "link":"https://example.invalid/x.zip",
            "checksum":"a4dbaa05fca4bd89d84584e58c459bd732efebb1722af80ab13c6516832004f9",
            "size":1}}}]"#;
        let release = parse_latest(body, WINDOWS).expect("应解析");
        assert_eq!(release.major, 17);
        assert_eq!(release.semver, None);
    }

    #[test]
    fn asset_without_binary_never_matches() {
        let asset: ApiAsset = serde_json::from_str(r#"{"release_name":"x"}"#).expect("最小条目");
        assert!(!asset.matches(WINDOWS));
    }
}
