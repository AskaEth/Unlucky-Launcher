//! 源健康度记忆：`源 × 资源类型` 的 EMA RTT / 失败率 / 连续失败。
//!
//! HMCL 没有这一层（analyse/03 §8.4 的「下载快」一栏是纯并发 + 缓存），于是同一台机器上
//! 一个常年 502 的镜像会被每个文件重试一遍。这里的规则：
//!
//! * 连续 **3** 次失败 → 冷却 **10 分钟**，期间该源排到最后（仍保留为兜底候选）；
//! * 冷却结束后自动进入**恢复探测**：下一次尝试就是一次探测，成功即清零回队，失败则重新冷却；
//! * 记录持久化到 `cache/health.json`（原子写），进程重启后仍然生效：
//!   **冷却状态迁移时立刻落盘**（进入/退出冷却这种罕见事件），其余变化标脏，
//!   由引擎在一次 `download()` / `download_bytes()` 收尾时 `flush()` 节流落盘，
//!   避免「批量下 3000 个资源 = 3000 次原子写」；
//! * 排序打分 = EMA RTT + 失败率惩罚 + 连续失败惩罚，冷却中的源加一个大惩罚。

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::health_store;
use super::types::{Candidate, ResourceKind, SOURCE_OFFICIAL};

/// 连续失败多少次进入冷却。
pub const FAILURE_THRESHOLD: u32 = 3;
/// 冷却时长（毫秒）：连续失败后的冷板凳时间。
pub const COOLDOWN_MS: u64 = 10 * 60 * 1000;
/// EMA 的平滑系数（0.3 意味着新样本占 30%）。
pub const EMA_ALPHA: f64 = 0.3;
/// 冷板凳/未知源的基础打分。
pub const SCORE_UNKNOWN: f64 = 400.0;
/// 落盘节流：两次 flush 之间的最小间隔（毫秒）。
pub const SAVE_THROTTLE_MS: u64 = 5_000;
/// 冷却中的惩罚项。
pub const SCORE_COOLDOWN_PENALTY: f64 = 1_000.0;

/// 健康度的记忆键：一个源在一种资源类型上的表现是独立的。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HealthKey {
    /// 源标识（`official` / `bmclapi` / `tuna` …）。
    pub source: String,
    /// 资源类型。
    pub kind: ResourceKind,
}

impl HealthKey {
    /// 构造键。
    #[must_use]
    pub fn new(source: impl Into<String>, kind: ResourceKind) -> Self {
        Self {
            source: source.into(),
            kind,
        }
    }
}

/// 单个 `源 × 资源类型` 的健康记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceHealth {
    /// 成功请求的 RTT 指数移动平均（毫秒）；无样本时为 0。
    pub ema_rtt_ms: f64,
    /// 参与 EMA 的样本数。
    pub samples: u64,
    /// 累计成功次数。
    pub successes: u64,
    /// 累计失败次数。
    pub failures: u64,
    /// 累计「该源没有这个资源」（4xx）。**不计失败、不进冷却**——院校镜像通常只留最新补丁，
    /// 请求旧补丁必然 404，把这种情况算成「源坏」会在一次安装里把唯一可用镜像冷却掉。
    #[serde(default)]
    pub not_found: u64,
    /// 连续失败次数。
    pub consecutive_failures: u32,
    /// 冷却截止时间（unix 毫秒，0 = 未冷却）。
    pub cooldown_until_ms: u64,
    /// 最近一次失败原因（诊断页用）。
    pub last_error: Option<String>,
    /// 最后更新时间（unix 毫秒）。
    pub updated_at_ms: u64,
}

impl Default for SourceHealth {
    fn default() -> Self {
        Self {
            ema_rtt_ms: 0.0,
            samples: 0,
            successes: 0,
            failures: 0,
            not_found: 0,
            consecutive_failures: 0,
            cooldown_until_ms: 0,
            last_error: None,
            updated_at_ms: 0,
        }
    }
}

impl SourceHealth {
    /// 记录一次成功：重置连续失败与冷却，更新 EMA RTT。
    pub fn record_success(&mut self, rtt_ms: u64, now_ms: u64) {
        self.successes += 1;
        self.consecutive_failures = 0;
        self.cooldown_until_ms = 0;
        self.last_error = None;
        self.updated_at_ms = now_ms;
        let rtt = rtt_ms as f64;
        self.ema_rtt_ms = if self.samples == 0 {
            rtt
        } else {
            EMA_ALPHA * rtt + (1.0 - EMA_ALPHA) * self.ema_rtt_ms
        };
        self.samples += 1;
    }

    /// 记录一次失败：达到阈值即进入冷却。
    pub fn record_failure(&mut self, reason: &str, now_ms: u64) {
        self.failures += 1;
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.updated_at_ms = now_ms;
        self.last_error = Some(reason.to_string());
        if self.consecutive_failures >= FAILURE_THRESHOLD {
            self.cooldown_until_ms = now_ms.saturating_add(COOLDOWN_MS);
        }
    }

    /// 记录一次「该源没有这个资源」（HTTP 4xx）：只留诊断痕迹，不动失败率与冷却。
    pub fn record_missing(&mut self, reason: &str, now_ms: u64) {
        self.not_found = self.not_found.saturating_add(1);
        self.updated_at_ms = now_ms;
        self.last_error = Some(reason.to_string());
    }

    /// 是否处于冷却期。
    #[must_use]
    pub fn is_cooling_down(&self, now_ms: u64) -> bool {
        self.cooldown_until_ms > now_ms
    }

    /// 上次冷却是否已到期（此时这次尝试就是「恢复探测」）。
    #[must_use]
    pub fn is_probe(&self, now_ms: u64) -> bool {
        self.cooldown_until_ms != 0 && self.cooldown_until_ms <= now_ms
    }

    /// 失败率（0.0~1.0）。
    #[must_use]
    pub fn failure_rate(&self) -> f64 {
        let total = self.successes + self.failures;
        if total == 0 {
            0.0
        } else {
            self.failures as f64 / total as f64
        }
    }

    /// 排序打分（越小越优先）。
    #[must_use]
    pub fn score(&self, now_ms: u64) -> f64 {
        if self.is_cooling_down(now_ms) {
            return SCORE_COOLDOWN_PENALTY + self.ema_rtt_ms;
        }
        if self.samples == 0 && self.failures == 0 {
            return SCORE_UNKNOWN;
        }
        self.ema_rtt_ms + self.failure_rate() * 500.0 + f64::from(self.consecutive_failures) * 50.0
    }
}

/// 健康度注册表（进程内共享，`Arc` 后传给引擎）。
#[derive(Debug)]
pub struct HealthRegistry {
    path: Option<PathBuf>,
    entries: Mutex<HashMap<HealthKey, SourceHealth>>,
    dirty: AtomicBool,
    last_save_ms: AtomicU64,
}

impl Default for HealthRegistry {
    fn default() -> Self {
        Self::in_memory()
    }
}

impl HealthRegistry {
    /// 纯内存实例（测试用 / 用户关掉健康度记忆时）。
    #[must_use]
    pub fn in_memory() -> Self {
        Self {
            path: None,
            entries: Mutex::new(HashMap::new()),
            dirty: AtomicBool::new(false),
            last_save_ms: AtomicU64::new(0),
        }
    }

    /// 打开持久化文件（不存在则从空表开始；损坏/超龄记录则忽略）。
    #[must_use]
    pub fn open(path: impl Into<PathBuf>, max_age_ms: u64, now_ms: u64) -> Self {
        let path = path.into();
        let entries = health_store::load(&path, max_age_ms, now_ms);
        Self {
            path: Some(path),
            entries: Mutex::new(entries),
            dirty: AtomicBool::new(false),
            last_save_ms: AtomicU64::new(now_ms),
        }
    }

    /// 按 `cache/` 根目录打开（**生产接线用这一条**）：文件固定在 `<cache>/health.json`，
    /// 保留期用默认值。
    #[must_use]
    pub fn open_in(cache_root: impl AsRef<Path>, now_ms: u64) -> Self {
        Self::open(
            cache_root.as_ref().join(health_store::FILE_NAME),
            health_store::MAX_AGE_MS,
            now_ms,
        )
    }

    /// 持久化文件路径（诊断页展示用）。
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<HealthKey, SourceHealth>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 读取某个键的记录（不存在则返回默认值）。
    #[must_use]
    pub fn health_of(&self, source: &str, kind: ResourceKind) -> SourceHealth {
        self.lock()
            .get(&HealthKey::new(source, kind))
            .cloned()
            .unwrap_or_default()
    }

    /// 记录成功；返回冷却状态是否发生变化（变化即立刻落盘）。
    pub fn record_success(
        &self,
        source: &str,
        kind: ResourceKind,
        rtt: Duration,
        now_ms: u64,
    ) -> bool {
        let changed = {
            let mut entries = self.lock();
            let health = entries.entry(HealthKey::new(source, kind)).or_default();
            let before = health.cooldown_until_ms;
            health.record_success(rtt.as_millis() as u64, now_ms);
            before != health.cooldown_until_ms
        };
        self.after_record(changed);
        changed
    }

    /// 记录失败；返回冷却状态是否发生变化（第 3 次连续失败会进入冷却 → 立刻落盘）。
    pub fn record_failure(
        &self,
        source: &str,
        kind: ResourceKind,
        reason: &str,
        now_ms: u64,
    ) -> bool {
        let changed = {
            let mut entries = self.lock();
            let health = entries.entry(HealthKey::new(source, kind)).or_default();
            let before = health.cooldown_until_ms;
            health.record_failure(reason, now_ms);
            before != health.cooldown_until_ms
        };
        self.after_record(changed);
        changed
    }

    /// 记录「该源没有这个资源」（4xx）：不计失败、不进冷却，只更新诊断字段。
    pub fn record_missing(&self, source: &str, kind: ResourceKind, reason: &str, now_ms: u64) {
        {
            let mut entries = self.lock();
            let health = entries.entry(HealthKey::new(source, kind)).or_default();
            health.record_missing(reason, now_ms);
        }
        self.after_record(false);
    }

    /// 记录后：标脏，冷却状态迁移则立刻落盘（这种事件很少，不会打爆磁盘）。
    fn after_record(&self, cooldown_changed: bool) {
        self.dirty.store(true, Ordering::Relaxed);
        if cooldown_changed {
            let _ = self.save();
        }
    }

    /// 该源当前是否在冷却期。
    #[must_use]
    pub fn is_cooling_down(&self, source: &str, kind: ResourceKind, now_ms: u64) -> bool {
        self.health_of(source, kind).is_cooling_down(now_ms)
    }

    /// 按健康度重排候选序列：健康好的在前，冷却中的排到最后（仍保留为兜底）。
    #[must_use]
    pub fn rank(
        &self,
        candidates: &[Candidate],
        kind: ResourceKind,
        now_ms: u64,
    ) -> Vec<Candidate> {
        let entries = self.lock();
        let mut scored: Vec<(f64, usize, Candidate)> = candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| {
                let score = entries
                    .get(&HealthKey::new(candidate.source.clone(), kind))
                    .map_or(SCORE_UNKNOWN, |health| health.score(now_ms));
                (score, index, candidate.clone())
            })
            .collect();
        scored.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        scored
            .into_iter()
            .map(|(_, _, candidate)| candidate)
            .collect()
    }

    /// 快照（诊断页/测试用），按源名排序保证稳定。
    #[must_use]
    pub fn snapshot(&self) -> Vec<(HealthKey, SourceHealth)> {
        let entries = self.lock();
        let mut rows: Vec<(HealthKey, SourceHealth)> = entries
            .iter()
            .map(|(key, health)| (key.clone(), health.clone()))
            .collect();
        rows.sort_by(|a, b| {
            a.0.source
                .cmp(&b.0.source)
                .then(a.0.kind.as_str().cmp(b.0.kind.as_str()))
        });
        rows
    }

    /// 清空（设置页「重置下载源健康度」）。
    pub fn clear(&self) {
        self.lock().clear();
    }

    /// 无条件原子持久化到 `cache/health.json`（纯内存实例上是空操作）。
    pub fn save(&self) -> io::Result<()> {
        let Some(path) = self.path.clone() else {
            self.dirty.store(false, Ordering::Relaxed);
            return Ok(());
        };
        health_store::store(&path, &self.lock())?;
        self.dirty.store(false, Ordering::Relaxed);
        self.last_save_ms.store(now_wall_ms(), Ordering::Relaxed);
        Ok(())
    }

    /// 下载收尾时的节流落盘：只有「脏」且距上次落盘超过 [`SAVE_THROTTLE_MS`] 才写。
    ///
    /// 引擎在每个 `download()` / `download_bytes()` 结束时调它——批量下载因此最多
    /// 每 5 秒写一次盘，而不是每个文件一次。
    pub fn flush(&self, now_ms: u64) -> io::Result<()> {
        if self.path.is_none() || !self.dirty.load(Ordering::Relaxed) {
            return Ok(());
        }
        let last = self.last_save_ms.load(Ordering::Relaxed);
        if now_ms.saturating_sub(last) < SAVE_THROTTLE_MS {
            return Ok(());
        }
        self.save()
    }

    /// 是否有未落盘的改动（诊断页/测试用）。
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }
}

/// 官方源的空健康记录便捷构造（game 层拼候选时常用）。
#[must_use]
pub fn official_key(kind: ResourceKind) -> HealthKey {
    HealthKey::new(SOURCE_OFFICIAL, kind)
}

/// 记录落盘时刻用的墙钟（不为它引入时钟抽象，测试走显式 `now_ms` 参数）。
fn now_wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}
