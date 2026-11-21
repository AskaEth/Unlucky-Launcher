//! `MotionScale::spring_ms_for_amplitude`（M433）的用例：像素量化口径 vs 连续量口径。
//!
//! 两条用例从 `mod.rs` 的内联 `tests` **逐字搬来**（M433 复审 r1：`mod.rs` 正好 400 行、零余量，
//! 按「贴线先拆，不许压行凑数」拆到这个文件），用例的函数名、断言、注释、顺序一字未改。

use super::*;

#[test]
fn amplitude_entry_keeps_pixel_quantized_settle_inside_the_short_ladder() {
    // PlanDoc/14 §7-1：时长短档 150~300ms（超 300ms 要在报告里说明理由）。这是产品口径，
    // 只在用例里引用，不写进 `spring` 当令牌。
    let spring = SPRING_EXPRESSIVE_SPATIAL_DEFAULT;
    for from_px in [12.0_f32, 16.0] {
        let ms = MotionScale::STANDARD.spring_ms_for_amplitude(spring, from_px);
        assert!(
            (150.0..=300.0).contains(&ms),
            "{from_px}px 落地 {ms}ms 不在 §7-1 的短档里"
        );
    }
    // 越线的恰是连续量口径：同一根弹簧 `spring_ms` 是 476ms、与幅度无关——12px 伸手拿它就撞线。
    let continuous = MotionScale::STANDARD.spring_ms(spring);
    assert!(continuous > 300.0, "spring_ms 实测 {continuous}ms 应越线");
}

#[test]
fn amplitude_entry_and_continuous_settle_differ_by_the_threshold_only() {
    let spring = SPRING_EXPRESSIVE_SPATIAL_DEFAULT;
    let scale = MotionScale::STANDARD;
    // 闭式解 `t = -ln(thr·ωd)/(ζωn)`（ωn=√380=19.4936 ⇒ 1/(ζωn)=0.064124s、ωd=0.6），逐条验：
    //   0.001 : ln(0.001×0.6)  = −7.4186 ⇒ 0.47571s ⇒ 475.71ms（残差 0.012px @12px）
    //   0.5/12: ln(0.5×0.6/12) = −3.6889 ⇒ 0.23654s ⇒ 236.54ms（残差 0.5px @12px）
    //   0.5/16: ln(0.5×0.6/16) = −3.9766 ⇒ 0.25499s ⇒ 254.99ms（残差 0.5px @16px）
    let continuous = scale.spring_ms(spring);
    let px12 = scale.spring_ms_for_amplitude(spring, 12.0);
    let px16 = scale.spring_ms_for_amplitude(spring, 16.0);
    assert!((continuous - 475.71).abs() < 0.05, "连续 {continuous}ms");
    assert!((px12 - 236.54).abs() < 0.05, "12px {px12}ms");
    assert!((px16 - 254.99).abs() < 0.05, "16px {px16}ms");
    // 两套阈值在 12px 上差 239ms——「差多少」要能被别人重拿一遍。
    let diff = continuous - px12;
    assert!((diff - 239.17).abs() < 0.1, "两套口径差 {diff}ms");
    // 幅度越大越早收尾（半像素对应的相对残差更宽）。
    assert!(px16 > px12, "16px 应比 12px 更晚收尾：{px16} vs {px12}");
    // 缩放口径与 `spring_ms` 相同：`HALF` 折半、`NONE` 归零。
    assert_eq!(MotionScale::NONE.spring_ms_for_amplitude(spring, 12.0), 0.0);
    let half = MotionScale::HALF.spring_ms_for_amplitude(spring, 12.0);
    assert!((half - px12 / 2.0).abs() < 1.0e-3, "HALF 实测 {half}ms");
    // 退化输入不许出 NaN/inf：没有位移就没有时长；方向无关。
    for degenerate in [0.0, f32::NAN, f32::INFINITY] {
        assert_eq!(scale.spring_ms_for_amplitude(spring, degenerate), 0.0);
    }
    assert_eq!(scale.spring_ms_for_amplitude(spring, -12.0), px12);
}
