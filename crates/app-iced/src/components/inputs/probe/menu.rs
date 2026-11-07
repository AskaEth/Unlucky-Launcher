//! M77 probe · menu（从 `probe.rs` 拆出：单文件 ≤400 行，AGENTS.md §3-1）。

use super::*;

/// 8.3 / 8.4 / 8.6：菜单容器高 161、项 pitch 48、选中指示器内缩 4、分组分隔线 1dp。
#[test]
fn menu_container_items_and_divider() {
    let dir = require_dir!();
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let shot = shot(&dir, "menu", flavor);
        let ui = ui(flavor);
        let container = rgb(ui.color(Role::SurfaceContainer));
        let selected = rgb(ui.color(Role::SecondaryContainer));
        let divider = rgb(ui.color(Role::OutlineVariant));

        // 右侧那只 = 「展开态」菜单。容器色块可能被**分组分隔线 / 选中项**切成多块，
        // 所以先把 x>290 的块并成一个外接框，再逐行量容器色（容差 2，滤掉阴影外扩的 ~5px）。
        let menus = shot.bbox_of(container, 2000);
        let right: Vec<BBox> = menus.iter().filter(|b| b.left > 290).copied().collect();
        assert!(!right.is_empty(), "找不到右侧的「展开态」菜单");
        let mut open = right[0];
        for b in right.iter().skip(1) {
            open.left = open.left.min(b.left);
            open.top = open.top.min(b.top);
            open.right = open.right.max(b.right);
            open.bottom = open.bottom.max(b.bottom);
        }
        let (top, bottom) = row_extent(&shot, &open, container, 1);
        open.top = top;
        open.bottom = bottom;

        // 布局高 **161** = 8 + 48×3 + 1 + 8；抗锯齿会吃掉上下各 1 行 ⇒ 实测 160
        assert!(
            (159..=162).contains(&open.height()),
            "8.3/8.4 菜单高应 161（= 8 + 48×3 + 1 + 8）：实测 {}",
            open.height()
        );

        // 选中项：SecondaryContainer，高 48、左右各内缩 4
        let selected_box = shot
            .bbox_of(selected, 2000)
            .into_iter()
            .filter(|b| b.height() >= 44 && b.height() <= 50)
            .max_by_key(|b| b.width())
            .expect("找不到选中项的 SecondaryContainer");
        assert!(
            (46..=48).contains(&selected_box.height()),
            "8.3 项高应 48：实测 {}",
            selected_box.height()
        );
        // 容器左右缘：在第一项那一行按严格同色扫（bbox 会被阴影外扩 ~2px，不能用）
        let scan_y = open.top + 10;
        let row: Vec<usize> = (open.left..=open.right)
            .filter(|x| matches_within(shot.at(*x, scan_y), container, 2))
            .collect();
        let (menu_left, menu_right) = (*row.first().unwrap(), *row.last().unwrap());
        assert_eq!(selected_box.left - menu_left, 4, "8.6 选中指示器左内缩应 4");
        assert_eq!(
            menu_right - selected_box.right,
            4,
            "8.6 选中指示器右内缩应 4"
        );

        // 分组分隔线：1dp OutlineVariant 墨量 ≈1.0（位置在选中项与最后一项之间）
        let ink = line_ink(
            &shot,
            open.left + 20,
            open.right - 20,
            selected_box.bottom + 1,
            selected_box.bottom + 6,
            container,
            divider,
        );
        assert!(
            (0.6..=1.4).contains(&ink),
            "8.x 分组分隔线应 1dp `OutlineVariant`：实测墨量 {ink:.2}"
        );

        // trailing 槽位必须靠到**行右内边距**上（视觉 r5 P1：早先它紧贴 label 右边）。
        //
        // 位置公式：item 右缘 = menu_right − 4（选中指示器内缩）；button 内容右缘 = item 右缘 −
        // (pad_h − inset) = menu_right − 12；trailing 图标盒 24 的**墨迹**再内缩 k（`Check` 字形
        // 在 24 viewBox 里右边界 ≈ 5，与 leading 图标「可视边 12、墨迹 16」同一口径）。
        // 所以墨迹右缘 ≈ menu_right − 17。变异（去掉撑开的 Fill）会让它贴着 label ⇒ 差 ~120。
        let trailing_ink_color = rgb(ui.color(Role::OnSecondaryContainer));
        let mut trailing_right = 0usize;
        for y in selected_box.top..=selected_box.bottom {
            for x in selected_box.left..=selected_box.right {
                if matches(shot.at(x, y), trailing_ink_color) {
                    trailing_right = trailing_right.max(x);
                }
            }
        }
        let trailing_offset = menu_right - trailing_right;
        assert!(
            (16..=18).contains(&trailing_offset),
            "8.6/trailing：带 trailing 的项，其 trailing 墨迹右缘应在 item 右内边距上\
             （menu_right − 17，允许 ±1）：实测 menu_right={menu_right}、墨迹右缘={trailing_right}\
             ⇒ 偏 {trailing_offset}（贴 label 时会是 ~120）"
        );
    }
}
