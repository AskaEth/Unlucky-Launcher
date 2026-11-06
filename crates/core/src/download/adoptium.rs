//! Adoptium（Temurin）JDK 的院校镜像展开。
//!
//! D13 定案：国内模式下 JDK 按健康度在 TUNA / 华为云 等镜像之间选，
//! 全部不可达才回官方。难点是**镜像路径与官方下载链接不同形**：
//!
//! 官方（api.adoptium.net 会 302 到这里）：
//! `https://github.com/adoptium/temurin17-binaries/releases/download/JDK17.0.13+11/OpenJDK17U-jdk_x64_windows_hotspot_17.0.13_11.zip`
//! 清华 TUNA：
//! `https://mirrors.tuna.tsinghua.edu.cn/Adoptium/17/jdk/x64/windows/OpenJDK17U-jdk_x64_windows_hotspot_17.0.13_11.zip`
//!
//! 所以这里按产物文件名反解 `(major, flavor, arch, os, filename)`，再按各镜像的模板拼目标 URL。
//! **`flavor` 必须是 `jdk` 或 `jre`**：院校镜像的目录结构是 `/{major}/{jdk|jre}/{arch}/{os}/`，
//! JRE 请求若错拼成 `/jdk/` 会 404 打满所有镜像，最后退回官方直链（镜像加速白费）。
//! 解析失败不报错——只保留官方源，下载照常能完成。
//!
//! # 清单可用性（M33 实测，2026-10-30）
//!
//! 按 `{base}/{21}/jdk/x64/windows/<Temurin 文件名>` 逐个打点：
//!
//! | 镜像 | 结果 |
//! | --- | --- |
//! | 清华 TUNA `mirrors.tuna.tsinghua.edu.cn/Adoptium` | 200（真实下载 205 MB / 22 秒成功） |
//! | 中科大 USTC | **404**——`/adoptium/` 下只有 `deb/` `rpm/` `releases/`，没有这套目录树 |
//! | 上交 SJTU / 北外 BFSU | **404**（同样没有该目录树） |
//! | 华为云 | **假阳性**：门户对任意路径都回 `200` + 12 KB HTML（M34 实测，`/v1/repositories` 里没有 adoptium） |
//!
//! 因此 `mirrors.json` 里这四项全是 `enabled: false`：留着当「将来恢复」的登记，
//! 但不让它们每次安装都白跑一趟（健康度虽然会冷却，首轮仍要付代价）。
//! **判可用性别只看状态码**：真仓库目录页含 `<pre>`，门户兜底页没有——状态码 200 说明不了什么。
//!
//! # 4xx 不是「源坏」
//!
//! TUNA 每个 `{major}/{jdk|jre}/{arch}/{os}/` 目录只保留**最新一个补丁**（实测 21.0.12 → 404，
//! 21.0.13 → 200 且大小与 Adoptium API 一致）。所以 `NotFound` 在引擎里走
//! `HealthRegistry::record_missing`：不计失败、不进冷却，只留诊断痕迹。

use serde::{Deserialize, Serialize};

use super::mirrors::MirrorTable;
use super::types::Candidate;

const fn default_true() -> bool {
    true
}

/// 一个 Adoptium 院校镜像。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptiumMirror {
    /// 源标识（`tuna` / `ustc` / `sjtu` / `bfsu`）。
    pub id: String,
    /// 展示名。
    pub name: String,
    /// 镜像根（形如 `https://mirrors.tuna.tsinghua.edu.cn/Adoptium`）。
    pub base: String,
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 该源只保留「最新补丁」（TUNA 实测：每个 `{major}/{jdk|jre}/{arch}/{os}/` 只有 1 个归档）。
    ///
    /// 标记本身不改 URL 形态，只给诊断与 `java vendors` 提示用：旧补丁在这里必然 404，
    /// 引擎侧把 4xx 当「正常降级」而不是「源坏」（见 `download::health::record_missing`）。
    #[serde(default)]
    pub latest_only: bool,
}

impl MirrorTable {
    /// Adoptium 类 URL 展开成院校镜像候选（按清单顺序；健康度排序由引擎负责）。
    #[must_use]
    pub fn adoptium_candidates(&self, url: &str) -> Vec<Candidate> {
        let Some(artifact) = parse_adoptium_artifact(url) else {
            return Vec::new();
        };
        self.adoptium
            .iter()
            .filter(|mirror| mirror.enabled)
            .map(|mirror| {
                let base = mirror.base.trim_end_matches('/');
                Candidate::new(mirror.id.clone(), artifact.mirror_url(base))
            })
            .collect()
    }
}

/// Adoptium 产物形态：JDK 还是 JRE（决定镜像的 `{major}/jdk|jre/` 路径段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JdkFlavor {
    /// 完整开发包（`OpenJDK17U-jdk_...`）。
    Jdk,
    /// 运行时（`OpenJDK17U-jre_...`）。
    Jre,
}

impl JdkFlavor {
    /// 镜像路径里的段名。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Jdk => "jdk",
            Self::Jre => "jre",
        }
    }

    /// 从产物文件名判形态：`OpenJDK17U-jdk_` / `OpenJDK17U-jre_` / `*-jre-17…` 兜底。
    #[must_use]
    fn from_filename(filename: &str) -> Self {
        if filename.contains("-jre") || filename.contains("_jre") {
            Self::Jre
        } else {
            Self::Jdk
        }
    }
}

/// 解析出来的 Adoptium 产物。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptiumArtifact {
    /// 大版本号（`17` / `21`）。
    pub major: String,
    /// JDK 还是 JRE。
    pub flavor: JdkFlavor,
    /// CPU 架构（`x64` / `aarch64` / …）。
    pub arch: String,
    /// 目标系统（`windows` / `linux` / `mac` / …）。
    pub os: String,
    /// 产物文件名（含扩展名）。
    pub filename: String,
}

impl AdoptiumArtifact {
    /// 拼某个镜像根下的下载地址：`{base}/{major}/{jdk|jre}/{arch}/{os}/{filename}`。
    #[must_use]
    pub fn mirror_url(&self, base: &str) -> String {
        format!(
            "{}/{}/{}/{}/{}/{}",
            base.trim_end_matches('/'),
            self.major,
            self.flavor.as_str(),
            self.arch,
            self.os,
            self.filename
        )
    }
}

/// 解析 Adoptium 产物名。
///
/// 输入形如 `https://github.com/adoptium/temurin17-binaries/releases/download/JDK17.0.13%2B11/OpenJDK17U-jdk_x64_windows_hotspot_17.0.13_11.zip`。
/// 解析失败返回 `None`（此时只保留官方源，不影响下载）。
#[must_use]
pub fn parse_adoptium_artifact(url: &str) -> Option<AdoptiumArtifact> {
    let filename = url.rsplit('/').next()?;
    if filename.is_empty()
        || (!filename.contains("OpenJDK") && !filename.contains("jdk") && !filename.contains("jre"))
    {
        return None;
    }
    let mut major: Option<String> = None;
    let mut arch: Option<&str> = None;
    let mut os: Option<&str> = None;
    for token in filename.split('_') {
        if let Some(stripped) = token.strip_prefix("OpenJDK") {
            let digits: String = stripped.chars().take_while(char::is_ascii_digit).collect();
            if !digits.is_empty() {
                major = Some(digits);
            }
        }
        arch = arch.or(match token {
            "x64" => Some("x64"),
            "x86-32" | "x86" => Some("x86-32"),
            "aarch64" => Some("aarch64"),
            "arm" => Some("arm"),
            "ppc64le" => Some("ppc64le"),
            "s390x" => Some("s390x"),
            _ => None,
        });
        os = os.or(match token {
            "windows" => Some("windows"),
            "linux" => Some("linux"),
            "mac" => Some("mac"),
            "alpine-linux" => Some("alpine-linux"),
            "aix" => Some("aix"),
            _ => None,
        });
    }
    let major = match major {
        Some(major) => major,
        None => {
            // 兜底形态：`jdk-17.0.13+11_...` 之类。
            let rest = filename.split("jdk-").nth(1)?;
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() {
                return None;
            }
            digits
        }
    };
    Some(AdoptiumArtifact {
        major,
        flavor: JdkFlavor::from_filename(filename),
        arch: arch?.to_string(),
        os: os?.to_string(),
        filename: filename.to_string(),
    })
}
