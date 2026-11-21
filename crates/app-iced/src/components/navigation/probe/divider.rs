//! 分隔线守卫（6.6）：**Secondary 的分隔线必须是一条连续直线**，且**1px 的重量不许随缩放跳变**。
//!
//! 逐 tab 分段画会在相邻段之间留缝（视觉 r1 P2-2 实测 `25..101 | 103..191 | 193..265`，
//! 交界像素偏亮）。这条守卫被复审三轮打过三次，每次都是「假绿」，所以下面把**每个判据
//! 挡的是哪一次假通过**写清楚——改之前先读一遍：
//!
//! | 判据 | 挡的假通过 |
//! |---|---|
//! | ① 锚定「细 + 近乎实心 + 薄 + 低对比 + 跨度 ≥200」的一行（见 `scan::Shot::find_thin_line`） | 五态行的 40px 高底色带、`Primary` 指示条顶替分隔线 |
//! | ② 线色必须是 `Surface → SurfaceVariant` 的插值（覆盖率 30%~105%） | 更靠上的同类细线顶替：`OutlineVariant` 组间线（t 出界）、hover 8% 带（三通道 t 散开） |
//! | ③ 参考色取**该行墨迹众数色**（见 `scan::Shot::line_segments`） | 缺口恰落在跨度中点时，参考色退化成页面底色 → 恒 1 段 |
//! | ④ 在该行**整段墨迹跨度**上数段（不是某次 run 的窗口） | 缺口在窗口外看不见、在窗口内被「跳过去重新起跑」滑过 |
//!
//! ## 锚定行**无条件打印**
//!
//! 不管红绿都往 stderr 打一行 `[probe] divider anchored at y=… span=x..x color=(r,g,b) segments=N`。
//! 之前绿的时候什么都不说，于是「锚错行」是静默的——不看断言在量什么，等于没有断言。
//!
//! ## 深浅两档
//!
//! 浅色图必跑；深色图（`tabs-dark.png`）**存在就跑**（深色的 divider 对比更低，更值得测）。
//! 没有深色图时不算失败，但会在输出里留一行 `dark 档跳过`。
//!
//! ## M454：线的**重量**（§22 那两条 `OutlineVariant` 线）
//!
//! 上面四条只管「断没断」。M448 取证挖出另一个真缺陷：同一条 1px 线在 **125% / 150%** 上
//! 有效逻辑粗会跳成 **0.80 / 1.33**（wgpu 的 `crisp` 吸附把 1 逻辑 px 的填充盒按整物理像素
//! 取整），而同一帧走 `Border` 掩膜的 1px 卡片描边恒 **1.00**。所以本文件另有
//! [`hairline_weight_is_scale_invariant`]：三档缩放下量**§22 那两条线**的剖面。
//!
//! ## M456：同一族的两条线也收口了，重量判据因此有两处入口
//!
//! M454 只改了 `components::divider`（`OutlineVariant`）。同一个缺陷机制还留在两处「各自的
//! 1px 填充盒」上：Tabs 的 Secondary 分隔线（`SurfaceVariant`，`tabs.rs`）与搜索面板头部线
//! （`Role::Outline`，`inputs/search.rs`）。M456 让它们都走 `divider::hairline`，于是：
//!
//! | 线 | 出图 | 判据 |
//! |---|---|---|
//! | §22 `divider::full` / `list`（`OutlineVariant`） | `ULL_GALLERY_SECTION=divider` | [`hairline_weight_is_scale_invariant`] |
//! | Tabs Secondary（`SurfaceVariant`） | `ULL_GALLERY_SECTION=navigation ULL_NAV_PART=tabs` | [`tabs_hairline_weight_is_scale_invariant`] |
//! | 搜索面板头部（`Outline`） | `ULL_GALLERY_SECTION=inputs` | `inputs::probe::search::panel_rule_weight_is_scale_invariant` |
//!
//! 三条线**颜色角色不许互相替换**（20.8 的 `Outline` 与 §22 的 `OutlineVariant` 是不同语义），
//! 所以量测助手 [`line_bands`] 的满档色是参数，不是写死的。

use std::path::Path;

use unlucky_design::color::{Role, SchemeFlavor};

use crate::theme::Ui;

use super::scan::{rgb, ui as ui_of, Rgb, Shot};

/// 在 `shot` 上验证分隔线是一条直线；`label` 只用于日志（`light` / `dark`）。
pub(super) fn check(shot: &Shot, ui: &Ui, label: &str) {
    let surface = rgb(ui.color(Role::Surface));
    let variant = rgb(ui.color(Role::SurfaceVariant));
    let (y, x0, x1, color) = shot
        .find_thin_line(surface, variant, 200, 120)
        .unwrap_or_else(|| panic!("[{label}] 找不到 Secondary 那一行细线（锚定失败）"));
    let segments = shot.line_segments(y, x0, x1);
    eprintln!(
        "[probe] divider anchored at y={y} span=x{x0}..{x1} color={color:?} segments={segments} \
         ({label}, {shot_width}x{shot_height})",
        shot_width = shot.width,
        shot_height = shot.height,
    );
    assert_eq!(
        segments, 1,
        "[{label}] Secondary 分隔线应是一条直线，实测 {segments} 段（量到 y={y}, x{x0}..{x1}）——\
         分段画会在交界处留缝"
    );
}
// ---------------------------------------------------------------- M454 / M456：线的重量

/// 一条 1px 线最多摊几行（物理行）：1.5 档上「半行 + 整行 + 半行」= 3 行，4 是留白。
///
/// 这条滤的是**面**不是线：Tabs 那一屏的「Primary 五态」行里，hover/pressed/focused 三个相邻格
/// 各带一层 8%/10% 状态层，连成一条 ~330 逻辑 px 宽、38 行高的淡带——它同样满足「长横墨迹」，
/// 只能靠厚度区分（M456 实测：不滤它就会把 `bands.len()` 判成 2）。
const MAX_BAND_ROWS: usize = 4;

/// 一条线带：`(y0, y1, 逐行覆盖率, 有效逻辑粗)`。
type Band = (usize, usize, Vec<f32>, f32);

/// 三档缩放的结果：`(scale, 该档的线带)`。
type ScaledBands = Vec<(f32, Vec<Band>)>;

/// 一幅图里**所有长横线**的 `(y0..=y1, 逐行覆盖率, 有效逻辑粗)`。
///
/// 为什么不用 `find_thin_line`：它按「Surface → 某个角色色」的插值身份去锚，只认一条线；
/// 而这两个场景各有自己的条数（§22 一屏两条、Tabs 一屏一条），要的是**全部**长横线。
///
/// `ink` 是这条线的满档色（`OutlineVariant` / `SurfaceVariant`），`min_run` 是「这一行算不算线」
/// 的最短墨迹段（**逻辑**像素，这里按 `scale` 换成物理）。像素归属按「距底色的偏移不超过满档
/// ink」判：比 ink 更暗的文字因此被排除，不会被当成线。
///
/// 取样窗口取**这条线自己的跨度**的中间一半（M456）：§22 的线通栏 1132 宽，Tabs 的线只有 242 宽，
/// 早先写死的 `width/4..width/2` 落在 Tabs 那条线外面（会量到 0.00）。
fn line_bands(shot: &Shot, ui: &Ui, ink: Rgb, scale: f32, min_run: f32) -> Vec<Band> {
    let surface = rgb(ui.color(Role::Surface));
    let min_run = (min_run * scale).round() as usize;
    // 「是这条线的墨」：每个通道的偏移量不超过满档偏移（+2 容差），且明显偏离底色。
    // `>= 2` 而不是 `> 2`：幽灵行的位移常常正好是 2/255（9% 覆盖 × 23 的满档差），`>2` 会把它丢掉。
    let is_ink = |pixel: Rgb| {
        let mut moved = 0;
        for i in 0..3 {
            let full = i32::from(ink[i]) - i32::from(surface[i]);
            let delta = i32::from(pixel[i]) - i32::from(surface[i]);
            if (full >= 0) != (delta >= 0) && delta.abs() > 2 {
                return false;
            }
            if delta.abs() > full.abs() + 2 {
                return false;
            }
            if delta.abs() >= 2 {
                moved += 1;
            }
        }
        moved > 0
    };
    let longest_run = |y: usize| -> Option<(usize, usize)> {
        let mut best: Option<(usize, usize)> = None;
        let mut start = 0;
        let mut run = 0;
        for x in 0..shot.width {
            if is_ink(shot.at(x, y)) {
                if run == 0 {
                    start = x;
                }
                run += 1;
                if best.is_none_or(|(a, b)| b - a + 1 < run) {
                    best = Some((start, x));
                }
            } else {
                run = 0;
            }
        }
        best.filter(|(a, b)| b - a + 1 >= min_run)
    };
    let mut bands = Vec::new();
    let mut y = 0;
    while y < shot.height {
        if longest_run(y).is_none() {
            y += 1;
            continue;
        }
        let y0 = y;
        let mut span: Option<(usize, usize)> = None;
        while y < shot.height {
            let Some(run) = longest_run(y) else { break };
            span = Some(match span {
                Some((a, b)) => (a.min(run.0), b.max(run.1)),
                None => run,
            });
            y += 1;
        }
        let y1 = y - 1;
        if y1 - y0 + 1 > MAX_BAND_ROWS {
            // 太厚 ⇒ 不是线（是状态层/填充块），跳过
            continue;
        }
        let Some((x0, x1)) = span else { continue };
        // 中间一半：两端各有端点/圆角抗锯齿，文字也可能压边
        let inset = (x1 - x0) / 4;
        let (lo, hi) = (x0 + inset, x1 - inset);
        let mut rows = Vec::new();
        let mut total = 0.0;
        for row in y0..=y1 {
            let t = row_coverage(shot, row, lo, hi, surface, ink);
            total += t;
            rows.push(t);
        }
        bands.push((y0, y1, rows, total / scale));
    }
    bands
}

/// `y` 行在 `lo..=hi` 上的平均覆盖率（0..=1；满覆盖 = 1.0）。
fn row_coverage(shot: &Shot, y: usize, lo: usize, hi: usize, surface: Rgb, ink: Rgb) -> f32 {
    let mut sum = 0.0;
    for x in lo..=hi {
        let pixel = shot.at(x, y);
        let mut channels = 0.0;
        let mut covered = 0.0;
        for i in 0..3 {
            let full = f32::from(ink[i]) - f32::from(surface[i]);
            if full.abs() < 1.0 {
                continue;
            }
            covered += ((f32::from(pixel[i]) - f32::from(surface[i])) / full).clamp(0.0, 1.0);
            channels += 1.0;
        }
        if channels > 0.0 {
            sum += covered / channels;
        }
    }
    sum / (hi - lo + 1) as f32
}

/// 三档缩放的图 → `(scale, 线带)`。分数档缺图只打印跳过（100% 那张是既有图，缺了直接红）。
fn bands_over_scales(
    dir: &Path,
    ui: &Ui,
    ink: Role,
    min_run: f32,
    cases: [(&str, f32); 3],
) -> ScaledBands {
    let ink = rgb(ui.color(ink));
    let mut measured = Vec::new();
    for (name, scale) in cases {
        let path = dir.join(name);
        if !path.exists() {
            assert!(
                scale != 1.0,
                "[weight] 缺 {name}：100% 档的图是既有的那张（出图命令见本文件模块文档）"
            );
            eprintln!("[weight] {name} 不存在，{scale} 档跳过（出图命令见 probe/divider.rs 文档）");
            continue;
        }
        let shot = Shot::load_unchecked(&path);
        let expect = (
            (1180.0 * scale).round() as usize,
            (760.0 * scale).round() as usize,
        );
        assert_eq!(
            (shot.width, shot.height),
            expect,
            "[weight {scale}] {name} 的尺寸不对：`--scale-factor` 只改光栅密度，\
             逻辑布局恒 1180×760"
        );
        measured.push((scale, line_bands(&shot, ui, ink, scale, min_run)));
    }
    measured
}

/// 逐条线断言「有效逻辑粗 ≈ 1.00」，并在分数档上要求墨量摊在两行（AA 幽灵）。
fn assert_weight(scale: f32, bands: &[Band], what: &str) {
    for (index, (y0, y1, rows, logical)) in bands.iter().enumerate() {
        eprintln!(
            "[weight] scale={scale} {what} line{index} rows={y0}..={y1} profile={rows:?} \
             logical={logical:.2}"
        );
        assert!(
            (0.90..=1.10).contains(logical),
            "[weight scale={scale} {what} line{index}] 1px 线的有效逻辑粗应恒 ≈1.00\
             （与同帧 1px `Border` 卡片描边同形：1+0.26 / 1+0.50），实测 {logical:.2}\
             （行 {y0}..={y1}，逐行 {rows:?}）——被 crisp 吸附成整物理行时，\
             125% 下取整成 0.80、150% 上取整成 1.33"
        );
        if scale.fract() > 0.01 {
            let heaviest = rows.iter().copied().fold(0.0, f32::max);
            let share = heaviest / (logical * scale);
            assert!(
                share < 0.85,
                "[weight scale={scale} {what} line{index}] 分数缩放下这条 1px 线的墨量必须摊在\
                 两行上（AA 幽灵）：最重那一行独占 {share:.2}（逐行 {rows:?}）——\
                 单行独占 = 被吸附成整物理行"
            );
        }
    }
}

/// M454：**§22 的两条 1px 分隔线在 100% / 125% / 150% 三档上的有效逻辑粗必须恒 ≈1.00**，
/// 且分数档上墨量必须摊在两行（AA 幽灵）——与同帧走 `Border` 掩膜的 1px 卡片描边同形。
///
/// 为什么单独立一条：本文件的 `check()` 只管「断没断」，对粗细无话可说。M448 实测 wgpu 上
/// 125% 有效粗 **0.80**、150% **1.33**（`Background` 填充的 1px 盒被 `crisp` 吸附成整物理行），
/// 而同一帧的 1px 卡片描边三档恒 **1.00**——同一帧两条同令牌线差 33%。
///
/// 出图（worktree 根；`divider-light.png` 就是 `gallery::probe` 那张，另外两档是 M454 新增）：
///
/// ```text
/// EXE=${CARGO_TARGET_DIR:-target}/debug/unlucky-launcher-iced
/// OUT=DevDocs/artifacts/m456/shots/probe-wgpu
/// for S in 1 1.25 1.5; do ULL_GALLERY_SECTION=divider $EXE --renderer=gpu \
///   --screen=gallery --screenshot "$OUT/divider-$S.png" --screenshot-theme=light \
///   --scale-factor $S; done    # 落成 divider-light.png / divider-s125-light.png / divider-s150-light.png
/// ULL_NAV_DIR=$OUT cargo test -p unlucky-app-iced navigation::probe::divider
/// ```
#[test]
fn hairline_weight_is_scale_invariant() {
    let Some(dir) = super::shot_dir() else {
        eprintln!(
            "[navigation::probe] SKIPPED：未设置 ULL_NAV_DIR（M454 重量判据要三档缩放的图，\
             出图命令见 `probe/divider.rs` 模块文档）"
        );
        return;
    };
    let ui = ui_of(SchemeFlavor::Light);
    let cases = [
        ("divider-light.png", 1.0_f32),
        ("divider-s125-light.png", 1.25),
        ("divider-s150-light.png", 1.5),
    ];
    let measured = bands_over_scales(&dir, &ui, Role::OutlineVariant, 200.0, cases);
    for (scale, bands) in &measured {
        assert_eq!(
            bands.len(),
            2,
            "[weight {scale}] §22 这一屏正好两条线（通栏 + 列表内缩），实测 {} 条：{bands:?}",
            bands.len()
        );
        assert_weight(*scale, bands, "§22");
    }
}

/// M456：**Tabs 的 Secondary 分隔线**（`SurfaceVariant`，1px）三档 × 两后端同样是有效逻辑粗 ≈1.00。
///
/// 这条线在 M456 之前是 `tabs.rs` 自己一处的 `SurfaceVariant` 填充盒，因此继承了 M454 那个缺陷
/// （wgpu 上 125% ⇒ 0.80、150% ⇒ 1.33）；改成 `divider::hairline` 后必须与 §22 那两条同形。
/// 出图用 `ULL_GALLERY_SECTION=navigation ULL_NAV_PART=tabs`（**当前 harness 就能重出**，
/// 不依赖任何副本工程）。
#[test]
fn tabs_hairline_weight_is_scale_invariant() {
    let Some(dir) = super::shot_dir() else {
        eprintln!(
            "[navigation::probe] SKIPPED：未设置 ULL_NAV_DIR（Tabs 分隔线的重量判据要三档缩放的图，\
             出图命令见 `probe/divider.rs` 模块文档）"
        );
        return;
    };
    let ui = ui_of(SchemeFlavor::Light);
    let cases = [
        ("tabs-light.png", 1.0_f32),
        ("tabs-s125-light.png", 1.25),
        ("tabs-s150-light.png", 1.5),
    ];
    // `min_run` 150（逻辑）：这条线只有 ~242 逻辑 px 长（三个 tab 的内容宽），
    // 而 §22 那两条是通栏 1132。阈值仍远高于任何文字行的连续墨迹段（最长的标签不到 100）。
    let measured = bands_over_scales(&dir, &ui, Role::SurfaceVariant, 150.0, cases);
    for (scale, bands) in &measured {
        assert_eq!(
            bands.len(),
            1,
            "[weight {scale}] Tabs 这一屏只有一条长横线（Secondary 分隔线），实测 {} 条：{bands:?}\
             ——多出来的那条要么是新画了一条线，要么是某行底色被判成墨",
            bands.len()
        );
        assert_weight(*scale, bands, "tabs");
    }
}
