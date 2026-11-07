//! 打开与首轮校验：目录遍历 / zip 中央目录扫描，两道穿越检查、符号链接、限额都在这里。

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;

use zip::ZipArchive;

use super::entry::PackEntryInfo;
use super::limits::{check_limits, PackLimits};
use super::{PackArchive, Source};
use crate::addon::pack::error::{PackError, PackResult};
use crate::addon::pack::path::{has_trailing_slash, normalize_file};

impl PackArchive {
    pub(super) fn open_dir(root: PathBuf, limits: PackLimits) -> PackResult<Self> {
        let mut entries = Vec::new();
        let mut total: u64 = 0;
        // 栈式遍历：不递归，避免深层目录把栈打穿。
        let mut pending = vec![String::new()];
        while let Some(relative) = pending.pop() {
            let dir = if relative.is_empty() {
                root.clone()
            } else {
                root.join(&relative)
            };
            let mut children = std::fs::read_dir(&dir)
                .map_err(|error| PackError::io(format!("读目录 {} 失败：{error}", dir.display())))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(PackError::from)?;
            children.sort_by_key(std::fs::DirEntry::file_name);
            for child in children {
                let file_name =
                    child
                        .file_name()
                        .to_str()
                        .map(str::to_string)
                        .ok_or_else(|| {
                            PackError::metadata(format!(
                                "包内文件名不是 UTF-8：{}",
                                child.path().display()
                            ))
                        })?;
                let raw = if relative.is_empty() {
                    file_name
                } else {
                    format!("{relative}/{file_name}")
                };
                let file_type = child
                    .file_type()
                    .map_err(|error| PackError::io(format!("读 {} 类型失败：{error}", raw)))?;
                if file_type.is_symlink() {
                    return Err(PackError::symlink(format!("整合包里有符号链接条目：{raw}")));
                }
                let name = normalize_file(&raw)?;
                if file_type.is_dir() {
                    check_limits(&limits, &mut total, 0, &name, entries.len())?;
                    pending.push(name.clone());
                    entries.push(PackEntryInfo {
                        name,
                        raw,
                        size: 0,
                        is_dir: true,
                    });
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }
                let size = child
                    .metadata()
                    .map_err(|error| PackError::io(format!("读 {raw} 大小失败：{error}")))?
                    .len();
                check_limits(&limits, &mut total, size, &name, entries.len())?;
                entries.push(PackEntryInfo {
                    name,
                    raw,
                    size,
                    is_dir: false,
                });
            }
        }
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        Self::finish(root, Source::Dir, entries, limits)
    }

    pub(super) fn open_zip(root: PathBuf, limits: PackLimits) -> PackResult<Self> {
        let file = File::open(&root)
            .map_err(|error| PackError::io(format!("打开 {} 失败：{error}", root.display())))?;
        let mut archive = ZipArchive::new(file)
            .map_err(|error| PackError::metadata(format!("不是可读的 zip：{error}")))?;
        if archive.len() > limits.max_entries {
            return Err(PackError::limits(format!(
                "zip 条目数 {} 超过上限 {}",
                archive.len(),
                limits.max_entries
            )));
        }
        let mut entries = Vec::with_capacity(archive.len());
        let mut total: u64 = 0;
        for index in 0..archive.len() {
            let entry = archive.by_index(index).map_err(|error| {
                PackError::metadata(format!("读第 {index} 个条目失败：{error}"))
            })?;
            let Ok(raw) = entry.name() else {
                return Err(PackError::metadata(format!(
                    "第 {index} 个条目名编码不合法"
                )));
            };
            let raw = raw.to_string();
            let is_dir = entry.is_dir() || has_trailing_slash(&raw);
            if entry.is_symlink() {
                return Err(PackError::symlink(format!("zip 里有符号链接条目：{raw}")));
            }
            let name = normalize_file(&raw)?;
            // 第二检：zip crate 按 Windows 规则再解析一遍（盘符 / ADS / 保留设备名）。
            if entry.enclosed_name().is_none() {
                return Err(PackError::traversal(format!(
                    "zip 条目名过不了 enclosed_name 检查：{raw}"
                )));
            }
            let size = if is_dir { 0 } else { entry.size() };
            check_limits(&limits, &mut total, size, &name, entries.len())?;
            entries.push(PackEntryInfo {
                name,
                raw,
                size,
                is_dir,
            });
        }
        Self::finish(root, Source::Zip(RefCell::new(archive)), entries, limits)
    }

    fn finish(
        root: PathBuf,
        source: Source,
        entries: Vec<PackEntryInfo>,
        limits: PackLimits,
    ) -> PackResult<Self> {
        let mut index = HashMap::with_capacity(entries.len());
        for (slot, entry) in entries.iter().enumerate() {
            if index.insert(entry.name.clone(), slot).is_some() {
                return Err(PackError::metadata(format!(
                    "包里有规整后重名的条目：{}",
                    entry.name
                )));
            }
        }
        Ok(Self {
            root,
            source,
            entries,
            index,
            limits,
        })
    }
}
