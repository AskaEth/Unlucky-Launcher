//! M77 probe · text（从 `probe.rs` 拆出：单文件 ≤400 行，AGENTS.md §3-1）。

use super::*;

/// 9.1 / 9.2：Filled 高 56、宽 320；Filled 指示线 静息 1.0 / 聚焦 2.0 墨量。
#[test]
fn text_field_height_width_and_indicator_thickness() {
    let dir = require_dir!();
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let shot = shot(&dir, "text", flavor);
        let ui = ui(flavor);
        let surface = rgb(ui.color(Role::Surface));
        let fill = rgb(ui.color(Role::SurfaceContainerHighest));
        let line = rgb(ui.color(Role::OnSurfaceVariant));
        let primary = rgb(ui.color(Role::Primary));

        let mut boxes = shot.bbox_of(fill, 400);
        boxes.sort_by_key(|b| (b.top, b.left));
        let wide: Vec<&BBox> = boxes.iter().filter(|b| b.width() >= 280).collect();
        // 320 宽的有两个 Filled（空 / 有值）；第三格是 Outlined 错误态——它没有容器色，
        // 所以这里只要求 ≥2，错误态由 `outlined_stroke_and_error_paint` 单独验。
        assert!(
            wide.len() >= 2,
            "{flavor:?}: 至少要有 2 个 320 宽的 Filled 文本字段（实测 {}）",
            wide.len()
        );

        // 320 宽那三个 = 「空 / 有值 / 错误」三格；高度必须精确 56
        for bbox in wide.iter().take(2) {
            assert!(
                (280..=320).contains(&bbox.width()),
                "9.2 最小宽 280：实测 {}（320 宽格）",
                bbox.width()
            );
            let column = bbox.right - 20;
            let (top, bottom) = vertical_extent(&shot, bbox, fill, surface, column);
            assert_eq!(
                bottom - top + 1,
                56,
                "9.1 高度必须精确 56（带浮动标签）：实测 {flavor:?} @x={column}"
            );
        }

        // 矩阵第一行（Filled 五态）＝最靠上的 5 个 160 宽格：列序 = Interaction::ALL
        // 五态里 disabled 的容器是 `OnSurface@12%`（另一种颜色），所以这里只有 4 块：
        // enabled / hovered / pressed / focused —— 列序与 `Interaction::ALL` 一致。
        let row: Vec<&BBox> = boxes
            .iter()
            .filter(|b| (140..=170).contains(&b.width()))
            .take(4)
            .collect();
        assert_eq!(row.len(), 4, "{flavor:?}: Filled 矩阵应有 4 个同色格");
        let enabled = row[0];
        let focused = row[3];

        // 静息指示线 1dp：墨量 ≈ 1.0
        let enabled_ink = line_ink(
            &shot,
            enabled.left + 8,
            enabled.right - 8,
            enabled.bottom,
            enabled.bottom + 4,
            fill,
            line,
        );
        assert!(
            (0.6..=1.4).contains(&enabled_ink),
            "9.4 静息指示线应 1dp：实测墨量 {enabled_ink:.2}"
        );

        // 聚焦指示线 2dp：墨量 ≈ 2.0（画成 1dp 会掉到 ~1.0）
        let focused_ink = line_ink(
            &shot,
            focused.left + 8,
            focused.right - 8,
            focused.bottom,
            focused.bottom + 5,
            fill,
            primary,
        );
        assert!(
            (1.5..=2.5).contains(&focused_ink),
            "9.4 聚焦指示线应 2dp：实测墨量 {focused_ink:.2}（1dp 会掉到 ≈1.0）"
        );
        assert!(
            focused_ink > enabled_ink + 0.5,
            "聚焦必须比静息粗一档（{focused_ink:.2} vs {enabled_ink:.2}）"
        );
    }
}

/// 9.4 / 9.7：Outlined 描边 静息 1dp / 聚焦 2dp；错误态线/标签/辅助文本全 `Error`。
///
/// 量法：**在某一列的竖线上扫**（不是找同色块）——深色档里 hover 描边的抗锯齿像素也会落进
/// `Outline` 的容差里，按「最上面的同色块」取会取错行（实测 dark 取到了 hover 那一行）。
#[test]
fn outlined_stroke_and_error_paint() {
    let dir = require_dir!();
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let shot = shot(&dir, "text", flavor);
        let ui = ui(flavor);
        let surface = rgb(ui.color(Role::Surface));
        let fill = rgb(ui.color(Role::SurfaceContainerHighest));
        let outline = rgb(ui.color(Role::Outline));
        let primary = rgb(ui.color(Role::Primary));
        let error = rgb(ui.color(Role::Error));

        // 矩阵第一列（enabled）的左缘与行距：列宽 160 + 间距 8 = 168
        let left = shot
            .bbox_of(fill, 400)
            .into_iter()
            .filter(|b| (140..=170).contains(&b.width()))
            .min_by_key(|b| (b.top, b.left))
            .expect("找不到 Filled 矩阵格")
            .left;
        let filled_bottom = shot
            .bbox_of(fill, 400)
            .into_iter()
            .filter(|b| (140..=170).contains(&b.width()))
            .min_by_key(|b| (b.top, b.left))
            .expect("找不到 Filled 矩阵格")
            .bottom;

        let column = |index: usize| left + index * 168 + 120;

        // 静息（第 1 列）描边 1dp：在竖线上找第一处比底色更接近 `Outline` 的像素
        let rest_y = ((filled_bottom + 8)..shot.height)
            .find(|y| nearer(shot.at(column(0), *y), outline, surface))
            .expect("竖线上找不到 Outlined 静息描边");
        let rest_ink = line_ink(
            &shot,
            column(0) - 40,
            column(0) + 40,
            rest_y.saturating_sub(2),
            rest_y + 3,
            surface,
            outline,
        );
        // 描边的墨量随**亚像素相位**在 1.0~1.67 之间浮动（同一 1dp：整行对齐 = 1.0，
        // 落在半像素上 = 1 行满 + 1 行 0.67）。所以静息只做宽松下限，**宽度差异由比值判**。
        assert!(
            (0.8..=1.9).contains(&rest_ink),
            "9.4 Outlined 静息描边应 1dp（实测墨量 {rest_ink:.2}；2dp 会到 2.0）"
        );

        // 聚焦（第 4 列）描边 2dp
        let focus_y = ((filled_bottom + 8)..shot.height)
            .find(|y| nearer(shot.at(column(3), *y), primary, surface))
            .expect("竖线上找不到 Outlined 聚焦描边");
        let focus_ink = line_ink(
            &shot,
            column(3) - 40,
            column(3) + 40,
            focus_y.saturating_sub(2),
            focus_y + 4,
            surface,
            primary,
        );
        assert!(
            (1.8..=2.3).contains(&focus_ink),
            "9.4 Outlined 聚焦描边应 2dp（实测墨量 {focus_ink:.2}；1dp 会掉到 1.67）"
        );
        let ratio = focus_ink / rest_ink;
        assert!(
            (1.6..=2.5).contains(&ratio),
            "聚焦必须正好粗一倍（2dp/1dp）：实测比值 {ratio:.2}（{focus_ink:.2} vs {rest_ink:.2}）"
        );

        // 9.7：错误格——描边 2dp + 标签（框内上带）+ 辅助文本（框下）全 `Error`
        let error_box = shot
            .bbox_of(error, 2000)
            .into_iter()
            .filter(|b| b.width() >= 280)
            .max_by_key(|b| b.width())
            .expect("找不到错误态字段");
        assert!(
            error_box.height() >= 50,
            "错误格应是一整个 320 宽的字段（实测 {}×{}）",
            error_box.width(),
            error_box.height()
        );
        let label_ink = shot.count_in(
            error,
            error_box.left,
            error_box.right + 1,
            error_box.top + 2,
            error_box.top + 22,
        );
        let supporting_ink = shot.count_in(
            error,
            error_box.left,
            error_box.right + 1,
            error_box.bottom + 1,
            error_box.bottom + 30,
        );
        assert!(
            label_ink > 20,
            "9.7 错误标签没变成 Error（框内上带墨迹 {label_ink}px）"
        );
        assert!(
            supporting_ink > 20,
            "9.7 错误辅助文本没变成 Error（框下墨迹 {supporting_ink}px）"
        );
    }
}
