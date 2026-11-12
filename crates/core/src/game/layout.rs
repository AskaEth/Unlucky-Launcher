//! `.minecraft` 实例目录布局：`versions/` `libraries/` `assets/` + ULL 扩展 `.ull/`。
//!
//! 依据：`DevDocs/analyse/03-version-download.md` §6.8（目录布局速查）、
//! `DevDocs/PlanDoc/04-data-and-config.md` §2（`.minecraft/` 遵循 Mojang 标准 +
//! ULL 扩展 `.ull/`）。
//!
//! 布局是**纯路径计算**：不建目录、不写文件。所有路径规则集中在这里，
//! 这样将来要支持 MultiMC 家族布局（`instances/<id>/minecraft/`）只需再加一个实现
//! （analyse/01 §3.3 的 `GameRepositoryLayout` 抽象意义）。

use std::path::{Path, PathBuf};

use super::error::GameError;
use super::library::MavenCoordinate;

/// 一个游戏根目录（通常就是 `.minecraft/`）的布局规则。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameLayout {
    root: PathBuf,
}

impl GameLayout {
    /// 以给定根目录构造布局。
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `versions/`。
    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }

    /// `libraries/`。
    pub fn libraries_dir(&self) -> PathBuf {
        self.root.join("libraries")
    }

    /// `assets/`。
    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }

    /// `assets/indexes/`。
    pub fn indexes_dir(&self) -> PathBuf {
        self.assets_dir().join("indexes")
    }

    /// `assets/objects/`（两级分片，避开 NTFS 单目录塌陷）。
    pub fn objects_dir(&self) -> PathBuf {
        self.assets_dir().join("objects")
    }

    /// `assets/log_configs/`（日志配置缓存）。
    pub fn log_configs_dir(&self) -> PathBuf {
        self.assets_dir().join("log_configs")
    }

    /// 实例根目录 `versions/<id>/`。
    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions_dir().join(id)
    }

    /// 实例清单 `versions/<id>/<id>.json`。
    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }

    /// 实例 jar `versions/<id>/<id>.jar`。
    pub fn version_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.jar"))
    }

    /// natives 解压目录 `versions/<id>/natives/`（ULL 约定，HMCL 用 `natives/<ver>`）。
    pub fn natives_dir(&self, id: &str) -> PathBuf {
        self.version_dir(id).join("natives")
    }

    /// 实例私有状态目录 `versions/<id>/.ull/`（上次会话日志索引、安装收据等）。
    pub fn version_state_dir(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(".ull")
    }

    /// 实例私有库目录 `versions/<id>/libraries/`（HMCL `hint=local` 的落点）。
    pub fn version_libraries_dir(&self, id: &str) -> PathBuf {
        self.version_dir(id).join("libraries")
    }

    /// 启动器全局私有目录 `<root>/.ull/`。
    pub fn ull_root(&self) -> PathBuf {
        self.root.join(".ull")
    }

    /// 资源索引 `assets/indexes/<assetsId>.json`。
    pub fn asset_index(&self, assets_id: &str) -> PathBuf {
        self.indexes_dir().join(format!("{assets_id}.json"))
    }

    /// 资源对象 `assets/objects/<hash[0..2]>/<hash>`。
    ///
    /// # Errors
    ///
    /// 哈希短于两个字符时返回 [`GameError::InvalidAssetHash`]。
    pub fn asset_object(&self, hash: &str) -> Result<PathBuf, GameError> {
        let shard = hash.get(0..2).ok_or_else(|| GameError::InvalidAssetHash {
            hash: hash.to_owned(),
        })?;
        Ok(self.objects_dir().join(shard).join(hash))
    }

    /// 公共库 `libraries/<group path>/<artifact>-<version>[-<classifier>].<ext>`。
    pub fn library(&self, coordinate: &MavenCoordinate) -> PathBuf {
        self.libraries_dir().join(coordinate.relative_path())
    }

    /// 实例私有库 `versions/<id>/libraries/...`。
    pub fn version_library(&self, version_id: &str, coordinate: &MavenCoordinate) -> PathBuf {
        self.version_libraries_dir(version_id)
            .join(coordinate.relative_path())
    }

    /// 日志配置文件 `assets/log_configs/<id>`。
    pub fn log_config(&self, file_id: &str) -> PathBuf {
        self.log_configs_dir().join(file_id)
    }

    /// 该版本是否已安装（判据：清单文件存在）。
    pub fn is_version_installed(&self, id: &str) -> bool {
        self.version_json(id).is_file()
    }

    /// 列出已安装的版本 id（有 `versions/<id>/<id>.json` 的目录），按字典序排列。
    ///
    /// # Errors
    ///
    /// 读 `versions/` 失败时返回 [`GameError::Io`]；目录不存在视为空列表。
    pub fn installed_versions(&self) -> Result<Vec<String>, GameError> {
        let dir = self.versions_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(GameError::io(&dir, e)),
        };
        let mut ids = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| GameError::io(&dir, e))?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if path.join(format!("{name}.json")).is_file() {
                ids.push(name.to_owned());
            }
        }
        ids.sort();
        Ok(ids)
    }
}
