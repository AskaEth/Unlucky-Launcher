//! 从磁盘上的 `mods/` 到「重复 / 前置」报告（M312 · MODDUP）。
//!
//! 这一层只做一件事：把 [`ModEntry`]（文件名那一层）逐个读成 [`ModRecord`]（id / 版本 / 前置那一层），
//! 然后把记录整体交给 [`super::report::report`] 判定。**判定逻辑不在这里**——重复与前置的判据
//! 只有 [`mod@super::report`] 一份，读盘失败的降级也只有这一处。
//!
//! # 读不到元数据 = 普通 jar
//!
//! [`ModMeta`] 读不出来（[`meta::MetaGap`]）时退回 [`ModRecord::from_file_name`]：文件名里的 `-版本号`
//! 猜解仍然给出 id/版本，前置为空。于是"一个没有元数据的 jar"不会让整份报告变成错误，
//! 只是它不参与前置判定的**声明**一侧（它自己没声明前置），也不影响别人——它读不出 id，
//! 但它的文件名 id 仍然在"盘上有哪些 id"的集合里（少报优于误报）。
//!
//! # 只读、不写、不阻塞
//!
//! 纯只读：不建目录、不改名、不删文件。调用方（presenter 的 `host::mods`）在**后台线程**里跑它，
//! UI 只收一份快照——与 [`super::scan`] 同一条纪律。
//!
//! # 量级
//!
//! 每个 jar 只做「打开 zip + 读中央目录名字 + 解最多四份小文件」，不做全量解压。
//! 200 个 mod 的实测数据见 M312 的交付报告（同一台机器、debug 构建）。

use std::path::Path;

use super::list::ModEntry;
use super::meta::{self, ModMeta};
use super::report::{self, ModReport};
use super::suffix::suffix_variant;
use crate::diagnose::collectors::ModRecord;

/// 一次检查的产物。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModInspection {
    /// 每个文件读出来的记录（`.old` 不进；元数据读不到的退回文件名启发式）。
    pub records: Vec<ModRecord>,
    /// 「本该读到却没读到元数据」的文件数（[`meta::MetaGap::is_suspicious`]）。
    ///
    /// 不是问题，是**证据**：它说明这份报告的「前置是否满足」少看了几个文件。
    pub opaque: usize,
    /// 判定结果。
    pub report: ModReport,
}

/// 读一个已经列好的 `mods/` 目录。
///
/// `dir` 是 [`super::mods_dir`] 的结果（单根语义：不自己派生、不合并共享目录）。
#[must_use]
pub fn inspect(dir: &Path, entries: &[ModEntry]) -> ModInspection {
    let mut records = Vec::with_capacity(entries.len());
    let mut opaque = 0_usize;
    for entry in entries {
        // `.old` 是旧版本残留：游戏不加载它，读它的元数据只是白花 IO。
        if suffix_variant(&entry.file).is_old() {
            continue;
        }
        match meta::read(&dir.join(&entry.file)) {
            Ok(meta) => records.push(record_of(entry, meta)),
            Err(gap) => {
                if gap.is_suspicious() {
                    opaque += 1;
                }
                records.push(ModRecord::from_file_name(&entry.file));
            }
        }
    }
    let report = report::report(&records);
    ModInspection {
        records,
        opaque,
        report,
    }
}

/// 元数据 + 磁盘上的事实 → 一条记录。
fn record_of(entry: &ModEntry, meta: ModMeta) -> ModRecord {
    ModRecord {
        id: meta.id,
        // 展示名缺了就用列表里那个（去掉两个后缀的名字），总比空着强。
        name: meta.name.or_else(|| Some(entry.name.clone())),
        version: meta.version,
        file: Some(entry.file.clone()),
        enabled: entry.enabled,
        deps: meta.deps,
    }
}
