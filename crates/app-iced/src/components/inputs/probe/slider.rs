//! M77 probe · slider（从 `probe.rs` 拆出：单文件 ≤400 行，AGENTS.md §3-1）。

use super::*;

/// 11.1~11.6：竖条 4×44、0% 贴左、50% 分界在中央、100% 无未激活段（thumb 永不越界）。
#[test]
fn slider_thumb_positions_at_zero_half_and_full() {
    let dir = require_dir!();
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let shot = shot(&dir, "slider", flavor);
        let ui = ui(flavor);
        let active = rgb(ui.color(Role::Primary));
        let inactive = rgb(ui.color(Role::SecondaryContainer));

        // 三条「值行」= 宽 ≥400 的轨道（0% / 50% / 100%），按 y 排序
        let mut rows: Vec<(usize, usize, usize)> = Vec::new();
        for y in 0..shot.height {
            if let Some((left, right)) = longest_run(&shot, y, active, inactive, 20) {
                // 先按「跨度 ≥300」收行（把抗锯齿行也纳进来），band 建好后再按宽度筛。
                if right - left + 1 >= 300 {
                    rows.push((y, left, right));
                }
            }
        }
        // 合并相邻的 y（同一条轨道占 16 行），并把每行的端点累积成 min/max——
        // 首行/末行都是抗锯齿行，只取它们会把轨道量窄 2~3px。
        let mut bands: Vec<(usize, usize, usize, usize)> = Vec::new();
        for (y, left, right) in rows {
            match bands.last_mut() {
                Some(last) if y - last.1 <= 3 => {
                    last.1 = y;
                    last.2 = last.2.min(left);
                    last.3 = last.3.max(right);
                }
                _ => bands.push((y, y, left, right)),
            }
        }
        // 值行是 448 宽（`Space::X8 × 14`）；五态行 5 条轨道会被并成 ~663 的带，按宽度筛掉
        let bands: Vec<(usize, usize, usize, usize)> = bands
            .into_iter()
            .filter(|(_, _, left, right)| (440..=470).contains(&(right - left + 1)))
            .collect();
        assert_eq!(
            bands.len(),
            4,
            "值行应有 4 条（0% / 50% / 100% / 离散）：实测 {}",
            bands.len()
        );

        for (label, band) in ["0%", "50%", "100%"].iter().zip(bands.iter()) {
            let (top, bottom, band_left, band_right) = *band;
            let mid = (top + bottom) / 2; // 轨道中心行
            let (left, right) = (band_left, band_right);
            let width = right - left + 1;
            let handle_centre = left + 2 + (width - 4) / 2; // 位移区间 = handle/2 .. W-handle/2
            let boundary = (left..=right)
                .find(|x| matches(shot.at(*x, mid), inactive))
                .unwrap_or(right + 1);
            match *label {
                "0%" => assert!(
                    boundary <= left + 4,
                    "11.x 0% 时未激活轨必须从竖条（4 宽）之后立刻开始：实测分界 {}",
                    boundary - left
                ),
                "50%" => assert!(
                    boundary.abs_diff(handle_centre) <= 2,
                    "11.x 50% 的轨道分界应在正中（{handle_centre}）：实测 {boundary}"
                ),
                _ => {
                    let tail = shot.count_in(inactive, left, right + 1, mid, mid + 1);
                    assert_eq!(tail, 0, "11.x 100% 时不该有未激活段：实测 {tail}px");
                }
            }

            // 竖条必须落在轨道内（0% / 100% 的最狠情形）
            let handles: Vec<BBox> = shot
                .bbox_of(active, 100)
                .into_iter()
                .filter(|b| b.width() == 4 && b.height() >= 40)
                .collect();
            assert!(!handles.is_empty(), "11.2 找不到 4×44 的竖条");
            for handle in handles {
                assert!(
                    handle.left >= left && handle.right <= right,
                    "11.4 竖条越界：{handle:?} 不在轨道 {left}..{right} 内"
                );
                assert!(
                    (43..=44).contains(&handle.height()),
                    "11.2 竖条高必须 44（抗锯齿会吃掉 1 行，实测 {}）",
                    handle.height()
                );
                assert_eq!(handle.width(), 4, "11.2 竖条宽必须 4");
            }
        }
    }
}
