//! M77 probe · search（从 `probe.rs` 拆出：单文件 ≤400 行，AGENTS.md §3-1）。

use super::*;

/// 20.1 / 20.4 / 20.6 / 20.8 / 20.9：条 56、图标墨迹偏移 20（= 16 + viewBox 4）、
/// docked 面板 360 / 头 56 / 分隔线 1.0 `Outline` / 体 240。
#[test]
fn search_bar_and_docked_panel_geometry() {
    let dir = require_dir!();
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let shot = shot(&dir, "search", flavor);
        let ui = ui(flavor);
        let bar_color = rgb(ui.color(Role::SurfaceContainerHighest));
        let ink = rgb(ui.color(Role::OnSurface));

        let bars = shot.bbox_of(bar_color, 2000);
        let bar = bars
            .iter()
            .filter(|b| b.height() <= 60 && b.width() >= 360)
            .min_by_key(|b| b.top)
            .copied()
            .expect("找不到搜索条（高 ≤60、宽 ≥360）");
        assert_eq!(bar.height(), 56, "20.1 搜索条高必须 56");
        assert!(bar.width() >= 360, "20.2 搜索条宽 ≥ 360");

        // 20.4：图标墨迹左缘 = pad 12 + offset 4 + viewBox 内 ≈4 = 20（±1）
        let inner = (bar.top + 12)..(bar.bottom - 12);
        let mut left_ink = usize::MAX;
        for y in inner.clone() {
            for x in bar.left..bar.right {
                if matches(shot.at(x, y), ink) {
                    left_ink = left_ink.min(x);
                }
            }
        }
        let offset = left_ink - bar.left;
        assert!(
            (19..=21).contains(&offset),
            "20.4 前置图标墨迹偏移应 20（= 12 + 4 + viewBox 4）：实测 {offset}"
        );

        // 20.6 / 20.9：docked 面板 360 宽，头 56 + 1px 分隔线 + 体 240
        // （头与体被那条线切成**两个同色块**，所以按 top 排序取前两块）
        let mut parts = bars
            .iter()
            .filter(|b| b.width() == 360)
            .copied()
            .collect::<Vec<_>>();
        parts.sort_by_key(|b| b.top);
        assert_eq!(parts.len(), 2, "docked 面板 = 头部 + 结果区两块");
        let (header, body) = (parts[0], parts[1]);
        assert!(
            (55..=56).contains(&header.height()),
            "20.6 面板头部高 56（与 bar 同高；顶部圆角会吃掉 1 行抗锯齿，实测 {}）",
            header.height()
        );
        assert_eq!(body.height(), 240, "20.9 结果区最小高 240");
        assert!(
            (2..=3).contains(&(body.top - header.bottom)),
            "20.8 头与体之间正好 1dp（CPU 档摊成 2 行 ⇒ 隔 2~3 行）：实测 {}",
            body.top - header.bottom
        );
        let panel = header;

        // 20.8：分隔线是 `Outline`（不是 OutlineVariant），1dp 墨量 ≈1.0
        let divider_ink = line_ink(
            &shot,
            panel.left + 40,
            panel.right - 40,
            panel.top + 50,
            panel.top + 62,
            bar_color,
            rgb(ui.color(Role::Outline)),
        );
        assert!(
            (0.7..=1.3).contains(&divider_ink),
            "20.8 头部分隔线应 1dp `Outline`：实测墨量 {divider_ink:.2}"
        );
    }
}
