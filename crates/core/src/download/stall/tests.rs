//! `stall` 的单测（M292 从 `stall.rs` 拆出：那边加完「改这几个数要付的代价」那节就 440 行、
//! 踩了全仓 400 行硬线）。**逐字搬来**，唯一变换是整体去 4 格缩进（`mod tests` 体 → 文件顶层），
//! 由 `stall.rs` 末尾的 `#[path = "stall/tests.rs"] mod tests;` 收编。

use super::{job_budget, Deadlines, StallReason, StallReport, StallStage, MIN_AVG_THROUGHPUT};
use std::time::Duration;

/// 总期限按平均吞吐反推：188MB 的作业拿到 ≈100 分钟，小作业只拿到下限。
///
/// 变异证明：把 `job_budget` 写成常量（比如 `Duration::from_secs(30)`）⇒ 第一条红；
/// 把 `floor.max(..)` 写成 `min` ⇒ 第二条红（188MB 只剩 100 分钟里的 `floor`）；
/// 把 `min_throughput` 参数忽略、写回常量 ⇒ 第三条红（调低下限时期限不变）。
#[test]
fn the_job_budget_scales_with_size_and_never_drops_below_the_floor() {
    let floor = Duration::from_secs(600);
    let big = job_budget(188 * 1024 * 1024, floor, MIN_AVG_THROUGHPUT);
    let by_throughput = Duration::from_secs(188 * 1024 * 1024 / MIN_AVG_THROUGHPUT);
    assert_eq!(big, by_throughput, "大作业的期限由平均吞吐决定");
    assert!(big > Duration::from_secs(90 * 60), "188MB 至少给到 90 分钟");
    assert_eq!(
        job_budget(8 * 1024, floor, MIN_AVG_THROUGHPUT),
        floor,
        "小作业只拿到下限（吞吐算式在这里没有意义）"
    );
    // M292：设置里的吞吐下限（`download.throughput_floor_kib`）真的进到这个算式。
    let half = job_budget(188 * 1024 * 1024, floor, MIN_AVG_THROUGHPUT / 2);
    assert_eq!(half, by_throughput * 2, "下限减半 ⇒ 期限翻倍");
}

/// 吞吐下限被设成 `0` **不会除零、不会 panic**（M292 任务里点名的那一条）。
///
/// 设置层的区间过滤（`download.throughput_floor_kib` 是 4~4096）本来就不让 0 进来，但
/// `EngineConfig` 可以被宿主手拼，所以这一层必须自己站住：`0` 按 1 B/s 算完再落到
/// `floor`/有限值上，而不是 `u64` 除零 panic 或者拿到 `Duration::MAX`。
///
/// 变异证明：把 `min_throughput.max(1)` 删掉 ⇒ 本用例第一个断言 panic（除零）；把
/// `floor.max(..)` 的顺序颠倒 ⇒ 第二条红（`0` 字节的作业拿到 0 秒期限）。
#[test]
fn a_zero_throughput_floor_neither_panics_nor_divides_by_zero() {
    let floor = Duration::from_secs(600);
    assert_eq!(
        job_budget(1024, floor, 0),
        Duration::from_secs(1024),
        "0 按 1 B/s 算：1024 字节 ⇒ 1024 秒，不 panic"
    );
    assert_eq!(
        job_budget(0, floor, 0),
        floor,
        "0 字节的作业不许拿到 0 秒期限"
    );
}

/// 诊断串必须同时有：阶段（中英文都给）、已收字节、预期字节、无进展时长、原因。
///
/// 变异证明：删掉 `stage` 那一段 ⇒ 前两条 red；把 `received` / `expected` 写成常量
/// 或删掉 ⇒ 对应断言 red；把 `reason` 删掉 ⇒ 最后一条 red。
#[test]
fn the_report_spells_out_stage_three_numbers_and_reason() {
    let report = StallReport::no_progress(
        StallStage::AwaitBytes,
        4096,
        Some(188 * 1024 * 1024),
        Duration::from_millis(30_000),
    );
    let text = report.to_string();
    assert!(text.contains("等字节"), "{text}");
    assert!(text.contains("await_bytes"), "{text}");
    assert!(text.contains("已收 4096 字节"), "{text}");
    assert!(text.contains("预期 197132288 字节"), "{text}");
    assert!(text.contains("已 30000ms 无进展"), "{text}");
    assert!(text.contains("原因=没有新字节"), "{text}");
    assert!(report.reason == StallReason::NoProgress);

    // 总长未知也要能说清楚（不是「预期 0」）。
    let unknown = StallReport::deadline(StallStage::AwaitHeaders, 0, None, Duration::from_secs(60));
    assert!(unknown.to_string().contains("预期未知"), "{unknown}");
    assert_eq!(unknown.reason, StallReason::Deadline);
}

/// 阶段 id 是日志里的 grep 锚点：改名字等于改诊断口径，得先改这里。
#[test]
fn stage_and_reason_ids_are_stable() {
    let stages = [
        (StallStage::Connect, "connect", "建立连接"),
        (StallStage::AwaitHeaders, "await_headers", "等响应头"),
        (StallStage::AwaitBytes, "await_bytes", "等字节"),
        (StallStage::AcquirePermit, "acquire_permit", "等许可"),
        (StallStage::AwaitChunkJoin, "await_chunk_join", "等分块回收"),
        (StallStage::Verify, "verify", "校验"),
    ];
    for (stage, id, label) in stages {
        assert_eq!(stage.as_str(), id);
        assert_eq!(stage.label(), label);
    }
    assert_eq!(StallReason::Deadline.as_str(), "deadline");
    assert_eq!(StallReason::NoProgress.as_str(), "no_progress");

    let deadlines = Deadlines {
        response: Duration::from_secs(60),
        read_idle: Duration::from_secs(30),
    };
    assert_eq!(deadlines.response, Duration::from_secs(60));
}
