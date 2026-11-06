//! 导出时的覆盖内容来源：[`PackPayload`]。
//!
//! # 两种来源
//!
//! | 构造 | 用在哪 |
//! |------|--------|
//! | [`PackPayload::from_root`] | 同格式往返：包内 `overrides/mods/x.jar` 就是 `<root>/overrides/mods/x.jar` |
//! | [`PackPayload::with_sources`] | 跨格式 / 从实例导出：包内路径由模型决定，源文件在实例目录的任意位置 |
//!
//! 第二种是 M112 需要的形状：实例里的 `mods/x.jar` 要落到包里的 `overrides/mods/x.jar`
//! （CF 与 mrpack 都是这个前缀），路径**不能靠拼**（拼接只能表达"目录同名"）。所以来源表
//! 以**包内条目名**为键：`{"overrides/mods/x.jar": <实例>/mods/x.jar}`。
//!
//! 查不到映射时退回 `root/dir/path`；两者都没有就报 `pack_unsupported`（不写空内容、不猜）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 导出时的覆盖内容来源。
#[derive(Debug, Clone, Copy)]
pub struct PackPayload<'a> {
    root: Option<&'a Path>,
    sources: Option<&'a BTreeMap<String, PathBuf>>,
}

impl<'a> PackPayload<'a> {
    /// 只写清单，不搬覆盖内容。
    #[must_use]
    pub const fn metadata_only() -> Self {
        Self {
            root: None,
            sources: None,
        }
    }

    /// 覆盖内容从 `root` 取（`<root>/<dir>/<path>`，同格式往返用）。
    #[must_use]
    pub const fn from_root(root: &'a Path) -> Self {
        Self {
            root: Some(root),
            sources: None,
        }
    }

    /// 覆盖内容按**包内条目名**查表（`overrides/mods/x.jar` → 实例里的绝对/相对路径）。
    ///
    /// `root` 仍然要给：映射没覆盖到的条目退回 `<root>/<dir>/<path>`，
    /// 这样"大部分文件原位、少数文件改名"的混合场景不必手写全表。
    #[must_use]
    pub const fn with_sources(root: &'a Path, sources: &'a BTreeMap<String, PathBuf>) -> Self {
        Self {
            root: Some(root),
            sources: Some(sources),
        }
    }

    /// 来源根目录。
    #[must_use]
    pub const fn root(&self) -> Option<&'a Path> {
        self.root
    }

    /// 包内条目名 → 源文件（没有来源表或表里没这条时返回 `None`）。
    #[must_use]
    pub fn source_for(&self, entry_name: &str) -> Option<&'a Path> {
        self.sources
            .and_then(|sources| sources.get(entry_name))
            .map(PathBuf::as_path)
    }
}
