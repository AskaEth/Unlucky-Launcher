//! 进度两式的纯函数断言（`tests` 的子模块，从 `tests.rs` 拆出来控行数）。
//!
//! 三条回归都住在这里：
//! - [`raw_segments_are_unordered_and_layout_sorts_them`]：`phase_segments` 的原始输出**不有序**
//!   （视觉 r1 P1-A 就是把两段按顺序铺 `FillPortion`，图上被压成一段）；
//! - [`portion_shares_always_sum_to_the_full_track`]：份额之和恒为 1000（旧实现 34/100 相位 >1）；
//! - [`circular_indeterminate_rotation_is_four_turns_and_continuous`]：每周期 4 圈、跨周期不跳变
//!   （代码 r1 P1-B：旧实现只有 1 圈，且末尾倒跳 270°）。

use unlucky_design::color::{BrandBoard, Role, Scheme, SchemeFlavor};
use unlucky_design::shape::ShapeScale;
use unlucky_design::typography::Platform;

use super::super::geometry::{phase_segments, CircularGeometry, LinearGeometry, PHASE_055};
use super::super::keyframes::{KeyPoint, Keyframes};
use super::super::progress::{
    circular_colors, layout_spans, linear_colors, portion_shares, progress_roles, Amount, Progress,
};
use super::ui;
use crate::theme::Ui;

// ---- §12 进度 ---------------------------------------------------------------

/// 12.1：线性厚度 **4**、圆角 full ⇒ 2（`ShapeScale::Full.radius_px(4)`）。
#[test]
fn linear_thickness_and_radius_come_from_the_tokens() {
    let geometry = LinearGeometry::of(&ui(SchemeFlavor::Light));
    assert_eq!(geometry.thickness, 4.0, "12.1 线性厚度 = 4（A1 Height）");
    assert_eq!(
        geometry.radius,
        ShapeScale::Full.radius_px(4.0),
        "12.1 圆角 full = 厚的一半"
    );
    assert_eq!(geometry.radius, 2.0);
    assert_eq!(
        geometry.cycle_ms, 1750,
        "不定态周期 = LinearAnimationDuration"
    );
}

/// 12.3：环形 **40 / 4**，环中线半径 (40 − 4) / 2 = **18**。
#[test]
fn circular_geometry_matches_the_tokens() {
    let geometry = CircularGeometry::of(&ui(SchemeFlavor::Dark));
    assert_eq!(
        geometry.diameter, 40.0,
        "A1 CircularProgressIndicatorTokens.Size"
    );
    assert_eq!(geometry.thickness, 4.0, "A1 ActiveThickness");
    assert_eq!(geometry.radius, 20.0, "圆角 full = 直径的一半");
    assert_eq!(geometry.arc_radius(), 18.0);
    assert_eq!(
        geometry.cycle_ms, 6000,
        "A1 CircularAnimationProgressDuration"
    );
}

/// 12.4：不定态两段**逐段非负**——这条只证「单段自身合法」，**不证两段的先后**
/// （视觉 r1 P1-A 就是从这里漏过去的：函数名曾经叫 `..._stay_ordered`，其实没验排序）。
#[test]
fn indeterminate_segments_are_each_well_formed() {
    let mut travelled = false;
    let mut previous_end = 0.0;
    for step in 0..=200 {
        let phase = step as f32 / 200.0;
        for (start, end) in phase_segments(phase) {
            assert!(
                start <= end + f32::EPSILON,
                "相位 {phase:.3}：段起点 {start} 大于终点 {end}（前缘/后缘写反就会这样）"
            );
            assert!(
                (0.0..=1.0).contains(&start) && (0.0..=1.0).contains(&end),
                "相位 {phase:.3}：段越出轨道 {start}..{end}"
            );
        }
        let first = phase_segments(phase)[0].1;
        if phase > 0.0 && phase < 0.5 && first > previous_end {
            travelled = true;
        }
        previous_end = first;
    }
    assert!(travelled, "段前沿必须随相位右移（否则就是一根不动的条）");
    assert_eq!(phase_segments(0.0), [(0.0, 0.0), (0.0, 0.0)]);
    let (_, head) = phase_segments(1000.0 / 1750.0)[0];
    assert!((head - 1.0).abs() < 1.0e-5, "第一线前沿应在 1000ms 到 1.0");
}

/// **复审 P1-A 的回归测试**：`phase_segments` 的原始输出**不有序**（第一线的前缘在右、第二线在左），
/// 所以铺布局前必须排序 + 合并；排序后的两段位置要与 A1 keyframes 对得上。
#[test]
fn raw_segments_are_unordered_and_layout_sorts_them() {
    // 原始顺序：第一线（右，0.355..0.87）在前，第二线（左，0.003..0.079）在后
    let raw = phase_segments(0.55);
    assert!(
        raw[0].0 > raw[1].0,
        "phase 0.55 的原始两段应当是「右段在前、左段在后」，实际 {raw:?}"
    );

    let laid = layout_spans(&raw);
    // 期望：间隙 / 左段 / 间隙 / 右段 / 间隙（5 项，总长 1.0）
    assert_eq!(laid.len(), 5, "两段应摊成 5 项：{laid:?}");
    assert!(!laid[0].0 && laid[1].0 && !laid[2].0 && laid[3].0 && !laid[4].0);
    let left = laid[1].1;
    let right = laid[3].1;
    let left_start: f32 = laid[0].1;
    let right_start: f32 = laid[0].1 + left + laid[2].1;
    // 期望值来自 `geometry::PHASE_055`——与渲染层探针**共用**（复审 B：早先两边差 0.011）
    let [(a0, a1), (b0, b1)] = PHASE_055;
    assert!(
        (left_start - a0).abs() < 0.01,
        "左段起点 {left_start} vs {a0}"
    );
    assert!(
        (left - (a1 - a0)).abs() < 0.01,
        "左段宽 {left} vs {}",
        a1 - a0
    );
    assert!(
        (right_start - b0).abs() < 0.01,
        "右段起点 {right_start} vs {b0}"
    );
    assert!(
        (right - (b1 - b0)).abs() < 0.01,
        "右段宽 {right} vs {}",
        b1 - b0
    );

    let total: f32 = laid.iter().map(|(_, fraction)| fraction).sum();
    assert!(
        (total - 1.0).abs() < 1.0e-5,
        "铺开的占比之和必须 = 1.0，实测 {total}"
    );
}

/// 任意相位下 `FillPortion` 份额之和**恒为 1000**（iced 按总和归一 ⇒ 不等于 1000 就会压扁）。
///
/// 复审 P1-A 的实测反例：旧实现 **34/100 个相位**总和 >1（最大 1.99）。
#[test]
fn portion_shares_always_sum_to_the_full_track() {
    for step in 0..=200 {
        let phase = step as f32 / 200.0;
        let items = layout_spans(&phase_segments(phase));
        let shares = portion_shares(&items);
        let total: u32 = shares.iter().map(|(_, share)| u32::from(*share)).sum();
        assert_eq!(total, 1000, "相位 {phase:.3} 的份额之和是 {total}");
        assert!(
            shares.iter().filter(|(is_bar, _)| *is_bar).count() <= 2,
            "最多两段指示条，相位 {phase:.3}"
        );
        // 指示条之间必须留间隙（除非两段相接，那时会被合并成一段）
        let mut previous_bar = false;
        for (is_bar, share) in &shares {
            if *is_bar && previous_bar && *share > 0 {
                panic!("相位 {phase:.3}：两段指示条相邻但没合并");
            }
            previous_bar = *is_bar;
        }
    }
}

/// 环形不定态：弧长在 0.1..0.87 之间（A1 min/max），旋转走满一圈。
#[test]
fn circular_indeterminate_sweeps_the_official_range() {
    let geometry = CircularGeometry::of(&ui(SchemeFlavor::Light));
    let mut min = f32::MAX;
    let mut max = f32::MIN;
    let mut rotation = f32::MIN;
    for step in 0..=600 {
        let (sweep, spin) = geometry.indeterminate(step as f32 / 600.0);
        min = min.min(sweep);
        max = max.max(sweep);
        rotation = rotation.max(spin);
    }
    assert!(
        (min - 0.1).abs() < 1.0e-3,
        "弧长下限 0.1（CircularIndeterminateMinProgress），实测 {min}"
    );
    assert!((max - 0.87).abs() < 1.0e-3, "弧长上限 0.87，实测 {max}");
    // P1-B：每周期 4 圈（1080° 全局 + 360° 关键帧），不是 1 圈
    assert!(
        (rotation - 4.0).abs() < 0.01,
        "旋转应走满 4 圈，实测 {rotation}"
    );
}

/// **复审 P1-B 的回归测试**：环形不定态每周期 4 圈（官方 1440°），且跨周期**不跳变**。
///
/// 修之前：`ROTATION` 首点在 300ms/90°，且没有全局旋转 ⇒
/// `turns(0.9999) = 1.0`、`turns(0.0) = 0.25`，跨周期差 0.75 圈 = **270° 瞬时倒跳**。
#[test]
fn circular_indeterminate_rotation_is_four_turns_and_continuous() {
    let geometry = CircularGeometry::of(&ui(SchemeFlavor::Light));

    // 每周期 4 圈 = 1440°（1080° 全局 + 360° 关键帧）
    let start = geometry.indeterminate(0.0).1;
    let end = geometry.indeterminate(1.0 - 1.0e-6).1;
    assert!(
        ((end - start) - 4.0).abs() < 1.0e-3,
        "每周期应为 4 圈（1440°），实测 {} 圈",
        end - start
    );

    // 跨周期连续性：按「圈」取最短距离，必须 ≤ 一帧的位移（60fps ⇒ 4/60 ≈ 0.067 圈）
    let before = geometry.indeterminate(1.0 - 1.0e-4).1;
    let after = geometry.indeterminate(0.0).1;
    let mut jump = (before - after).rem_euclid(1.0);
    if jump > 0.5 {
        jump = 1.0 - jump;
    }
    assert!(
        jump <= 4.0 / 60.0,
        "跨周期旋转跳变 {jump} 圈（应 ≤ 4/60；修之前是 0.75 圈 = 270°）"
    );
    // 给报告用的一行实测数值（`--nocapture` 可见）：跨周期跳变 + 每周期圈数
    eprintln!(
        "[rotation] turns(0.9999)={before:.6} turns(0.0)={after:.6} 跳变={jump:.6} 圈（{:.2}°）·          每周期 {:.4} 圈 = {:.1}°",
        jump * 360.0,
        end - start,
        (end - start) * 360.0
    );

    // 旋转必须单调（相位 0 → 1 逐帧不回退，按圈算）
    // 只扫 [0, 1) 的相位：phase == 1.0 就是下一周期的 0，回绕是**正常的**（mod 1 等价）。
    let mut previous = geometry.indeterminate(0.0).1;
    for step in 1..600 {
        let phase = step as f32 / 600.0;
        let current = geometry.indeterminate(phase).1;
        assert!(
            current >= previous - 1.0e-6,
            "相位 {phase:.3}：旋转从 {previous} 回退到 {current}"
        );
        previous = current;
    }
}

/// 确定态：越界值夹到 0..1（0% 只画轨道，100% 画满）。
#[test]
fn determinate_values_are_clamped() {
    assert_eq!(
        Progress::determinate(1.5).amount(),
        Amount::Determinate(1.0)
    );
    assert_eq!(
        Progress::determinate(-2.0).amount(),
        Amount::Determinate(0.0)
    );
    assert_eq!(Progress::determinate(0.5).spans(), vec![(0.0, 0.5)]);
    assert_eq!(Progress::determinate(0.0).spans(), vec![(0.0, 0.0)]);
}

/// keyframes 求值器：端点保持、相位循环、越界相位按周期取模。
#[test]
fn keyframes_hold_ends_and_wrap() {
    const LINE: Keyframes = Keyframes {
        cycle_ms: 1000,
        points: &[
            KeyPoint {
                ms: 250,
                value: 0.0,
                easing: unlucky_design::motion::Easing::Linear,
            },
            KeyPoint {
                ms: 750,
                value: 1.0,
                easing: unlucky_design::motion::Easing::Linear,
            },
        ],
    };
    assert_eq!(LINE.sample(0.0), 0.0, "第一点之前保持首值");
    assert_eq!(LINE.sample(0.25), 0.0, "正好在第一点");
    assert!((LINE.sample(0.5) - 0.5).abs() < 1.0e-5, "中点线性插值");
    assert_eq!(LINE.sample(0.75), 1.0);
    assert_eq!(LINE.sample(0.9), 1.0, "最后一点之后保持末值");
    assert_eq!(LINE.sample(1.25), 0.0, "相位 1.25 = 0.25（循环）");
    assert_eq!(
        LINE.sample(-0.75),
        0.0,
        "负相位按周期折返：-0.75 → 0.25 相位 = 250ms = 首点"
    );
    assert!(
        (LINE.sample(-0.6) - 0.3).abs() < 1.0e-5,
        "-0.6 → 0.4 相位 = 400ms = 30%"
    );
    assert_eq!(LINE.sample(f32::NAN), 0.0, "NaN 不 panic，给首值");
}

// ---- M248：进度轨道的角色语义与「可主题化」 ---------------------------------

/// 轨道的配色判据**不是某个 hex**：两块出厂品牌板 + 一套自定义种子方案（`Scheme::from_seed`），轨道都必须解出
/// 「本方案的容器面角色」，且三套互不相同。把取角色改成取死值（`Role::Surface`）或写死
/// `SecondaryContainer`，这条都会红。
///
/// 抓不住什么：**像素**（角色对、图上却画错了）——那是 `super::super::probe` 的事；
/// 也抓不住「两个角色在令牌表里恰好同值」的情形，那条由这一个用例里的角色 pin 兜。
#[test]
fn track_is_the_highest_container_step_and_follows_the_scheme() {
    // 角色 pin：轨道 = 比画布高一档的容器面；指示条 = 强调色。
    // （A1 `LinearProgressIndicatorTokens.TrackColor` 是 `SecondaryContainer`，M248 按
    //  「轨道是背景尺、强调色只属于指示条」改口径，差异登记在 `geometry.rs` 的 12.2 行。）
    assert_eq!(
        progress_roles(),
        (Role::SurfaceContainerHighest, Role::Primary),
        "进度：轨道 SurfaceContainerHighest + 指示条 Primary"
    );

    // 第三套方案用**取色链的默认种子**（`DEFAULT_SEED`，天蓝）：design 的令牌，不是色值字面量。
    let themes = [
        (
            "ink",
            Ui::with_board(BrandBoard::Ink, Platform::Windows, super::FAMILIES),
        ),
        (
            "silver",
            Ui::with_board(BrandBoard::Silver, Platform::Windows, super::FAMILIES),
        ),
        (
            "custom",
            Ui::from_scheme(
                Scheme::from_seed(SchemeFlavor::Dark, unlucky_design::color::DEFAULT_SEED),
                super::FAMILIES,
            ),
        ),
    ];

    let mut tracks = Vec::new();
    for (name, theme) in &themes {
        let (track, active) = linear_colors(theme);
        assert_eq!(
            track,
            theme.color(Role::SurfaceContainerHighest),
            "{name}: 轨道 = 本方案的最高容器面"
        );
        assert_ne!(
            track,
            theme.color(Role::Surface),
            "{name}: 轨道退化成画布色了（取死值 Surface 的变异会打红这条）"
        );
        assert_ne!(
            track,
            theme.color(Role::SecondaryContainer),
            "{name}: 轨道又借回强调色族了"
        );
        assert_eq!(
            active,
            theme.color(Role::Primary),
            "{name}: 指示条 = Primary"
        );
        assert_eq!(
            circular_colors(theme),
            (track, active),
            "{name}: 环形取色与线性同源"
        );
        tracks.push(track);
    }

    assert_ne!(tracks[0], tracks[1], "Ink 与 Silver 的轨道必须不同色");
    assert_ne!(
        tracks[0], tracks[2],
        "换一套种子方案，轨道必须跟着变（可主题化的证据）"
    );
}
