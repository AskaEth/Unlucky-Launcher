//! 重复 mod 与前置缺失的判定（M312 · MODDUP）。
//!
//! 输入是 [`ModRecord`]（`diagnose::environment` 那一份），输出是给页面与诊断用的
//! [`ModReport`]。**判定只从这一条输入出发**——它以前在报告这条链上是零消费者，
//! 于是「读出来的 id/版本」没有任何用处。所有判定都在这里，不另读一遍盘（读盘在
//! [`mod@super::inspect`]）。
//!
//! # 三条判据
//!
//! | 判据 | 结论 | 为什么不更激进 |
//! |---|---|---|
//! | 同一个 modid、**两个已知且不同的版本** | [`ModReport::duplicates`] 一条，给出建议保留哪一份 | 同 id 同版本（只是拷了两份）**不算重复**：那不是"装了两个版本"，删一份对用户毫无收益 |
//! | 必需前置**没装** | [`ModReport::missing`]（问题） | 这是用户真实痛点的原话：「装了附属 mod 却没装前置」 |
//! | 必需前置装了、但版本区间**判不了** | [`ModReport::unverifiable`]（"无法判定"，不是问题） | 宁可少报：判不了就说过判不了，别让用户去删一个其实没问题的 mod |
//!
//! 可选前置（Fabric 的 `recommends` / `mods.toml` 的 `mandatory = false`）缺失时进
//! [`ModReport::optional`]：**不**算问题、不计数、页面只当一句提示——那本来就是"作者客气地提了一句"。
//!
//! # 不删任何文件
//!
//! 报告只有文件名与建议，**没有**「已经帮你删掉/禁用」这种副作用。禁用是用户点开关
//! （[`super::set_enabled`]）之后的事：删用户的文件不可逆，而"重复"这件事我们可能有判错的时候
//! （版本号写歪、两个 jar 其实是同一个 mod 的两个模块……）。
//!
//! # `.old` 不算
//!
//! `x.jar.old` 是 HMCL 生态的旧版本残留，游戏**不会加载**它（名字不以 `.jar` 结尾）。
//! 把它算进重复会让用户去删一个本来就没用的文件，算进"已装"则会让前置判定谎报满足。

mod version;

#[cfg(test)]
mod tests;

pub use version::{cmp_version, satisfies, Satisfaction};

use std::collections::BTreeMap;

use super::suffix::{display_name, suffix_variant};
use crate::diagnose::collectors::ModRecord;

/// 加载器 / 平台**自己**提供的前置：它们永远不是 `mods/` 里的 jar。
///
/// 这张表是"少报"的一侧：漏一条的下场是给用户报一条假的缺失（让他去装 `minecraft` 这个"模组"），
/// 多一条的下场是某个真前置不检查。所以表里每一条都是**确定由加载器提供**的。
const PLATFORM_IDS: &[&str] = &[
    "minecraft",
    "java",
    "fabricloader",
    "fabric-loader",
    "quilt_loader",
    "quilt-loader",
    "forge",
    "neoforge",
    "liteloader",
    "javafml",
    "lowcodefml",
    "mixin",
];

/// 一次检查的结论（`Default` = 什么问题都没有）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModReport {
    /// 同一个 modid 装了多份**不同版本**。
    pub duplicates: Vec<ModDuplicate>,
    /// 必需前置没装（**问题**）。
    pub missing: Vec<ModPrerequisite>,
    /// 必需前置装了，但版本区间判不了（"无法判定"，**不是问题**，只作提示）。
    pub unverifiable: Vec<ModPrerequisite>,
    /// 可选前置没装（作者说"推荐"，缺失不影响启动）。
    pub optional: Vec<ModPrerequisite>,
}

impl ModReport {
    /// 有没有**问题**（重复 + 必需缺失）。「无法判定」与「可选」不算。
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.duplicates.is_empty() && self.missing.is_empty()
    }

    /// 问题的条数（重复组数 + 必需缺失项数）。
    #[must_use]
    pub fn problem_count(&self) -> usize {
        self.duplicates.len() + self.missing.len()
    }
}

/// 一族的重复（同一个 modid 的多份安装）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModDuplicate {
    /// modid（小写）。
    pub id: String,
    /// 展示名（元数据的 `name`，没有就用 id）。
    pub display: String,
    /// 建议保留的**磁盘文件名**。
    pub keep: String,
    /// 建议保留那一份的版本。
    pub keep_version: Option<String>,
    /// 这一族的全部副本（**含**建议保留的那一份，新的在前）。
    pub copies: Vec<ModCopy>,
}

/// 重复族里的一份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModCopy {
    /// 磁盘文件名。
    pub file: String,
    /// 版本（读不出时为 `None`）。
    pub version: Option<String>,
    /// 是否启用。
    pub enabled: bool,
}

/// 一条前置的事（缺失 / 判不了 / 可选缺失）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModPrerequisite {
    /// 前置的 modid。
    pub id: String,
    /// 声明的版本范围原文（`None` = 没写）。
    pub range: Option<String>,
    /// 盘上那一份的版本（只有"装了但判不了"才有；确实没装时为 `None`）。
    pub installed_version: Option<String>,
    /// 谁声明了它（磁盘文件名，已排序去重）。
    pub required_by: Vec<String>,
}

impl ModPrerequisite {
    fn new(id: String) -> Self {
        Self {
            id,
            range: None,
            installed_version: None,
            required_by: Vec::new(),
        }
    }

    fn declare(&mut self, file: &str) {
        if self.required_by.iter().any(|known| known == file) {
            return;
        }
        self.required_by.push(file.to_string());
        self.required_by.sort();
    }
}

/// 算一份报告。
///
/// 纯函数：只读 [`ModRecord`]，不碰盘、不改任何状态。
#[must_use]
pub fn report(records: &[ModRecord]) -> ModReport {
    let live = live_records(records);
    let (missing, unverifiable, optional) = prerequisites(&live);
    ModReport {
        duplicates: duplicates(&live),
        missing,
        unverifiable,
        optional,
    }
}

/// 过滤掉 `.old`（游戏不加载它，见模块文档）。
fn live_records(records: &[ModRecord]) -> Vec<&ModRecord> {
    records
        .iter()
        .filter(|record| {
            record
                .file
                .as_deref()
                .is_none_or(|file| !suffix_variant(file).is_old())
        })
        .collect()
}

/// 按 modid 分组，只留「两个以上**已知且不同**版本」的族。
fn duplicates(records: &[&ModRecord]) -> Vec<ModDuplicate> {
    let mut groups: BTreeMap<String, Vec<&ModRecord>> = BTreeMap::new();
    for record in records {
        let id = record.id.trim().to_ascii_lowercase();
        if id.is_empty() {
            continue;
        }
        groups.entry(id).or_default().push(record);
    }
    groups
        .into_iter()
        .filter_map(|(id, group)| duplicate(id, group))
        .collect()
}

fn duplicate(id: String, group: Vec<&ModRecord>) -> Option<ModDuplicate> {
    if group.len() < 2 {
        return None;
    }
    // 「同 id 同版本」不是重复：那只是同一个文件拷了两份。版本读不出的副本也不参与这个判断
    // （判据要的是**已知且不同**的两个版本）。
    let mut versions = group
        .iter()
        .filter_map(|record| record.version.as_deref())
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .collect::<Vec<_>>();
    versions.sort_unstable();
    versions.dedup();
    if versions.len() < 2 {
        return None;
    }

    // 建议保留：版本新的 → 启用中的 → 文件名靠前的（最后一条只为两帧之间不抖）。
    let mut ranked = group;
    ranked.sort_by(|left, right| {
        cmp_version(
            right.version.as_deref().unwrap_or(""),
            left.version.as_deref().unwrap_or(""),
        )
        .then_with(|| right.enabled.cmp(&left.enabled))
        .then_with(|| left.file.cmp(&right.file))
    });
    let keep = ranked.first()?;
    let display = keep
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map_or_else(|| keep.id.clone(), str::to_string);
    Some(ModDuplicate {
        id,
        display,
        keep: keep.file.clone().unwrap_or_default(),
        keep_version: keep.version.clone(),
        copies: ranked
            .iter()
            .map(|record| ModCopy {
                file: record.file.clone().unwrap_or_default(),
                version: record.version.clone(),
                enabled: record.enabled,
            })
            .collect(),
    })
}

/// 元数据读不出时的兜底：文件名以 `<id>` 开头，且后面紧跟**扩展名或 `-<数字>`**。
///
/// | 文件名 | 认成 `<id>` = `fabric-api`？ |
/// |---|---|
/// | `fabric-api-0.92.2.jar` | ✅（`-0` 是版本） |
/// | `fabric-api.jar` | ✅（`.jar` 是扩展名） |
/// | `fabric-api-unofficial.jar` | ❌（`-u` 不是版本，那是另一个 mod） |
///
/// 只用来**少报**一种假缺失：那个 jar 的元数据读不出来时，它的 id 是文件名猜的
/// （`fabric-api-0`），精确匹配查不到，于是「装着的说没装」。宁可少报，不要误报。
/// 边界：版本段以字母开头的（`sodium-unofficial`）认不出来 ⇒ 仍然按「缺失」报——那一种
/// 我们**不能**确认是同一个 mod。
fn by_file_name<'a>(records: &[&'a ModRecord], id: &str) -> Option<&'a ModRecord> {
    records.iter().copied().find(|record| {
        let Some(file) = record.file.as_deref() else {
            return false;
        };
        let name = display_name(file).to_ascii_lowercase();
        name.strip_prefix(id).is_some_and(|rest| {
            rest.starts_with('.')
                || rest
                    .strip_prefix('-')
                    .is_some_and(|tail| tail.starts_with(|ch: char| ch.is_ascii_digit()))
        })
    })
}

/// 三桶前置：必需缺失 / 判不了 / 可选缺失。
fn prerequisites(
    records: &[&ModRecord],
) -> (
    Vec<ModPrerequisite>,
    Vec<ModPrerequisite>,
    Vec<ModPrerequisite>,
) {
    // 盘上有哪几个 id（含禁用中的：它**装着**，只是没启用——那属于启停区的事，不是"缺失"）。
    let installed: BTreeMap<String, &ModRecord> = records
        .iter()
        .filter_map(|record| {
            let id = record.id.trim().to_ascii_lowercase();
            (!id.is_empty()).then_some((id, *record))
        })
        .collect();

    let mut missing: BTreeMap<String, ModPrerequisite> = BTreeMap::new();
    let mut unverifiable: BTreeMap<String, ModPrerequisite> = BTreeMap::new();
    let mut optional: BTreeMap<String, ModPrerequisite> = BTreeMap::new();

    for record in records {
        let owner = record.id.trim().to_ascii_lowercase();
        let by = record.file.clone().unwrap_or_else(|| record.id.clone());
        for dep in &record.deps {
            let id = dep.id.trim().to_ascii_lowercase();
            if id.is_empty() || id == owner || PLATFORM_IDS.contains(&id.as_str()) {
                continue;
            }
            let Some(found) = installed
                .get(&id)
                .copied()
                .or_else(|| by_file_name(records, &id))
            else {
                let bucket = if dep.mandatory {
                    &mut missing
                } else {
                    &mut optional
                };
                let entry = bucket
                    .entry(id.clone())
                    .or_insert_with(|| ModPrerequisite::new(id.clone()));
                entry.declare(&by);
                entry.range = dep.version_range.clone();
                continue;
            };
            // 装了：只有「能确认满足」才闭嘴。判不了 / 判出不满足都进"无法判定"——
            // 我们不解完整的版本范围语法，主张"缺失"会误报。
            if !dep.mandatory {
                continue;
            }
            if matches!(
                satisfies(found.version.as_deref(), dep.version_range.as_deref()),
                Satisfaction::No | Satisfaction::Unknown
            ) {
                let entry = unverifiable
                    .entry(id.clone())
                    .or_insert_with(|| ModPrerequisite::new(id.clone()));
                entry.declare(&by);
                entry.range = dep.version_range.clone();
                entry.installed_version = found.version.clone();
            }
        }
    }

    (
        missing.into_values().collect(),
        unverifiable.into_values().collect(),
        optional.into_values().collect(),
    )
}
