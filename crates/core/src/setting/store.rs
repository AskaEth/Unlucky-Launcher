//! 一个配置文件 = 一个 [`SettingStore`]：加载降级 → 编辑 → 原子保存 → 坏文件备份。
//!
//! 出处：`DevDocs/analyse/07-settings-packaging-update.md` §1.5-§1.6（8 态 schema / 三档访问权 /
//! 「文件损坏 → 默认值 + 下次保存前备份 `xxx.json.1`」），`DevDocs/PlanDoc/04-data-and-config.md` §3。
//!
//! 加载永不 panic：读不了/版本不认识就降级成只读或不可读，让启动器带着默认值继续跑
//! （analyse/07 §1「锁文件/权限异常时降级运行而非崩溃」）。

use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::platform::atomic;

use super::catalog::SettingSchema;
use super::document::{LoadProblem, SettingDocument};
use super::error::{SettingError, SettingResult};
use super::migrate::{MigrationChain, MigrationReport};
use super::schema::{FileAccess, SchemaCompat, SCHEMA_KEY, SCHEMA_VERSION_KEY};

/// 保存结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    /// 已原子落盘。
    Saved(PathBuf),
    /// 没有写盘，原因见 [`SkipReason`]。
    Skipped(SkipReason),
}

impl SaveOutcome {
    /// 是否真的写了盘。
    #[must_use]
    pub fn wrote(&self) -> bool {
        matches!(self, Self::Saved(_))
    }
}

/// 跳过保存的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// 没有脏字段。
    NotDirty,
    /// 文件只读（`READ_ONLY`）。
    ReadOnly,
    /// 文件不可信（`UNREADABLE`），全程不写。
    Unreadable,
}

impl SkipReason {
    /// 稳定名。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NotDirty => "not_dirty",
            Self::ReadOnly => "read_only",
            Self::Unreadable => "unreadable",
        }
    }
}

/// 单个配置文件的内存 + 磁盘视图。
#[derive(Debug, Clone)]
pub struct SettingStore {
    path: PathBuf,
    doc: SettingDocument,
    access: FileAccess,
    compat: SchemaCompat,
    problems: Vec<LoadProblem>,
    backup_on_next_save: bool,
}

impl SettingStore {
    /// 加载（或首建）一个配置文件；永不失败，状态从 [`Self::access`] 读。
    #[must_use]
    pub fn load(path: impl Into<PathBuf>, schema: SettingSchema) -> Self {
        let mut store = Self {
            path: path.into(),
            doc: SettingDocument::new(schema.clone()),
            access: FileAccess::ReadWrite,
            compat: SchemaCompat::Missing,
            problems: Vec::new(),
            backup_on_next_save: false,
        };
        store.reload_inner();
        store
    }

    /// 文件路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 访问权三档。
    #[must_use]
    pub const fn access(&self) -> FileAccess {
        self.access
    }

    /// `$schema` 兼容状态。
    #[must_use]
    pub const fn compat(&self) -> SchemaCompat {
        self.compat
    }

    /// 加载时的问题清单（未知键 / 坏值 / 版本镜像不一致…）。
    #[must_use]
    pub fn problems(&self) -> &[LoadProblem] {
        &self.problems
    }

    /// 只读视图。
    #[must_use]
    pub const fn doc(&self) -> &SettingDocument {
        &self.doc
    }

    /// 可变视图（迁移 / 导入用；正常改配置请走 [`Self::set`]，那里带访问权检查）。
    pub const fn doc_mut(&mut self) -> &mut SettingDocument {
        &mut self.doc
    }

    /// 生效值：文件值 → 默认值。
    #[must_use]
    pub fn effective(&self, key: &str) -> Option<&Value> {
        self.doc.effective(key)
    }

    /// 是否有未落盘改动。
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.doc.is_dirty()
    }

    /// 改一个键（只读/不可读时直接拒绝，不静默丢改动）。
    ///
    /// # Errors
    /// 文件只读 → [`SettingError::ReadOnly`]；不可读 → [`SettingError::Unreadable`]；
    /// 键值不合法 → [`SettingError::InvalidValue`] / [`SettingError::UnknownKey`]。
    pub fn set(&mut self, key: &str, value: Value) -> SettingResult<()> {
        match self.access {
            FileAccess::ReadWrite => self.doc.set(key, value),
            FileAccess::ReadOnly => Err(SettingError::ReadOnly {
                path: self.path.clone(),
            }),
            FileAccess::Unreadable => Err(SettingError::Unreadable {
                path: self.path.clone(),
                compat: self.compat,
            }),
        }
    }

    /// 保存（只写脏字段、原子落盘；不满足条件时返回 [`SaveOutcome::Skipped`]）。
    ///
    /// # Errors
    /// 序列化 / 备份 / 写盘失败时返回 [`SettingError`]。
    pub fn save(&mut self) -> SettingResult<SaveOutcome> {
        if !self.access.can_save() {
            let reason = if matches!(self.access, FileAccess::ReadOnly) {
                SkipReason::ReadOnly
            } else {
                SkipReason::Unreadable
            };
            return Ok(SaveOutcome::Skipped(reason));
        }
        if !self.doc.is_dirty() {
            return Ok(SaveOutcome::Skipped(SkipReason::NotDirty));
        }
        if self.backup_on_next_save {
            self.backup_existing()?;
        }
        self.write_document()?;
        Ok(SaveOutcome::Saved(self.path.clone()))
    }

    /// 显式强写（用户在 UI 上确认「覆盖只读/损坏文件」后调用）：先备份，再覆盖，恢复可写。
    ///
    /// # Errors
    /// 备份或写盘失败时返回 [`SettingError`]。
    pub fn save_forced(&mut self) -> SettingResult<SaveOutcome> {
        self.backup_existing()?;
        self.write_document()?;
        self.access = FileAccess::ReadWrite;
        self.backup_on_next_save = false;
        Ok(SaveOutcome::Saved(self.path.clone()))
    }

    /// 重新从磁盘加载（丢弃内存改动）。
    pub fn reload(&mut self) {
        self.reload_inner();
    }

    /// 按迁移链把文件从旧版本推进到 schema 版本（补丁后再落盘由调用方决定）。
    ///
    /// # Errors
    /// 链不完整或某步迁移失败时返回 [`SettingError`]。
    pub fn apply_chain(
        &mut self,
        chain: &MigrationChain,
    ) -> SettingResult<Option<MigrationReport>> {
        let schema = self.doc.schema().clone();
        if !self.access.can_read() || self.doc.version() == schema.version() {
            return Ok(None);
        }
        let Value::Object(mut root) = self.doc.to_value() else {
            return Ok(None);
        };
        let report = chain.migrate(self.doc.version(), &mut root)?;
        root.insert(
            SCHEMA_KEY.to_owned(),
            Value::String(schema.schema_ref().url()),
        );
        root.insert(
            SCHEMA_VERSION_KEY.to_owned(),
            Value::from(schema.version().major),
        );

        let (mut document, problems) = SettingDocument::load(&schema, &Value::Object(root));
        document.set_version(schema.version());
        document.mark_all_dirty();
        self.doc = document;
        self.problems = problems;
        Ok(Some(report))
    }

    /// 下一个空位的备份路径（`launcher.json.1`、`.2`…）。
    #[must_use]
    pub fn next_backup_path(&self) -> PathBuf {
        for index in 1..=999u32 {
            let candidate = PathBuf::from(format!("{}.{index}", self.path.display()));
            if !candidate.exists() {
                return candidate;
            }
        }
        PathBuf::from(format!("{}.1", self.path.display()))
    }

    fn reload_inner(&mut self) {
        let schema = self.doc.schema().clone();
        self.doc = SettingDocument::new(schema.clone());
        self.problems.clear();
        self.access = FileAccess::ReadWrite;
        self.compat = SchemaCompat::Missing;
        self.backup_on_next_save = false;

        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            // 首建：可写 + 全默认值。
            Err(err) if err.kind() == io::ErrorKind::NotFound => return,
            // 读不了（权限/被占用）：值不可信，全程不写。
            Err(_) => {
                self.access = FileAccess::Unreadable;
                self.compat = SchemaCompat::Unparseable;
                return;
            }
        };

        match serde_json::from_slice::<Value>(&bytes) {
            // 文件损坏：默认值 + 下次保存前先备份（不把坏文件当未知字段回写）。
            Err(_) => {
                self.compat = SchemaCompat::Unparseable;
                self.backup_on_next_save = true;
            }
            Ok(value) => {
                let compat = SchemaCompat::check(&value, schema.schema_ref());
                self.compat = compat;
                self.access = compat.access();
                if !self.access.can_read() {
                    return;
                }
                let (document, problems) = SettingDocument::load(&schema, &value);
                self.doc = document;
                self.problems = problems;
            }
        }
    }

    fn write_document(&mut self) -> SettingResult<()> {
        let value = self.doc.to_value();
        let bytes = serde_json::to_vec_pretty(&value).map_err(|source| SettingError::Json {
            path: self.path.clone(),
            source,
        })?;
        atomic::write_atomic(&self.path, &bytes)?;
        self.doc.mark_clean();
        Ok(())
    }

    fn backup_existing(&mut self) -> SettingResult<Option<PathBuf>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let backup = self.next_backup_path();
        std::fs::rename(&self.path, &backup).map_err(|source| SettingError::Io {
            path: self.path.clone(),
            source,
        })?;
        Ok(Some(backup))
    }
}
