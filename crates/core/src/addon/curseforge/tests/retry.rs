//! 重试策略本身（次数上界、退避、`Retry-After` 优先、可关）——纯函数，不碰网络。

use std::time::Duration;

use crate::addon::curseforge::RetryPolicy;

#[test]
fn the_default_policy_is_bounded() {
    let policy = RetryPolicy::standard();
    assert_eq!(policy.attempts(), 3);
    assert!(policy.is_enabled());
    assert_eq!(policy.delay(0, None), Duration::from_secs(1));
    assert_eq!(policy.delay(1, None), Duration::from_secs(2));
    assert_eq!(policy.delay(2, None), Duration::from_secs(4));
    // 1<<5 = 32s 超过上限 ⇒ 封顶 30s
    assert_eq!(policy.delay(5, None), Duration::from_secs(30));
    // 移位不能溢出（第 20 次仍然只是封顶值）
    assert_eq!(policy.delay(20, None), Duration::from_secs(30));
    assert_eq!(RetryPolicy::default(), policy);
}

#[test]
fn retry_after_wins_over_backoff_and_is_capped() {
    let policy = RetryPolicy::standard();
    assert_eq!(policy.delay(0, Some(7)), Duration::from_secs(7));
    // 站点写错了（86400s）也最多等 30s：不能因为一个头卡死一整天
    assert_eq!(policy.delay(0, Some(86_400)), Duration::from_secs(30));
}

#[test]
fn disabling_means_exactly_one_attempt() {
    let policy = RetryPolicy::disabled();
    assert_eq!(policy.attempts(), 1);
    assert!(!policy.is_enabled(), "1 次就是「不重试」");
    // 没有「发零次」这种策略：0 会被钳成 1
    assert_eq!(RetryPolicy::standard().with_attempts(0).attempts(), 1);
    assert_eq!(RetryPolicy::standard().with_attempts(5).attempts(), 5);
}

#[test]
fn delays_can_be_pressed_to_zero_for_offline_tests() {
    let policy = RetryPolicy::standard().with_delays(Duration::ZERO, Duration::ZERO);
    assert_eq!(policy.delay(0, None), Duration::ZERO);
    assert_eq!(policy.delay(3, None), Duration::ZERO);
    // 站点给的值也要被封顶吃掉：max=0 ⇒ 等 0
    assert_eq!(policy.delay(0, Some(9)), Duration::ZERO);
}
