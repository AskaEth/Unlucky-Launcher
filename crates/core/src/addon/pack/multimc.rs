//! MultiMC / Prism 格式：`mmc-pack.json` + `instance.cfg` + `.minecraft/` 目录。
//!
//! # 形状（A 级来源：HMCL `MultiMCModpackProvider` / `toGameSettings`，见 analyse/05 §2.6）
//!
//! ```jsonc
//! // mmc-pack.json
//! {"formatVersion": 1, "components": [
//!   {"uid": "net.minecraft", "version": "1.20.1", "important": true},
//!   {"uid": "net.fabricmc.fabric-loader", "version": "0.15.11"}]}
//! // instance.cfg（ini）
//! InstanceType=OneSix
//! name=我的整合包
//! ```
//!
//! **它不是一个"包"，而是一个"实例目录"**：模组就在 `.minecraft/mods/` 里，没有任何远端
//! 条目、没有 URL、没有摘要表。所以：
//!
//! - 读：把 `.minecraft` 当成一个覆盖目录（[`PackSide::Both`]），模组按目录里的实际文件逐条
//!   记 SHA-1；[`PackManifest::files`] 恒为空——这是格式事实，不是我们的偷懒；
//! - 写：**不支持**。写回 MultiMC 需要重组 `mmc-pack.json` 的组件图（还要处理
//!   `patches/`、`libraries/`、`jarmods/`），那是另一套协议，不在本模块的模型职责内，
//!   也不该伪装成"能导出"。`metadata_entry` 返回 `None`，写会得到明确的
//!   `pack_unsupported`。
//!
//! `components` 是**保序列表**，里面既有加载器，也有 mappings（`net.fabricmc.intermediary`
//! 等）与 `org.lwjgl3` 这类非加载器组件——所以模型把**每一条组件**都留在
//! [`PackManifest::loaders`] 里（顺序不动、原始 uid 在 [`PackLoader::raw`]、`kind` 能认就认），
//! 组件自身的 `important` / `dependencyOnly` / `cachedName` 落在 [`PackLoader::extra`]（M115
//! 调研 §字段缺口 3）。这样"加载器集合"是它的派生视图，而不是丢信息的收窄。
//!
//! 也就是说：本格式**能探测、能导入**，**不能导出**——报告里已把这条写清楚。

use serde::Deserialize;
use serde_json::{Map, Value};

use super::archive::PackArchive;
use super::error::{PackError, PackResult};
use super::format::PackFormat;
use super::manifest::{PackLoader, PackManifest, PackOverrides, PackSide};

/// 格式 id。
pub const ID: &str = "multimc";
/// 清单入口名。
pub const METADATA_ENTRY: &str = "mmc-pack.json";
/// 实例配置文件名（包名在这里）。
pub const INSTANCE_ENTRY: &str = "instance.cfg";
/// 实例内容目录（模组/配置都在里面）。
pub const CONTENT_DIR: &str = ".minecraft";
/// 游戏本体的组件 uid。
pub const MINECRAFT_UID: &str = "net.minecraft";

/// 单例（进注册表用）。
pub static MULTIMC: MultiMc = MultiMc;

/// MultiMC / Prism 格式的值类型实现。
#[derive(Debug, Clone, Copy, Default)]
pub struct MultiMc;

/// `mmc-pack.json` 的原始形状。
#[derive(Debug, Default, Deserialize)]
struct RawPack {
    #[serde(rename = "formatVersion", default)]
    format_version: Option<i64>,
    #[serde(default)]
    components: Vec<RawComponent>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// `components[]` 的一条。
#[derive(Debug, Default, Deserialize)]
struct RawComponent {
    #[serde(default)]
    uid: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl PackFormat for MultiMc {
    fn id(&self) -> &'static str {
        ID
    }

    fn label(&self) -> &'static str {
        "MultiMC / Prism"
    }

    fn detect(&self, pack: &PackArchive) -> PackResult<Option<&'static str>> {
        let Some(value) = pack.read_json(METADATA_ENTRY)? else {
            return Ok(None);
        };
        let object = value
            .as_object()
            .ok_or_else(|| PackError::metadata(format!("{METADATA_ENTRY} 存在但不是 JSON 对象")))?;
        // 形状校验：必须有非空 components 数组（MultiMC 的组件图是它的身份证）。
        let matched = object
            .get("components")
            .and_then(Value::as_array)
            .is_some_and(|components| !components.is_empty());
        Ok(matched.then_some(METADATA_ENTRY))
    }

    fn read(&self, pack: &PackArchive) -> PackResult<PackManifest> {
        let value = pack
            .read_json(METADATA_ENTRY)?
            .ok_or_else(|| PackError::metadata(format!("包里没有 {METADATA_ENTRY}")))?;
        let raw: RawPack = serde_json::from_value(value)
            .map_err(|error| PackError::metadata(format!("{METADATA_ENTRY} 形状不符：{error}")))?;

        let mut minecraft = None;
        let mut loaders: Vec<PackLoader> = Vec::new();
        for component in &raw.components {
            let Some(uid) = component.uid.as_deref() else {
                continue;
            };
            let Some(version) = component.version.as_deref() else {
                continue;
            };
            if uid == MINECRAFT_UID {
                minecraft = Some(version.to_string());
                continue;
            }
            let mut loader = PackLoader::parse_multimc(uid, version, false);
            if loader.kind.is_some() && !loaders.iter().any(|seen| seen.kind.is_some()) {
                loader.primary = true;
            }
            loader.extra = component.extra.clone();
            loaders.push(loader);
        }

        let name = instance_name(pack).unwrap_or_else(|| {
            pack.root()
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_default()
        });

        let mut overrides = Vec::new();
        let content = pack.override_files(CONTENT_DIR)?;
        if pack.has_entry(CONTENT_DIR) || !content.is_empty() {
            overrides.push(PackOverrides {
                dir: CONTENT_DIR.to_string(),
                side: PackSide::Both,
                files: content,
            });
        }

        Ok(PackManifest {
            name,
            version: None,
            author: None,
            summary: None,
            format_version: raw.format_version.or(Some(1)),
            game: Some("minecraft".to_string()),
            minecraft,
            loaders,
            // MultiMC 没有远端条目：模组就是覆盖目录里的文件。
            files: Vec::new(),
            overrides,
            extra: raw.extra,
            minecraft_extra: Map::new(),
        })
    }

    fn render(&self, _manifest: &PackManifest) -> PackResult<String> {
        Err(PackError::unsupported(
            "MultiMC 格式只支持导入：写回需要重组组件图与 patches/libraries，不在模型职责内",
        )
        .with_hint("导出请选 Modrinth（.mrpack）或 CurseForge 格式"))
    }
}

/// 从 `instance.cfg` 的 `name=` 行取实例名（MultiMC 的 ini 很宽松，只认这一行）。
fn instance_name(pack: &PackArchive) -> Option<String> {
    let text = pack.read_text(INSTANCE_ENTRY).ok().flatten()?;
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("name="))
        .map(str::trim)
        .find(|name| !name.is_empty())
        .map(str::to_string)
}
