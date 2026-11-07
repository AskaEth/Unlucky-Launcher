//! 渲染层几何断言：**对着真截图量像素**（M68 的教训——纯函数返回对，图可能是错的）。
//!
//! ## 为什么自带一份最小读图器
//!
//! `gallery::scan` 是 `pub(super)`（私有于 `gallery`），而 `gallery/mod.rs` 不在本 mission 的 scope 里，
//! 所以这里自带 ~60 行读图器（读 PNG / 取点 / 连通块 / 横带墨迹范围），判据全在本文件。
//! 与 `inputs/probe.rs` 同一个处置（已作为 finding 报给 tower：建议把 `gallery::scan` 提升为 `pub(crate)`）。
//!
//! ## 怎么跑（worktree 根目录）
//!
//! ```text
//! # ① 出 4 张图（每块一条命令，块选择器见 gallery/navigation.rs 的模块文档）
//! ULL_GALLERY_SECTION=navigation ULL_NAV_PART=list ./target/debug/unlucky-launcher-iced.exe \
//!   --software-renderer --screen=gallery --screenshot target/m79/list-light.png --screenshot-theme=light
//! # ② 量图
//! ULL_NAV_DIR=target/m79 cargo test -p unlucky-app-iced navigation::probe
//! ```
//!
//! 没给 `ULL_NAV_DIR` 就**大声跳过**（不静默通过），与 `gallery::probe` 的口径一致。
//!
//! ## 量什么（每条都对着一个「已经画错过」的形态）
//!
//! | 断言 | 抓的错误 |
//! |---|---|
//! | ListItem 三行高 **44 / 60 / 76** | 抄 A1 的 56/72/88、或用内容撑高（行高被文本吃掉） |
//! | ListItem 禁用内容 = `OnSurface`@38% | 走 `disabled_colors()` 的默认 `OnSurfaceVariant` |
//! | Primary 指示条宽 = 选中项文本宽 + 2×16 | 条写成 `Fill`（= 整行宽，Q8 的另一面）或写死 |
//! | drawer **240** / rail **88**，药丸内缩 **12** / **16** | 药丸不贴边（Q8）、或两套组件各写死一个宽度 |
//! | AppBar **56** 高（静息 + 滚动） | 页头被内容撑高、或按 A1 改回 64 |

use std::path::{Path, PathBuf};

use unlucky_design::color::{Role, SchemeFlavor};
use unlucky_design::spacing::Space;

use crate::app::components::interaction;

mod divider;
mod long;
mod scan;
use scan::{rgb, ui as ui_of, BBox, Rgb, Shot};

/// 有这张图就读，没有就 `None`（深色档的可选覆盖用）。
fn shot_opt(dir: &Path, part: &str, flavor: SchemeFlavor) -> Option<Shot> {
    let slug = match flavor {
        SchemeFlavor::Light => "light",
        SchemeFlavor::Dark => "dark",
    };
    let path = dir.join(format!("{part}-{slug}.png"));
    path.exists().then(|| Shot::load(&path))
}

/// `ULL_NAV_DIR` → 截图目录（相对路径按**工作区根**解释：`cargo test` 的 cwd 是 crate 目录）。
fn shot_dir() -> Option<PathBuf> {
    let raw = std::env::var_os("ULL_NAV_DIR")?;
    let path = PathBuf::from(raw);
    if path.is_absolute() || path.exists() {
        return Some(path);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Some(root.join(path))
}

macro_rules! require_dir {
    () => {
        match shot_dir() {
            Some(dir) => dir,
            None => {
                eprintln!(
                    "[navigation::probe] SKIPPED：未设置 ULL_NAV_DIR。这是**渲染层**断言，\
                     需要先按本文件模块文档出 4 张图（target/m79/*.png）。"
                );
                return;
            }
        }
    };
}

fn shot(dir: &Path, part: &str, flavor: SchemeFlavor) -> Shot {
    let slug = match flavor {
        SchemeFlavor::Light => "light",
        SchemeFlavor::Dark => "dark",
    };
    Shot::load(&dir.join(format!("{part}-{slug}.png")))
}

/// ListItem：三行高 **44 / 60 / 76**（选中行的 `SecondaryContainer` 底就是行高）。
#[test]
fn list_item_rows_are_44_60_76() {
    let dir = require_dir!();
    let ui = ui_of(SchemeFlavor::Light);
    let shot = shot(&dir, "list", SchemeFlavor::Light);

    // 矩阵格宽 240（`nav_drawer_width`）：选中列的三块填充就是三条行
    let selected = rgb(ui.color(Role::SecondaryContainer));
    let mut rows: Vec<BBox> = shot
        .blocks(selected)
        .into_iter()
        .filter(|b| (238..=242).contains(&b.width()) && b.height() > 30)
        .collect();
    rows.sort_by_key(|b| b.top);
    assert_eq!(
        rows.len(),
        3,
        "选中列应有 3 条（一 / 两 / 三行），实际 {} —— 图里格子宽度或选中底丢了",
        rows.len()
    );
    let heights: Vec<usize> = rows.iter().map(BBox::height).collect();
    // 边缘那一行是抗锯齿（半覆盖）→ 实测常常少 1px；判 43..=44 / 59..=60 / 75..=76，
    // 而 A1 的 56/72/88 仍然落区间外（这条照样能被打红）。
    for (measured, expected) in heights.iter().zip([44usize, 60, 76]) {
        assert!(
            (expected - 1..=expected).contains(measured),
            "12b §4.1/4.2 的项目口径是 44/60/76（A1 是 56/72/88）；实测 {heights:?}"
        );
    }

    // 禁用内容角色 = `OnSurface`@38%（A1 ListTokens），不是默认的 `OnSurfaceVariant`@38%。
    // 判据：禁用格里**最深**的像素就是核心笔画，它必须精确等于 A1 的那个混合色。
    let cell = rows[0].width() + 8; // 格宽 240 + 列间距 8
    let disabled = BBox {
        left: rows[0].left + cell * 2,
        right: (rows[0].left + cell * 2 + rows[0].width() - 1).min(shot.width - 1),
        top: rows[0].top,
        bottom: rows[2].bottom,
    };
    let darkest = shot.darkest(&disabled).expect("禁用格里应有墨迹");
    let right = rgb(interaction::disabled_content_over_role(
        &ui,
        Role::OnSurface,
        false,
    ));
    let wrong = rgb(interaction::disabled_content_over_role(
        &ui,
        Role::OnSurfaceVariant,
        false,
    ));
    let near = |a: Rgb, b: Rgb| {
        a.iter()
            .zip(b)
            .all(|(x, y)| x.abs_diff(y) <= scan::TOLERANCE)
    };
    assert!(
        near(darkest, right),
        "禁用内容最深像素是 {darkest:?}，A1 的 OnSurface@38% 是 {right:?}（用 OnSurfaceVariant 会得到 {wrong:?}）"
    );
    assert!(
        !near(darkest, wrong),
        "禁用内容用了 OnSurfaceVariant@38%（{wrong:?}）而不是 A1 的 OnSurface@38%（{right:?}）"
    );

    eprintln!(
        "[navigation::probe] list: 行高 {heights:?}，禁用最深像素 {darkest:?}（A1 = {right:?}）"
    );
}

/// Tabs：Primary 指示条宽 = **选中项**文本宽 + 2×16（短/长文案各选中一次 → 两条宽度必须不同）。
#[test]
fn tabs_indicator_tracks_the_selected_tab() {
    let dir = require_dir!();
    let ui = ui_of(SchemeFlavor::Light);
    let shot = shot(&dir, "tabs", SchemeFlavor::Light);

    let primary = rgb(ui.color(Role::Primary));
    let surface = rgb(ui.color(Role::Surface));
    // 指示条：3px 高、宽 ≥ 40 的 `Primary` 块（选中项的标签是同色，但笔画块远小于 40 宽）
    let mut bars: Vec<BBox> = shot
        .blocks(primary)
        .into_iter()
        .filter(|b| b.height() <= 4 && b.width() >= 40)
        .collect();
    bars.sort_by_key(|b| b.top);
    assert!(
        bars.len() >= 2,
        "至少应有 2 条 Primary 指示条（短文案选中一行 + 长文案选中一行），实际 {}",
        bars.len()
    );
    // 前两条 = 两行 Primary（后面还有「五态」那一行的选中条，不参与「跟随宽度」的判定）
    bars.truncate(2);

    let mut measured = Vec::new();
    for bar in &bars {
        // 该行的标签墨迹：指示条上方 2..32 像素、**指示条自身跨度内**与底色不同的像素范围
        let span = shot
            .band_ink(
                surface,
                bar.left,
                bar.right,
                bar.top.saturating_sub(32),
                bar.top.saturating_sub(2),
            )
            .expect("指示条上方应能找到选中标签的墨迹");
        let ink = span.1 - span.0 + 1;
        let expected = ink + 32; // 2 × pad(16)
        assert!(
            bar.width() >= ink,
            "指示条 {} 比标签墨迹 {ink} 还窄",
            bar.width()
        );
        assert!(
            bar.width() <= expected + 12,
            "指示条宽 {} 远大于「标签墨迹 {ink} + 2×16 = {expected}」——条被写成了整行宽（Q8）",
            bar.width()
        );
        assert!(
            bar.width() + 12 >= expected,
            "指示条宽 {} 小于「标签墨迹 {ink} + 2×16 = {expected}」——条没有覆盖内边距",
            bar.width()
        );
        measured.push((bar.width(), ink));
    }
    // 6.6：Secondary 的分隔线必须是**一条连续直线**（逐 tab 分段画会在相邻段之间留缝）。
    // 判据与锚定规则全在 `probe/divider.rs`（含「无论红绿都打印锚定行」）。
    divider::check(&shot, &ui, "light");
    if let Some(dark) = shot_opt(&dir, "tabs", SchemeFlavor::Dark) {
        divider::check(&dark, &ui_of(SchemeFlavor::Dark), "dark");
    }

    let (short_bar, long_bar) = (measured[0].0, measured[1].0);
    assert!(
        long_bar > short_bar + 20,
        "两行的指示条宽度必须随选中项变化（短文案 {short_bar} / 长文案 {long_bar}）——\
         写死或 Fill 的条会一样宽"
    );

    eprintln!("[navigation::probe] tabs: 指示条 {measured:?}（宽, 标签墨迹宽）");
}

/// Drawer：**同一个容器两个宽度** —— rail 88 / drawer 240，药丸 56×32 / 216×56，内缩 16 / 12（D33）。
#[test]
fn drawer_is_240_and_rail_is_88_with_inset_pills() {
    let dir = require_dir!();
    let ui = ui_of(SchemeFlavor::Light);
    let shot = shot(&dir, "drawer", SchemeFlavor::Light);

    let pad = Space::X2.dp();
    let plate = rgb(ui.color(Role::SurfaceContainerLow));
    let mut plates: Vec<BBox> = shot
        .blocks(plate)
        .into_iter()
        .filter(|b| b.height() > 150)
        .collect();
    plates.sort_by_key(|b| b.left);
    assert_eq!(
        plates.len(),
        2,
        "应有 2 块取景板（rail 与 drawer），实际 {}",
        plates.len()
    );
    let (rail_plate, drawer_plate) = (plates[0], plates[1]);
    // 取景板比导航容器大一圈（左右各 pad）：板宽 − 2×pad 才是容器宽
    let rail = rail_plate.width() as f32 - pad * 2.0;
    let drawer = drawer_plate.width() as f32 - pad * 2.0;
    assert_eq!(rail, 88.0, "LayoutMetrics::nav_rail_width");
    assert_eq!(drawer, 240.0, "LayoutMetrics::nav_drawer_width");

    let pill = rgb(ui.color(Role::SecondaryContainer));
    let mut pills: Vec<BBox> = shot
        .blocks(pill)
        .into_iter()
        .filter(|b| b.width() > 40 && b.height() > 20)
        .collect();
    pills.sort_by_key(|b| b.left);
    assert_eq!(
        pills.len(),
        2,
        "rail 与 drawer 各一个选中药丸，实际 {}",
        pills.len()
    );
    let (rail_pill, drawer_pill) = (pills[0], pills[1]);

    // rail：药丸 56×32，左右各缩 16（88 − 2×16 = 56，M68 的结论）
    // 边缘抗锯齿可能少 1px（32 → 31）；判 ±1 区间，216/56 的 drawer 药丸同理
    assert!(
        (55..=56).contains(&rail_pill.width()) && (31..=32).contains(&rail_pill.height()),
        "12b §5.4：rail 药丸应是 56×32，实测 {}×{}",
        rail_pill.width(),
        rail_pill.height()
    );
    assert_eq!(
        rail_pill.left - (rail_plate.left + pad as usize),
        16,
        "rail 药丸左内缩必须是 16（Q8 回归时这里是 0，药丸会贴边）"
    );
    // drawer：药丸 216×56，左右各缩 12（A1 `NavigationDrawerItemDefaults.ItemPadding = 12`）
    assert!(
        (215..=216).contains(&drawer_pill.width()) && (55..=56).contains(&drawer_pill.height()),
        "21.5：drawer 药丸应是 216×56（240 − 2×12），实测 {}×{}",
        drawer_pill.width(),
        drawer_pill.height()
    );
    assert_eq!(
        drawer_plate.width() - drawer_pill.width(),
        24 + pad as usize * 2,
        "药丸两侧内缩之和必须是 24（12 + 12），再加取景板的两圈 pad"
    );
    assert_eq!(
        drawer_pill.left - (drawer_plate.left + pad as usize),
        12,
        "ItemPadding"
    );
    // 圆角语言不变：两条药丸都居中（左右内缩相等）
    assert_eq!(
        rail_plate.right - pad as usize - rail_pill.right,
        rail_pill.left - (rail_plate.left + pad as usize),
        "rail 药丸必须水平居中（两侧内缩相等）"
    );

    eprintln!(
        "[navigation::probe] drawer: rail {rail} / drawer {drawer}，药丸 {}×{} / {}×{}，内缩 {} / {}",
        rail_pill.width(),
        rail_pill.height(),
        drawer_pill.width(),
        drawer_pill.height(),
        rail_pill.left - (rail_plate.left + pad as usize),
        drawer_pill.left - (drawer_plate.left + pad as usize)
    );
}

/// AppBar：静息 56 高（`Surface`）、滚动 56 高（`SurfaceContainer`）——高度不随滚动变化。
#[test]
fn app_bar_is_56_high_in_both_states() {
    let dir = require_dir!();
    let ui = ui_of(SchemeFlavor::Light);
    let shot = shot(&dir, "appbar", SchemeFlavor::Light);

    let resting = rgb(ui.color(Role::Surface));
    let scrolled = rgb(ui.color(Role::SurfaceContainer));
    let mut measured = Vec::new();
    for (name, color) in [("静息", resting), ("滚动", scrolled)] {
        let bars: Vec<BBox> = shot
            .blocks(color)
            .into_iter()
            .filter(|b| b.width() > 300 && (54..=58).contains(&b.height()))
            .collect();
        assert_eq!(
            bars.len(),
            1,
            "{name} AppBar 应恰好一条（56 高、>300 宽），实际 {} 条",
            bars.len()
        );
        measured.push((name, bars[0].height()));
    }
    // 两条同高：滚动只换容器色与高程，不改高度（A1 只有 `OnScrollContainerColor` / `OnScrollContainerElevation`）
    // 滚动态带 L2 阴影，阴影会把边缘那一行染成接近容器的颜色 → 允许差 1px
    assert!(
        (measured[0].1 as i32 - measured[1].1 as i32).abs() <= 1,
        "静息与滚动必须同高（滚动不改高度），实测 {measured:?}"
    );
    // 抗锯齿让边缘行可能差 1px：判 55..=57；A1 的 64 必须落区间外
    assert!(
        (55..=57).contains(&measured[0].1),
        "AppBar 高度应是 56（LM.page_header_height），实测 {measured:?}"
    );
    assert!(
        !(63..=65).contains(&measured[0].1),
        "64 是 A1 AppBarSmallTokens.ContainerHeight；本项目按骨架令牌取 56（D-M79-2）"
    );

    // Q21：动作图标必须在 40×40 命中区正中出现（`Container` 默认 Left/Top，
    // 只给 `align_y` 会贴左缘——视觉 r1 实测前导墨迹 `38..58`，应 `44..68`）。
    // 判据：左内缩（bar.left → 前导墨迹左缘）== 右内缩（尾随墨迹右缘 → bar.right），±2px。
    let bars = shot
        .blocks(resting)
        .into_iter()
        .filter(|b| b.width() > 300 && (54..=58).contains(&b.height()))
        .collect::<Vec<_>>();
    let bar = bars[0];
    let leading_ink = rgb(ui.color(Role::OnSurface));
    let trailing_ink = rgb(ui.color(Role::OnSurfaceVariant));
    let leading = shot
        .band_extent(leading_ink, bar.top, bar.bottom, bar.left, bar.left + 48)
        .expect("页头左侧应有前导图标墨迹");
    let trailing = shot
        .band_extent(
            trailing_ink,
            bar.top,
            bar.bottom,
            bar.right.saturating_sub(48),
            bar.right,
        )
        .expect("页头右侧应有尾随图标墨迹");
    let left_inset = leading.0 as i32 - bar.left as i32;
    let right_inset = bar.right as i32 - trailing.1 as i32;
    assert!(
        (left_inset - right_inset).abs() <= 2,
        "动作图标不在命中区正中：左内缩 {left_inset} / 右内缩 {right_inset}（Q21：`align_x` 漏给会差 8px）"
    );

    eprintln!(
        "[navigation::probe] appbar: 高度 {measured:?}，图标内缩 左 {left_inset} / 右 {right_inset}"
    );
}
