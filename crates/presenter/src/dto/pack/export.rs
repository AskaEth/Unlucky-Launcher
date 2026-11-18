//! 导出侧 DTO：三分类 + 逐条原因 + **预检**（不写盘）。
//!
//! `ExportReasonView::class()` 是「原因 → 归类」的**唯一**映射处（与 core 同一条判据）；
//! 预检（[`ExportPreviewView`]）与写盘结果用同一套计数方法，于是「将内联 N / 无法导出 M」
//! 与最终报告的 N/M 同源。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::dto::InstanceId;

use super::format::PackFormatView;

/// 导出时一条文件的归类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportClassView {
    /// 写进清单 `files[]`（靠远端引用代替本地字节）。
    Remote,
    /// 写进 `overrides/`（原字节照搬）。
    Inline,
    /// 没进包（读不了 / 符号链接 / 路径不安全）。
    Skipped,
}

impl ExportClassView {
    /// 从 core 的 id 反解。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "remote" => Some(Self::Remote),
            "inline" => Some(Self::Inline),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

/// 一条文件为什么归到那个类里（与 core 的 `FileReason` 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportReasonView {
    /// 远程引用：Modrinth SHA-1 命中。
    ModrinthSha1,
    /// 远程引用：CurseForge 指纹命中。
    CurseForgeFingerprint,
    /// 内联：两个目录都不认识它。
    NoRemoteMatch,
    /// 内联：只有 CF 认识，目标格式用不上。
    CurseOnlyMatch,
    /// 内联：只有 Modrinth 认识，CF 清单用不上。
    ModrinthOnlyMatch,
    /// 内联：命中缺 `sha512`。
    MissingSha512,
    /// 内联：命中缺 `projectID`/`fileID`。
    MissingCurseIds,
    /// 内联：禁用文件，目标格式表达不了禁用位。
    DisabledNotExpressible,
    /// 跳过：符号链接。
    SymbolicLink,
    /// 跳过：路径过不了包内规整。
    UnsafePath,
    /// 跳过：读盘或算摘要失败。
    Unreadable,
}

impl ExportReasonView {
    /// 从 core 的 id 反解。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "modrinth_sha1" => Some(Self::ModrinthSha1),
            "curseforge_fingerprint" => Some(Self::CurseForgeFingerprint),
            "no_remote_match" => Some(Self::NoRemoteMatch),
            "curse_only_match" => Some(Self::CurseOnlyMatch),
            "modrinth_only_match" => Some(Self::ModrinthOnlyMatch),
            "missing_sha512" => Some(Self::MissingSha512),
            "missing_curse_ids" => Some(Self::MissingCurseIds),
            "disabled_not_expressible" => Some(Self::DisabledNotExpressible),
            "symbolic_link" => Some(Self::SymbolicLink),
            "unsafe_path" => Some(Self::UnsafePath),
            "unreadable" => Some(Self::Unreadable),
            _ => None,
        }
    }

    /// 这个原因属于哪一类（**唯一映射处**，与 core 的 `FileReason::class` 同一条判据）。
    #[must_use]
    pub const fn class(self) -> ExportClassView {
        match self {
            Self::ModrinthSha1 | Self::CurseForgeFingerprint => ExportClassView::Remote,
            Self::NoRemoteMatch
            | Self::CurseOnlyMatch
            | Self::ModrinthOnlyMatch
            | Self::MissingSha512
            | Self::MissingCurseIds
            | Self::DisabledNotExpressible => ExportClassView::Inline,
            Self::SymbolicLink | Self::UnsafePath | Self::Unreadable => ExportClassView::Skipped,
        }
    }
}

/// 一条文件的导出预检结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportFileView {
    /// 实例内相对路径（含 `.disabled` 后缀）。
    pub path: String,
    /// 归类。
    pub class: ExportClassView,
    /// 原因。
    pub reason: ExportReasonView,
    /// 字节数。
    pub bytes: u64,
    /// 是否禁用态（`.disabled`）。
    pub disabled: bool,
}

/// **预检**：还没有写盘，先把「会怎样」摊开给用户看。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportPreviewView {
    /// 目标格式。
    pub format: PackFormatView,
    /// 源实例。
    pub instance: InstanceId,
    /// 包名。
    pub name: String,
    /// 包版本。
    pub version: String,
    /// 预定的落盘路径（确认之后写这里）。
    pub target: PathBuf,
    /// 逐文件结果（按路径排序）。
    pub files: Vec<ExportFileView>,
    /// 已知的表达不了的差异 / 取舍（core 的 `notes`）。
    pub notes: Vec<String>,
}

impl ExportPreviewView {
    /// 某一类的条数。
    #[must_use]
    pub fn count(&self, class: ExportClassView) -> usize {
        self.files.iter().filter(|file| file.class == class).count()
    }

    /// 远程引用条数。
    #[must_use]
    pub fn remote_count(&self) -> usize {
        self.count(ExportClassView::Remote)
    }

    /// 内联条数（「将内联 N 个文件」的那个 N）。
    #[must_use]
    pub fn inline_count(&self) -> usize {
        self.count(ExportClassView::Inline)
    }

    /// 跳过条数（「无法导出 M 项」的那个 M）。
    #[must_use]
    pub fn skipped_count(&self) -> usize {
        self.count(ExportClassView::Skipped)
    }

    /// 内联文件总字节数。
    #[must_use]
    pub fn inline_bytes(&self) -> u64 {
        self.files
            .iter()
            .filter(|file| file.class == ExportClassView::Inline)
            .map(|file| file.bytes)
            .sum()
    }

    /// 包会完整吗（`false` = 有文件压根没进包，必须单独列给用户）。
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.skipped_count() == 0
    }
}

/// 导出格式选择 + 一次导出请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportRequestView {
    /// 源实例。
    pub instance: InstanceId,
    /// 目标格式（只能是 [`PackFormatView::EXPORTABLE`] 里的两个）。
    pub format: PackFormatView,
    /// 包名（写进清单 `name`）。
    pub name: String,
    /// 包版本（写进清单 `version` / `versionId`）。
    pub version: String,
    /// 落盘路径（`None` = host 按「下载目录 + 包名 + 扩展名」给一个缺省）。
    pub target: Option<PathBuf>,
}

/// 导出完成（写盘之后的账）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportDoneView {
    /// 产出的包路径。
    pub path: PathBuf,
    /// 包的字节数（写完量的，不是估算）。
    pub bytes: u64,
    /// 远程引用条数。
    pub remote: u32,
    /// 内联条数。
    pub inline: u32,
    /// 跳过条数。
    pub skipped: u32,
}

/// 导出侧的宿主状态（`AppState::export` 的全部内容）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExportState {
    /// 预检结果（`None` = 还没预检）。
    pub preview: Option<ExportPreviewView>,
    /// **这次预检是对着哪份请求做的**（确认写盘时原样发回，防「预检 A、写 B」）。
    pub request: Option<ExportRequestView>,
    /// 预检在飞。
    pub pending: bool,
    /// 最近一次写盘的结果。
    pub done: Option<ExportDoneView>,
    /// 失败原因（预检或写盘）。
    pub failed: Option<String>,
}
