//! 依赖库模型：Maven 坐标、natives 选择、库文件相对路径。
//!
//! 依据：`DevDocs/analyse/03-version-download.md` §6.4（libraries 含 natives 规则）、
//! §6.8（目录布局速查）。
//!
//! 这里只做"数据 → 路径/候选"的纯计算，**不碰网络与文件系统**：
//! 真正的下载归 `core::download`，放置归 [`super::layout`]。

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::arguments::{rules_allow, Environment, Rule};
use super::error::GameError;
use super::manifest::DownloadInfo;

/// 一条依赖库。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    /// Maven 坐标 `group:artifact:version[:classifier][@ext]`。
    #[serde(default)]
    pub name: String,
    /// 下载信息；老 JSON/魔改 JSON 可能没有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<LibraryDownloads>,
    /// natives 选择表：`{ "windows": "natives-windows-${arch}" }`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natives: Option<BTreeMap<String, String>>,
    /// 生效规则（空 = 生效）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    /// natives 解压规则。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<Extract>,
    /// Maven 仓库基地址覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// HMCL 私有：`local` 表示库放在实例私有的 `versions/<id>/libraries/` 下。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// 未识别字段原样保留。
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 一条依赖库的下载表。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryDownloads {
    /// 主构件。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<DownloadInfo>,
    /// 分类器构件（natives 走这里）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub classifiers: BTreeMap<String, DownloadInfo>,
}

/// natives 解压规则：命中 `exclude` 的条目不解压。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extract {
    /// 排除的路径模式（`META-INF/`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
}

/// Maven 坐标（`group:artifact:version[:classifier][@ext]`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenCoordinate {
    /// group，如 `org.lwjgl`。
    pub group: String,
    /// artifact，如 `lwjgl`。
    pub artifact: String,
    /// version，如 `3.3.2`。
    pub version: String,
    /// classifier（natives 常用），如 `natives-windows`。
    pub classifier: Option<String>,
    /// 扩展名，缺省 `jar`。
    pub extension: String,
}

impl MavenCoordinate {
    /// 解析坐标字符串。
    pub fn parse(raw: &str) -> Result<Self, GameError> {
        let invalid = |reason: &'static str| GameError::InvalidCoordinate {
            raw: raw.to_owned(),
            reason,
        };
        let (body, extension) = match raw.split_once('@') {
            Some((body, ext)) if !ext.is_empty() => (body, ext.to_owned()),
            Some(_) => return Err(invalid("扩展名为空")),
            None => (raw, String::from("jar")),
        };
        let parts: Vec<&str> = body.split(':').collect();
        if parts.len() < 3 {
            return Err(invalid("至少需要 group:artifact:version 三段"));
        }
        if parts.iter().any(|p| p.is_empty()) {
            return Err(invalid("存在空字段"));
        }
        match parts.len() {
            3 => Ok(Self {
                group: parts[0].to_owned(),
                artifact: parts[1].to_owned(),
                version: parts[2].to_owned(),
                classifier: None,
                extension,
            }),
            4 => Ok(Self {
                group: parts[0].to_owned(),
                artifact: parts[1].to_owned(),
                version: parts[2].to_owned(),
                classifier: Some(parts[3].to_owned()),
                extension,
            }),
            _ => Err(invalid("字段多于四段")),
        }
    }

    /// group 转目录形式（`org.lwjgl` → `org/lwjgl`）。
    pub fn group_path(&self) -> String {
        self.group.replace('.', "/")
    }

    /// 文件名：`<artifact>-<version>[-<classifier>].<ext>`。
    pub fn file_name(&self) -> String {
        match &self.classifier {
            Some(classifier) => format!(
                "{}-{}-{}.{}",
                self.artifact, self.version, classifier, self.extension
            ),
            None => format!("{}-{}.{}", self.artifact, self.version, self.extension),
        }
    }

    /// 相对 `libraries/` 的路径：
    /// `<group path>/<artifact>/<version>/<artifact>-<version>[-<classifier>].<ext>`。
    pub fn relative_path(&self) -> PathBuf {
        PathBuf::from(self.group_path())
            .join(&self.artifact)
            .join(&self.version)
            .join(self.file_name())
    }

    /// 换一个 classifier 的副本（natives 用）。
    pub fn with_classifier(&self, classifier: impl Into<String>) -> Self {
        Self {
            classifier: Some(classifier.into()),
            ..self.clone()
        }
    }
}

impl Library {
    /// 解析本库的 Maven 坐标。
    pub fn coordinate(&self) -> Result<MavenCoordinate, GameError> {
        MavenCoordinate::parse(&self.name)
    }

    /// 该库在当前环境下是否生效。
    pub fn applies(&self, env: &Environment) -> bool {
        rules_allow(&self.rules, env)
    }

    /// 是否是实例私有库（HMCL `hint=local`）。
    pub fn is_local(&self) -> bool {
        self.hint.as_deref() == Some("local")
    }

    /// 主构件下载信息。
    pub fn artifact(&self) -> Option<&DownloadInfo> {
        self.downloads.as_ref()?.artifact.as_ref()
    }

    /// natives 分类器名：按 OS 查 `natives` 表并展开 `${arch}`（64 位为 `64`）。
    pub fn native_classifier(&self, env: &Environment) -> Option<String> {
        let raw = self.natives.as_ref()?.get(&env.os)?;
        let arch = if matches!(env.arch.as_str(), "x86_64" | "arm64") {
            "64"
        } else {
            ""
        };
        Some(raw.replace("${arch}", arch))
    }

    /// natives 构件下载信息（按分类器查 `downloads.classifiers`）。
    pub fn native_artifact(&self, classifier: &str) -> Option<&DownloadInfo> {
        self.downloads.as_ref()?.classifiers.get(classifier)
    }

    /// 主构件相对 `libraries/` 的路径。
    ///
    /// 优先用 JSON 里给出的 `artifact.path`（Mojang 官方的权威值），否则按坐标推导。
    pub fn relative_path(&self) -> Result<PathBuf, GameError> {
        if let Some(path) = self.artifact().and_then(|a| a.path.as_deref()) {
            return Ok(PathBuf::from(path));
        }
        Ok(self.coordinate()?.relative_path())
    }

    /// natives 构件相对 `libraries/` 的路径。
    pub fn native_relative_path(&self, classifier: &str) -> Result<PathBuf, GameError> {
        if let Some(path) = self
            .native_artifact(classifier)
            .and_then(|a| a.path.as_deref())
        {
            return Ok(PathBuf::from(path));
        }
        Ok(self
            .coordinate()?
            .with_classifier(classifier)
            .relative_path())
    }
}
