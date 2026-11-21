//! M456 probe · search（从 `probe.rs` 拆出：单文件 ≤400 行，AGENTS.md §3-1）。
//!
//! ## 出图（**当前 harness 就能重出**，M456 起不再依赖已退役的副本工程）
//!
//! M77 时代这四张图来自一个**副本工程**（`target/m77/sync.py` 把 `inputs::section()` 换成单个
//! `part`，`M77_PART=text` 出图）。那个工程与它的产物**都已不在仓里**，于是这段探针长期
//! 「写得出来、跑不起来」——M454 因此**故意不改** `inputs/search.rs` 的那条线（改了就会留下一条
//! 无法验证的判据）。M456 把出图路径换成 harness 自己的段落开关：
//!
//! ```text
//! EXE=${CARGO_TARGET_DIR:-target}/debug/unlucky-launcher-iced   # 一树一 target
//! OUT=DevDocs/artifacts/m456/shots/probe-wgpu && mkdir -p "$OUT"
//! # 100% / 125% / 150% 三档（`--scale-factor` 只改光栅密度，逻辑布局恒 1180×760）
//! for S in 1 1.25 1.5; do ULL_GALLERY_SECTION=inputs $EXE --renderer=gpu --screen=gallery \
//!   --screenshot "$OUT/search-$S.png" --screenshot-theme=light --scale-factor $S; done
//! # 落成 search-light.png / search-s125-light.png / search-s150-light.png 后：
//! ULL_INPUTS_DIR=$OUT cargo test -p unlucky-app-iced inputs::probe::search
//! ```
//!
//! ## 这张图上量得到什么、量不到什么
//!
//! `inputs` 段整段比 760 的视口高，所以**面板底部被切掉**：`panel.top(511) + 56 + 1 + 240 = 808 > 760`。
//! 因此：
//!
//! | 项 | 这张图上 | 判据在哪 |
//! |---|---|---|
//! | 条高 56 / 宽 ≥360 / 图标墨迹偏移 20（20.1 / 20.2 / 20.4） | 量得到 | 本文件 |
//! | 面板宽 360、头部高 56、头/体相位 = 1dp（20.6 / 20.8） | 量得到 | 本文件 |
//! | 头部分隔线 1dp 且**三档两后端重量恒 1.00**（20.8 / M456） | 量得到 | 本文件（[`panel_rule_weight_is_scale_invariant`]） |
//! | 结果区最小高 240（20.9） | **量不到**（被视口切掉 48 行） | `inputs/search/tests.rs` 的令牌断言；本文件只断言「确实被切掉了」 |
//!
//! 最后一行是刻意的：断言「被切掉」而不是悄悄删掉那条判据——将来若有块选择器让面板整块进屏，
//! 这条会红，提醒把 `assert_eq!(body.height(), 240)` 装回去。

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;

use super::*;

/// 20.1 / 20.4 / 20.6 / 20.8 / 20.9：条 56、图标墨迹偏移 20（= 16 + viewBox 4）、
/// docked 面板 360 / 头 56 / 分隔线 1.0 `Outline` / 体 240（被视口切掉，见模块文档）。
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
        // 1px 的圆角边缘会有一行抗锯齿落在 ±10 容差外（CPU 档实测 55）⇒ 判 55..=56；
        // 42 / 48 这类真回归仍落区间外。
        assert!(
            (55..=56).contains(&bar.height()),
            "20.1 搜索条高必须 56（实测 {}）",
            bar.height()
        );
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

        // 20.6：docked 面板 360 宽，头 56 + 1px 分隔线 + 体 240
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
        assert!(
            (2..=3).contains(&(body.top - header.bottom)),
            "20.8 头与体之间正好 1dp（CPU 档摊成 2 行 ⇒ 隔 2~3 行）：实测 {}",
            body.top - header.bottom
        );
        // 20.9 的 240 **在这张图上量不到**：`inputs` 段比 760 的视口高，布局把结果区压成 168
        // （= 760 − 511 − 56 − 1 − 24 的页底留白）。这里断言「被压扁 + 面板底边离图底正好一个
        // 页留白」两条，而不是把那条判据悄悄删掉：面板一旦能整块进屏，这里会红，提醒把
        // `assert_eq!(body.height(), 240)` 装回去（M456 记录的 harness 口径限制）。
        assert!(
            body.height() < 240,
            "20.9 结果区在这张图上被压到 {}（面板比视口高）；若它已经等于 240，说明面板能整块进屏，\
             请把 `assert_eq!(body.height(), 240)` 装回来",
            body.height()
        );
        assert_eq!(
            shot.height - 1 - body.bottom,
            ui.space(Space::X6) as usize,
            "被压扁的结果区应正好顶到页面底留白（`Space::X6` = 24）"
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

/// M456：**20.8 那条 1px `Outline` 线在 100% / 125% / 150% 三档上的有效逻辑粗必须稳定。**
///
/// 它原来是本文件自己一处的 `Role::Outline` 填充盒，与 M454 修的 §22 分隔线是同一个缺陷：
/// wgpu 的 `crisp` 吸附把 1 逻辑 px 的填充盒按整物理像素取整 ⇒ **125% 上量到 1.60、150% 上 0.67**
/// （M456 改前实测，见 `DevDocs/artifacts/m456/README.md`）。改成 `divider::hairline`
/// （`snap: false` 的自绘 quad）后 wgpu 三档 0.81 / 0.85 / 0.92，CPU 三档 1.00 / 1.00 / 1.00。
///
/// ## 两档绝对值为什么不同（0.81 vs 1.00），以及判据为什么是这个形状
///
/// 同一条 1px 线：**CPU 后端（tiny-skia）在 sRGB 空间混合，wgpu 在**线性**空间混合**，而本探针的
/// 覆盖率口径是 sRGB 比值（与 §22 那条判据同一口径）。换算验证（`after-search-s100-wgpu` 那两行
/// `(192,194,196)` / `(171,174,177)`）：
///
/// | 后端 | sRGB 口径 | 线性口径 |
/// |---|---|---|
/// | wgpu | **0.81** | **1.00** |
/// | CPU | **1.00** | **1.18** |
///
/// 也就是说两边画出来的**几何高度都是 1.00 px**，差别只在「哪个域里做混合」。所以：
///
/// - 绝对区间取 **0.75~1.15**：两个后端、三档缩放全落得进去（实测 0.81~1.00），而「线没了（≈0）」
///   「线画成 2px（≈2）」照样红。§22 那两条是**低对比**（`OutlineVariant` 对 `Surface`，满档差 ~23），
///   两个域几乎一致，所以它们的判据仍是 M454 的 0.90~1.10，不动。
/// - **极差 ≤0.25** 才是这条判据的主角：改前 1.60-0.67 = **0.93**，改后 0.11。「重量随缩放档跳变」
///   正是 M448/M454 那个缺陷，而它与对比度、后端、混合域全都无关。
///
/// 量法：面板的**内带**（左右各缩 40，避开图标/输入框），在头部下沿那个 12 行的窗口里逐行求
/// 「相对面板底色的覆盖率」之和，再除以 scale 得到有效逻辑粗。窗口固定取 `panel.top+50 .. +62`：
/// 行高 56 ⇒ 线在 +56，窗口上界在正文第一行之前，所以窗口里只有这条线。
#[test]
fn panel_rule_weight_is_scale_invariant() {
    let dir = require_dir!();
    let ui = ui(SchemeFlavor::Light);
    let cases = [
        ("search-light.png", 1.0_f32),
        ("search-s125-light.png", 1.25),
        ("search-s150-light.png", 1.5),
    ];
    let mut measured: Vec<(f32, f32, Vec<f32>)> = Vec::new();
    for (name, scale) in cases {
        let path = dir.join(name);
        if !path.exists() {
            assert!(
                scale != 1.0,
                "[search weight] 缺 {name}：100% 档是既有那张（出图命令见本文件模块文档）"
            );
            eprintln!("[search weight] {name} 不存在，{scale} 档跳过");
            continue;
        }
        let shot = Shot::load(&path).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            (shot.width, shot.height),
            (
                (1180.0 * scale).round() as usize,
                (760.0 * scale).round() as usize
            ),
            "[search weight {scale}] {name} 的尺寸不对：`--scale-factor` 只改光栅密度，\
             逻辑布局恒 1180×760"
        );
        let panel = panel_block(&shot, &ui, scale);
        let surface = rgb(ui.color(Role::SurfaceContainerHigh));
        let ink = rgb(ui.color(Role::Outline));
        let inset = (40.0 * scale).round() as usize;
        let (x0, x1) = (panel.left + inset, panel.right - inset);
        let (y0, y1) = (
            panel.top + (50.0 * scale).round() as usize,
            panel.top + (62.0 * scale).round() as usize,
        );
        let rows: Vec<f32> = (y0..y1)
            .map(|y| line_ink(&shot, x0, x1, y, y + 1, surface, ink))
            .collect();
        let logical: f32 = rows.iter().sum::<f32>() / scale;
        eprintln!(
            "[search weight] scale={scale} rows={y0}..{y1} profile={rows:?} logical={logical:.2}"
        );
        assert!(
            (0.75..=1.15).contains(&logical),
            "[search weight scale={scale}] 20.8 的分隔线有效逻辑粗出界（1px 应在 0.75~1.15），\
             实测 {logical:.2}（逐行 {rows:?}）——线消失或画成 2px 都会落在这里"
        );
        // 线色身份：最重那一行的众数色必须等于「面板底色按该行覆盖率混合 `Outline`」的结果。
        // 这条挡的是「把 20.8 换成别的角色色」：换成更深的 `OnSurface` 覆盖率会被夹到 1.0，
        // 实测色于是比预测的浅很多，直接红。
        let heaviest = (y0..y1)
            .max_by(|a, b| {
                rows[*a - y0]
                    .partial_cmp(&rows[*b - y0])
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("窗口里至少有一行");
        let coverage = rows[heaviest - y0];
        let modal = modal_row(&shot, heaviest, x0, x1);
        let predicted = blend(surface, ink, coverage);
        assert!(
            modal.iter().zip(predicted).all(|(a, b)| a.abs_diff(b) <= 4),
            "[search weight scale={scale}] 线色不是 20.8 的 `Role::Outline`：最重那一行 \
             (y={heaviest}, 覆盖率 {coverage:.2}) 的众数色 {modal:?}，按 `Outline` 混合应是 \
             {predicted:?}"
        );
        measured.push((scale, logical, rows));
    }
    assert!(
        !measured.is_empty(),
        "[search weight] 一张图都没有，判据等于没跑"
    );
    let (min, max) = measured.iter().fold((f32::MAX, f32::MIN), |(lo, hi), m| {
        (lo.min(m.1), hi.max(m.1))
    });
    assert!(
        max - min <= 0.25,
        "[search weight] 同一根 1px 线在三档缩放上的有效逻辑粗极差应 ≤0.25，实测 {:.2}\
         （{measured:?}）——改回填充盒时 125% 会量到 1.60、150% 掉到 0.67（极差 0.93）",
        max - min
    );
}

/// 面板块（宽 = 360 × scale 的 `SurfaceContainerHigh`，按 top 取第一个 = 头部）。
///
/// 为什么不用 `SurfaceContainerHighest`（M77 那版的口径）：两者在浅色档只差 (8,7,7)，落在
/// 探针的 ±10 容差里，于是「按 Highest 找面板」实际上是靠容差蒙对的。这里按面板自己的角色找。
fn panel_block(shot: &Shot, ui: &Ui, scale: f32) -> BBox {
    let color = rgb(ui.color(Role::SurfaceContainerHigh));
    let width = (360.0 * scale).round() as usize;
    let mut blocks: Vec<BBox> = shot
        .bbox_of(color, 2000)
        .into_iter()
        .filter(|b| b.width().abs_diff(width) <= 3 && b.height() > 20)
        .collect();
    blocks.sort_by_key(|b| b.top);
    *blocks
        .first()
        .unwrap_or_else(|| panic!("找不到 docked 面板（宽 {width} ±3 的 SurfaceContainerHigh 块）"))
}

/// `y` 行 `[x0, x1]` 上出现次数最多的颜色（分隔线的线色）。
fn modal_row(shot: &Shot, y: usize, x0: usize, x1: usize) -> Rgb {
    let mut counts: std::collections::HashMap<Rgb, usize> = std::collections::HashMap::new();
    for x in x0..=x1.min(shot.width - 1) {
        *counts.entry(shot.at(x, y)).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(color, _)| color)
        .unwrap_or(shot.at(x0, y))
}

/// 底色 `surface` 与满档色 `ink` 按覆盖率 `t` 混合出来的颜色。
fn blend(surface: Rgb, ink: Rgb, t: f32) -> Rgb {
    let mut out = [0u8; 3];
    for i in 0..3 {
        let value = f32::from(surface[i]) + (f32::from(ink[i]) - f32::from(surface[i])) * t;
        out[i] = value.round().clamp(0.0, 255.0) as u8;
    }
    out
}
