//! 引擎的只读查询面：**从 `engine.rs` 拆出只为守 400 行硬线**（M289 加失速诊断前先纯搬移，逐字节未改）。
//!
//! 这里只有「读」——配置、并发闸门探针、HTTP 客户端、地区档案、镜像表、健康度、内容缓存。
//! 编排（候选循环、退避、落盘）仍在 [`super`]。

use super::super::cache::ContentCache;
use super::super::config::EngineConfig;
use super::super::health::HealthRegistry;
use super::super::mirrors::MirrorTable;
use super::super::region::RegionProfile;
use super::DownloadEngine;

impl DownloadEngine {
    /// 配置。
    #[must_use]
    pub fn config(&self) -> &EngineConfig {
        &self.cfg
    }

    /// **真实并发闸门**此刻的许可数（探针，M193）：读 [`Self::acquire`] 会去抢的那张
    /// `Semaphore`，**不是** `config().max_concurrency`。有下载在飞时偏小，空闲等于容量。
    #[must_use]
    pub fn concurrency_probe(&self) -> usize {
        self.sem.available_permits()
    }

    /// HTTP 客户端（同一条连接池与超时配置）：给「一锤子查询」用（如 Adoptium 版本查询），
    /// 没必要为了一个 GET 再造一个 client。
    #[must_use]
    pub fn client(&self) -> &reqwest::Client {
        &self.http
    }

    /// 代理这一档**没生效**的原因（`None` = 按 `config().proxy` 生效）。
    ///
    /// 只有一种情况会给 `Some`：`manual` 的地址解析不出来 ⇒ 客户端回落**直连**（见
    /// [`DownloadEngine::new`]），而 `config().proxy` 仍是用户写的 `manual` —— 配置不撒谎，实际行为
    /// 由这个字段说明。宿主该把它打给用户（CLI 的 `download::engine` 打一行 stderr），否则「填了代理
    /// 但没走代理」又成了没法解释的半真话。
    #[must_use]
    pub fn proxy_problem(&self) -> Option<&str> {
        self.proxy_problem.as_deref()
    }

    /// 当前地区档案（设置页展示）。
    #[must_use]
    pub fn region(&self) -> &RegionProfile {
        &self.region
    }

    /// 镜像表（`game::java` 的 Oracle / MS / Zulu 分发表在这里查）。
    #[must_use]
    pub fn mirrors(&self) -> &MirrorTable {
        &self.mirrors
    }

    /// 健康度注册表（诊断页）。
    #[must_use]
    pub fn health(&self) -> &HealthRegistry {
        &self.health
    }

    /// 内容寻址缓存。
    #[must_use]
    pub fn cache(&self) -> &ContentCache {
        &self.cache
    }
}
