//! 导出结果：**三分类 + 逐条原因**，给 CLI 的 `--json` 与 M113 的页面直接消费。
//!
//! # 为什么每条都要有原因
//!
//! 用户要的是「不认识的 mod 不能丢、不能编」。光说"导出了 120 个文件"会掩盖真正的信息：
//! 其中多少条**能证明**可被远程替代、多少条**证明不了所以内联**、多少条**干脆没进包**。
//! 所以 [`ExportFile`] 的 `class` 与 `reason` 是两个独立维度，且 `reason` 决定 `class`
//! （[`FileReason::class`] 是唯一映射处，写死在这里，别在别处再判一遍）。
//!
//! # 「允许的差异」也进报告
//!
//! [`ExportReport::notes`] 收的是**格式层面表达不了**的东西（CF 的 `author`、mrpack 的
//! `summary`、CF 表达不了禁用位……）。它们是已知损失，必须显式列出，不能让用户以为"往返无损"。

use std::path::PathBuf;

use serde::{Serialize, Serializer};

/// 一条文件在导出结果里的归类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileClass {
    /// 写进清单 `files[]`（靠远端直链/项目 id 代替本地字节）。
    Remote,
    /// 写进 `overrides/`（原字节照搬）。
    Inline,
    /// 没进包（读不了 / 符号链接 / 路径不安全）。
    Skipped,
}

impl FileClass {
    /// 稳定字符串 id（进 `--json` 与日志，**不要改名**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Remote => "remote",
            Self::Inline => "inline",
            Self::Skipped => "skipped",
        }
    }
}

impl Serialize for FileClass {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// 一条文件为什么归到那个类里。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileReason {
    /// 远程引用：Modrinth SHA-1 查到了版本文件。
    ModrinthSha1,
    /// 远程引用：CurseForge 指纹查到了项目/文件。
    CurseForgeFingerprint,
    /// 内联：两个目录都不认识它。
    NoRemoteMatch,
    /// 内联：CF 认识，但目标格式（mrpack）用不上——CF 命中不含 `sha512`。
    CurseOnlyMatch,
    /// 内联：Modrinth 认识，但目标格式（CF 清单）只认 `projectID`/`fileID`。
    ModrinthOnlyMatch,
    /// 内联：目录给了命中但缺 `hashes.sha512`（mrpack 的硬要求）。
    MissingSha512,
    /// 内联：目录给了命中但缺 `projectID`/`fileID`（CF 清单的硬要求）。
    MissingCurseIds,
    /// 内联：文件是禁用的（`.disabled`），而目标格式表达不了禁用位（CF）。
    DisabledNotExpressible,
    /// 跳过：符号链接。
    SymbolicLink,
    /// 跳过：文件名过不了包内路径规整（`:` / `\` / `..` / 绝对路径…）。
    UnsafePath,
    /// 跳过：读盘或算摘要失败。
    Unreadable,
}

impl FileReason {
    /// 稳定字符串 id（进 `--json` 与日志，**不要改名**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModrinthSha1 => "modrinth_sha1",
            Self::CurseForgeFingerprint => "curseforge_fingerprint",
            Self::NoRemoteMatch => "no_remote_match",
            Self::CurseOnlyMatch => "curse_only_match",
            Self::ModrinthOnlyMatch => "modrinth_only_match",
            Self::MissingSha512 => "missing_sha512",
            Self::MissingCurseIds => "missing_curse_ids",
            Self::DisabledNotExpressible => "disabled_not_expressible",
            Self::SymbolicLink => "symbolic_link",
            Self::UnsafePath => "unsafe_path",
            Self::Unreadable => "unreadable",
        }
    }

    /// 人读说明（中文，进报告与日志；UI 文案后续按 key 本地化）。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ModrinthSha1 => "Modrinth 命中（SHA-1）",
            Self::CurseForgeFingerprint => "CurseForge 命中（文件指纹）",
            Self::NoRemoteMatch => "两个目录都不认识，已内联",
            Self::CurseOnlyMatch => "只有 CF 认识，目标格式用不上，已内联",
            Self::ModrinthOnlyMatch => "只有 Modrinth 认识，CF 清单用不上，已内联",
            Self::MissingSha512 => "命中缺 sha512，mrpack 写不出，已内联",
            Self::MissingCurseIds => "命中缺 projectID/fileID，CF 清单写不出，已内联",
            Self::DisabledNotExpressible => "禁用文件 CF 表达不了，已内联（保留 .disabled 后缀）",
            Self::SymbolicLink => "符号链接，已跳过",
            Self::UnsafePath => "路径过不了包内规整，已跳过",
            Self::Unreadable => "读不出来，已跳过",
        }
    }

    /// 这个原因属于哪一类（**唯一映射处**）。
    #[must_use]
    pub const fn class(self) -> FileClass {
        match self {
            Self::ModrinthSha1 | Self::CurseForgeFingerprint => FileClass::Remote,
            Self::NoRemoteMatch
            | Self::CurseOnlyMatch
            | Self::ModrinthOnlyMatch
            | Self::MissingSha512
            | Self::MissingCurseIds
            | Self::DisabledNotExpressible => FileClass::Inline,
            Self::SymbolicLink | Self::UnsafePath | Self::Unreadable => FileClass::Skipped,
        }
    }
}

impl Serialize for FileReason {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// 一条文件的导出结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportFile {
    /// 实例内相对路径（含 `.disabled` 后缀，与磁盘上一致）。
    pub path: String,
    /// 归类。
    pub class: FileClass,
    /// 原因。
    pub reason: FileReason,
    /// 字节数（跳过项记磁盘上的大小，读不到就是 0）。
    pub bytes: u64,
    /// 远程引用给的直链（内联/跳过为空）。
    pub downloads: Vec<String>,
    /// CurseForge `projectID`（远程引用到 CF 时才有）。
    pub project_id: Option<i64>,
    /// CurseForge `fileID`。
    pub file_id: Option<i64>,
    /// 是否禁用态（`.disabled`）。
    pub disabled: bool,
}

/// 一次导出的结构化结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportReport {
    /// 目标格式 id（`modrinth` / `curseforge`）。
    pub format: &'static str,
    /// 目标格式人读名。
    pub label: &'static str,
    /// 产出的包路径。
    pub path: PathBuf,
    /// 包的字节数（写完之后量的，不是估算）。
    pub bytes: u64,
    /// 包名（写进清单的 `name`）。
    pub name: String,
    /// 包版本（`versionId` / `version`）。
    pub version: Option<String>,
    /// 游戏版本（来自实例快照）。
    pub minecraft: Option<String>,
    /// 加载器（`fabric-loader 0.15.11` 这种人读串，来自实例快照）。
    pub loaders: Vec<String>,
    /// 逐文件结果（按路径排序）。
    pub files: Vec<ExportFile>,
    /// 已知的表达不了的差异 / 值得用户知道的取舍。
    pub notes: Vec<String>,
}

impl ExportReport {
    /// 远程引用的条数。
    #[must_use]
    pub fn remote_count(&self) -> usize {
        self.count(FileClass::Remote)
    }

    /// 内联的条数。
    #[must_use]
    pub fn inline_count(&self) -> usize {
        self.count(FileClass::Inline)
    }

    /// 跳过的条数。
    #[must_use]
    pub fn skipped_count(&self) -> usize {
        self.count(FileClass::Skipped)
    }

    fn count(&self, class: FileClass) -> usize {
        self.files.iter().filter(|file| file.class == class).count()
    }

    /// 有没有文件没进包（`false` = 包内容完整，只是有些走了内联）。
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.skipped_count() == 0
    }

    /// 内联文件（`overrides/` 里的那些）。
    pub fn inline_files(&self) -> impl Iterator<Item = &ExportFile> {
        self.files
            .iter()
            .filter(|file| file.class == FileClass::Inline)
    }

    /// 不可导出的项（逐条给 `path` + `reason` 用）。
    pub fn skipped_files(&self) -> impl Iterator<Item = &ExportFile> {
        self.files
            .iter()
            .filter(|file| file.class == FileClass::Skipped)
    }

    /// 一句话摘要（日志/CLI 一行输出用）。
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{}：{} 个远程引用 / {} 个内联 / {} 个跳过，{} 字节",
            self.label,
            self.remote_count(),
            self.inline_count(),
            self.skipped_count(),
            self.bytes
        )
    }
}
