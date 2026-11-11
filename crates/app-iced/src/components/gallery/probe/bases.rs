//! `ULL_GALLERY_SECTION=disabledbase` 的像素判据：**禁用态叠的是哪一层底**（M234）。
//!
//! ## 为什么必须用「精确匹配」而不是扫描工具的 `matches`（±10）
//!
//! Ink / Silver 板上 `Surface` 与 `SurfaceContainer` 只差个位数 RGB，10% 之后差 **≈1/255**：
//! `scan::matches` 的 ±10 容差会把「叠在画布上」与「叠在卡片上」判成同一个颜色——
//! 那样这条判据就永远绿。所以本文件自己数**逐位相等**的像素（[`count_exact`]），
//! 并且只数**板内**区域，避开页面底色。
//!
//! ## 量什么
//!
//! 1. 卡列 / 面板列里：禁用按钮容器（按钮族 10%）与禁用字段容器（面状族 12%）的像素值
//!    = 各自「@10%/12% 叠**该列板底**」；**不许**出现「叠画布底」那一档；
//! 2. 画布列（卡列左侧、同样的纵向范围）里反过来：出现画布档、且不是卡/面板档；
//! 3. 三档的期望值两两不同（哪一对相等，说明 `Base` 被丢了或两族 α 合流了）。
//!
//! ## 抓得住 / 抓不住
//!
//! - 抓得住：调用点把底传错（叠画布）或没传（默认 `Base::CANVAS`）——卡列里会出现画布档的像素
//!   而缺卡片档；两族 α 合流、入口内部写死 `Role::Surface`，同样在这里红。
//! - 抓不住：① 容器**自己**的画法（板底 vs 卡组件的 `BASE` 是否一致——那条由
//!   `surfaces::elevated::tests` 的「卡底必须是 SurfaceContainer」守）；
//!   ② 页面里的真实卡片（pages/ 的调用点尚未接 `.base(..)`，见报告「没验的部分」）；
//!   ③ GPU(wgpu) 档：本判据读的是 tiny-skia(`--software-renderer`) 出的图；
//!   ④ 差 <1/255 的第三层底（例如 `SurfaceContainer` 与 `SurfaceContainerHigh` 的 10%）——
//!   那种情况本文件会退化成「两档相等」，第 3 条断言会红，**不会静默放过**。

use unlucky_design::color::Role;

use super::super::scan::{rgb, Shot};
use crate::app::components::interaction::{self, Base};
use crate::app::components::surfaces;
use crate::theme::Ui;

/// 板子的最小尺寸（判据只认「一整列」，不许把某个控件误当成板）。
const MIN_PLATE_W: usize = 300;
const MIN_PLATE_H: usize = 260;
/// 禁用按钮容器在板内的最少像素（Filled 药丸 40 高 × ≥58 宽，扣掉字形与圆角）。
const MIN_BUTTON_PX: usize = 300;
/// 禁用字段容器的最少像素（板宽 ~336 × 56，扣掉标签与图标）。
const MIN_FIELD_PX: usize = 3000;
/// 「正确档位 : 错误档位」的最小倍数。
///
/// 为什么不干脆要求「错误档位 = 0 px」：**抗锯齿会造出中间色**——板底与控件容器之间那一圈
/// 1px 过渡像素会顺路撞上「另一档」的精确值（实测卡片列里 226/226/227 有 58 px，全是边缘）。
/// 真实回归的量级完全不同（整颗按钮 ≈3000 px、整块字段 ≈18000 px），所以判据写成**倍数**：
/// 正确档位必须比错误档位多一个数量级。
const STRAY_RATIO: usize = 10;

/// 逐位相等的像素计数器（`scan::matches` 的 ±10 容差在这里不够用，见模块文档）。
fn count_exact(shot: &Shot, color: [u8; 3], x0: usize, x1: usize, y0: usize, y1: usize) -> usize {
    (y0..y1.min(shot.height))
        .map(|y| {
            (x0..x1.min(shot.width))
                .filter(|x| shot.at(*x, y) == color)
                .count()
        })
        .sum()
}

/// 一块板（`left..right × top..bottom`，闭区间；`x1`/`y1` 是开区间上界）。
type Rect = (usize, usize, usize, usize);

/// 按**逐位相等**的颜色切出竖列：每列 = 一段连续且够宽的 x，y 取该段内的最小/最大行。
///
/// 为什么不用 `scan::bbox_of`（±10 容差）：本板的三档底（`Surface` / `SurfaceContainer` /
/// `SurfaceContainerLow`）两两只差 5~11 个 RGB 单位，落在容差里会被并成一块
/// （实测把卡片列与面板列判成同一块）。
fn exact_columns(shot: &Shot, color: [u8; 3]) -> Vec<Rect> {
    let mut columns = Vec::new();
    let mut current: Option<(usize, usize, usize, usize)> = None;
    for x in 0..shot.width {
        let rows: Vec<usize> = (0..shot.height)
            .filter(|y| shot.at(x, *y) == color)
            .collect();
        match (&mut current, rows.first()) {
            (None, Some(top)) => {
                current = Some((x, x + 1, *top, rows[rows.len() - 1] + 1));
            }
            (Some(col), Some(top)) => {
                col.1 = x + 1;
                col.2 = col.2.min(*top);
                col.3 = col.3.max(rows[rows.len() - 1] + 1);
            }
            (Some(col), None) => {
                if col.1 - col.0 >= MIN_PLATE_W && col.3 - col.2 >= MIN_PLATE_H {
                    columns.push(*col);
                }
                current = None;
            }
            (None, None) => {}
        }
    }
    if let Some(col) = current {
        if col.1 - col.0 >= MIN_PLATE_W && col.3 - col.2 >= MIN_PLATE_H {
            columns.push(col);
        }
    }
    columns
}

fn find_plate(shot: &Shot, role: Role, ui: &Ui, name: &str) -> Rect {
    let color = rgb(ui.color(role));
    let mut columns = exact_columns(shot, color);
    assert_eq!(
        columns.len(),
        1,
        "{name} 列（{role:?} 底，{color:?}）应当恰好是 1 列，实测 {}： 板子尺寸判据 MIN {MIN_PLATE_W}×{MIN_PLATE_H}",
        columns.len()
    );
    columns.remove(0)
}

/// 判据入口（`probe.rs` 的 `disabledbase` 分支调它）。
pub(super) fn check(shot: &Shot, ui: &Ui, flavor: unlucky_design::color::SchemeFlavor) {
    let card = Base::of(Role::SurfaceContainer);
    let panel = Base::of(Role::SurfaceContainerLow);

    let card_rect = find_plate(shot, Role::SurfaceContainer, ui, "card");
    let panel_rect = find_plate(shot, Role::SurfaceContainerLow, ui, "panel");
    // 画布列：三列等宽等距（`disabledbase::PLATE_WIDTH` 固定 360、间距 8），所以它 =
    // 卡列左侧同样宽的一列。画布板的底与页面同色（`Surface`），**没法**靠颜色切出来。
    let step = panel_rect.0 - card_rect.0;
    assert!(
        step > MIN_PLATE_W,
        "两列间距 {step} 不合理（列宽 {MIN_PLATE_W} 起）—— 布局变了，画布列的镜像推算不再成立"
    );
    let canvas_rect = (0, card_rect.0, card_rect.2, card_rect.3);
    let canvas_left = card_rect.0.saturating_sub(step);

    let button = |base: Base| rgb(interaction::disabled_button_colors(ui, base).0);
    let field = |base: Base| rgb(interaction::disabled_colors(ui, base).0);

    // 三档必须两两不同（哪一对相等就说明底被丢了 / 两族 α 合流了）
    let (canvas_button, canvas_field) = (button(Base::CANVAS), field(Base::CANVAS));
    let (card_button, card_field) = (button(card), field(card));
    let (panel_button, panel_field) = (button(panel), field(panel));
    assert_ne!(
        canvas_button, card_button,
        "{flavor:?}：画布底与卡片底算出来的按钮容器相同 —— 底被丢了"
    );
    assert_ne!(
        canvas_field, card_field,
        "{flavor:?}：画布底与卡片底算出来的字段容器相同"
    );
    assert_ne!(
        card_button, panel_button,
        "{flavor:?}：卡片底与面板底必须可分"
    );
    assert_ne!(
        card_field, panel_field,
        "{flavor:?}：卡片底与面板底必须可分"
    );
    assert_eq!(
        surfaces::elevated::BASE,
        card,
        "卡片列用的底就该是 `surfaces::elevated::BASE`（容器导出的那个常量）"
    );

    for (rect, base, wanted_button, wanted_field, name) in [
        (card_rect, card, card_button, card_field, "card"),
        (panel_rect, panel, panel_button, panel_field, "panel"),
    ] {
        let (x0, x1, y0, y1) = rect;
        // 两个计数**分开命名**：它们是不同量测对象，拿字段数去比按钮 stray 会让断言恒绿
        // （M234 复审 P2-1 的现场）。
        let button_hits = count_exact(shot, wanted_button, x0, x1, y0, y1);
        assert!(
            button_hits >= MIN_BUTTON_PX,
            "{name} 列里「叠 {:?}」的禁用按钮容器只有 {button_hits} px（应 ≥{MIN_BUTTON_PX}）： 这一列的禁用控件没把真实父底传下去",
            base.role()
        );
        let field_hits = count_exact(shot, wanted_field, x0, x1, y0, y1);
        assert!(
            field_hits >= MIN_FIELD_PX,
            "{name} 列里「叠 {:?}」的禁用字段容器只有 {field_hits} px（应 ≥{MIN_FIELD_PX}）",
            base.role()
        );

        // 反向 ①：**按钮**档 —— 这一列里「叠画布」的按钮容器不许和「叠该列底」的比肩。
        // 这条抓的是「只修了一半」：同列里一部分按钮仍走画布底时，正确档与错误档的像素量接近。
        let stray_button = count_exact(shot, canvas_button, x0, x1, y0, y1);
        assert!(
            button_hits > stray_button * STRAY_RATIO,
            "{name} 列里「叠 {:?}」的禁用按钮容器 {button_hits} px，而「叠画布」的**按钮容器** 有 {stray_button} px（要求前者 ≥ 后者 ×{STRAY_RATIO}）—— 这一列的按钮仍在按 Role::Surface 合成",
            base.role()
        );
        // 反向 ②：**字段**档（同理，各用各的计数）
        let stray_field = count_exact(shot, canvas_field, x0, x1, y0, y1);
        assert!(
            field_hits > stray_field * STRAY_RATIO,
            "{name} 列里「叠 {:?}」的禁用字段容器 {field_hits} px，而「叠画布」的**字段容器** 有 {stray_field} px（要求前者 ≥ 后者 ×{STRAY_RATIO}）—— 这一列的字段仍在按 Role::Surface 合成",
            base.role()
        );
        assert!(
            base.role() != Role::Surface,
            "{name} 列不该是画布底（判据自己写错了）"
        );
    }

    // 画布列：反过来必须出现画布档
    let (_, cx1, cy0, cy1) = canvas_rect;
    let cx0 = canvas_left;
    let canvas_hits = count_exact(shot, canvas_button, cx0, cx1, cy0, cy1);
    assert!(
        canvas_hits >= MIN_BUTTON_PX,
        "画布列里没有「叠画布」的禁用按钮容器 —— 对照组不成立"
    );
    let canvas_card_hits = count_exact(shot, card_button, cx0, cx1, cy0, cy1);
    assert!(
        canvas_hits > canvas_card_hits * STRAY_RATIO,
        "画布列里「叠画布」的禁用按钮容器 {canvas_hits} px，而「叠卡片底」的有 {canvas_card_hits} px —— 画布列的控件被传了卡片底"
    );

    eprintln!(
        "[probe] OK section=disabledbase flavor={flavor:?}：\
         画布列 x<{} · 卡片列左缘 x={}..{} · 面板列左缘 x={}..{}；\
         按钮容器色 canvas {:?}（{canvas_hits} px）⇒ 三列各按自己的底合成\
         （两个数字都是列左缘横坐标，不是像素计数）",
        card_rect.0, card_rect.0, card_rect.1, panel_rect.0, panel_rect.1, canvas_button
    );
}
