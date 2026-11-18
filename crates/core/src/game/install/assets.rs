//! 资源索引（`assets/indexes/<id>.json`）的模型与逐对象展开。
//!
//! 依据：`DevDocs/analyse/03-version-download.md` §6.5（索引下载）、§6.6（逐对象下载）。
//!
//! 对象是**内容寻址**的：路径 `assets/objects/<hash[0..2]>/<hash>`、URL
//! `https://resources.download.minecraft.net/<hash[0..2]>/<hash>`，校验值就是哈希本身。
//! 因此索引里不需要 `sha1` 字段——文件名即摘要，这也是"两个实例共用同一资源"零成本的原因。
//!
//! 本模块只做"索引 → 待下载对象"的纯计算，不碰网络；落盘路径一律问
//! [`GameLayout`]，不在本地拼字符串。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::super::error::GameError;
use super::super::layout::GameLayout;
use super::plan::{FileRole, PlanFile};
use crate::download::ResourceKind;

/// 资源对象的官方源基址。
pub const ASSET_OBJECT_BASE: &str = "https://resources.download.minecraft.net";

/// 一个资源对象。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetObject {
    /// 内容 SHA-1（同时是文件名）。
    #[serde(default)]
    pub hash: String,
    /// 字节数。
    #[serde(default)]
    pub size: u64,
}

impl AssetObject {
    /// 相对 `assets/objects/` 的路径 `xx/<hash>`。
    #[must_use]
    pub fn location(&self) -> String {
        let shard = self.hash.get(0..2).unwrap_or_default();
        format!("{shard}/{}", self.hash)
    }

    /// 官方下载地址（镜像由下载引擎按前缀规则展开）。
    #[must_use]
    pub fn url(&self) -> String {
        format!("{ASSET_OBJECT_BASE}/{}", self.location())
    }

    /// 哈希是否够做两级分片（短于两个字符说明索引坏了）。
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.hash.len() >= 2 && self.hash.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

/// 一份资源索引。
///
/// `virtual` / `map_to_resources` 只影响**启动期**的目录映射（`assets/virtual/<id>`、
/// `resources/`），不影响下载，这里保真存下但不消费。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetIndex {
    /// 对象表：键是资源名（`minecraft/sounds/...ogg`），值是内容描述。
    #[serde(default)]
    pub objects: BTreeMap<String, AssetObject>,
    /// 旧版：是否要在启动期铺到 `assets/virtual/<id>/`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#virtual: Option<bool>,
    /// 旧版：是否要映射到存档目录旁的 `resources/`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_to_resources: Option<bool>,
}

impl AssetIndex {
    /// 解析索引 JSON。
    ///
    /// # Errors
    ///
    /// JSON 非法（例如镜像返回了 HTML 错误页）时返回 [`GameError::Json`]。
    pub fn from_json_slice(bytes: &[u8]) -> Result<Self, GameError> {
        serde_json::from_slice(bytes).map_err(|e| GameError::json("asset_index", e))
    }

    /// 对象数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// 是否没有对象。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// 全部对象的总字节数（进度预算与 `--dry-run` 总大小用）。
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.objects.values().map(|object| object.size).sum()
    }
}

/// 把索引展开成待下载文件清单（按资源名排序，保证计划可复现）。
///
/// # Errors
///
/// 某个对象哈希不合法（短于两字符 / 非十六进制）时返回 [`GameError::InvalidAssetHash`]。
pub fn plan(layout: &GameLayout, index: &AssetIndex) -> Result<Vec<PlanFile>, GameError> {
    let mut files = Vec::with_capacity(index.objects.len());
    for (name, object) in &index.objects {
        if !object.is_well_formed() {
            return Err(GameError::InvalidAssetHash {
                hash: object.hash.clone(),
            });
        }
        files.push(PlanFile {
            role: FileRole::Asset,
            name: name.clone(),
            url: object.url(),
            sha1: Some(object.hash.clone()),
            size: Some(object.size),
            dest: layout.asset_object(&object.hash)?,
            resource: ResourceKind::Asset,
        });
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::{plan, AssetIndex, AssetObject};
    use crate::game::layout::GameLayout;

    const HASH: &str = "0f9d6b1c0d5f4e3b2a1908070605040302010f0e";

    #[test]
    fn location_and_url_use_two_char_shard() {
        let object = AssetObject {
            hash: HASH.to_string(),
            size: 7,
        };
        assert_eq!(object.location(), format!("0f/{HASH}"));
        assert_eq!(
            object.url(),
            format!("https://resources.download.minecraft.net/0f/{HASH}")
        );
    }

    #[test]
    fn parses_index_and_plans_objects() {
        let json = format!(
            r#"{{"objects":{{"minecraft/lang/zh_cn.json":{{"hash":"{HASH}","size":42}}}}}}"#
        );
        let index = AssetIndex::from_json_slice(json.as_bytes()).expect("索引应可解析");
        assert_eq!(index.len(), 1);
        assert_eq!(index.total_bytes(), 42);

        let layout = GameLayout::new("/games/.minecraft");
        let files = plan(&layout, &index).expect("展开应成功");
        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(file.sha1.as_deref(), Some(HASH));
        assert_eq!(file.size, Some(42));
        assert!(file.dest.ends_with(format!("objects/0f/{HASH}")));
    }

    #[test]
    fn rejects_short_hash() {
        let mut index = AssetIndex::default();
        index.objects.insert(
            "a".to_string(),
            AssetObject {
                hash: "a".to_string(),
                size: 1,
            },
        );
        let layout = GameLayout::new("/games/.minecraft");
        assert!(plan(&layout, &index).is_err());
    }

    #[test]
    fn keeps_asset_keys_as_names() {
        let json = format!(
            r#"{{"objects":{{"icons/icon_16x16.png":{{"hash":"{HASH}","size":1}},"lang/en_us.json":{{"hash":"{HASH}","size":2}}}}}}"#
        );
        let index = AssetIndex::from_json_slice(json.as_bytes()).expect("索引应可解析");
        let files = plan(&GameLayout::new("/g"), &index).expect("展开应成功");
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["icons/icon_16x16.png", "lang/en_us.json"]);
    }
}
