//! `.ULL/` 四分区布局——所有模块取路径的唯一入口，禁止各自手拼路径。
//!
//! 布局依据 `DevDocs/PlanDoc/04-data-and-config.md` §2（承 HMCL 四分区）：
//! `config/`（schema 化 JSON）、`data/`（可重建但优先保留）、`cache/`（随时可删）、`logs/`。

use std::path::{Path, PathBuf};

use super::error::{io_at, PlatformError, PlatformResult};

/// Portable 模式下启动器数据目录名（exe 同目录）。
pub const LAUNCHER_HOME_DIR: &str = ".ULL";
/// 游戏数据目录名。
pub const GAME_HOME_DIR: &str = ".minecraft";
/// 分区：配置。
pub const CONFIG_DIR: &str = "config";
/// 分区：状态数据。
pub const DATA_DIR: &str = "data";
/// 分区：纯缓存。
pub const CACHE_DIR: &str = "cache";
/// 分区：启动器日志。
pub const LOGS_DIR: &str = "logs";

/// 全局设置文件名。
pub const LAUNCHER_CONFIG_FILE: &str = "launcher.json";
/// 实例档案列表文件名。
pub const PROFILES_CONFIG_FILE: &str = "profiles.json";
/// 每实例设置所在子目录。
pub const INSTANCES_DIR: &str = "instances";
/// 迁移后留在旧目录的指针文件名。
pub const REDIRECT_FILE: &str = "redirect.json";
/// 目录迁移收据文件名（写在目标 `config/`）。
pub const MIGRATION_RECEIPT_FILE: &str = "migration-receipt.json";

/// `.ULL/` 下的四个分区。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Partition {
    /// `config/`：配置（迁移时优先搬）。
    Config,
    /// `data/`：状态数据（不可丢）。
    Data,
    /// `cache/`：纯缓存（可丢，迁移可选）。
    Cache,
    /// `logs/`：启动器日志。
    Logs,
}

impl Partition {
    /// 全部四个分区。
    pub const ALL: [Self; 4] = [Self::Config, Self::Data, Self::Cache, Self::Logs];

    /// 分区目录名。
    #[must_use]
    pub const fn dir_name(&self) -> &'static str {
        match self {
            Self::Config => CONFIG_DIR,
            Self::Data => DATA_DIR,
            Self::Cache => CACHE_DIR,
            Self::Logs => LOGS_DIR,
        }
    }

    /// 稳定名（日志 / 收据用）。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        self.dir_name()
    }

    /// 是否为可丢弃分区（迁移时可跳过）。
    #[must_use]
    pub const fn is_disposable(&self) -> bool {
        matches!(self, Self::Cache)
    }
}

/// 一个 launcher home（`.ULL` 的等价物）的路径视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherLayout {
    home: PathBuf,
}

impl LauncherLayout {
    /// 绑定到某个 home 目录（不创建任何东西，创建用 [`Self::ensure`]）。
    #[must_use]
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self { home: home.into() }
    }

    /// home 根目录。
    #[must_use]
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// 任一分区目录。
    #[must_use]
    pub fn partition(&self, partition: Partition) -> PathBuf {
        self.home.join(partition.dir_name())
    }

    /// `config/`。
    #[must_use]
    pub fn config(&self) -> PathBuf {
        self.partition(Partition::Config)
    }

    /// `data/`。
    #[must_use]
    pub fn data(&self) -> PathBuf {
        self.partition(Partition::Data)
    }

    /// `cache/`。
    #[must_use]
    pub fn cache(&self) -> PathBuf {
        self.partition(Partition::Cache)
    }

    /// `logs/`。
    #[must_use]
    pub fn logs(&self) -> PathBuf {
        self.partition(Partition::Logs)
    }

    /// `config/<name>`。
    #[must_use]
    pub fn config_file(&self, name: &str) -> PathBuf {
        self.config().join(name)
    }

    /// `config/launcher.json`。
    #[must_use]
    pub fn launcher_config(&self) -> PathBuf {
        self.config_file(LAUNCHER_CONFIG_FILE)
    }

    /// `config/profiles.json`。
    #[must_use]
    pub fn profiles_config(&self) -> PathBuf {
        self.config_file(PROFILES_CONFIG_FILE)
    }

    /// `config/instances/<id>.json`。
    ///
    /// # Errors
    /// 实例 id 只允许 `[A-Za-z0-9_-]`（正常是稳定 UUID），否则返回
    /// [`super::error::PlatformError::InvalidInstanceId`]，挡掉 `../` 穿越。
    pub fn instance_config(&self, id: &str) -> PlatformResult<PathBuf> {
        if !is_valid_instance_id(id) {
            return Err(PlatformError::InvalidInstanceId { id: id.to_owned() });
        }
        Ok(self.config().join(INSTANCES_DIR).join(format!("{id}.json")))
    }

    /// `cache/sha1/<h2>`——内容寻址缓存的两级分片（避开 NTFS 单目录塌陷）。
    #[must_use]
    pub fn sha1_cache(&self, shard: &str) -> PathBuf {
        self.cache().join("sha1").join(shard)
    }

    /// `cache/etag.json`——共享 ETag 表。
    #[must_use]
    pub fn etag_cache(&self) -> PathBuf {
        self.cache().join("etag.json")
    }

    /// `cache/meta/`——列表 API 响应缓存（带 TTL）。
    #[must_use]
    pub fn meta_cache(&self) -> PathBuf {
        self.cache().join("meta")
    }

    /// `data/java-managed/`——自动下载的 JDK。
    #[must_use]
    pub fn java_managed(&self) -> PathBuf {
        self.data().join("java-managed")
    }

    /// `data/auth/`——账户元数据与凭据。
    #[must_use]
    pub fn auth_data(&self) -> PathBuf {
        self.data().join("auth")
    }

    /// `data/agent/sessions/`——AI 会话 jsonl 与审计日志。
    #[must_use]
    pub fn agent_sessions(&self) -> PathBuf {
        self.data().join("agent").join("sessions")
    }

    /// `<home>/redirect.json`——「我已迁走」指针。
    #[must_use]
    pub fn redirect_path(&self) -> PathBuf {
        self.home.join(REDIRECT_FILE)
    }

    /// `config/migration-receipt.json`——迁移收据。
    #[must_use]
    pub fn migration_receipt(&self) -> PathBuf {
        self.config_file(MIGRATION_RECEIPT_FILE)
    }

    /// 一次性建齐四分区。
    ///
    /// # Errors
    /// 目录创建失败时返回带路径的 [`super::error::PlatformError::Io`]。
    pub fn ensure(&self) -> PlatformResult<()> {
        for partition in Partition::ALL {
            let dir = self.partition(partition);
            std::fs::create_dir_all(&dir).map_err(io_at(&dir))?;
        }
        Ok(())
    }
}

/// 实例 id 白名单：字母数字与 `_`/`-`。
#[must_use]
pub fn is_valid_instance_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}
