//! CurseForge 格式：`manifest.json` + `overrides/`。
//!
//! # 形状（A 级来源：HMCL `CurseManifest.java` / `CurseManifestFile.java`，见 analyse/05 §2.2）
//!
//! ```jsonc
//! {
//!   "manifestType": "minecraftModpack", "manifestVersion": 1,
//!   "name": "…", "version": "1.0.0", "author": "…",
//!   "overrides": "overrides",
//!   "minecraft": {"version": "1.20.1", "modLoaders": [{"id": "forge-47.2.0", "primary": true}]},
//!   "files": [{"projectID": 238222, "fileID": 5102025, "required": true}]
//! }
//! ```
//!
//! 关键缺口（**模型如实记录，不假装有**）：
//!
//! - `files[]` **没有路径、没有 URL、没有哈希**。路径要按项目 `classId` 猜（`mods/` /
//!   `resourcepacks/` / `shaderpacks/`），文件名要按 ForgeCDN 规则或 API 补——这正是 D10 §2
//!   两阶段补全要补的东西，属于 M111，不在本模块。所以 [`PackFile::path`] 读出来是 `None`。
//! - `minecraft.modLoaders[].id` 只认 `forge-` / `neoforge-` / `fabric-` / `quilt-` 前缀
//!   （CurseForge 没有 Quilt 的正规表达方式）；不认识的 id **原样保留**在
//!   [`PackLoader::raw`]，写回时逐字还原。
//! - 只有一个 `required` 布尔 → 客户端/服务端同值；导出时 `client == Optional` 或
//!   `disabled` 即 `required: false`。`client`/`server` 分侧语义 CF 表达不了（mrpack 专属）。
//! - **跨格式导出不完整**（M112 的活）：`modLoaders[].id` 只对 CF 来源是合法形状；
//!   MultiMC 的 `net.fabricmc.fabric-loader` / 非加载器组件会被逐字写进去（CF 不认）；
//!   `overrides` 字段会照抄模型里的目录名（从 MultiMC 转过来是 `.minecraft`）。
//!   按 CF 语义 `overrides` 只是 zip 内**前缀**，导入器把该前缀下的内容铺到游戏目录，
//!   所以内容不会丢；但"前缀目录本身要不要跟着拷一层"取决于导入器实现（Prism/ULL 的实例根
//!   本身就叫 `.minecraft` 时可能多出一层）。同一份模型只有 `read` 出来的那个格式才保证往返。
//!
//! CF 的**文件指纹**（`MurmurHash2-32` + 空白过滤）是导出侧「能否被远程替代」的判据，见
//! [`fingerprint`]：它是"CF 格式知识"，不属于导出流程，所以留在这里而不是 `addon::export`。

pub mod fingerprint;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::archive::PackArchive;
use super::error::{PackError, PackResult};
use super::format::PackFormat;
use super::manifest::{
    merge_unknown, EnvSupport, PackFile, PackLoader, PackManifest, PackOverrides, PackSide,
};
use super::path::normalize_dir;

/// 格式 id。
pub const ID: &str = "curseforge";
/// 清单入口名。
pub const METADATA_ENTRY: &str = "manifest.json";
/// `manifestType` 的固定值。
pub const MARKER: &str = "minecraftModpack";
/// 默认覆盖目录名。
pub const DEFAULT_OVERRIDES: &str = "overrides";

/// 单例（进注册表用）。
pub static CURSEFORGE: CurseForge = CurseForge;

/// CurseForge 格式的值类型实现。
#[derive(Debug, Clone, Copy, Default)]
pub struct CurseForge;

/// `manifest.json` 的原始形状（未知字段落 `extra`）。
#[derive(Debug, Default, Deserialize)]
struct RawManifest {
    #[serde(rename = "manifestType", default)]
    manifest_type: Option<String>,
    #[serde(rename = "manifestVersion", default)]
    manifest_version: Option<i64>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    overrides: Option<String>,
    #[serde(default)]
    minecraft: Option<RawMinecraft>,
    #[serde(default)]
    files: Vec<RawFile>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// `minecraft` 对象。
#[derive(Debug, Default, Deserialize)]
struct RawMinecraft {
    #[serde(default)]
    version: Option<String>,
    #[serde(rename = "modLoaders", default)]
    mod_loaders: Vec<RawLoader>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// `minecraft.modLoaders[]` 的一条。
#[derive(Debug, Default, Deserialize)]
struct RawLoader {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    primary: bool,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// `files[]` 的一条。
#[derive(Debug, Default, Deserialize)]
struct RawFile {
    #[serde(rename = "projectID", default)]
    project_id: Option<i64>,
    #[serde(rename = "fileID", default)]
    file_id: Option<i64>,
    #[serde(default = "default_required")]
    required: bool,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// CF 的 `required` 缺省是 `true`（老包可能不写）。
const fn default_required() -> bool {
    true
}

impl PackFormat for CurseForge {
    fn id(&self) -> &'static str {
        ID
    }

    fn label(&self) -> &'static str {
        "CurseForge"
    }

    fn metadata_entry(&self) -> Option<&'static str> {
        Some(METADATA_ENTRY)
    }

    fn detect(&self, pack: &PackArchive) -> PackResult<Option<&'static str>> {
        // 文件不存在 ≠ 包坏了：没有 `manifest.json` 就是"不是本格式"。
        let Some(value) = pack.read_json(METADATA_ENTRY)? else {
            return Ok(None);
        };
        // 文件在、但不是对象 = 形状坏了（`pack_metadata`），不是"不认识"（r1 p3-2）。
        let object = value
            .as_object()
            .ok_or_else(|| PackError::metadata(format!("{METADATA_ENTRY} 存在但不是 JSON 对象")))?;
        let matched = match object.get("manifestType").and_then(Value::as_str) {
            // 有标记：只认固定值（`manifest.json` 这名字太泛，光凭名字不能认领）。
            Some(kind) => kind == MARKER,
            // 没有标记的老包：要求形状对得上（有 minecraft 对象 + files 数组）。
            None => {
                object.get("minecraft").is_some_and(Value::is_object)
                    && object.get("files").is_some_and(Value::is_array)
            }
        };
        Ok(matched.then_some(METADATA_ENTRY))
    }

    fn check_overrides(&self, manifest: &PackManifest) -> PackResult<()> {
        // CF 只有**一个** `overrides` 目录，且清单里只有一个字段能声明它 ⇒ 只表达得了
        // 一个 Both 侧组。Client/Server 分侧是 mrpack 的东西，这里宁可拒也不猜（r1 p2-1）。
        let mut both = 0_usize;
        for group in &manifest.overrides {
            if group.side != PackSide::Both {
                return Err(PackError::unsupported(format!(
                    "CurseForge 只有一个 overrides 目录，表达不了 {} 侧的覆盖内容（{} 个文件）",
                    group.side.as_str(),
                    group.files.len()
                ))
                .with_hint("先按侧合并（或去掉不该带的组）再导出；分侧目录映射属 M112"));
            }
            both += 1;
        }
        if both > 1 {
            return Err(PackError::unsupported(
                "CurseForge 只有一个 overrides 目录字段，模型里却有多个 Both 侧覆盖组",
            )
            .with_hint("把它们合成一个组再导出；目录名映射属 M112"));
        }
        Ok(())
    }

    fn read(&self, pack: &PackArchive) -> PackResult<PackManifest> {
        let value = pack
            .read_json(METADATA_ENTRY)?
            .ok_or_else(|| PackError::metadata(format!("包里没有 {METADATA_ENTRY}")))?;
        let raw: RawManifest = serde_json::from_value(value)
            .map_err(|error| PackError::metadata(format!("{METADATA_ENTRY} 形状不符：{error}")))?;
        if let Some(kind) = &raw.manifest_type {
            if kind != MARKER {
                return Err(PackError::unsupported(format!(
                    "{METADATA_ENTRY} 的 manifestType 是 {kind:?}，不是 {MARKER:?}"
                )));
            }
        }
        let name = raw
            .name
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| PackError::metadata(format!("{METADATA_ENTRY} 缺 name")))?;
        let minecraft = raw.minecraft.unwrap_or_default();
        let game_version = minecraft
            .version
            .filter(|version| !version.trim().is_empty())
            .ok_or_else(|| PackError::metadata(format!("{METADATA_ENTRY} 缺 minecraft.version")))?;

        let loaders = minecraft
            .mod_loaders
            .into_iter()
            .map(|loader| {
                let mut parsed = PackLoader::parse_curse(
                    loader.id.as_deref().unwrap_or_default(),
                    loader.primary,
                );
                parsed.extra = loader.extra;
                parsed
            })
            .collect();

        let files = raw
            .files
            .into_iter()
            .map(|file| {
                let env = if file.required {
                    EnvSupport::Required
                } else {
                    EnvSupport::Optional
                };
                PackFile {
                    project_id: file.project_id,
                    file_id: file.file_id,
                    client: env,
                    server: env,
                    extra: file.extra,
                    ..PackFile::default()
                }
            })
            .collect();

        // CF 的 `overrides` 字段缺省就是 `"overrides"`（规范默认值），`render` 也总会写它；
        // 读侧因此**永远**产出一个 Both 侧组，读写才对称（dir 不存在时 files 为空表）。
        let overrides_dir = raw
            .overrides
            .as_deref()
            .filter(|dir| !dir.trim().is_empty())
            .unwrap_or(DEFAULT_OVERRIDES);
        let overrides_dir = normalize_dir(overrides_dir)?;
        let overrides = vec![PackOverrides {
            files: pack.override_files(&overrides_dir)?,
            dir: overrides_dir,
            side: PackSide::Both,
        }];

        Ok(PackManifest {
            name,
            version: raw.version,
            author: raw.author,
            summary: None,
            format_version: raw.manifest_version,
            game: None,
            minecraft: Some(game_version),
            loaders,
            files,
            overrides,
            extra: raw.extra,
            minecraft_extra: minecraft.extra,
        })
    }

    fn render(&self, manifest: &PackManifest) -> PackResult<String> {
        let game_version = manifest.minecraft.as_deref().ok_or_else(|| {
            PackError::metadata("CurseForge 清单需要 minecraft.version，模型里没有")
        })?;
        let mut root = Map::new();
        root.insert("manifestType".into(), json!(MARKER));
        root.insert(
            "manifestVersion".into(),
            json!(manifest.format_version.unwrap_or(1)),
        );
        root.insert("name".into(), json!(manifest.name));
        if let Some(version) = &manifest.version {
            root.insert("version".into(), json!(version));
        }
        if let Some(author) = &manifest.author {
            root.insert("author".into(), json!(author));
        }
        let overrides_dir = manifest
            .overrides_for(PackSide::Both)
            .map_or(DEFAULT_OVERRIDES, |group| group.dir.as_str());
        root.insert("overrides".into(), json!(overrides_dir));

        let mut minecraft = Map::new();
        minecraft.insert("version".into(), json!(game_version));
        let mut mod_loaders = Vec::with_capacity(manifest.loaders.len());
        for loader in &manifest.loaders {
            let id = loader.curse_id_for(Some(game_version)).ok_or_else(|| {
                PackError::unsupported(format!(
                    "CurseForge 表达不了这条加载器依赖：raw={:?} kind={:?} version={:?}",
                    loader.raw, loader.kind, loader.version
                ))
            })?;
            let mut object = Map::new();
            object.insert("id".into(), json!(id));
            object.insert("primary".into(), json!(loader.primary));
            merge_unknown(&mut object, &loader.extra);
            mod_loaders.push(Value::Object(object));
        }
        minecraft.insert("modLoaders".into(), Value::Array(mod_loaders));
        merge_unknown(&mut minecraft, &manifest.minecraft_extra);
        root.insert("minecraft".into(), Value::Object(minecraft));

        let mut files = Vec::with_capacity(manifest.files.len());
        for file in &manifest.files {
            let (Some(project_id), Some(file_id)) = (file.project_id, file.file_id) else {
                return Err(PackError::unsupported(format!(
                    "CurseForge 的 files[] 需要 projectID/fileID，这条没有（path={:?}）",
                    file.path
                ))
                .with_hint("先用 CurseForge API 补全条目再导出，或改用 Modrinth 格式"));
            };
            let mut object = Map::new();
            object.insert("projectID".into(), json!(project_id));
            object.insert("fileID".into(), json!(file_id));
            // CF 只有一个 `required` 布尔：`optional` 与"禁用"都只能落到 `false`
            // （Prism 也是用 `required: false` 表达禁用的，见 M115 调研 §6）。
            object.insert(
                "required".into(),
                json!(!(file.client.is_optional() || file.disabled)),
            );
            merge_unknown(&mut object, &file.extra);
            files.push(Value::Object(object));
        }
        root.insert("files".into(), Value::Array(files));

        merge_unknown(&mut root, &manifest.extra);
        serde_json::to_string_pretty(&Value::Object(root)).map_err(Into::into)
    }
}
