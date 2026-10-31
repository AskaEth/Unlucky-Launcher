//! 指数退避与抖动的单测（纯函数，无网络）。

use std::time::Duration;

use unlucky_core::download::backoff::{BackoffPolicy, Jitter};

#[test]
fn exponential_growth_doubles_and_caps() {
    let policy = BackoffPolicy::default();
    assert_eq!(policy.delay(0), Duration::from_millis(1_000));
    assert_eq!(policy.delay(1), Duration::from_millis(2_000));
    assert_eq!(policy.delay(2), Duration::from_millis(4_000));
    assert_eq!(policy.delay(3), Duration::from_millis(8_000));
    // 上限封顶，不会涨到分钟级。
    assert_eq!(policy.delay(10), Duration::from_millis(30_000));
    assert_eq!(policy.delay(u32::MAX), Duration::from_millis(30_000));
}

#[test]
fn no_retry_policy_waits_nothing() {
    let policy = BackoffPolicy::none();
    assert_eq!(policy.delay(0), Duration::ZERO);
    assert_eq!(policy.max_attempts, 1);
    assert_eq!(policy.delay_jittered(0, 0.5), Duration::ZERO);
}

#[test]
fn jitter_stays_within_ratio_bounds() {
    let policy = BackoffPolicy::default();
    // base 1000ms ± 30% → [700, 1300]
    let low = policy.delay_jittered(0, 0.0);
    let high = policy.delay_jittered(0, 1.0);
    assert_eq!(low, Duration::from_millis(700));
    assert_eq!(high, Duration::from_millis(1_300));
    // 抖动永不超过 max_ms。
    for rng in [0.0, 0.25, 0.5, 0.75, 1.0] {
        assert!(policy.delay_jittered(9, rng) <= Duration::from_millis(30_000));
    }
}

#[test]
fn jitter_without_ratio_is_deterministic() {
    let policy = BackoffPolicy {
        jitter_ratio: 0.0,
        ..BackoffPolicy::default()
    };
    assert_eq!(
        policy.delay_jittered(2, 0.0),
        policy.delay_jittered(2, 0.99)
    );
}

#[test]
fn jitter_source_is_reproducible_and_in_range() {
    let mut first = Jitter::new(42);
    let mut second = Jitter::new(42);
    for _ in 0..32 {
        let a = first.next_fraction();
        let b = second.next_fraction();
        assert_eq!(a, b, "同种子的抖动序列必须一致");
        assert!((0.0..1.0).contains(&a));
    }

    let mut other = Jitter::new(7);
    assert_ne!(Jitter::new(1).next_u64(), other.next_u64());
    // 种子 0 会被替换成固定非零种子，但仍然可用。
    assert!((0.0..1.0).contains(&Jitter::new(0).next_fraction()));
}
