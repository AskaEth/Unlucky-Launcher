//! 一轮请求的**时限**与**取消**：把「provider 一句都不回」从「界面挂死」变成「一条结论」。
//!
//! ## 它补的是哪个洞（M388 / GAPS §8 X5）
//!
//! M384 把真 provider 接进桌面会话之后，[`super::Brain::run_turn`] 里只有一句
//! `provider.complete(..).await`：模型或网络半路不说话时，这个 await **永远不会回来**，
//! worker 线程卡在 socket 上，UI 那侧的「停止」没有对端。用户能做的只有杀进程。
//!
//! 本模块在 await 外面套两条判据：
//!
//! | 判据 | 触发 | 语义 |
//! |---|---|---|
//! | **空闲时限**（[`DEFAULT_IDLE_TIMEOUT`]） | 连续 N 秒**没有任何响应**（不是「总耗时」） | 挂住 ⇒ 以一条结论收尾，不静默 |
//! | **取消**（[`CancelToken`]） | 用户按了界面上的「停止」 | 本轮结果一律丢弃（见下） |
//!
//! ## 为什么是「空闲」而不是「总耗时」
//!
//! 一个长回答会流 30~90 秒，那是健康的；「总耗时」判据会把正常回复砍断。真正的病是**沉默**
//! ——所以计时器在每收到一段增量时重置（[`Activity::touch`]，由 [`super::Brain::run_turn`]
//! 包在增量回调上）。默认 60 秒：本地大模型的首 token 与思考型模型的长 TTFT 都能容纳，
//! 而「端点根本不理你」会在 60 秒内现形。
//!
//! ## 「能打断」与「只丢弃」——一句准确的话
//!
//! 判据成立时本模块从 poll 返回错误并**丢掉** provider 的 future（`ProviderFuture` 是
//! `Pin<Box<dyn Future>>`，drop 它会连同 reqwest 那侧的请求一起析构 ⇒ 连接被关）。所以在
//! **本模块覆盖的那一段**（等 provider 的 await）里，这是**真的打断**，不是「等它跑完再丢结果」。
//!
//! 覆盖不到的场景必须说清楚：worker 停在**工具执行**或**等用户点允许**时，取消旗标要等那一段
//! 结束才会被看见（那些 await 不在本模块里）。那种情形下 UI 侧仍会**立刻**收尾（结论 + 释放
//! 输入区）并把后续事件丢掉——那是**丢弃**，不是打断。`brain::session` 会在每轮结束时把会话
//! 历史回滚到提交前的快照，所以「丢掉」不会污染下一轮。
//!
//! ## 为什么不用 `tokio::select!`
//!
//! `crates/agent/tests/` 的集成测试用手写 `block_on`（noop waker、**没有运行时**）驱动循环，
//! 那里 `tokio::time::sleep` 会 panic。判据的实现因此是「**先 poll provider**」：立刻 Ready 的
//! 后端（脚本 rig / 单测 mock）一次就返回，一个定时器都不会碰；只有 provider 真的 Pending 时
//! 才需要唤醒源（有运行时走 tokio 定时器，没有就走线程定时器兜底）。

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use super::chat::{ChatReply, ProviderFuture};
use super::provider::ProviderError;

/// 默认空闲时限：**连续**这么多秒没有任何响应就算挂住（不是「总耗时」）。
///
/// 60 秒的依据见模块文档：长回答的流式增量会不断重置它，所以它只对「沉默」生效。
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// 环境变量覆盖（单位：秒，1~3600）。
///
/// 为什么是环境变量而不是设置项：设置 → AI 的四个键在 `core::setting` 的 schema 里
/// （`crates/core` 不在本 mission 的 scope），加 `ai.request_timeout` 要另开 mission
/// （已按 finding 报塔）。这条通路是**每轮现读**的（[`resolve_timeout`] 在轮次开头调用），
/// 不是启动期快照——正是 M384「配置是启动期快照」被返工的那个坑。
pub const TIMEOUT_ENV: &str = "ULL_AGENT_TIMEOUT_SECS";

/// 时限的上下界（越界的取值按边界钳位，不是「关掉超时」）。
const MIN_TIMEOUT_SECS: u64 = 1;
const MAX_TIMEOUT_SECS: u64 = 3600;

/// 这一轮实际生效的空闲时限：`configured` 是配置里的值，环境变量优先。
///
/// **每轮调用一次**（见 [`TIMEOUT_ENV`] 的文档）：用户改了环境变量，下一句话就生效。
/// 解析本身在 [`parse_timeout`]（纯函数，有用例）；这里只负责把环境变量读出来。
#[must_use]
pub fn resolve_timeout(configured: Duration) -> Duration {
    let raw = std::env::var(TIMEOUT_ENV).ok();
    parse_timeout(raw.as_deref(), configured)
}

/// [`resolve_timeout`] 的纯函数部分：`raw` 是环境变量的原文（`None` = 没设）。
///
/// 三条口径：没设 ⇒ 用配置值；不是整数秒 ⇒ 用配置值（并在日志里留一行，不静默）；
/// 认得的整数按 `[1, 3600]` 钳位（**不给「填 0 = 关掉超时」这条路**——那会重新打开本节要关的门）。
#[must_use]
pub fn parse_timeout(raw: Option<&str>, configured: Duration) -> Duration {
    let Some(raw) = raw else {
        return configured;
    };
    let Ok(secs) = raw.trim().parse::<u64>() else {
        eprintln!("[agent] {TIMEOUT_ENV}={raw:?} 不是整数秒，按配置值 {configured:?} 处理");
        return configured;
    };
    Duration::from_secs(secs.clamp(MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS))
}

/// 取消旗标（UI 线程置位、worker 线程在判据里读）。
///
/// 与 `Arc<AtomicBool>` 的区别只有一处：它**能唤醒**正在等 provider 的那个 future
/// （[`CancelToken::park`] 存下 waker，[`CancelToken::cancel`] 取出并唤醒）。没有这一步，
/// 「按停止」最坏要等到空闲时限到期才被看见——那是 60 秒，等于没停。
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    inner: Arc<CancelState>,
}

#[derive(Debug, Default)]
struct CancelState {
    cancelled: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl CancelToken {
    /// 新旗标（未取消）。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 置位并**立刻唤醒**正在等的那一个 future。
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        let waker = self
            .inner
            .waker
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// 这一轮被取消了吗。
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    /// 清掉旗标（**每轮开头**调用）。
    ///
    /// 不清就会「取消一次、永久取消」：worker 是同一个线程跑下一轮的，旗标留着等于下一句话
    /// 一开始就被判取消——那正是任务书里「取消后必须能立刻重试」要防的。
    pub(crate) fn reset(&self) {
        self.inner.cancelled.store(false, Ordering::SeqCst);
        if let Ok(mut slot) = self.inner.waker.lock() {
            *slot = None;
        }
    }

    /// 登记「现在在等」的 waker；[`Self::cancel`] 会唤醒它。
    fn park(&self, waker: &Waker) {
        if let Ok(mut slot) = self.inner.waker.lock() {
            let stale = slot.as_ref().is_some_and(|old| !old.will_wake(waker));
            if slot.is_none() || stale {
                *slot = Some(waker.clone());
            }
        }
    }
}

/// 本轮的「最后一次有响应」时刻（[`super::Brain::run_turn`] 每收到一段增量就 [`Self::touch`]）。
#[derive(Debug)]
pub(crate) struct Activity {
    start: Instant,
    last_ms: AtomicU64,
}

impl Activity {
    /// 从「现在」开始计时（= 请求刚发出，还没收到任何东西）。
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            start: Instant::now(),
            last_ms: AtomicU64::new(0),
        })
    }

    /// 记一次响应（**只有增量**：provider 真的往回送了东西）。
    ///
    /// 工具结果不在这里 touch——「工具跑完了」不等于「模型有响应」，而空闲时限量的是后者。
    /// 请求**刚发出**那一刻的锚点由 [`Self::restart`] 负责（复审 r1 P2-2）。
    pub(crate) fn touch(&self) {
        self.last_ms.store(self.elapsed_ms(), Ordering::Relaxed);
    }

    /// 把锚点挪到「现在」：**一次请求刚刚发出**（复审 r1 P2-2）。
    ///
    /// 为什么必须有它：`Activity` 每轮只建一次，若只在增量处 touch，那么「某一步的工具执行比时限
    /// 还长」时，下一步请求**刚发出去**就已经 `idle >= timeout` ⇒ 第一次 poll 当场判超时
    /// （用户看到「请求超时」而请求才刚发出），而且 `Session` 会把工具那一轮白跑掉。
    pub(crate) fn restart(&self) {
        self.touch();
    }

    /// 距离上次响应过了多久。
    pub(crate) fn idle_elapsed(&self) -> Duration {
        Duration::from_millis(
            self.elapsed_ms()
                .saturating_sub(self.last_ms.load(Ordering::Relaxed)),
        )
    }

    fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// 判据的三种结局（[`super::Brain::run_turn`] 把它翻成 [`super::BrainError`]）。
#[derive(Debug)]
pub(crate) enum GuardError {
    /// provider 自己报错（原样交回，语义不变）。
    Provider(ProviderError),
    /// 空闲时限到：`secs` 秒没有任何响应。
    Timeout {
        /// 生效的空闲秒数（写进用户能看懂的那条结论）。
        secs: u64,
    },
    /// 用户按了停止。
    Cancelled,
}

/// 套在 provider 那个 await 外面的判据（见模块文档）。
pub(crate) struct Guard<'a> {
    inner: ProviderFuture<'a>,
    cancel: CancelToken,
    activity: Arc<Activity>,
    timeout: Duration,
}

impl<'a> Guard<'a> {
    pub(crate) fn new(
        inner: ProviderFuture<'a>,
        cancel: CancelToken,
        activity: Arc<Activity>,
        timeout: Duration,
    ) -> Self {
        Self {
            inner,
            cancel,
            activity,
            timeout,
        }
    }
}

impl Future for Guard<'_> {
    type Output = Result<ChatReply, GuardError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // `ProviderFuture` 是 `Pin<Box<dyn Future>>`（Unpin），整个 Guard 因此也是 Unpin。
        let this = self.get_mut();

        // ① 取消优先：用户按了停止就不该再等 provider 那一步的结果。
        if this.cancel.is_cancelled() {
            return Poll::Ready(Err(GuardError::Cancelled));
        }

        // ② 先问 provider：立刻 Ready 的后端一次就返回，不需要任何唤醒源
        //    （`crates/agent/tests` 的手写 block_on 没有运行时，走到 ③ 会 panic）。
        match this.inner.as_mut().poll(cx) {
            Poll::Ready(Ok(reply)) => return Poll::Ready(Ok(reply)),
            Poll::Ready(Err(error)) => return Poll::Ready(Err(GuardError::Provider(error))),
            Poll::Pending => {}
        }

        // ③ 它挂了：空转多久了？
        let idle = this.activity.idle_elapsed();
        if idle >= this.timeout {
            return Poll::Ready(Err(GuardError::Timeout {
                secs: this.timeout.as_secs(),
            }));
        }

        // ④ 两种唤醒源：取消（park 的 waker）与空闲时限（定时器）。
        this.cancel.park(cx.waker());
        arm_wake(cx.waker(), this.timeout - idle);
        Poll::Pending
    }
}

/// 到点唤醒这个 future。
///
/// 有 tokio 运行时（生产：core 的阻塞桥）用 tokio 定时器；没有（`crates/agent/tests` 的手写
/// `block_on`）用线程定时器兜底——那条路径上的 provider 都是立刻 Ready，通常一次都不会走到这里。
fn arm_wake(waker: &Waker, wait: Duration) {
    let waker = waker.clone();
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            tokio::time::sleep(wait).await;
            waker.wake();
        });
    } else {
        std::thread::spawn(move || {
            std::thread::sleep(wait);
            waker.wake();
        });
    }
}

/// 把增量回调包一层：每收到一段就 [`Activity::touch`]，其余原样转发。
pub(crate) struct ActivityObserver<'a> {
    inner: &'a dyn super::chat::DeltaObserver,
    activity: Arc<Activity>,
}

impl<'a> ActivityObserver<'a> {
    pub(crate) fn new(inner: &'a dyn super::chat::DeltaObserver, activity: Arc<Activity>) -> Self {
        Self { inner, activity }
    }
}

impl super::chat::DeltaObserver for ActivityObserver<'_> {
    fn on_delta(&self, text: &str) {
        self.activity.touch();
        self.inner.on_delta(text);
    }
}

#[cfg(test)]
mod tests;
