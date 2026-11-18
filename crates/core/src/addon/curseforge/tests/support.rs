//! 测试替身：把录制好的响应喂给客户端，并记下**每条请求的 URL 与请求头**。
//!
//! 替身**只做注入点该做的事**：按前缀回放、记录实际发出去的字节。不校验、不重试、不判断
//! 「该不该成功」——那些都在被测代码里，否则用例只是「验证桩自己」。
//!
//! 记请求头是为了两件事：证明 `x-api-key` 真的发出去了（否则「有 Key」与「没 Key」在用例里等价），
//! 以及证明 Key **没有**出现在任何错误串里（两件事必须分别有用例，只做一个会漏一半）。

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use crate::addon::curseforge::{ApiKey, CfError, CfFuture, CfResponse, CfTransport, RetryPolicy};

/// 一条被记下来的请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Seen {
    /// 请求的 URL。
    pub url: String,
    /// 发出去的请求头（名, 值），**原样**记录：用例要证明 Key 真的在里头。
    pub headers: Vec<(String, String)>,
}

/// 按 URL 前缀回放的传输层。
///
/// 每个前缀挂一串回答：第 N 次命中取第 N 条，用完了就**一直重复最后一条**。这一条就够表达
/// 「先 429 再 200」这类序列，不需要第二个替身。
pub(super) struct FixtureTransport {
    routes: BTreeMap<String, Vec<Result<CfResponse, CfError>>>,
    counters: Mutex<BTreeMap<String, usize>>,
    seen: Mutex<Vec<Seen>>,
}

impl FixtureTransport {
    /// 空路由表。
    pub(super) fn new() -> Self {
        Self {
            routes: BTreeMap::new(),
            counters: Mutex::new(BTreeMap::new()),
            seen: Mutex::new(Vec::new()),
        }
    }

    /// 挂一条 200 + 正文。
    pub(super) fn with(mut self, prefix: &str, body: &str) -> Self {
        self.routes.insert(
            prefix.to_string(),
            vec![Ok(CfResponse::new(200, None, body))],
        );
        self
    }

    /// 挂一条任意状态的回答。
    pub(super) fn with_status(
        mut self,
        prefix: &str,
        status: u16,
        retry_after: Option<u64>,
    ) -> Self {
        self.routes.insert(
            prefix.to_string(),
            vec![Ok(CfResponse::new(status, retry_after, "{}"))],
        );
        self
    }

    /// 挂一串回答（按调用顺序取，最后一条重复）。
    pub(super) fn with_sequence(mut self, prefix: &str, replies: Vec<CfResponse>) -> Self {
        self.routes
            .insert(prefix.to_string(), replies.into_iter().map(Ok).collect());
        self
    }

    /// 挂一条「传输层就失败了」（连不上 / 超时）。
    pub(super) fn with_error(mut self, prefix: &str, error: CfError) -> Self {
        self.routes.insert(prefix.to_string(), vec![Err(error)]);
        self
    }

    /// 实际发出去的请求（按顺序）。
    pub(super) fn seen(&self) -> Vec<Seen> {
        self.seen.lock().expect("锁没中毒").clone()
    }

    /// 发了几次请求。
    pub(super) fn calls(&self) -> usize {
        self.seen.lock().expect("锁没中毒").len()
    }
}

impl CfTransport for FixtureTransport {
    fn get<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
    ) -> CfFuture<'a, Result<CfResponse, CfError>> {
        Box::pin(async move {
            self.seen.lock().expect("锁没中毒").push(Seen {
                url: url.to_string(),
                headers: headers.to_vec(),
            });
            // 最长前缀优先：`/mods/394468` 与 `/mods/394468/files` 不能互相抢。
            let hit = self
                .routes
                .iter()
                .filter(|(prefix, _)| url.starts_with(prefix.as_str()))
                .max_by_key(|(prefix, _)| prefix.len())
                .map(|(prefix, replies)| (prefix.clone(), replies.clone()));
            let Some((prefix, replies)) = hit else {
                // 没挂路由是**用例的错**，所以给 Malformed 而不是更"正常"的类别：
                // 若这里回 NotConfigured，一条「缺 Key」用例会因为路由没挂而假绿。
                return Err(CfError::malformed(format!("测试没为 {url} 挂路由")));
            };
            let index = {
                let mut counters = self.counters.lock().expect("锁没中毒");
                let slot = counters.entry(prefix).or_insert(0);
                let current = *slot;
                *slot += 1;
                current
            };
            let reply = replies
                .get(index)
                .or_else(|| replies.last())
                .cloned()
                .expect("至少挂了一条回答");
            reply
        })
    }
}

/// 一把测试 Key（**故意不像真 Key**：它只会出现在断言里，不可能是真凭据）。
pub(super) const TEST_KEY: &str = "test-key-not-a-real-credential";

/// 测试用的 Key。
pub(super) fn key() -> ApiKey {
    ApiKey::new(TEST_KEY).expect("测试 Key 非空")
}

/// 把等待压成 0 的策略（用例要跑得动，就不能真等 1s + 2s）。
pub(super) fn fast_policy(attempts: u32) -> RetryPolicy {
    RetryPolicy::standard()
        .with_attempts(attempts)
        .with_delays(Duration::ZERO, Duration::ZERO)
}
