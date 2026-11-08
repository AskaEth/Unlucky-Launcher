//! 内容寻址缓存：两级分片 + 「记录信任」命中快路径。
//!
//! 布局承设计文档 `04 §2` 与 analyse/03 §5.7：`<cache>/sha1/<hash[0..2]>/<hash>`（两级分片，
//! 避开 NTFS 单目录几十万条目塌陷）。
//!
//! 修 HMCL 的坑（analyse/03 §8.2 第 7 点）：它每次命中都重算整个文件的 SHA-1，
//! 3000 个资源对象意味着启动期几百 MB 的无谓读盘。这里写入时记录 `{hash, size, mtime}`，
//! **此后信任记录**：size/mtime 都没变就直接命中；只有元数据变了才重算一次摘要并刷新记录。

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::atomic::write_atomic;
use super::types::{HashAlgo, Integrity};
use super::verify;

/// 索引文件名（放在缓存根下）。
pub const INDEX_FILE: &str = "index.json";
/// 索引格式版本。
pub const INDEX_VERSION: u32 = 1;

/// 一条缓存记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheRecord {
    /// 算法。
    pub algo: HashAlgo,
    /// 摘要（小写十六进制）。
    pub hash: String,
    /// 字节数。
    pub size: u64,
    /// 文件修改时间（unix 毫秒）——记录信任的锚点。
    pub mtime_ms: u64,
    /// 写入缓存时间。
    pub cached_at_ms: u64,
    /// 最近一次真正校验的时间（0 = 从未校验，走记录信任）。
    pub last_verified_ms: u64,
}

/// 缓存统计（诊断页）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheStats {
    /// 条目数。
    pub entries: usize,
    /// 总字节数。
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheIndex {
    version: u32,
    #[serde(default)]
    entries: BTreeMap<String, CacheRecord>,
}

impl Default for CacheIndex {
    fn default() -> Self {
        Self {
            version: INDEX_VERSION,
            entries: BTreeMap::new(),
        }
    }
}

/// 内容寻址缓存。
#[derive(Debug)]
pub struct ContentCache {
    root: PathBuf,
    index: Mutex<CacheIndex>,
}

impl ContentCache {
    /// 打开（或初始化）缓存根目录；索引损坏时从空表重建（缓存随时可丢，不阻断启动）。
    #[must_use]
    pub fn open(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let index_path = root.join(INDEX_FILE);
        let index = std::fs::read(&index_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<CacheIndex>(&bytes).ok())
            .unwrap_or_default();
        Self {
            root,
            index: Mutex::new(index),
        }
    }

    /// 缓存根目录。
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 某个摘要对应的缓存路径（两级分片）。
    #[must_use]
    pub fn path_for(&self, algo: HashAlgo, hash: &str) -> PathBuf {
        let shard = hash.get(..2).unwrap_or(hash);
        self.root.join(algo.as_str()).join(shard).join(hash)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CacheIndex> {
        self.index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn key(algo: HashAlgo, hash: &str) -> String {
        format!("{}:{hash}", algo.as_str())
    }

    /// 命中查询（记录信任：size + mtime 未变则**不重算**摘要）。
    #[must_use]
    pub fn lookup(&self, integrity: &Integrity, now_ms: u64) -> Option<PathBuf> {
        let path = self.path_for(integrity.algo, &integrity.hex);
        let record = self
            .lock()
            .entries
            .get(&Self::key(integrity.algo, &integrity.hex))
            .cloned()?;
        let Ok(meta) = std::fs::metadata(&path) else {
            // 文件被删：记录作废，避免每次命中都白查一次磁盘。
            self.forget(integrity);
            return None;
        };
        if !meta.is_file() || meta.len() != record.size {
            // 文件被删/被截断：记录作废。
            self.forget(integrity);
            return None;
        }
        let mtime_ms = mtime_ms(&meta);
        if mtime_ms == record.mtime_ms {
            return Some(path);
        }
        // 元数据变了（外部动过文件）：重算一次摘要再决定。
        let Ok(actual) = verify::digest_file(&path, integrity.algo) else {
            self.forget(integrity);
            return None;
        };
        if !verify::matches(&integrity.hex, &actual) {
            self.forget(integrity);
            return None;
        }
        self.refresh_record(integrity, &meta, now_ms);
        Some(path)
    }

    /// 强制校验的命中查询（抽样/定期后台校验用，慢但可信）。
    #[must_use]
    pub fn lookup_verified(&self, integrity: &Integrity, now_ms: u64) -> Option<PathBuf> {
        let path = self.path_for(integrity.algo, &integrity.hex);
        let meta = std::fs::metadata(&path).ok()?;
        if !meta.is_file() {
            self.forget(integrity);
            return None;
        }
        let actual = verify::digest_file(&path, integrity.algo).ok()?;
        if !verify::matches(&integrity.hex, &actual) {
            self.forget(integrity);
            return None;
        }
        self.refresh_record(integrity, &meta, now_ms);
        Some(path)
    }

    /// 把已校验通过的文件放进缓存（硬链接优先，跨卷退化为复制）。
    pub fn insert(&self, source: &Path, integrity: &Integrity, now_ms: u64) -> io::Result<PathBuf> {
        let target = self.path_for(integrity.algo, &integrity.hex);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if !target.is_file() {
            link_or_copy(source, &target)?;
        }
        self.register(&target, integrity, now_ms)?;
        Ok(target)
    }

    /// 登记一个已在缓存位的文件（`assets` 已在游戏目录里的补登记场景）。
    pub fn register(&self, path: &Path, integrity: &Integrity, now_ms: u64) -> io::Result<()> {
        let meta = std::fs::metadata(path)?;
        let record = CacheRecord {
            algo: integrity.algo,
            hash: integrity.hex.clone(),
            size: meta.len(),
            mtime_ms: mtime_ms(&meta),
            cached_at_ms: now_ms,
            last_verified_ms: now_ms,
        };
        self.lock()
            .entries
            .insert(Self::key(integrity.algo, &integrity.hex), record);
        Ok(())
    }

    /// 删除一条缓存（含文件）。
    pub fn remove(&self, integrity: &Integrity) -> io::Result<()> {
        let path = self.path_for(integrity.algo, &integrity.hex);
        self.forget(integrity);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    }

    fn forget(&self, integrity: &Integrity) {
        self.lock()
            .entries
            .remove(&Self::key(integrity.algo, &integrity.hex));
    }

    fn refresh_record(&self, integrity: &Integrity, meta: &std::fs::Metadata, now_ms: u64) {
        if let Some(record) = self
            .lock()
            .entries
            .get_mut(&Self::key(integrity.algo, &integrity.hex))
        {
            record.size = meta.len();
            record.mtime_ms = mtime_ms(meta);
            record.last_verified_ms = now_ms;
        }
    }

    /// 统计。
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        let index = self.lock();
        CacheStats {
            entries: index.entries.len(),
            total_bytes: index.entries.values().map(|record| record.size).sum(),
        }
    }

    /// 索引条目数（`stats` 的便捷版本）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    /// 索引是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 原子持久化索引（`cache/index.json`）。
    pub fn persist(&self) -> io::Result<()> {
        let json = {
            let index = self.lock();
            serde_json::to_vec_pretty(&*index).map_err(io::Error::other)?
        };
        write_atomic(&self.root.join(INDEX_FILE), &json)
    }
}

/// 硬链接优先，失败退化为复制（跨卷/权限不足时）。
fn link_or_copy(source: &Path, target: &Path) -> io::Result<()> {
    match std::fs::hard_link(source, target) {
        Ok(()) => Ok(()),
        Err(_) => std::fs::copy(source, target).map(|_| ()),
    }
}

fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

/// 当前 unix 毫秒（引擎统一取一次再往下传，避免测试里到处读时钟）。
#[must_use]
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}
