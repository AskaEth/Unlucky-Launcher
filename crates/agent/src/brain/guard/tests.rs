//! [`super`] 的判据用例（拆出来守 400 行硬线；纯搬移，语义一字未改）。

use super::*;

use std::future::Future;
use std::task::Wake;

use crate::brain::chat::{ChatReply, DeltaObserver};

/// 永远不 Ready 的 provider future（模拟「端点收了请求就不说话」）。
fn silent() -> ProviderFuture<'static> {
    Box::pin(std::future::pending())
}

/// 把 future 推进到「挂起」并返回它的 waker（断言 park / 唤醒用）。
struct CountingWaker(std::sync::atomic::AtomicUsize);

impl Wake for CountingWaker {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn poll_once<F: Future + Unpin>(future: &mut F, waker: &Waker) -> Poll<F::Output> {
    let mut cx = Context::from_waker(waker);
    Pin::new(future).poll(&mut cx)
}

fn waker_pair() -> (Arc<CountingWaker>, Waker) {
    let counter = Arc::new(CountingWaker(std::sync::atomic::AtomicUsize::new(0)));
    let waker = Waker::from(Arc::clone(&counter));
    (counter, waker)
}

#[test]
fn a_silent_provider_trips_the_idle_timeout_instead_of_hanging() {
    let activity = Activity::new();
    let token = CancelToken::new();
    let mut guard = Guard::new(
        silent(),
        token,
        Arc::clone(&activity),
        Duration::from_millis(10),
    );
    let (_, waker) = waker_pair();

    assert!(poll_once(&mut guard, &waker).is_pending(), "先挂起");
    std::thread::sleep(Duration::from_millis(15));
    match poll_once(&mut guard, &waker) {
        Poll::Ready(Err(GuardError::Timeout { secs })) => assert_eq!(secs, 0, "10ms 不足 1 秒"),
        other => panic!("挂了 15ms 必须判超时：{other:?}"),
    }
}

#[test]
fn a_fresh_delta_pushes_the_deadline_back() {
    let activity = Activity::new();
    let mut guard = Guard::new(
        silent(),
        CancelToken::new(),
        Arc::clone(&activity),
        Duration::from_millis(30),
    );
    let (_, waker) = waker_pair();

    assert!(poll_once(&mut guard, &waker).is_pending());
    std::thread::sleep(Duration::from_millis(20));
    // 一段增量回来 ⇒ 计时重置
    activity.touch();
    assert!(
        poll_once(&mut guard, &waker).is_pending(),
        "刚收到增量就不该判超时（那是「总耗时」判据的病）"
    );
}

#[test]
fn cancel_wakes_the_waiter_immediately_and_wins_over_a_pending_provider() {
    let token = CancelToken::new();
    let counter = Arc::new(CountingWaker(std::sync::atomic::AtomicUsize::new(0)));
    let waker = Waker::from(Arc::clone(&counter));
    let mut guard = Guard::new(
        silent(),
        token.clone(),
        Activity::new(),
        Duration::from_secs(60),
    );

    assert!(poll_once(&mut guard, &waker).is_pending());
    token.cancel();
    assert_eq!(counter.0.load(Ordering::SeqCst), 1, "取消必须当场唤醒");
    match poll_once(&mut guard, &waker) {
        Poll::Ready(Err(GuardError::Cancelled)) => {}
        other => panic!("取消后必须返回 Cancelled：{other:?}"),
    }
}

#[test]
fn reset_makes_the_next_turn_eligible_again() {
    let token = CancelToken::new();
    token.cancel();
    assert!(token.is_cancelled());
    token.reset();
    assert!(!token.is_cancelled(), "取消一次不许变成永久取消");
}

#[test]
fn a_ready_provider_never_needs_a_timer() {
    // 这条守住「没有运行时的驱动者也能跑」：立刻 Ready 的后端一次都不进 ③。
    struct Ready;
    impl Future for Ready {
        type Output = Result<ChatReply, ProviderError>;
        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Ready(Ok(ChatReply::text("好了")))
        }
    }
    let mut guard = Guard::new(
        Box::pin(Ready),
        CancelToken::new(),
        Activity::new(),
        Duration::from_secs(60),
    );
    let (_, waker) = waker_pair();
    match poll_once(&mut guard, &waker) {
        Poll::Ready(Ok(reply)) => assert_eq!(reply.text, "好了"),
        other => panic!("立刻 Ready 的 provider 应当一次返回：{other:?}"),
    }
}

#[test]
fn the_activity_observer_forwards_every_delta_and_keeps_the_clock_warm() {
    let raw = Arc::new(crate::event::MemoryEventSink::new());
    let sink: Arc<dyn crate::event::AgentEventSink> = raw.clone();
    let inner = crate::brain::chat::EventDeltaObserver::new(sink);
    let activity = Activity::new();
    let observer = ActivityObserver::new(&inner, Arc::clone(&activity));
    observer.on_delta("你好");
    assert_eq!(raw.streamed_text(), "你好");
    assert!(activity.idle_elapsed() < Duration::from_secs(1));
}

#[test]
fn the_timeout_env_parse_is_clamped_and_never_opens_the_door() {
    let configured = Duration::from_secs(60);
    // 没设 ⇒ 配置值（生产上是 60s 默认 / 用例里是压短的几百 ms）
    assert_eq!(parse_timeout(None, configured), configured);
    // 认得 ⇒ 秒数
    assert_eq!(
        parse_timeout(Some("5"), Duration::from_secs(60)),
        Duration::from_secs(5)
    );
    assert_eq!(
        parse_timeout(Some(" 7 "), configured),
        Duration::from_secs(7)
    );
    // 0 不许「关掉超时」：钳到 1 秒（否则这条通路会重新打开本节要关的门）
    assert_eq!(parse_timeout(Some("0"), configured), Duration::from_secs(1));
    assert_eq!(
        parse_timeout(Some("999999"), configured),
        Duration::from_secs(3600)
    );
    // 读不懂 ⇒ 配置值（不静默改档，也不会 panic）
    assert_eq!(parse_timeout(Some("abc"), configured), configured);
    assert_eq!(parse_timeout(Some("-3"), configured), configured);
    assert_eq!(parse_timeout(Some(""), configured), configured);
}
