//! 打包输入的安全读取层：[`PackArchive`] 把"目录"与"zip"统一成"一组规整过的条目"。
//!
//! # 为什么要有这一层
//!
//! 格式探测器要能对**目录**（MultiMC 实例、已解开的整合包）与**zip**（`.zip` / `.mrpack`）
//! 一视同仁地问"有没有 `modrinth.index.json`"，而安全判据只有一份实现才不会漂。
//!
//! # 安全判据（每条都有恶意样本测试，见 `tests/safety.rs`）
//!
//! | 威胁 | 做法 |
//! |------|------|
//! | 路径穿越（`../x`、`/etc/x`、`C:\x`、`a\..\b`、`file:ads`） | 条目名过 [`normalize_file`]（第一检，平台无关的七条规则），再过 `zip` crate 的 `enclosed_name()`（第二检，**按 Windows 规则**解析盘符/ADS/保留设备名）；任一拒绝即整包拒绝 |
//! | 符号链接条目 | `unix_mode()` 的 `S_IFLNK` 位命中即拒绝（Windows 上无合法用途；解压器会把目标当内容写出来） |
//! | 条目数爆炸 | [`PackLimits::max_entries`]，打开时按中央目录长度先判 |
//! | 解压体积爆炸（zip 炸弹） | [`PackLimits::max_uncompressed_bytes`] 累计 + [`PackLimits::max_entry_bytes`] 单条目；**读取时**再按实际读出的字节数封顶（中央目录里声明的大小可以撒谎） |
//! | 同名条目（`./a` 与 `a` 规整后撞车） | 规整后的名字重复即拒绝 |
//!
//! **两条检查各补一条对方没有的规则**（谁也不能省）：
//!
//! - 第一检管**平台无关**的形状：空名、`\0`、`\`、`:`、前导 `/`、任何 `..` 段。
//!   `enclosed_name()` 在这一点上更松——它允许"深度 0 处的根目录"，也就是 `"/etc/passwd"`
//!   会返回 `Some("/etc/passwd")`，绝对路径必须由我们先干掉。
//! - 第二检管 **Win32 命名空间**：DOS 设备名（`CON` / `NUL` / `AUX` / `COM1..9` / `LPT1..9`）、
//!   结尾的点或空格（`a.`、`a `）、`< > " | ? *` 与控制字符。这些第一检全放行，而它们会让
//!   落盘时撞到设备、或与另一个名字等价。第二检只在 **Windows 宿主**上生效（`zip` 的
//!   `cfg(windows)` 分支），所以 `tests/safety.rs` 里对应的用例也是 `#[cfg(windows)]`。
//!
//! 也就是说：`CON` / `NUL` 这类在 Windows 上**是拦的**（由第二检），非 Windows 宿主上不拦
//! （那里它们只是普通文件名，拦了反而拒掉合法包）。

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use zip::ZipArchive;

use super::error::{PackError, PackResult};
use super::manifest::OverrideFile;
use super::path::normalize_dir;
use crate::download::types::HashAlgo;
use crate::download::verify::{self, Hasher};

pub mod entry;
pub mod limits;
mod open;

pub use entry::PackEntryInfo;
pub use limits::{
    PackLimits, ReadBudget, DEFAULT_MAX_ENTRIES, DEFAULT_MAX_ENTRY_BYTES,
    DEFAULT_MAX_UNCOMPRESSED_BYTES,
};

use limits::read_capped;

/// 输入来源。
pub(super) enum Source {
    /// 目录（MultiMC 实例、已解开的包）。
    Dir,
    /// zip（`.zip` / `.mrpack`）。`RefCell` 让只读方法能取 `&mut ZipArchive`（解压需要）。
    Zip(RefCell<ZipArchive<File>>),
}

/// 一个已校验的整合包输入（目录或 zip）。
pub struct PackArchive {
    root: PathBuf,
    source: Source,
    entries: Vec<PackEntryInfo>,
    index: HashMap<String, usize>,
    limits: PackLimits,
}

impl std::fmt::Debug for PackArchive {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PackArchive")
            .field("root", &self.root)
            .field(
                "source",
                &match self.source {
                    Source::Dir => "dir",
                    Source::Zip(_) => "zip",
                },
            )
            .field("entries", &self.entries.len())
            .finish()
    }
}

impl PackArchive {
    /// 打开一个目录或 zip，按 [`PackLimits::default`] 校验。
    pub fn open_default(path: impl AsRef<Path>) -> PackResult<Self> {
        Self::open(path, PackLimits::default())
    }

    /// 打开一个目录或 zip，按给定限额校验。
    ///
    /// 校验是**全量**的：打开成功就说明整包的条目名都过了两道穿越检查、没有符号链接、
    /// 条数与体积都在限额内。后续读取不再需要重复判断来源是否恶意。
    pub fn open(path: impl AsRef<Path>, limits: PackLimits) -> PackResult<Self> {
        let root = path.as_ref().to_path_buf();
        let metadata = std::fs::symlink_metadata(&root)
            .map_err(|error| PackError::io(format!("打不开整合包 {}：{error}", root.display())))?;
        if metadata.file_type().is_symlink() {
            return Err(PackError::symlink(format!(
                "整合包本身是符号链接：{}",
                root.display()
            )));
        }
        if metadata.is_dir() {
            Self::open_dir(root, limits)
        } else {
            Self::open_zip(root, limits)
        }
    }

    /// 输入路径。
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 是否目录输入。
    #[must_use]
    pub fn is_dir_source(&self) -> bool {
        matches!(self.source, Source::Dir)
    }

    /// 全部条目（目录输入按名字排序，zip 输入保持中央目录顺序）。
    #[must_use]
    pub fn entries(&self) -> &[PackEntryInfo] {
        &self.entries
    }

    /// 按规整名查一条。
    #[must_use]
    pub fn entry(&self, name: &str) -> Option<&PackEntryInfo> {
        self.index.get(name).map(|slot| &self.entries[*slot])
    }

    /// 有没有这个条目（探测器用：只问存不存在，不读内容）。
    #[must_use]
    pub fn has_entry(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    /// 读一个条目；不存在返回 `Ok(None)`。
    pub fn read(&self, name: &str) -> PackResult<Option<Vec<u8>>> {
        let Some(entry) = self.entry(name) else {
            return Ok(None);
        };
        if entry.is_dir {
            return Ok(None);
        }
        match &self.source {
            Source::Dir => {
                let path = self.root.join(&entry.raw);
                let bytes = std::fs::read(&path).map_err(|error| {
                    PackError::io(format!("读 {} 失败：{error}", path.display()))
                })?;
                Ok(Some(bytes))
            }
            Source::Zip(archive) => {
                let mut archive = archive.borrow_mut();
                let mut reader = archive
                    .by_name(&entry.raw)
                    .map_err(|error| PackError::metadata(format!("解压 {name} 失败：{error}")))?;
                let bytes = read_capped(&mut reader, self.limits.max_entry_bytes, None, name)?;
                Ok(Some(bytes))
            }
        }
    }

    /// 按 UTF-8 读文本条目。
    pub fn read_text(&self, name: &str) -> PackResult<Option<String>> {
        let Some(bytes) = self.read(name)? else {
            return Ok(None);
        };
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|error| PackError::metadata(format!("{name} 不是 UTF-8 文本：{error}")))
    }

    /// 读 JSON 条目；不存在返回 `Ok(None)`。
    pub fn read_json(&self, name: &str) -> PackResult<Option<Value>> {
        let Some(text) = self.read_text(name)? else {
            return Ok(None);
        };
        let value = serde_json::from_str(&text)
            .map_err(|error| PackError::metadata(format!("{name} 不是合法 JSON：{error}")))?;
        Ok(Some(value))
    }

    /// 算一个条目的 SHA-1；不存在返回 `Ok(None)`。
    ///
    /// 单条读取只受 [`PackLimits::max_entry_bytes`] 约束；要跨条目累计限额的批量读取
    /// 请用 [`Self::override_files`]，或自己建 [`ReadBudget`] 交给 [`Self::override_files_with`]。
    pub fn sha1(&self, name: &str) -> PackResult<Option<String>> {
        self.sha1_with(name, None)
    }

    /// 算 SHA-1 并从 `budget` 里扣减实际读出的字节（批量读取用）。
    fn sha1_with(&self, name: &str, budget: Option<&ReadBudget>) -> PackResult<Option<String>> {
        let Some(entry) = self.entry(name) else {
            return Ok(None);
        };
        if entry.is_dir {
            return Ok(None);
        }
        match &self.source {
            Source::Dir => {
                let path = self.root.join(&entry.raw);
                let digest = verify::digest_file(&path, HashAlgo::Sha1).map_err(|error| {
                    PackError::io(format!("算 {} 的 SHA-1 失败：{error}", path.display()))
                })?;
                Ok(Some(digest))
            }
            Source::Zip(archive) => {
                let mut archive = archive.borrow_mut();
                let mut reader = archive
                    .by_name(&entry.raw)
                    .map_err(|error| PackError::metadata(format!("解压 {name} 失败：{error}")))?;
                let mut hasher = Hasher::new(HashAlgo::Sha1);
                let mut total = 0_u64;
                let mut buffer = [0_u8; 64 * 1024];
                loop {
                    let read = reader
                        .read(&mut buffer)
                        .map_err(|error| PackError::io(format!("读 {name} 失败：{error}")))?;
                    if read == 0 {
                        break;
                    }
                    total += read as u64;
                    if total > self.limits.max_entry_bytes {
                        return Err(PackError::limits(format!(
                            "{name} 实际解压超过单条目上限 {}",
                            self.limits.max_entry_bytes
                        )));
                    }
                    if let Some(budget) = budget {
                        budget.spend(read as u64, name)?;
                    }
                    hasher.update(&buffer[..read]);
                }
                Ok(Some(hasher.finish()))
            }
        }
    }

    /// 收集一个覆盖目录下的文件（相对该目录的路径 + SHA-1 + 字节数），按路径排序。
    ///
    /// 目录不存在时返回空表——"包没带 overrides"不是错误。
    ///
    /// 这是**唯一会批量解压的 API**，所以它自己持有一个累计预算
    /// （`PackLimits::max_uncompressed_bytes`），本次调用内逐条按实际读出的字节扣减：
    /// 声明大小撒谎的包不会靠"很多条各自合规"把 CPU 时间耗光（r1 p3-4）。
    ///
    /// **额度是"每次调用一份"，不是"整包一份"**（r2 p3-b）：格式读者若对多个覆盖目录各调一次
    /// （mrpack 会连调三次：`overrides` / `client-overrides` / `server-overrides`），理论上限
    /// 是 `max_uncompressed_bytes × 调用次数`。要整包共用一份额度，就自己建 [`ReadBudget`]
    /// 并走 [`Self::override_files_with`]；`open` 阶段的"声明总量"仍然是整包只判一次。
    pub fn override_files(&self, dir: &str) -> PackResult<Vec<OverrideFile>> {
        let budget = ReadBudget::new(self.limits.max_uncompressed_bytes);
        self.override_files_with(dir, &budget)
    }

    /// 同 [`Self::override_files`]，但用调用方给的累计预算（M111 想与别处的解压共用额度时用）。
    pub fn override_files_with(
        &self,
        dir: &str,
        budget: &ReadBudget,
    ) -> PackResult<Vec<OverrideFile>> {
        let dir = normalize_dir(dir)?;
        let prefix = format!("{dir}/");
        let mut collected = Vec::new();
        for entry in &self.entries {
            if entry.is_dir || !entry.name.starts_with(&prefix) {
                continue;
            }
            let relative = &entry.name[prefix.len()..];
            if relative.is_empty() {
                continue;
            }
            let sha1 = self
                .sha1_with(&entry.name, Some(budget))?
                .unwrap_or_default();
            collected.push(OverrideFile {
                path: relative.to_string(),
                sha1,
                size: entry.size,
            });
        }
        collected.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(collected)
    }
}
