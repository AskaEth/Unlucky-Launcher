//! 长文案的渲染层断言（**D-M79-5**：我们的单行约束是否真的成立）。
//!
//! 矩阵图全用短文案（"Instances" / "Instance details"），`Text` 默认 `Wrapping::Word` 的坑
//! 在图上根本看不出来——页面一接真实实例名（`All the Mods 10 - ATM10 - 1.21.1 - NeoForge`）
//! 就会折成 2~3 行、顶穿 56 的页头、压住 trailing 动作。所以这里用 `ULL_NAV_PART=long`
//! 那张图单独量三件事：
//!
//! | 断言 | 抓的错误 |
//! |---|---|
//! | ListItem 三行高仍 44/60/76 | 长文本折行把行撑高 / 被固定行高裁掉 |
//! | AppBar 仍 56 高 | 标题折行顶穿页头 |
//! | 标题墨迹右缘 < trailing 图标左缘 | 长标题覆盖到动作按钮上 |
//!
//! 跑法见 `navigation/probe.rs` 的模块文档（`ULL_NAV_DIR=target/m79`，图名 `long-light.png`）。

use unlucky_design::color::{Role, SchemeFlavor};

use super::scan::{rgb, ui};
use super::{shot, shot_dir};

#[test]
fn long_text_stays_single_line() {
    let Some(dir) = shot_dir() else {
        eprintln!(
            "[navigation::probe::long] SKIPPED：未设置 ULL_NAV_DIR（需要先出 long-light.png）"
        );
        return;
    };
    let shot = shot(&dir, "long", SchemeFlavor::Light);
    let ui = ui(SchemeFlavor::Light);

    // ---- ① ListItem：长实例名不折行 → 行高仍是 44 / 60 / 76 ----
    let selected = rgb(ui.color(Role::SecondaryContainer));
    let mut rows = shot
        .blocks(selected)
        .into_iter()
        .filter(|b| (238..=242).contains(&b.width()) && b.height() > 30)
        .collect::<Vec<_>>();
    rows.sort_by_key(|b| b.top);
    assert_eq!(
        rows.len(),
        3,
        "长文案块应有 3 条 ListItem（实 {}）",
        rows.len()
    );
    let heights = rows.iter().map(|b| b.height()).collect::<Vec<_>>();
    for (measured, expected) in heights.iter().zip([44usize, 60, 76]) {
        assert!(
            (expected - 1..=expected).contains(measured),
            "长文案把行高撑成 {heights:?}（应为 44/60/76 ±1）——说明折行了"
        );
    }

    // ---- ② AppBar：长标题不顶穿页头（仍 56 高）----
    let bar = shot
        .blocks(rgb(ui.color(Role::Surface)))
        .into_iter()
        .find(|b| b.width() > 300 && (54..=58).contains(&b.height()))
        .expect("长文案块应有一条 56 高的页头");
    assert!(
        (55..=57).contains(&bar.height()),
        "长标题把页头撑成 {} 高（应 56）——说明折行了",
        bar.height()
    );

    // ---- ③ 标题不许压到 trailing 动作上 ----
    let title_ink = shot
        .band_extent(
            rgb(ui.color(Role::OnSurface)),
            bar.top,
            bar.bottom,
            bar.left + 40,
            bar.right.saturating_sub(90),
        )
        .expect("页头里应有标题墨迹");
    let trailing = shot
        .band_extent(
            rgb(ui.color(Role::OnSurfaceVariant)),
            bar.top,
            bar.bottom,
            bar.right.saturating_sub(90),
            bar.right,
        )
        .expect("页头右侧应有 trailing 图标墨迹");
    assert!(
        title_ink.1 + 2 < trailing.0,
        "长标题墨迹右缘 {} 压到 trailing 图标左缘 {} 上了（应被裁在文本区里）",
        title_ink.1,
        trailing.0
    );

    eprintln!(
        "[navigation::probe] long: 行高 {heights:?}，页头 {} 高，标题右缘 {} < trailing {}",
        bar.height(),
        title_ink.1,
        trailing.0
    );
}
