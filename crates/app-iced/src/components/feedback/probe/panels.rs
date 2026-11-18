//! 两个「面」的渲染层判据：Dialog（§7）与空态（§15）。`probe` 的子模块。
//!
//! ## Dialog 量什么
//!
//! - **逐卡宽度**：按列剖面把每张卡单独量出来（早先只量「最宽那张」，一宽一窄并排时会漏）；
//!   本项目 480，窄宿主那格 360（`Dialog::width(360)`），且都必须落在官方 280..560 内；
//! - 内距：标题墨迹左缩 ≈24、顶缩 24+bearing（24..=30）；
//! - **动作色**：`Role::Primary` 的**精确色**命中数（复审 A：早先用「离谁更近」把标题/正文字形
//!   也算成了 Primary —— 一张没有动作的卡也能"通过"）；并配两条反向证据：
//!   ①无动作的那张卡里精确 Primary 必须是 **0**；②有动作的那张卡下半区必须有字形行。
//! - scrim：一块 ≥700 宽的遮罩带，色值 = 预合成实色。
//!
//! ## 空态量什么
//!
//! 图标块 96±2（1px 描边各吃 1 行）· 块↔标题 ≈16+bearing · 条目总高 ≈232 · CTA 40。
//!
//! 常量与 [`super::super::panel`] 里登记值对齐（480 / 280 / 96 / 232），动作色用**精确色**
//! 而不是斜坡判据。

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;

use super::super::dialog::scrim;
use super::super::empty_state::BLOCK_LAYER;
use super::super::pixel::{band_width, bands, ink_inside, matches, near, rgb, BBox, Shot};
use crate::theme::Ui;

/// Dialog 宽（本项目取值，官方区间 280..560 内）。
const DIALOG_W: usize = 480;
/// 窄宿主那格的宽（`Dialog::width(360)`）。
const DIALOG_NARROW: usize = 360;
/// 空态图标块边长。
const BLOCK: usize = 96;
/// 空态条目总高：96 块 + 16 + 32 标题 + 8 + 16 副文案 + 24 + 40 CTA。
const EMPTY_TOTAL: usize = 232;
/// 动作字形的精确色命中下限（实测 3 张有动作的卡合计 325 px）。
const ACTION_INK_MIN: usize = 120;

/// §7：逐卡宽度 / 内距 / 动作色 / scrim。
pub(super) fn check_dialog(shot: &Shot, ui: &Ui) {
    let surface = ui.color(Role::Surface);
    let card = ui.layer(MaterialLayer::Overlay);
    let primary = rgb(ui.color(Role::Primary));

    // ---- 7.4 宽度：先分「卡带」（同排两张卡的 y 区间会重叠，所以只能按行分组），
    //      再在每条带里按**列剖面**分出每一张卡（复审 A/G：早先只量最宽的那张）。
    // 卡带用**精确色**数行（不用斜坡）：深色档里正文/描边的抗锯齿像素都落在
    // 「surface → card」那条很短的斜坡上，用斜坡会把整屏都算成卡带（实测出现 1180 宽的假卡）。
    let card_rgb = rgb(card);
    let scrim_rgb = rgb(scrim(ui));
    let card_rows: Vec<usize> = (0..shot.height)
        .filter(|y| {
            (0..shot.width)
                .filter(|x| {
                    let pixel = shot.at(*x, *y);
                    matches(pixel, card_rgb) && !matches(pixel, scrim_rgb)
                })
                .count()
                >= 200
        })
        .collect();
    let mut card_bands: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    let mut previous: usize = 0;
    for y in card_rows {
        match start {
            None => {
                start = Some(y);
                previous = y;
            }
            Some(_) if y == previous + 1 => previous = y,
            Some(top) => {
                card_bands.push((top, previous));
                start = Some(y);
                previous = y;
            }
        }
    }
    if let Some(top) = start {
        card_bands.push((top, previous));
    }
    assert!(
        card_bands.len() >= 3,
        "至少要找到 3 条卡带（第一排两张 + 双动作 + scrim 里那张），实测 {card_bands:?}"
    );

    // 每张卡记 (左缘, 宽)：宽度给「逐卡」断言，左缘给内距断言用（不能按单行找左缘——
    // 圆角 28 会让 band_top+4 那一行的卡片像素内缩 ~10px，实测内距被量成 12 而不是 24）。
    //
    // **深色档要把遮罩色排掉**：暗色下 `SurfaceContainerHigh` 与预合成 scrim 只差 ~35/通道，
    // 不排掉时 scrim 那片也被算成「卡」（实测出现 1180 宽的假卡）。
    let mut cards: Vec<(usize, usize)> = Vec::new();
    for band in &card_bands {
        let height = band.1 - band.0 + 1;
        let profile: Vec<usize> = (0..shot.width)
            .map(|x| {
                (band.0..=band.1)
                    .filter(|y| {
                        let pixel = shot.at(x, *y);
                        matches(pixel, card_rgb) && !matches(pixel, scrim_rgb)
                    })
                    .count()
            })
            .collect();
        let mut start: Option<usize> = None;
        for (x, ink) in profile.iter().enumerate() {
            let column = *ink >= height / 3;
            match (column, start) {
                (true, None) => start = Some(x),
                (false, Some(from)) => {
                    if x - from >= 280 {
                        cards.push((from, x - from));
                    }
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(from) = start {
            if shot.width - from >= 280 {
                cards.push((from, shot.width - from));
            }
        }
    }
    let mut widths: Vec<usize> = cards.iter().map(|(_, width)| *width).collect();
    widths.sort_unstable();
    assert!(
        widths.iter().any(|w| near(*w, DIALOG_W)),
        "7.4：应有 {DIALOG_W} 宽的卡（G17-e 项目取值），实测逐卡宽度 {widths:?}"
    );
    assert!(
        widths.iter().any(|w| near(*w, DIALOG_NARROW)),
        "7.4：窄宿主那格应量到 {DIALOG_NARROW}（`Dialog::width(360)`，复审 E），实测 {widths:?}"
    );
    for width in &widths {
        assert!(
            (280..=560).contains(width),
            "7.4：每张卡都必须落在官方 280..560 内，实测 {width}：{widths:?}"
        );
    }

    // 内距与 scrim 只在一张卡上量（第一张，最靠左上）。
    let first = card_bands[0];
    let (card_left, width) = cards[0];
    let top = first.0;
    let bottom = first.1;
    let bbox = BBox {
        left: card_left,
        right: card_left + width - 1,
        top,
        bottom,
    };
    let title =
        ink_inside(shot, &bbox, ui.color(Role::OnSurface), card, 2).expect("卡片里必须有标题墨迹");
    let inset = title.left.saturating_sub(card_left);
    assert!(
        (22..=28).contains(&inset),
        "7.3：四边内距 24 ⇒ 标题墨迹左缩 ≈24（含 bearing 22..28），实测 {inset}"
    );
    let top_inset = title.top.saturating_sub(top);
    assert!(
        (24..=30).contains(&top_inset),
        "7.3：上内距 24 ⇒ 标题墨迹顶缩 24+字形 bearing（24..30），实测 {top_inset}"
    );

    // ---- 7.6 动作色（复审 A：换掉「离谁更近」那条会被标题字形顶账的判据）--------
    let exact_primary = |band: (usize, usize)| -> usize {
        (band.0..=band.1)
            .map(|y| {
                (0..shot.width)
                    .filter(|x| matches(shot.at(*x, y), primary))
                    .count()
            })
            .sum()
    };
    let action_pixels: usize = (0..shot.height)
        .map(|y| {
            (0..shot.width)
                .filter(|x| matches(shot.at(*x, y), primary))
                .count()
        })
        .sum();
    assert!(
        action_pixels >= ACTION_INK_MIN,
        "7.6：Dialog 动作标签应是 `Primary` 的**精确色**（实测只命中 {action_pixels} px；\
         改成 OnSurfaceVariant 就会掉到 0）"
    );

    // 反向证据 ①：没有动作的那张卡（scrim 里那张）里精确 Primary 必须是 0 ——
    // 这条挡住「把标题/正文的深色字形算成 Primary」这条假绿路径。
    let quiet: Vec<(usize, usize)> = card_bands
        .iter()
        .copied()
        .filter(|band| exact_primary(*band) == 0)
        .collect();
    assert!(
        !quiet.is_empty(),
        "7.6：本屏应有一张**无动作**的卡（scrim 里那张）其精确 Primary 为 0，实测每带都有 Primary 墨迹"
    );

    // 反向证据 ②：动作行确实存在 —— 命中最多的那条带下半区必须有字形行。
    let loud = card_bands
        .iter()
        .copied()
        .max_by_key(|band| exact_primary(*band))
        .expect("卡带非空");
    let half = loud.0 + (loud.1 - loud.0) / 2;
    let action_rows = (half..=loud.1)
        .filter(|y| {
            (0..shot.width)
                .filter(|x| matches(shot.at(*x, *y), primary))
                .count()
                >= 3
        })
        .count();
    assert!(
        action_rows >= 1,
        "7.6：动作字形必须在卡片下半区（实测下半区精确 Primary 行数 {action_rows}）"
    );

    // ---- 7.7 scrim -------------------------------------------------------------
    let backdrop = scrim(ui);
    let scrim_bands = bands(shot, backdrop, surface, 700);
    assert!(
        !scrim_bands.is_empty(),
        "7.7：没找到 ≥700 宽的遮罩带（宿主 800 宽那格）"
    );

    eprintln!(
        "[feedback-probe] dialog：逐卡宽度 {widths:?} · 标题左缩 {inset} · 顶缩 {top_inset} · \
         精确 Primary {action_pixels}px（无动作卡 {} 张）· 动作行 {action_rows} 行 · scrim 带 {} 条",
        quiet.len(),
        scrim_bands.len()
    );
}

/// §15：图标块 96（`[估]`）+ 间距 16 / 8 / 24 + CTA 40。
pub(super) fn check_empty(shot: &Shot, ui: &Ui) {
    let surface = ui.color(Role::Surface);
    let block = ui.layer(BLOCK_LAYER);

    let found = bands(shot, block, surface, 40);
    let (top, bottom) = found
        .iter()
        .copied()
        .max_by_key(|(top, bottom)| bottom - top)
        .filter(|(top, bottom)| (BLOCK - 2..=BLOCK + 2).contains(&(bottom - top + 1)))
        .unwrap_or_else(|| {
            panic!(
                "15.1：没找到高 {BLOCK}(±2) 的图标块（`[估]` 项目自定值），实测量到的带：{found:?}"
            )
        });

    let width = band_width(shot, (top, bottom), block, surface);
    assert!(
        (BLOCK - 2..=BLOCK + 2).contains(&width),
        "15.1：图标块必须 {BLOCK}(±2，1px 描边）宽，实测 {width}"
    );

    // 图标本身：`view_at` 按 32 烘焙（stroke 同步重算），墨迹 ≈26~28。
    let glyph = ink_inside(
        shot,
        &BBox {
            left: 0,
            right: shot.width - 1,
            top,
            bottom,
        },
        ui.color(Role::OnSurfaceVariant),
        block,
        2,
    )
    .expect("块里必须有图标墨迹");
    assert!(
        (22..=34).contains(&glyph.width()) && (22..=34).contains(&glyph.height()),
        "15.1：图标墨迹应在 22..34（32 盒；`view` 时代只有 20×20），实测 {}×{}",
        glyph.width(),
        glyph.height()
    );

    // ---- P2-1 回归：块↔标题的实际间距（16 而不是 32）---------------------------
    let bbox = BBox {
        left: 0,
        right: shot.width - 1,
        top: bottom + 1,
        bottom: shot.height - 1,
    };
    let title = ink_inside(shot, &bbox, ui.color(Role::OnSurface), surface, 0)
        .expect("块下面必须有标题墨迹");
    let gap = title.top.saturating_sub(bottom + 1);
    assert!(
        (16..=24).contains(&gap),
        "15.3：块↔标题应为 16（+ 字形 bearing），实测 {gap}（32 = spacing 与 gap 双算回来了）"
    );

    let cta = bands(shot, ui.color(Role::Primary), surface, 20);
    let (cta_top, cta_bottom) = cta
        .iter()
        .copied()
        .find(|(top, bottom)| near(bottom - top + 1, 40))
        .unwrap_or_else(|| panic!("15.6：CTA 应是标准按钮（可见高 40），实测带 {cta:?}"));
    let total = cta_bottom + 1 - top;
    assert!(
        total.abs_diff(EMPTY_TOTAL) <= 6,
        "15.x：条目总高应 ≈{EMPTY_TOTAL}（96+16+32+8+16+24+40），实测 {total}"
    );

    eprintln!(
        "[feedback-probe] empty：块 y {top}..{bottom}（高 {}）· 宽 {width} · 图标 {}×{} · \
         块↔标题 {gap} · 总高 {total} · CTA {cta_top}..{cta_bottom}",
        bottom - top + 1,
        glyph.width(),
        glyph.height()
    );
}
