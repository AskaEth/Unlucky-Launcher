//! 探针**取带规则**的回归（M248 r1 复审 P2-1 / P2-2）——用合成图，不依赖真实截图。
//!
//! ## 为什么必须用合成图
//!
//! 真实渲染里轨道**永远占满宿主**，「轨道被裁短」这类几何回归在图上不会自然出现。
//! 复审是手工把 after 图里 0% 那条轨道右半截涂回画布色才造出反例的（一次性动作），
//! 而那一涂正好打中第一版的漏洞：阈值 200 时该行**直接不再被认成轨道行**，
//! `check_linear` 靠另一条带照样绿——「判据被放宽」在真实图上永远不露馅。
//! 合成图把这个形状变成可复现夹具。
//!
//! ## 两条用例各钉住什么
//!
//! | 用例 | 钉住的取值区间 | 打红它的变异 |
//! |---|---|---|
//! | [`narrowed_track_is_still_a_band`] | `TRACK_MIN_RUN` ≤ 160（窄轨道必须仍成带），且 `TRACK_MIN_WIDTH` > 160（它必须过不了宽度判据） | ① 调用点换回计数判据 `bands(.., 200)`（窄轨道那条带消失 → `bands.len()` 变 1）；② `TRACK_MIN_WIDTH` 调到 ≤160（宽度判据失去区分力） |
//! | [`text_rows_are_never_track_bands`] | `TRACK_MIN_RUN` > 22（文字行最长连续段就是 22） | 阈值调到 ≤22（图注行会被数成轨道，正是第一版的假红） |
//!
//! **抓不住**：真实截图里轨道被裁/被折线的具体像素形态（那仍要复审涂图或用真实 probe），
//! 也抓不住厚度那一条（厚度仍由真实截图的 `track::check` 判）。

use unlucky_design::color::{BrandBoard, Role};
use unlucky_design::typography::Platform;

use super::super::pixel::{band_width, rgb, Shot};
use super::super::probe::track::{track_bands, TRACK_MIN_RUN, TRACK_MIN_WIDTH};
use super::FAMILIES;
use crate::theme::Ui;

/// 合成图的配色来源：用 Ink 板（深色）——轨道 `SurfaceContainerHighest` 与画布 `Surface`
/// 在这里是**两个不同的中性色**，正好复现「混色轴穿过中性文字」那一幕的坐标轴。
fn strip_ui() -> Ui {
    Ui::with_board(BrandBoard::Ink, Platform::Windows, FAMILIES)
}

/// 一段横线：`y0..=y1` 行里前 `len` 个像素涂 `color`，其余留底色。
fn strip(
    width: usize,
    height: usize,
    rows: &[(usize, usize, usize)],
    color: [u8; 3],
    under: [u8; 3],
) -> Shot {
    Shot::from_rows(width, height, |x, y| {
        let hit = rows
            .iter()
            .any(|(top, bottom, len)| (*top..=*bottom).contains(&y) && x < *len);
        if hit {
            color
        } else {
            under
        }
    })
}

/// 复审反例的形态：**轨道比宿主窄一半**（160 = 320 宿主的一半）。
///
/// 阈值 200 的版本在这里会**只找到一条带**（窄的那条整行掉出判据）——这正是那条 p2 的漏判，
/// 所以 `bands.len() == 2` 是这条用例的主断言之一。
#[test]
fn narrowed_track_is_still_a_band() {
    let ui = strip_ui();
    let track = rgb(ui.color(Role::SurfaceContainerHighest));
    let surface = rgb(ui.color(Role::Surface));

    // 360×20：y=5..8 是**被裁到 160px** 的轨道；y=12..15 是占满宿主的轨道。
    let shot = strip(360, 20, &[(5, 8, 160), (12, 15, 360)], track, surface);

    let bands = track_bands(
        &shot,
        ui.color(Role::SurfaceContainerHighest),
        ui.color(Role::Surface),
    );
    assert_eq!(
        bands,
        vec![(5, 8), (12, 15)],
        "窄轨道也必须被认成一条带（TRACK_MIN_RUN={TRACK_MIN_RUN}；调大到 ≥160 时它会消失，\
         于是 `check_linear` 只能看见另一条带、漏判——复审就是在这里抓到第一版的）"
    );

    let widths: Vec<usize> = bands
        .iter()
        .map(|band| {
            band_width(
                &shot,
                *band,
                ui.color(Role::SurfaceContainerHighest),
                ui.color(Role::Surface),
            )
        })
        .collect();
    assert_eq!(widths, vec![160, 360], "两条带的最宽连续段");
    assert!(
        widths[0] < TRACK_MIN_WIDTH,
        "窄轨道必须过不了宽度判据（TRACK_MIN_WIDTH={TRACK_MIN_WIDTH}），否则「轨道被裁短」\
         这类回归会绿着过"
    );
    assert!(widths[1] >= TRACK_MIN_WIDTH, "占满宿主的轨道必须过宽度判据");
}

/// 文字行永远不能进判据：图注行的最长连续段实测就是 **22px**（`after/gallery-progress-*`）。
///
/// 这条是「阈值别调得太小」的那一侧：阈值 ≤22 时，图注行会被数成轨道带，
/// 浅色板那次假红（`12.1：线性进度厚必须 4（实测 6 @ 行 63）`）就会回来。
#[test]
fn text_rows_are_never_track_bands() {
    let ui = strip_ui();
    let track = rgb(ui.color(Role::SurfaceContainerHighest));
    let surface = rgb(ui.color(Role::Surface));

    // 6 行「文字」：每段 22px、段间隔 12px（字形与字距的粗略形态），整行铺满。
    let shot = Shot::from_rows(360, 6, |x, _y| if x % 34 < 22 { track } else { surface });

    let bands = track_bands(
        &shot,
        ui.color(Role::SurfaceContainerHighest),
        ui.color(Role::Surface),
    );
    assert!(
        bands.is_empty(),
        "22px 的文字段不能算轨道行（TRACK_MIN_RUN={TRACK_MIN_RUN}；调到 ≤22 时这里会红，\
         那正是浅色板假红 `实测 6 @ 行 63` 的来源），实测 {bands:?}"
    );
}
