//! 导入链路的**致命**失败分类（能容忍的缺文件不在这里，它们在
//! [`super::report::MissingFile`] 里逐条记账）。
//!
//! # 为什么要跟 `MissingFile` 分开
//!
//! D10 §2 的失败分级只有一条判据：**这个错误是不是"某个文件搞不到"**。
//!
//! | 情形 | 处置 | 落在哪 |
//! |------|------|--------|
//! | 某个远端文件下载失败 / 补全不出地址 / 没有网络 | 容忍，实例照装，状态降级 `Partial` | [`super::report::MissingFile`] |
//! | 路径穿越 / 符号链接 / 超限额 / 清单坏 / 本地 IO / 摘要不符 | **整包回滚**，目标目录零残留 | 本模块 |
//!
//! 因此 [`ImportError`] 只描述**必须回滚**的失败；能容忍的东西一旦也塞进来，调用方就再也
//! 分不清"该重试"和"该报恶意包"了。
//!
//! # 稳定码
//!
//! 包相关六类**直接沿用 M110 的码**（`pack_unsupported` / `pack_metadata` /
//! `pack_traversal` / `pack_symlink` / `pack_limits` / `pack_io`），不另起一套同义词：
//! 同一个事实在 CLI、日志、事件流里只能有一个字符串。导入独有的三类加 `import_` 前缀。

use std::fmt;

use super::super::pack::{PackError, PackErrorKind};
use super::fetch::{FetchError, FetchErrorKind};

/// 导入失败大类（稳定码进 `--json` 的 `error.code` 与日志，**不要改名**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportErrorKind {
    /// 透传 M110 的六类包错误（稳定码沿用 `pack_*`）。
    Pack(PackErrorKind),
    /// 下载到的内容与清单声明的摘要/大小**不符**（`import_integrity`）。
    Integrity,
    /// 远端补全/下载在**致命**情形下失败（`import_fetch`；本地 IO 归 `pack_io`）。
    Fetch,
    /// 清单里的加载器信息不足以组出安装计划（`import_loader`）。
    Loader,
    /// 实例登记失败（`import_instance`）。
    Instance,
}

impl ImportErrorKind {
    /// 稳定字符串 id（进 `--json` 与日志）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pack(kind) => kind.as_str(),
            Self::Integrity => "import_integrity",
            Self::Fetch => "import_fetch",
            Self::Loader => "import_loader",
            Self::Instance => "import_instance",
        }
    }

    /// 人读大类名（报告表格用）。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pack(kind) => pack_label(kind),
            Self::Integrity => "内容与清单不符",
            Self::Fetch => "远端补全失败",
            Self::Loader => "加载器信息不足",
            Self::Instance => "实例登记失败",
        }
    }
}

/// M110 六个大类的人读名（与 `PackErrorKind` 的稳定码一一对应，不另起一套枚举）。
const fn pack_label(kind: PackErrorKind) -> &'static str {
    match kind {
        PackErrorKind::Unsupported => "格式不支持",
        PackErrorKind::Metadata => "清单损坏",
        PackErrorKind::Traversal => "路径穿越",
        PackErrorKind::Symlink => "符号链接条目",
        PackErrorKind::Limits => "超出安全限额",
        PackErrorKind::Io => "本地 IO 失败",
    }
}

/// 一次**必须回滚**的导入失败。
#[derive(Debug, Clone)]
pub struct ImportError {
    kind: ImportErrorKind,
    message: String,
    hint: Option<String>,
}

impl ImportError {
    /// 构造。
    #[must_use]
    pub fn new(kind: ImportErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            hint: None,
        }
    }

    /// 内容与清单声明不符。
    #[must_use]
    pub fn integrity(message: impl Into<String>) -> Self {
        Self::new(ImportErrorKind::Integrity, message)
    }

    /// 远端补全/下载的致命失败。
    #[must_use]
    pub fn fetch(message: impl Into<String>) -> Self {
        Self::new(ImportErrorKind::Fetch, message)
    }

    /// 加载器信息不足。
    #[must_use]
    pub fn loader(message: impl Into<String>) -> Self {
        Self::new(ImportErrorKind::Loader, message)
    }

    /// 实例登记失败。
    #[must_use]
    pub fn instance(message: impl Into<String>) -> Self {
        Self::new(ImportErrorKind::Instance, message)
    }

    /// 补一句提示。
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// 大类。
    #[must_use]
    pub const fn kind(&self) -> ImportErrorKind {
        self.kind
    }

    /// 稳定码。
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.kind.as_str()
    }

    /// 人读说明。
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 下一步提示。
    #[must_use]
    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ImportError {}

impl From<PackError> for ImportError {
    fn from(error: PackError) -> Self {
        Self {
            kind: ImportErrorKind::Pack(error.kind()),
            message: error.message().to_string(),
            hint: error.hint().map(str::to_string),
        }
    }
}

impl From<FetchError> for ImportError {
    fn from(error: FetchError) -> Self {
        // 分级判据只有一处：`Integrity` 与 `Io` 必须回滚，其余三类是"搞不到"。
        let kind = match error.kind() {
            FetchErrorKind::Integrity => ImportErrorKind::Integrity,
            FetchErrorKind::Io => ImportErrorKind::Pack(PackErrorKind::Io),
            FetchErrorKind::NotFound
            | FetchErrorKind::Network
            | FetchErrorKind::Offline
            | FetchErrorKind::Unsupported => ImportErrorKind::Fetch,
        };
        Self {
            kind,
            message: error.message().to_string(),
            hint: None,
        }
    }
}

impl serde::Serialize for ImportError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("code", self.code())?;
        map.serialize_entry("kind", self.kind.label())?;
        map.serialize_entry("message", &self.message)?;
        if let Some(hint) = &self.hint {
            map.serialize_entry("hint", hint)?;
        }
        map.end()
    }
}
