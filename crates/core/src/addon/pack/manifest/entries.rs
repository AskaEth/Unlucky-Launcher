//! 条目级模型：远端文件、覆盖目录、以及"未知字段并回"的唯一实现。
//!
//! 拆在这里是为了守 400 行上限；对外仍是 [`crate::addon::pack::manifest`] 的一部分
//! （`manifest` 用 `pub use` 重新导出）。

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::{EnvSupport, PackSide};

/// 一个远端文件条目。
///
/// CF 只填 `project_id`/`file_id`/`client`/`server`；mrpack 只填 `path`/`hashes`/`downloads`/
/// `size`/`client`/`server`。两者可以同时存在（例如 M111 补全 CF 条目之后）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackFile {
    /// 实例内相对路径（`mods/jei.jar`）。CF 官方清单没有这一项 → `None`。
    pub path: Option<String>,
    /// CF `projectID`。
    pub project_id: Option<i64>,
    /// CF `fileID`。
    pub file_id: Option<i64>,
    /// 摘要表（键名小写：`sha1` / `sha512` / `sha256` / `md5`）。mrpack 要求有 `sha512`。
    pub hashes: BTreeMap<String, String>,
    /// 下载候选（mrpack `downloads`，多源；CF 官方清单没有，要靠 CDN 规则或 API 拼）。
    pub downloads: Vec<String>,
    /// 文件字节数（mrpack `fileSize`）。
    pub size: Option<u64>,
    /// 是否"以禁用态安装"（`.disabled`）。
    ///
    /// 社区里 `.disabled` 有三种表达（M115 调研 §字段缺口 2 / §8.1）：
    /// - 官方 mrpack 导出器：**路径本身带 `.disabled` 后缀**（A 级）；
    /// - HMCL/D10 旧措辞：去掉后缀，用 `env.client = optional`（B 级，**跨启动器不保真**）；
    /// - CF：没有原生表达，Prism 借 `required: false`。
    ///
    /// **本仓库的定案（tower，M112）**：一律走路径后缀。`env.client=optional` 只是"安装时可
    /// 取消勾选"，装回来默认仍然启用，不能当禁用；CF 表达不了禁用位，所以禁用文件只能内联进
    /// `overrides/`（导出管线负责，见 `crate::addon::export`）。
    pub disabled: bool,
    /// 客户端支持度。
    pub client: EnvSupport,
    /// 服务端支持度。
    pub server: EnvSupport,
    /// 该条目的未知字段。
    pub extra: Map<String, Value>,
    /// `env` 对象里的未知字段。
    pub env_extra: Map<String, Value>,
}

impl PackFile {
    /// 只带路径的条目（mrpack 侧最简形态）。
    #[must_use]
    pub fn with_path(path: impl Into<String>) -> Self {
        Self {
            path: Some(path.into()),
            ..Self::default()
        }
    }

    /// 只带 CF 标识的条目。
    #[must_use]
    pub fn with_curse(project_id: i64, file_id: i64) -> Self {
        Self {
            project_id: Some(project_id),
            file_id: Some(file_id),
            ..Self::default()
        }
    }

    /// SHA-1（有就返回）。
    #[must_use]
    pub fn sha1(&self) -> Option<&str> {
        self.hashes.get("sha1").map(String::as_str)
    }

    /// SHA-512（有就返回）。
    #[must_use]
    pub fn sha512(&self) -> Option<&str> {
        self.hashes.get("sha512").map(String::as_str)
    }

    /// 稳定标识：用于"用户取消勾选某文件"的排除集合，跨版本比对要稳。
    ///
    /// 与 HMCL 同构：`curseforge:<projectID>:<fileID>` / `modrinth:<sha512>:<path>`，
    /// 两者都没有时退回路径。
    #[must_use]
    pub fn key(&self) -> String {
        if let (Some(project), Some(file)) = (self.project_id, self.file_id) {
            return format!("curseforge:{project}:{file}");
        }
        if let (Some(sha512), Some(path)) = (self.sha512(), self.path.as_deref()) {
            return format!("modrinth:{sha512}:{path}");
        }
        self.path.clone().unwrap_or_default()
    }

    /// 这条目是否有远端下载来源（决定导入时能否自己补齐）。
    #[must_use]
    pub fn is_remote_reachable(&self) -> bool {
        if self.downloads.iter().any(|url| !url.trim().is_empty()) {
            return true;
        }
        self.project_id.is_some() && self.file_id.is_some()
    }
}

/// 覆盖目录里的一个文件（三方合并的 `base` 侧：上次整合包给的哈希）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideFile {
    /// 相对该覆盖目录的路径（等价于实例内相对路径，例如 `mods/jei.jar`）。
    pub path: String,
    /// 内容 SHA-1（读包时现算）。
    pub sha1: String,
    /// 字节数。
    pub size: u64,
}

/// 一个覆盖目录（`overrides` / `client-overrides` / `server-overrides` / MultiMC 的 `.minecraft`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackOverrides {
    /// 包内目录前缀（`/` 分隔，无尾斜杠）。
    pub dir: String,
    /// 目标侧。
    pub side: PackSide,
    /// 目录内的文件清单（按路径排序，保证同一份包两次读出的模型相等）。
    pub files: Vec<OverrideFile>,
}

impl PackOverrides {
    /// 建一个空目录清单。
    #[must_use]
    pub fn new(dir: impl Into<String>, side: PackSide) -> Self {
        Self {
            dir: dir.into(),
            side,
            files: Vec::new(),
        }
    }

    /// 按实例内相对路径查一条。
    #[must_use]
    pub fn find(&self, path: &str) -> Option<&OverrideFile> {
        self.files.iter().find(|file| file.path == path)
    }
}

/// 把未知字段并回 JSON 对象：**已知字段优先**，extra 里的同名键丢弃（不覆盖模型的判断）。
pub fn merge_unknown(target: &mut Map<String, Value>, extra: &Map<String, Value>) {
    for (key, value) in extra {
        if !target.contains_key(key) {
            target.insert(key.clone(), value.clone());
        }
    }
}
