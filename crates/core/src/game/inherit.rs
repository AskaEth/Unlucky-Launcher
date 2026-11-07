//! `inheritsFrom` 继承解析、逐字段合并语义、patch 优先级排序。
//!
//! 依据：`DevDocs/analyse/03-version-download.md` §4.3（`resolve()`）、§4.4（merge 语义表）、
//! §4.5（patch 生命周期）。
//!
//! 与 HMCL 的三处**有意差异**（都在 §4.3 的对照里）：
//! 1. **成环即报错**（[`GameError::CircularInheritance`]）而不是"断链后降级启动"：
//!    静默降级会让用户拿到"能启动但缺库"的实例，宁可明确告知清单坏了。
//! 2. **缺失父版本报错**（[`GameError::MissingParent`]），与 HMCL 的
//!    `NoSuchGameInstanceException` 一致。
//! 3. 不实现 `toPatch()` 的"把已解析父层固化进 patches"优化——P0 实例数量小，
//!    每次启动重算继承链（毫秒级）；等有性能证据再加缓存。

use std::collections::BTreeMap;

use super::arguments::Arguments;
use super::error::GameError;
use super::manifest::VersionJson;

/// 父版本来源（仓库快照或测试里的内存表）。
pub trait VersionSource {
    /// 按 id 取一份版本 JSON；不存在返回 `None`。
    ///
    /// 实现方应自行处理 IO 错误——把失败转成 `None` 会让"父版本缺失"和"读盘失败"
    /// 混为一谈，因此读盘失败的实现应当在调用 [`resolve`] 前就把它暴露给用户。
    fn load(&self, id: &str) -> Option<VersionJson>;
}

impl VersionSource for BTreeMap<String, VersionJson> {
    fn load(&self, id: &str) -> Option<VersionJson> {
        self.get(id).cloned()
    }
}

impl<F> VersionSource for F
where
    F: Fn(&str) -> Option<VersionJson>,
{
    fn load(&self, id: &str) -> Option<VersionJson> {
        self(id)
    }
}

/// 解析继承链，返回可交给 launch/download 的扁平清单。
///
/// 返回的清单保证：`inherits_from == None`、`patches == None`、`jar` 已回退为实例 id、
/// `root == Some(true)`（见 [`VersionJson::is_resolved`]）。
///
/// # Errors
///
/// - 清单缺 `id` → [`GameError::MissingVersionId`]
/// - 父版本缺失 → [`GameError::MissingParent`]
/// - 继承成环 → [`GameError::CircularInheritance`]
pub fn resolve<S: VersionSource>(
    source: &S,
    manifest: &VersionJson,
) -> Result<VersionJson, GameError> {
    let root_id = manifest.require_id()?.to_owned();

    // 1. 自根向祖先收集层（迭代而非递归：深层整合包的链可以很长）。
    let mut chain = vec![root_id.clone()];
    let mut layers = vec![manifest.clone()];
    while let Some(parent_id) = layers.last().and_then(|l| l.inherits_from.clone()) {
        if chain.iter().any(|id| id == &parent_id) {
            chain.push(parent_id);
            return Err(GameError::CircularInheritance { chain });
        }
        let required_by = layers
            .last()
            .and_then(|l| l.id.clone())
            .unwrap_or_else(|| root_id.clone());
        let parent = source
            .load(&parent_id)
            .ok_or_else(|| GameError::MissingParent {
                parent: parent_id.clone(),
                required_by,
            })?;
        chain.push(parent_id);
        layers.push(parent);
    }

    // 2. 自最祖先向根折叠：每一层先与父层合并，再合并它自己的 patches。
    let mut folded: Option<VersionJson> = None;
    for layer in layers.into_iter().rev() {
        let merged = match &folded {
            Some(parent) => merge(&layer, parent),
            None => merge(&layer, &VersionJson::default()),
        };
        folded = Some(apply_patches(
            merged,
            &layer.patches.clone().unwrap_or_default(),
        ));
    }

    // 3. 收尾：清继承标记、jar 兜底、补 id。
    let mut resolved = folded.unwrap_or_default();
    resolved.inherits_from = None;
    resolved.patches = None;
    resolved.priority = None;
    resolved.root = Some(true);
    if resolved.id.is_none() {
        resolved.id = Some(root_id.clone());
    }
    if resolved.jar.is_none() {
        resolved.jar = Some(root_id);
    }
    Ok(resolved)
}

/// 按 `priority` 升序排好的 patch 列表（`None` 视作最小值，稳定排序保持插入顺序）。
pub fn sorted_patches(patches: &[VersionJson]) -> Vec<&VersionJson> {
    let mut sorted: Vec<&VersionJson> = patches.iter().collect();
    sorted.sort_by_key(|p| p.priority.unwrap_or(i32::MIN));
    sorted
}

/// 把 patch 层按优先级依次叠加到基底清单上（先原版、后 loader：`0 < 30000`）。
pub fn apply_patches(base: VersionJson, patches: &[VersionJson]) -> VersionJson {
    let mut out = base;
    for patch in sorted_patches(patches) {
        out = merge(patch, &out);
    }
    out
}

/// 合并两层清单：**子（或后合并的层）覆盖父**，语义逐字段照 analyse/03 §4.4。
///
/// 非直觉的两点，测试里各有一条守门：
/// - `downloads`/`logging` **整体覆盖**：子层只写 `server` 会让父层的 `client` 消失；
/// - `compliance_level` **只取子**：父层的值被丢弃（缺则是 `None`）。
///
/// 这两点**与 analyse/03 §8.3 第 15 条建议相左**（该条主张逐字段/逐 key 合并）。
/// 保留现状的理由是生态兼容：现实 modpack/loader 的版本 JSON 按 HMCL/原版语义编写，
/// 逐 key 合并会让"子层想整份换掉 downloads"的场景语义漂移。经 tower 裁决 2026-10-29 维持。
pub fn merge(child: &VersionJson, parent: &VersionJson) -> VersionJson {
    let mut extra = parent.extra.clone();
    extra.extend(child.extra.iter().map(|(k, v)| (k.clone(), v.clone())));

    VersionJson {
        id: child.id.clone().or_else(|| parent.id.clone()),
        name: child.name.clone().or_else(|| parent.name.clone()),
        // 解析后不再带继承。
        inherits_from: None,
        jar: child.jar.clone().or_else(|| parent.jar.clone()),
        main_class: child
            .main_class
            .clone()
            .or_else(|| parent.main_class.clone()),
        minecraft_arguments: child
            .minecraft_arguments
            .clone()
            .or_else(|| parent.minecraft_arguments.clone()),
        arguments: Arguments::merge(parent.arguments.as_ref(), child.arguments.as_ref()),
        assets: child.assets.clone().or_else(|| parent.assets.clone()),
        asset_index: child
            .asset_index
            .clone()
            .or_else(|| parent.asset_index.clone()),
        compliance_level: child.compliance_level,
        java_version: child
            .java_version
            .clone()
            .or_else(|| parent.java_version.clone()),
        libraries: match (&parent.libraries, &child.libraries) {
            (None, None) => None,
            (Some(parent), None) => Some(parent.clone()),
            (None, Some(child)) => Some(child.clone()),
            // 父在前、子在后追加（重复项由使用方按需去重）。
            (Some(parent), Some(child)) => {
                Some(parent.iter().chain(child.iter()).cloned().collect())
            }
        },
        // 整体覆盖，不逐 key 合并。
        downloads: child.downloads.clone().or_else(|| parent.downloads.clone()),
        logging: child.logging.clone().or_else(|| parent.logging.clone()),
        version_type: child.version_type.or(parent.version_type),
        time: child.time.clone().or_else(|| parent.time.clone()),
        release_time: child
            .release_time
            .clone()
            .or_else(|| parent.release_time.clone()),
        minimum_launcher_version: match (
            parent.minimum_launcher_version,
            child.minimum_launcher_version,
        ) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, None) => a,
            (None, b) => b,
        },
        // 合并结果一律是"根清单"。
        root: Some(true),
        hidden: child.hidden,
        // patch 生命周期由 resolve/apply_patches 管，合并结果不带 patches。
        patches: None,
        priority: None,
        extra,
    }
}
