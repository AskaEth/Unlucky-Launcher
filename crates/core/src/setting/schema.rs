//! `$schema` 与 `schemaVersion`：版本标识、8 态兼容判定、三档访问权。
//!
//! 出处：`DevDocs/analyse/07-settings-packaging-update.md` §1.5（`JsonSchema.check` 8 态 +
//! `SettingFileAccess` 三档），`DevDocs/PlanDoc/04-data-and-config.md` §3。
//!
//! 我们抄的是「一个 `$schema` 字段决定可读/可写/是否保留原 schema」，不抄 HMCL 的
//! `major.minor.patch` 三级分支——ULL 的迁移链按 **major** 走：`1.0.0 → 2.0.0 → 3.0.0`。

use std::fmt;

use serde_json::Value;

/// `$schema` 键名。
pub const SCHEMA_KEY: &str = "$schema";
/// `schemaVersion` 键名（整数镜像 = major）。
pub const SCHEMA_VERSION_KEY: &str = "schemaVersion";
/// schema URL 前缀（逻辑标识，暂不对外发布）。
pub const SCHEMA_URL_PREFIX: &str = "https://schemas.unlucky-launcher.dev/ull/";

/// 配置文件的语义版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct SchemaVersion {
    /// 主版本（迁移链单位）。
    pub major: u32,
    /// 次版本（新版本文件只读）。
    pub minor: u32,
    /// 修订号。
    pub patch: u32,
}

impl SchemaVersion {
    /// 构造。
    #[must_use]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// 解析 `1.0.0` 形式。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self::new(major, minor, patch))
    }

    /// 下一个主版本（迁移链的一步）。
    #[must_use]
    pub const fn next_major(&self) -> Self {
        Self::new(self.major + 1, 0, 0)
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// 配置文件种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaId(&'static str);

impl SchemaId {
    /// `config/launcher.json`。
    pub const LAUNCHER: Self = Self("launcher");
    /// `config/profiles.json`。
    pub const PROFILES: Self = Self("profiles");
    /// `config/instances/<id>.json`。
    pub const INSTANCE: Self = Self("instance");

    /// 全部已知 id。
    pub const ALL: [Self; 3] = [Self::LAUNCHER, Self::PROFILES, Self::INSTANCE];

    /// 稳定名。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        self.0
    }

    /// 从字符串解析（只认已知 id）。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.0 == text)
    }
}

/// `$schema` 的完整标识：种类 + 版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaRef {
    /// 种类。
    pub id: SchemaId,
    /// 版本。
    pub version: SchemaVersion,
}

impl SchemaRef {
    /// 构造。
    #[must_use]
    pub const fn new(id: SchemaId, version: SchemaVersion) -> Self {
        Self { id, version }
    }

    /// 写进文件的 `$schema` 字符串。
    #[must_use]
    pub fn url(&self) -> String {
        format!(
            "{SCHEMA_URL_PREFIX}{}/{}.{}.{}",
            self.id.as_str(),
            self.version.major,
            self.version.minor,
            self.version.patch
        )
    }

    /// 反解 `$schema` 字符串。
    #[must_use]
    pub fn parse_url(text: &str) -> Option<Self> {
        let tail = text.strip_prefix(SCHEMA_URL_PREFIX)?;
        let (id, version) = tail.split_once('/')?;
        Some(Self::new(
            SchemaId::parse(id)?,
            SchemaVersion::parse(version)?,
        ))
    }
}

/// 文件与本实现期望 schema 的兼容状态（8 态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaCompat {
    /// 同版本或更旧的版本：可读可写，正常升级。
    ReadWrite,
    /// 同 minor、更高 patch：可读可写，但保留原 `$schema`。
    ReadWritePreserveSchema,
    /// 同 major、更高 minor：可读不可写（新版写的文件），保留原 `$schema`。
    ReadOnlyPreserveSchema,
    /// 文件没有 `$schema`（首建 / 遗留格式）。
    Missing,
    /// `$schema` 不是我们能解析的 URL。
    ///
    /// 手写配置最容易踩的一格：前缀写错（`https://unlucky-launcher.local/schema/...`）后整份
    /// 文件的价值都不该被丢掉，所以 M40 起按**只读**处理——值照读、不静默回落默认值，
    /// 同时由 [`super::document::LoadProblem::UnknownSchema`] 明确告警。
    Invalid,
    /// 文件连 JSON 都解析不了（由加载方给出）。
    Unparseable,
    /// `$schema` 指向别的配置文件种类。
    UnexpectedId,
    /// 文件 major 高于本实现——不认识，绝不能覆盖。
    UnsupportedMajor,
}

impl SchemaCompat {
    /// 判定文件里的 `$schema`（缺失时由调用方给 [`Self::Missing`]）。
    #[must_use]
    pub fn check(value: &Value, expected: SchemaRef) -> Self {
        match value.get(SCHEMA_KEY).and_then(Value::as_str) {
            None => Self::Missing,
            Some(url) => Self::check_url(url, expected),
        }
    }

    /// 判定 `$schema` 字符串。
    #[must_use]
    pub fn check_url(url: &str, expected: SchemaRef) -> Self {
        let Some(found) = SchemaRef::parse_url(url) else {
            return Self::Invalid;
        };
        if found.id != expected.id {
            return Self::UnexpectedId;
        }
        if found.version.major > expected.version.major {
            return Self::UnsupportedMajor;
        }
        if found.version.major < expected.version.major {
            // 旧文件：正常升级（迁移链负责搬运语义）。
            return Self::ReadWrite;
        }
        if found.version.minor > expected.version.minor {
            return Self::ReadOnlyPreserveSchema;
        }
        if found.version.minor == expected.version.minor
            && found.version.patch > expected.version.patch
        {
            return Self::ReadWritePreserveSchema;
        }
        Self::ReadWrite
    }

    /// 稳定名（日志 / 收据）。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ReadWrite => "read_write",
            Self::ReadWritePreserveSchema => "read_write_preserve_schema",
            Self::ReadOnlyPreserveSchema => "read_only_preserve_schema",
            Self::Missing => "missing",
            Self::Invalid => "invalid",
            Self::Unparseable => "unparseable",
            Self::UnexpectedId => "unexpected_id",
            Self::UnsupportedMajor => "unsupported_major",
        }
    }

    /// 是否可读。
    #[must_use]
    pub const fn readable(&self) -> bool {
        matches!(
            self,
            Self::ReadWrite
                | Self::ReadWritePreserveSchema
                | Self::ReadOnlyPreserveSchema
                | Self::Missing
                | Self::Invalid
        )
    }

    /// 保存时是否沿用文件里原有的 `$schema` 字符串（不降级覆盖新版本标记）。
    #[must_use]
    pub const fn preserve_schema(&self) -> bool {
        matches!(
            self,
            Self::ReadWritePreserveSchema | Self::ReadOnlyPreserveSchema
        )
    }

    /// 映射到运行时三档访问权。
    ///
    /// [`Self::Invalid`] 走**只读**：认不出的 `$schema` 前缀不该让整份配置失效（值照读，
    /// 由 [`super::document::LoadProblem::UnknownSchema`] 告警），但也不该被我们悄悄改写。
    #[must_use]
    pub const fn access(&self) -> FileAccess {
        match self {
            Self::ReadWrite | Self::ReadWritePreserveSchema | Self::Missing => {
                FileAccess::ReadWrite
            }
            Self::ReadOnlyPreserveSchema | Self::Invalid => FileAccess::ReadOnly,
            Self::Unparseable | Self::UnexpectedId | Self::UnsupportedMajor => {
                FileAccess::Unreadable
            }
        }
    }
}

/// 运行时访问权三档（锁文件/权限异常时降级运行而非崩溃）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAccess {
    /// 可读可写。
    ReadWrite,
    /// 可读不可写（新版文件 / 权限受限）。
    ReadOnly,
    /// 读不了：值不可信，全程不写（防止覆盖新版本配置）。
    Unreadable,
}

impl FileAccess {
    /// 能否使用文件里的值。
    #[must_use]
    pub const fn can_read(&self) -> bool {
        !matches!(self, Self::Unreadable)
    }

    /// 能否保存。
    #[must_use]
    pub const fn can_save(&self) -> bool {
        matches!(self, Self::ReadWrite)
    }

    /// UI 是否该禁用编辑。
    #[must_use]
    pub const fn blocks_editing(&self) -> bool {
        !self.can_save()
    }

    /// 稳定名。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ReadWrite => "read_write",
            Self::ReadOnly => "read_only",
            Self::Unreadable => "unreadable",
        }
    }
}
