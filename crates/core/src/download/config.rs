//! 引擎配置：设置层（core::setting）直接映射到这里。

use std::time::Duration;

use super::backoff::BackoffPolicy;
use super::chunked::ChunkedConfig;
use super::types::ResumePolicy;

/// 引擎配置。
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// 全局并发许可数（tokio `Semaphore`），默认 64。
    pub max_concurrency: usize,
    /// 每个源的尝试次数上限，默认 3。
    pub max_attempts_per_source: u32,
    /// 连接超时，默认 10s。
    pub connect_timeout: Duration,
    /// 单次读超时（读空闲判定，不是整体超时），默认 30s。
    pub read_timeout: Duration,
    /// User-Agent。
    pub user_agent: String,
    /// 进度事件节流间隔，默认 250ms。
    pub progress_throttle: Duration,
    /// 退避策略。
    pub backoff: BackoffPolicy,
    /// 分块下载配置。
    pub chunked: ChunkedConfig,
    /// 默认续传策略（请求级 `Never` 优先级更高）。
    pub resume: ResumePolicy,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 64,
            max_attempts_per_source: 3,
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(30),
            user_agent: concat!("UnluckyLauncher/", env!("CARGO_PKG_VERSION")).to_string(),
            progress_throttle: Duration::from_millis(250),
            backoff: BackoffPolicy::default(),
            chunked: ChunkedConfig::default(),
            resume: ResumePolicy::Conservative,
        }
    }
}
