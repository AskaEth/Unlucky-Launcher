//! 账户存储：`data/auth/` 下的**元数据 / 凭据分文件**落盘（内存视图与增删）。
//!
//! 契约（`PlanDoc/07-accounts.md` §2、`PlanDoc/04-data-and-config.md`）：
//! 1. **分文件**：`accounts.json`（可公开）+ `credentials.dat`（凭据）。列账户不碰凭据文件；
//! 2. **先写凭据、再写元数据**（HMCL 安全顺序）——元数据永不指向不存在的凭据；
//! 3. **原子写**：走 `platform::atomic::write_atomic`（tmp → fsync → rename）；
//! 4. **容错**：单条坏记录只跳过该条并留 warning，绝不整体加载失败；
//! 5. **孤儿清理**：`save()` 只写「元数据里还存在」的凭据。
//!
//! 落盘细节（payload 形状、权限收紧）在同目录的 [`payload`] 子模块，本文件只管模型与不变量。
//!
//! 保护级别：本期 `plain-v1`（分文件 + 权限 + 全链脱敏）。OS keychain / AES-256-GCM
//! 分级是 07 §2 的第 1–2 条，需要 keyring/aes-gcm/argon2 的依赖决策，属后续 mission。

mod payload;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::auth::account::{AccountId, AccountMeta, Credentials};
use crate::auth::error::{AuthError, AuthResult};

/// 元数据文件名（`data/auth/accounts.json`）。
pub const ACCOUNTS_FILE: &str = "accounts.json";
/// 凭据文件名（`data/auth/credentials.dat`）。
pub const CREDENTIALS_FILE: &str = "credentials.dat";
/// 元数据 schema 版本。
pub const AUTH_SCHEMA_VERSION: u32 = 1;
/// 本期凭据保护级别（明文 + 分文件 + 权限 + 脱敏）。
pub const PROTECTION_PLAIN_V1: &str = "plain-v1";

/// `data/auth/` 的两个文件位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPaths {
    root: PathBuf,
}

impl AuthPaths {
    /// 以数据分区为根（`LauncherLayout::auth_data()`）。
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 根目录。
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 元数据文件。
    #[must_use]
    pub fn accounts_file(&self) -> PathBuf {
        self.root.join(ACCOUNTS_FILE)
    }

    /// 凭据文件。
    #[must_use]
    pub fn credentials_file(&self) -> PathBuf {
        self.root.join(CREDENTIALS_FILE)
    }
}

/// 账户库（内存视图 + 落盘）。
#[derive(Debug, Clone)]
pub struct AccountStore {
    paths: AuthPaths,
    accounts: Vec<AccountMeta>,
    credentials: BTreeMap<String, Credentials>,
    warnings: Vec<String>,
}

impl AccountStore {
    /// 打开（不存在就是空库；坏记录进 warnings）。
    #[must_use]
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self::open_paths(AuthPaths::new(root))
    }

    /// 打开指定文件位置。
    #[must_use]
    pub fn open_paths(paths: AuthPaths) -> Self {
        let mut warnings = Vec::new();
        let mut accounts = payload::read_accounts(&paths.accounts_file(), &mut warnings);
        accounts.sort_by_key(|meta| meta.created_at_ms);
        let known: Vec<String> = accounts
            .iter()
            .map(|meta| meta.id.as_str().to_string())
            .collect();
        let credentials =
            payload::read_credentials(&paths.credentials_file(), &known, &mut warnings);
        Self {
            paths,
            accounts,
            credentials,
            warnings,
        }
    }

    /// 文件位置。
    #[must_use]
    pub fn paths(&self) -> &AuthPaths {
        &self.paths
    }

    /// 账户列表（按创建时间排序，展示顺序稳定）。
    #[must_use]
    pub fn accounts(&self) -> &[AccountMeta] {
        &self.accounts
    }

    /// 加载过程中的告警（坏记录、孤儿凭据等）。
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// 是否没有任何账户。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    /// 按名字 / 登录名 / 条目 id 查找（大小写不敏感）。
    #[must_use]
    pub fn find(&self, key: &str) -> Option<&AccountMeta> {
        self.accounts.iter().find(|meta| meta.matches(key))
    }

    /// 取某条目的凭据。
    #[must_use]
    pub fn credentials(&self, id: &AccountId) -> Option<&Credentials> {
        self.credentials.get(id.as_str())
    }

    /// 取某条目的「元数据 + 凭据」（找不到凭据时报错）。
    ///
    /// # Errors
    ///
    /// 条目不存在 → [`AuthError::NotFound`]；条目在但凭据缺失 → [`AuthError::Malformed`]
    /// （说明文件被手工删过，别静默当成「没登录」）。
    pub fn entry(&self, id: &AccountId) -> AuthResult<(&AccountMeta, &Credentials)> {
        let meta = self
            .accounts
            .iter()
            .find(|meta| meta.id == *id)
            .ok_or_else(|| AuthError::NotFound {
                account: id.to_string(),
            })?;
        let credentials = self
            .credentials
            .get(id.as_str())
            .ok_or_else(|| AuthError::Malformed(format!("账户 {id} 的凭据缺失")))?;
        Ok((meta, credentials))
    }

    /// 新增账户（内存）。同名同轨冲突时报错——`--account <名字>` 才有唯一解。
    ///
    /// # Errors
    ///
    /// 重复、条目 id 重复、轨道与凭据不符都返回错误。
    pub fn insert(&mut self, meta: AccountMeta, credentials: Credentials) -> AuthResult<()> {
        meta.check_credentials(&credentials)?;
        if self.accounts.iter().any(|existing| existing.id == meta.id) {
            return Err(AuthError::Conflict {
                account: meta.id.to_string(),
            });
        }
        let duplicate = self
            .accounts
            .iter()
            .any(|existing| existing.kind == meta.kind && existing.matches(meta.display_name()));
        if duplicate {
            return Err(AuthError::Conflict {
                account: meta.display_name().to_string(),
            });
        }
        self.credentials
            .insert(meta.id.as_str().to_string(), credentials);
        self.accounts.push(meta);
        self.accounts.sort_by_key(|meta| meta.created_at_ms);
        Ok(())
    }

    /// 删除账户（连同凭据）。
    ///
    /// # Errors
    ///
    /// 找不到时返回 [`AuthError::NotFound`]。
    pub fn remove(&mut self, key: &str) -> AuthResult<AccountMeta> {
        let index = self
            .accounts
            .iter()
            .position(|meta| meta.matches(key))
            .ok_or_else(|| AuthError::NotFound {
                account: key.to_string(),
            })?;
        let meta = self.accounts.remove(index);
        self.credentials.remove(meta.id.as_str());
        Ok(meta)
    }

    /// 更新某条目的凭据（刷新后轮换 token 用）。
    ///
    /// # Errors
    ///
    /// 条目不存在或轨道不符时返回错误。
    pub fn update_credentials(
        &mut self,
        id: &AccountId,
        credentials: Credentials,
    ) -> AuthResult<()> {
        let meta = self
            .accounts
            .iter()
            .find(|meta| meta.id == *id)
            .ok_or_else(|| AuthError::NotFound {
                account: id.to_string(),
            })?;
        meta.check_credentials(&credentials)?;
        self.credentials
            .insert(id.as_str().to_string(), credentials);
        Ok(())
    }

    /// 同步某条目的元数据字段（刷新补齐了玩家名之后写回）。
    ///
    /// # Errors
    ///
    /// 条目不存在时返回 [`AuthError::NotFound`]。
    pub fn update_meta(&mut self, meta: AccountMeta) -> AuthResult<()> {
        let slot = self
            .accounts
            .iter_mut()
            .find(|existing| existing.id == meta.id)
            .ok_or_else(|| AuthError::NotFound {
                account: meta.id.to_string(),
            })?;
        *slot = meta;
        Ok(())
    }

    /// 落盘：**先凭据、后元数据**。
    ///
    /// # Errors
    ///
    /// 两个文件任一写失败都返回 [`AuthError::Store`]。
    pub fn save(&self) -> AuthResult<()> {
        payload::write(&self.paths, &self.accounts, &self.credentials)
    }
}
