//! 反馈家族的**渲染层**几何断言：对着真截图量像素（M68 的教训：纯函数对、图可能是错的）。
//!
//! 为什么住在 `feedback/` 而不是 `gallery/`：`gallery/feedback.rs` 只负责把矩阵摆进 harness，
//! 断言加上去会把它顶到 400 行红线。像素工具在 [`super::pixel`]，环形与两个「面」的判据
//! 拆在同级子模块 [`circular`] / [`panels`]（`inputs/probe/` 的同一先例）。
//!
//! ## 量什么（每条对一个具体错误，打红它的是「变异」那一列）
//!
//! | 类 | 判据 | 打红它的变异 |
//! |---|---|---|
//! | 线性厚度 / 圆角 | 轨道带**恰好 4 行**；条的四角**透出底色** | 厚度 4→6 / `Full`→`None` |
//! | 线性不定态 | phase 0.55 列必须**数出两段**（P1-A 回归） | 段不排序（旧实现：两段压成一段） |
//! | 环形 | 环 40 且四角无墨（真圆） | 直径 40→48 |
//! | Snackbar 宽上下限 | 宿主 800→**600**、368→**344**、324→**300**（无下限 G17-d） | 上限 600→560 |
//! | Snackbar 高 | 单行带 **48** / 双行带 **68** | 48→40 |
//! | Dialog 宽 / 内距 | 最宽行 **480**（在 280..560 内）· 标题墨迹左缩 **≈24**、顶缩 **≈26** | 480→488 / pad 24→16 |
//! | 空态块 / 间距 / CTA | 块 **96×96** · 块↔标题 **≈16**+bearing · 条目总高 **≈232** · CTA **40** | 96→88 / spacing 双算回来 |
//! | scrim | 一块 ≥700 宽的遮罩带，颜色 = 预合成实色 | 遮罩改成 surface |
//!
//! ## 复现（worktree 根目录；四个部分各一张图，浅/深各一轮）
//!
//! ```text
//! for part in progress snackbar dialog empty; do
//!   ULL_GALLERY_SECTION=feedback ULL_FEEDBACK_PART=$part \
//!     ./target/debug/unlucky-launcher-iced --software-renderer --screen=gallery \
//!     --screenshot target/m78/$part-light.png --screenshot-theme=light
//!   ULL_FEEDBACK_PART=$part ULL_GALLERY_SHOT=target/m78/$part-light.png \
//!     cargo test -p unlucky-app-iced feedback::probe
//! done
//! ```

mod circular;
mod panels;
pub(super) mod track;

use unlucky_design::color::{Role, SchemeFlavor};
use unlucky_design::typography::Platform;

use super::geometry::PHASE_055;
use super::pixel::{band_width, bands, bands_by_run, matches, near, rgb, runs, shot_path, Shot};
use crate::theme::{Families, Ui};

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(flavor, Platform::Windows, FAMILIES)
}

/// 第三条（phase 0.55）线性轨道的左缘与宽度（gallery 的宿主尺寸：页内边距 24 + 2×332）。
const TRACK3_X0: usize = 688;
/// 单条轨道宽（gallery 的 `TRACK_HOST`）。
const TRACK3_W: usize = 320;

/// 认定为「主色进度行」所需的连续主色段长度。
///
/// phase 0.55 的两段分别是 ≈24px 与 ≈165px，取 60 既收得住第二段、又挡得住文字行（实测 ≤22px）。
const INDETERMINATE_MIN_RUN: usize = 60;

/// 一条测试覆盖当前截图对应的那一段（段由 `ULL_FEEDBACK_PART` 指定，默认 `all`）。
#[test]
fn feedback_geometry_matches_the_reference() {
    let Some(path) = shot_path() else {
        eprintln!(
            "[feedback-probe] SKIPPED：未设置 ULL_GALLERY_SHOT。这是**渲染层**几何断言，\
             需要先出图（命令见本文件模块文档，注意 ULL_GALLERY_SECTION=feedback + ULL_FEEDBACK_PART）。"
        );
        return;
    };
    let shot = Shot::load(&path).expect("读 feedback 截图");
    assert_eq!(
        (shot.width, shot.height),
        (1180, 760),
        "验收图必须 1180×760 @100% DPI"
    );

    let flavor = if matches(
        shot.at(2, 2),
        rgb(ui(SchemeFlavor::Light).color(Role::Surface)),
    ) {
        SchemeFlavor::Light
    } else {
        SchemeFlavor::Dark
    };
    let ui = ui(flavor);
    let part = std::env::var("ULL_FEEDBACK_PART").unwrap_or_else(|_| "all".to_owned());
    let part = part.trim().to_ascii_lowercase();

    match part.as_str() {
        "progress" => {
            check_linear(&shot, &ui);
            circular::check(&shot, &ui);
        }
        "snackbar" => check_snackbar(&shot, &ui),
        "dialog" => panels::check_dialog(&shot, &ui),
        "empty" => panels::check_empty(&shot, &ui),
        other => {
            // 整屏时只查最不容易被折叠线切掉的那一类；分图才是逐项判据的场合。
            check_linear(&shot, &ui);
            eprintln!("[feedback-probe] part={other:?} 只查了 progress 那一类；其余请在分图里查");
        }
    }

    eprintln!("[feedback-probe] OK flavor={flavor:?} part={part}");
}

/// §12.1 + §12.4：线性厚度 / 圆角 / **不定态两段**。
fn check_linear(shot: &Shot, ui: &Ui) {
    // 参考值写字面量（A1）：**不能**从 `LinearGeometry` 读回期望值——那样把 4 改成 6 之后
    // 「实测 6 == 期望 6」会照样绿，等于没有断言。
    let surface = ui.color(Role::Surface);
    let active = ui.color(Role::Primary);

    // §12.1（厚度 / 宽度 / 圆角）搬去 `probe/track.rs`：那边撞了 400 行红线，见该文件文档。
    track::check(shot, ui);

    // ---- 12.4 不定态：phase 0.55 那一列必须**数出两段**（复审 P1-A 的回归断言）----
    //
    // 旧实现把两段按返回顺序铺 `FillPortion`（第一线在右、第二线在左），并且份额之和
    // 在某 34/100 相位上 >1，被 iced 归一 ⇒ 图上只剩一段 100px 的台阶（x 749..848）。
    let row = indeterminate_row(shot, active, surface);
    let found: Vec<(usize, usize)> = runs(shot, row, active, surface)
        .into_iter()
        .filter(|(start, len)| {
            *start >= TRACK3_X0 && start + len <= TRACK3_X0 + TRACK3_W + 2 && *len >= 8
        })
        .collect();
    assert_eq!(
        found.len(),
        2,
        "12.4：phase 0.55 必须是**两段**（P1-A 修完才成立），实测 {found:?} @ 行 {row}"
    );

    // 期望位置与单测**共用同一个常量**（`geometry::PHASE_055`）：早先这里写 0.871、单测写
    // 0.860，实测 963 距 want_end 966.7 只剩 0.28px，靠容差活着（复审 B）。
    let expected = PHASE_055;
    for (index, (start, len)) in found.iter().enumerate() {
        let want_start = TRACK3_X0 as f32 + expected[index].0 * TRACK3_W as f32;
        let want_end = TRACK3_X0 as f32 + expected[index].1 * TRACK3_W as f32;
        assert!(
            (*start as f32 - want_start).abs() <= 4.0,
            "12.4 第 {} 段左端应在 {want_start:.1}（实测 {start}）",
            index + 1
        );
        assert!(
            ((start + len) as f32 - want_end).abs() <= 4.0,
            "12.4 第 {} 段右端应在 {want_end:.1}（实测 {}）",
            index + 1,
            start + len
        );
    }

    eprintln!("[feedback-probe] progress·12.4：phase 0.55 两段 {found:?}（行 {row}）");
}

/// 线性不定态那一行的 y：主色墨迹在**第一条轨道**里出现、且不在确定态行的最上一组。
///
/// 不想硬编码坐标（gallery 一改间距就失准），所以按「主色横带 + 排除最高那条带」找。
///
/// **按连续段判，不按计数判**（M248 修）：标题与图注的行里主色像素能凑到 20 个以上，
/// 按旧口径 `bands(.., 20)` 会把标题行（深色板实测 y≈64）当成第一组主色带，`found[1]`
/// 于是落在文字上、`runs` 只返回一个 11px 的段 —— 这条在**改前**就红（用改前的图 + 改前的
/// 探针复现过，与换轨道颜色无关）。轨道行是 60px 以上的**连续**主色段，文字行不是。
fn indeterminate_row(shot: &Shot, active: iced::Color, surface: iced::Color) -> usize {
    let found = bands_by_run(shot, active, surface, INDETERMINATE_MIN_RUN);
    assert!(
        found.len() >= 2,
        "至少要找到确定态行与不定态行两组主色横带，实测 {found:?}"
    );
    // 第一组是确定态（0% 无墨、50%/100% 有墨），第二组是不定态
    let (top, bottom) = found[1];
    (top + bottom) / 2
}

/// §14：宽度上下限 + 单/双行高。
fn check_snackbar(shot: &Shot, ui: &Ui) {
    // 参考值写字面量：600 = SnackbarKt.ContainerMaxWidth，52/72 = 规格卡（单行 52 + 每多一行 20）。
    let surface = ui.color(Role::Surface);
    let bar = ui.color(Role::InverseSurface);

    // 行墨量下限 200：最窄的那条只有 300 宽，里面还有 ~60px 的文字 ⇒ 每行 ~240 个实心点。
    // 高度 ≥40 才算提示条（标题行偶有一两行落在这条混色线上，那类 1 行的带要滤掉）。
    let found: Vec<(usize, usize)> = bands(shot, bar, surface, 200)
        .into_iter()
        .filter(|(top, bottom)| bottom - top + 1 >= 40)
        .collect();
    assert!(
        found.len() >= 3,
        "14.x：应有 ≥3 条提示条（上限宽 / 双行 / 两条窄的），实测 {}",
        found.len()
    );

    // 宽度集 = 每条带里最宽的那一行（留白行没有文字，量得到全宽）。
    let mut widths: Vec<usize> = found
        .iter()
        .map(|band| band_width(shot, *band, bar, surface))
        .collect();
    let mut heights: Vec<usize> = found.iter().map(|(top, bottom)| bottom - top + 1).collect();
    widths.sort_unstable();
    heights.sort_unstable();

    assert!(
        !widths.iter().any(|w| *w > 601),
        "没有一条可以超过上限 600：实测 {widths:?}"
    );
    assert!(
        widths.iter().any(|w| near(*w, 600)),
        "上限宽必须是 600（SnackbarKt.ContainerMaxWidth，宿主 800 时夹取），实测宽度集 {widths:?}"
    );
    assert!(
        widths.iter().any(|w| near(*w, 344)),
        "宿区 368 那格必须量到 344（G17-d [估] 最小宽口径），实测 {widths:?}"
    );
    assert!(
        widths.iter().any(|w| near(*w, 300)),
        "宿区 324 那格必须量到 300（组件没有 min-width，G17-d），实测 {widths:?}"
    );
    assert!(
        heights.iter().any(|h| near(*h, 52)),
        "规格卡：单行高 52，实测高度集 {heights:?}"
    );
    assert!(
        heights.iter().any(|h| near(*h, 72)),
        "双行高 72（52 + 20），实测高度集 {heights:?}"
    );

    // 14.6 动作色：`InversePrimary` 必须出现在提示条内（不是按钮家族的 OnSurfaceVariant）
    let action = rgb(ui.color(Role::InversePrimary));
    let action_pixels: usize = (0..shot.height)
        .map(|y| {
            (0..shot.width)
                .filter(|x| matches(shot.at(*x, y), action))
                .count()
        })
        .sum();
    assert!(
        action_pixels > 40,
        "14.6：动作标签应是 InversePrimary（实测该色只有 {action_pixels} px）"
    );

    // ---- 位置语义（复审 D）：两条 bottom 条的**条底距取景框底 = 12** ---------------
    //
    // 取景框是 gallery 给窄宿主画的 1px `OutlineVariant` 边（`framed_host`）：透明宿主没有
    // 可见底边时「贴底」在像素上不可证（旧宿主 72 = 48 + 2×12，上下留白相等）。
    let frame = rgb(ui.color(Role::OutlineVariant));
    let mut margins: Vec<usize> = Vec::new();
    for (top, bottom) in &found {
        let width = band_width(shot, (*top, *bottom), bar, surface);
        if !(near(width, 300) || near(width, 344)) {
            continue;
        }
        // 1px 描边常落在半像素上（两行各 ~50% 覆盖），精确色匹配抓不住 ⇒ 用「更靠近框色
        // 而不是画布」判。但条自身（深色）比画布**更接近**框色，所以还要一条：这一行的
        // 框色连续段必须**比条本身更宽**（框横贯整个宿主），否则会认成条自己（实测过：差值 1）。
        let distance = |pixel: [u8; 3], target: [u8; 3]| -> u32 {
            pixel
                .iter()
                .zip(target)
                .map(|(a, b)| u32::from(a.abs_diff(b)))
                .sum()
        };
        let surface_rgb = rgb(surface);
        let frame_run = |y: usize| -> usize {
            let mut longest = 0;
            let mut current = 0;
            for x in 0..shot.width {
                let pixel = shot.at(x, y);
                if distance(pixel, frame) < distance(pixel, surface_rgb) {
                    current += 1;
                    longest = longest.max(current);
                } else {
                    current = 0;
                }
            }
            longest
        };
        // 框的横贯长度 > 条宽 + 10 ⇒ 只有框那一两行满足
        let is_frame_row = |y: usize| frame_run(y) >= width + 10;

        let frame_row = ((bottom + 1)..shot.height)
            .find(|y| is_frame_row(*y))
            .unwrap_or_else(|| {
                panic!("D：窄条下方没找到宿主取景框（条 y {top}..{bottom}，宽 {width}）")
            });
        let frame_top_row = (0..*top)
            .rev()
            .find(|y| is_frame_row(*y))
            .unwrap_or_else(|| panic!("D：窄条上方没找到宿主取景框（条顶 y {top}）"));
        // 复审 P2-1：宿高回退到 72（上下留白相等）时这条会红。
        assert!(
            (*top as i64 - frame_top_row as i64 - 36).abs() <= 1,
            "D：贴底 ⇒ 框顶到条顶应为 36（96 − 48 − 12），实测 {}",
            *top as i64 - frame_top_row as i64
        );
        let frame_height = frame_row - frame_top_row + 1;
        assert!(
            frame_height.abs_diff(96) <= 1,
            "D：取景框高应 96（宿主高），实测 {frame_height}"
        );
        margins.push(frame_row - bottom);
    }
    assert!(
        !margins.is_empty(),
        "D：应至少量到一条 bottom 条的宿主边距，实测 {margins:?}"
    );
    for margin in &margins {
        assert!(
            near(*margin, 12),
            "D：贴底 ⇒ 条底距宿主底 12（官方外边距），实测 {margin}：{margins:?}"
        );
    }

    eprintln!(
        "[feedback-probe] snackbar：宽度集 {widths:?} · 高度集 {heights:?} ·          InversePrimary {action_pixels}px · 底边距 {margins:?}"
    );
}
