//! 渲染层几何断言：**对着真截图量像素**（M68 的教训：纯函数返回对，图可能是错的）。
//!
//! `buttons::tests` 只能证明「几何结构体里的数字是 40/20/24」——M68 的药丸正是这样：单测全绿、
//! 图上是 88×32（容器被 `center_x(Length::Fill)` 覆盖）。唯一能抓住这类错误的是**真渲染的像素**。
//! CI 没有桌面，所以这里不自己开窗：先用 `--screen=gallery --screenshot` 出真窗截图，本测试读
//! 那张 PNG 量几何、与令牌逐项比对。**没给图就跳过并大声说出来**（不静默通过）。
//!
//! 复现命令（在 worktree 根目录；**必须同时给 `ULL_GALLERY_SECTION`**，只支持 `buttons` / `divider`）：
//!
//! ```text
//! EXE=${CARGO_TARGET_DIR:-target}/debug/unlucky-launcher-iced   # 本仓纪律：一树一 target
//! OUT=${CARGO_TARGET_DIR:-target}/m229-probe && mkdir -p "$OUT"
//!
//! # ① 按钮矩阵（实心块 / 药丸 / 字形 / 逐行禁用色 / Outlined 环）
//! ULL_GALLERY_SECTION=buttons $EXE --software-renderer --screen=gallery \
//!   --screenshot "$OUT/buttons-light.png" --screenshot-theme=light
//! ULL_GALLERY_SECTION=buttons ULL_GALLERY_SHOT="$OUT/buttons-light.png" \
//!   cargo test -p unlucky-app-iced gallery::probe
//!
//! # ② 分隔线（`buttons` 段里没画线，必须单独出一张）
//! ULL_GALLERY_SECTION=divider $EXE --software-renderer --screen=gallery \
//!   --screenshot "$OUT/divider-light.png" --screenshot-theme=light
//! ULL_GALLERY_SECTION=divider ULL_GALLERY_SHOT="$OUT/divider-light.png" \
//!   cargo test -p unlucky-app-iced gallery::probe
//! ```
//!
//! `all` **不支持**：gallery 整页比 1180×760 的视口高，分隔线段与按钮矩阵不可能同屏，而 sizes 段的
//! 实心块又会被「按钮 40 高」的判据量到 —— 所以按段出图、按段断言，`all` 会被明确拒绝（见测试体）。
//!
//! 量什么（每条对一个已发生过的错误）：实心块高 **40**（12b 1.1）· 药丸顶行**左内缩 ≈14** ⇒ 圆角 full=20
//! （12b 1.2，8 圆角会接近通栏）· IconButton **40×40**（12b 2.1）· 字形墨迹 **16~26**
//! （Q9：24 盒被填满会到 40）· 分隔线 **1px**（22.1）· 禁用容器 **on-surface@10%**（按钮族）存在（12b 1.8）。

use unlucky_design::color::{Role, SchemeFlavor};
use unlucky_design::typography::Platform;

use crate::theme::{Families, Ui};

use crate::app::components::interaction::{self, Base};

use super::scan::{matches, nearer, rgb, Shot};

mod bases;
mod rows;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(flavor, Platform::Windows, FAMILIES)
}

/// 一条 1dp 细线在竖直方向的「墨量覆盖率」：各行相对底色的不透明度之和（1dp ⇒ ≈1.0）。
///
/// **只统计「长横线行」**（连续非底色像素 > 900）：标题/正文那些行也有几百个零散墨迹像素，
/// 按「任意非底色像素数」筛会把文字算进墨量（首版实测 57.9，明显是文字被算进去了）。
fn line_coverage(shot: &Shot, surface: [u8; 3], line: [u8; 3]) -> f32 {
    let mut total = 0.0;
    for y in 0..shot.height {
        let mut longest = 0;
        let mut current = 0;
        let mut sum = 0.0f32;
        let mut run_sum = 0.0f32;
        for x in 0..shot.width {
            let pixel = shot.at(x, y);
            if !matches(pixel, surface) {
                current += 1;
                run_sum += delta_ratio(pixel, surface, line);
                if current > longest {
                    longest = current;
                    sum = run_sum;
                }
            } else {
                current = 0;
                run_sum = 0.0;
            }
        }
        if longest > 900 {
            total += sum / longest as f32;
        }
    }
    total
}

/// `pixel` 相对 `surface` 的偏移占「满档 line 色偏移」的比例（0..=1）。
fn delta_ratio(pixel: [u8; 3], surface: [u8; 3], line: [u8; 3]) -> f32 {
    let channel = |i: usize| {
        let full = f32::from(line[i]) - f32::from(surface[i]);
        if full.abs() < 1.0 {
            return 0.0;
        }
        ((f32::from(pixel[i]) - f32::from(surface[i])) / full).clamp(0.0, 1.0)
    };
    let (dr, dg, db) = (channel(0), channel(1), channel(2));
    (dr + dg + db) / 3.0
}

/// `ULL_GALLERY_SHOT` → 截图路径（相对路径按**工作区根**解释：`cargo test` 的 cwd 是 crate 目录）。
///
/// 只把 `ULL_GALLERY_SHOT` 读成路径；**「这张图属于哪一段」由测试体开头那条 assert 管**
/// （`buttons` / `divider` 之外一律失败），这里不做校验——图与判据的配对是测试的前提条件，
/// 放在测试里读起来才连贯（复审 r3 P2-1：这段文档曾写「此处不满足就失败」，而 assert 早已搬走）。
fn shot_path() -> Option<std::path::PathBuf> {
    let raw = std::env::var_os("ULL_GALLERY_SHOT")?;
    let path = std::path::PathBuf::from(raw);
    if path.is_absolute() || path.exists() {
        return Some(path);
    }
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Some(root.join(path))
}

/// 一条测试覆盖全部几何断言（每张图各跑一次）。
#[test]
fn gallery_geometry_matches_the_tokens() {
    let Some(path) = shot_path() else {
        eprintln!(
            "[probe] SKIPPED：未设置 ULL_GALLERY_SHOT。这是**渲染层**几何断言，\
             需要先出一张真窗截图（命令见本文件模块文档）。"
        );
        return;
    };

    let shot = Shot::load(&path).expect("读 gallery 截图");
    assert_eq!(
        (shot.width, shot.height),
        (1180, 760),
        "验收图必须是 D30 的 1180×760 @100% DPI"
    );

    // 用左上角背景色判定深浅（页面 padding 区就是 `Surface`）
    let corner = shot.at(2, 2);
    let flavor = if matches(corner, rgb(ui(SchemeFlavor::Light).color(Role::Surface))) {
        SchemeFlavor::Light
    } else {
        SchemeFlavor::Dark
    };
    let ui = ui(flavor);

    // 只认两种 sheet（`all` 与整页图都拒绝，理由见 [`shot_path`] 与模块文档）：
    // `buttons` = 按钮矩阵的主判据；`divider` = 分隔线的两条判据（1px / 墨量覆盖率）。
    let section = std::env::var("ULL_GALLERY_SECTION")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    assert!(
        section == "buttons" || section == "divider" || section == "disabledbase",
        "这份 probe 只支持 `ULL_GALLERY_SECTION=buttons`（按钮矩阵）/ `divider`（分隔线）/ `disabledbase`（禁用态合成底）；当前 {section:?}。`all` 不行：整页比视口高，分隔线段与按钮矩阵不可能同屏。"
    );
    // 分隔线那段是**独立出口**：`divider` 段的 sheet 里没有按钮矩阵，判据也不同。
    if section == "divider" {
        check_divider(&shot, &ui);
        return;
    }
    // 禁用态合成底（M234）：三块板各按自己的底合成，判据在 `probe/bases.rs`。
    if section == "disabledbase" {
        bases::check(&shot, &ui, flavor);
        return;
    }

    // ---- 12b 1.1 / 1.2：实心按钮 40 高、药丸圆角 20 ------------------------------
    let primary = shot.bbox_of(rgb(ui.color(Role::Primary)));
    assert!(
        primary.len() >= 2,
        "至少该找到 Filled 按钮与 Filled IconButton 两块实心（实际 {}）",
        primary.len()
    );

    // **只判「按钮矩阵那一段」**：矩阵区由「第一块文字按钮（宽 > 60、高 40）」的顶边起、往下 9 行
    // + 8px。`buttons` 段的 sheet 里没有后续段，所以这条过滤是防御性的（`all` 现在被拒；将来若允许
    // 拼接多段，它保证判据只落在矩阵上，不会被别段的 chip/卡片撞到）。
    let matrix_top = primary
        .iter()
        .filter(|b| b.area() >= 800 && b.width() > 60 && (39..=41).contains(&b.height()))
        .map(|b| b.top)
        .min()
        .expect("没找到文字按钮的实心块（矩阵不在画面里？）");
    let matrix_bottom = matrix_top + rows::ROW_COUNT * rows::ROW_STEP + 8;

    let mut label_button = None;
    for bbox in &primary {
        // 字形（OnPrimary）会把实心块割出小碎片，只判「成块的」（40×40 = 1600）
        if bbox.area() < 800 {
            continue;
        }
        // 矩阵之外（后续段）的实心块不在这条判据的量程里。
        if bbox.top >= matrix_bottom {
            continue;
        }
        assert!(
            (39..=41).contains(&bbox.height()),
            "实心块高 {}（参考 40，12b 1.1）@ {:?}",
            bbox.height(),
            (bbox.left, bbox.top)
        );
        if bbox.width() > 60 {
            label_button = Some(*bbox);
        }
        // 12b 2.1：IconButton 容器精确 40×40（别再用区间——reviewer 已证 +2px 抓不住）
        if bbox.width() < 60 {
            // 宽 = 40（最宽那行是实色，量得准）；高会把底部抗锯齿行排除掉，所以是 39 或 40。
            // 区间收到 39..=40 仍能抓 ±2px 的回归（42 → 41、38 → 37 都会落区间外）。
            assert_eq!(bbox.width(), 40, "IconButton 容器宽必须 40（12b 2.1）");
            assert!(
                (39..=40).contains(&bbox.height()),
                "IconButton 容器高应 40（实色 39 + 抗锯齿行），实测 {}",
                bbox.height()
            );
        }
    }

    let label_button = label_button.expect("没找到文字按钮的实心块");
    assert!(
        label_button.width() >= 58,
        "12b 1.7：最小宽 58，实测 {}",
        label_button.width()
    );

    // 含抗锯齿的真实外框：中心列上「更接近填充色而不是底色」的连续行
    let fill = rgb(ui.color(Role::Primary));
    let surface = rgb(ui.color(Role::Surface));
    let center = (label_button.left + label_button.right) / 2;
    let mut true_top = label_button.top;
    while true_top > 0 && nearer(shot.at(center, true_top - 1), fill, surface) {
        true_top -= 1;
    }
    let mut true_bottom = label_button.bottom;
    while true_bottom + 1 < shot.height && nearer(shot.at(center, true_bottom + 1), fill, surface) {
        true_bottom += 1;
    }
    let true_height = true_bottom - true_top + 1;

    // **精确 40**：变异成 42 必须红。上一版写的是 `(39..=41).contains(..)`，
    // reviewer 把高度改成 42 重新出图后断言照样绿 —— 那就是「渲层断言等于没有」。
    assert_eq!(
        true_height, 40,
        "可见高必须精确 40（12b 1.1）；变异成 42 这里就会红"
    );

    // 圆角档位：量**顶部那一行的左内缩**。按像素中心口径，第 k 行（k 从 0 起）的闭式是
    // inset(k) = r − √(r² − (r − k − 0.5)²)；r=20、k=1 给 **12.40**（实测 12~14），
    // r=8 只有 ≈3.6，直角为 0。比「数圆角行数」抗锯齿得多。
    let top_row_left = (label_button.left..=label_button.right)
        .find(|x| matches(shot.at(*x, label_button.top), fill))
        .expect("顶部行应至少有一个填充像素");
    let top_inset = top_row_left - label_button.left;
    assert!(
        (11..=17).contains(&top_inset),
        "药丸圆角应 = 20（12b 1.2）：顶部行左内缩应 ≈14，实测 {top_inset}（8 圆角 ≈4，直角 0）"
    );

    // ---- 12b 1.4：带前置图标的左内边距 = 16（用 Filled 格量，避开圆角只扫中间带）----
    // 判「墨迹」的方式：比容器色更接近 OnPrimary。不要用字面量白——浅色页面底色 (246,250,255)
    // 与白在容差内，会把圆角外的底色当成墨迹（首次实现就踩了，量出 3）。
    let on_primary = rgb(ui.color(Role::OnPrimary));
    let mut left_ink: Option<usize> = None;
    for y in (label_button.top + 12)..=(label_button.bottom - 12) {
        for x in (label_button.left + 4)..=label_button.right {
            if nearer(shot.at(x, y), on_primary, fill) {
                left_ink = Some(left_ink.map_or(x, |best: usize| best.min(x)));
            }
        }
    }
    let icon_pad = left_ink.expect("Filled 格应带前置图标（量内边距用）") - label_button.left;
    // 图标盒左缘 = 16；字形在 24 viewBox 里自带 ≈5.3 左边距 ⇒ 墨迹落在 21 附近
    // （16 → 21；24 → 29；8 → 13，三档互不重叠）
    assert!(
        (18..=24).contains(&icon_pad),
        "12b 1.4：带图标左内边距应 16（墨迹 ≈21），实测 {icon_pad}"
    );

    // ---- Q9：IconButton 的字形不许撑满 40 盒 -----------------------------------
    let tonal = rgb(ui.color(Role::SecondaryContainer));
    let tonal_boxes = shot.bbox_of(tonal);
    let icon_states = tonal_boxes
        .iter()
        .filter(|bbox| (39..=41).contains(&bbox.width()) && (39..=41).contains(&bbox.height()))
        .count();
    // 只有 **enabled** 列是精确的 SecondaryContainer（hover/pressed/focus 列叠了状态层，颜色不同），
    // 所以这里只要求 ≥1：量几何一只就够，五态的可见性由图本身给。
    assert!(
        icon_states >= 1,
        "tonal IconButton 至少 1 个（实际 {icon_states}）"
    );

    let gear = rgb(ui.color(Role::OnSecondaryContainer));
    let mut measured_glyph = false;
    let mut glyph_width = 0;
    for bbox in tonal_boxes
        .iter()
        .filter(|bbox| (39..=41).contains(&bbox.width()))
    {
        if let Some(ink) = shot.ink_inside(bbox, gear) {
            let width = ink.width();
            if width > glyph_width {
                glyph_width = width;
            }
            assert!(
                (16..=26).contains(&width),
                "图标盒 24 的字形墨迹应 16~26 宽，实测 {width}（Q9 回归时会接近 40）"
            );
            measured_glyph = true;
        }
    }
    assert!(measured_glyph, "没量到 IconButton 的字形");

    // ---- 12b 1.8 禁用态：容器 + **每一行**的内容色（A1 逐变体不同）----------------
    //
    // 上一版把两个 reviewer 都指出的重复块留在这里（`disabled_colors` 被调两次、
    // `disabled_container` 被绑定两次），且只有一条「全图数墨迹」的护栏 —— 深色下阈值失效、
    // 单行回退也抓不住。现在按矩阵的行逐条断言。
    //
    // **按钮族**：容器 10%（官方 `*ButtonTokens.DisabledContainerOpacity = 0.1f`）、内容 38%。
    // M86 之前这里和 chip 共用同一个 12% 常量，所以断言与实现「一起错」、永远同源——拆族后才对得上。
    let (disabled_container, _) = interaction::disabled_button_colors(&ui, Base::CANVAS);
    assert!(
        shot.bbox_of(rgb(disabled_container))
            .iter()
            .any(|b| b.width() > 60),
        "没找到禁用态实心按钮（12b 1.8：容器 = on-surface@10%）"
    );

    // 禁用态与描边环是**按矩阵行**判的，逐行表与判据搬去了 `probe/rows.rs`（守 400 行硬线）。
    rows::assert_matrix_rows(&shot, &ui, label_button.top, label_button.left);

    // filled 禁用容器里不许出现满档 OnPrimary（旧实现是白字压在容器上 = 1.23:1）
    let full_content = rgb(ui.color(Role::OnPrimary));
    for bbox in shot.bbox_of(rgb(disabled_container)) {
        if bbox.width() < 60 {
            continue;
        }
        // 只扫**内带**：圆角外的页面底色与 OnPrimary 在容差内，贴边扫描会误报。
        for y in (bbox.top + 12)..=(bbox.bottom - 12) {
            for x in (bbox.left + 12)..=(bbox.right - 12) {
                assert!(
                    !matches(shot.at(x, y), full_content),
                    "禁用容器内出现满档 OnPrimary @({x},{y})：禁用标签没降到 38%"
                );
            }
        }
    }

    eprintln!(
        "[probe] OK flavor={:?}：
  Filled 药丸 {}×{}（参考 高 40 / 宽 ≥58）
  顶行左内缩 {}（r=20 应 ≈14；8 圆角 ≈4；直角 0）
  实心块 {} 个（含 40×40 IconButton）
  禁用容器 (on-surface@10%，按钮族) 命中 · 逐行禁用内容断言 {} 行通过
  字形墨迹 {}px（24 盒应 16~26）",
        flavor,
        label_button.width(),
        true_height,
        top_inset,
        primary.len(),
        rows::ROW_COUNT,
        glyph_width
    );
}

/// `ULL_GALLERY_SECTION=divider` 的两条判据：分隔线 2 条（各摊 1~2 行）与单条墨量覆盖率 ≈1.0。
///
/// 单独成函数：`divider` 段的 sheet 没有按钮矩阵，走不到主流程的按钮判据；而主流程也不需要
/// 分隔线（`buttons` 段的 sheet 里根本没画线，断言只会误报）。
fn check_divider(shot: &Shot, ui: &Ui) {
    let surface = rgb(ui.color(Role::Surface));
    let mut runs = 0usize;
    for y in 0..shot.height {
        let mut longest = 0;
        let mut current = 0;
        for x in 0..shot.width {
            if !matches(shot.at(x, y), surface) {
                current += 1;
                longest = longest.max(current);
            } else {
                current = 0;
            }
        }
        if longest > 300 {
            runs += 1;
        }
    }
    // 两条线 × 各摊 1~2 行 ⇒ 2..=4 行
    assert!(
        (2..=4).contains(&runs),
        "分隔线应 2 条（每条 1px，抗锯齿最多摊 2 行）⇒ 2..=4 行，实测 {runs} 行"
    );
    // **墨量覆盖率**：一条 1dp 的线，其各行相对底色的不透明覆盖率之和应 ≈1.0。
    // 1.5dp 的线会到 ≈1.5，2dp 会到 ≈2.0 —— 这是「容忍 1.5 倍粗」的补丁。
    let full = rgb(ui.color(Role::OutlineVariant));
    // 屏上正好 2 条线（通栏 + 列表内缩），所以把总墨量除以 2 得到「单条线的覆盖率」
    let coverage = line_coverage(shot, surface, full) / 2.0;
    assert!(
        (0.8..=1.3).contains(&coverage),
        "单条分隔线墨量覆盖率应 ≈1.0（= 1dp），实测 {coverage:.2}（1.5dp 会到 ~1.5）"
    );
    eprintln!("[probe] OK section=divider：分隔线 {runs} 行、单条覆盖率 {coverage:.2}");
}
