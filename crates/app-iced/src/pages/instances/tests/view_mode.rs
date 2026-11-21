//! M455 的**版式判据**：实例页的两种视图与三档磁贴尺寸。
//!
//! ## 为什么这三条这么写（「真生效」与「看着像」的分界）
//!
//! iced 0.14 的 `Element` 没有几何内省（见 `pages/instances/tests.rs` 的「为什么读源码」），
//! 所以「这一屏画了几列」不能从控件树里问出来。两条退路都不够：
//!
//! - 「断言 `tile_size.columns()` 是 6/4/3」= 只测常量表（就算渲染端一个消费者都没有也绿，
//!   那正是 M452 之后的现场）；
//! - 「断言渲染返回了一个 `Element`」= 什么都不断言。
//!
//! 所以取样口放在**画的那一段自己**（`instances/grid.rs` 的 `note`）：它记的是**分行之后第一排
//! 真放了几个格子**。渲染一遍再读这个数，于是判据是「渲染出的列数 == `columns()`」而不是
//! 「渲染了一个网格」。
//!
//! ## 三条判据
//!
//! | 用例 | 抓得住 |
//! |---|---|
//! | [`the_tiles_view_lays_out_exactly_the_column_count_of_the_tile_size`] | 网格按 `columns()` 分行（硬编码 4 列 / 夹取 / 退回一列宽卡都会红），且三档**各不相同** |
//! | [`the_list_view_draws_no_grid_and_ignores_the_tile_size`] | `List` 模式下 `tile_size` 真的不影响渲染（列数取样为 0、行数逐档相同） |
//! | [`the_empty_snapshot_still_gets_the_three_state_screen`] | 磁贴视图下「还没扫完 / 确实零实例」那一屏仍然走 `detail_side` 的三态，不是一张空网格 |

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;
use unlucky_presenter::dto::LaunchArgsView;
use unlucky_presenter::prefs::{InstanceView, TileSize};
use unlucky_presenter::router::NavSection;

use crate::pages::layout::{self, PaneKit};
use crate::pages::model::{Instances, Row, ScanPhase};
use crate::pages::{LaunchForm, Model, Screen};
use crate::theme::{Families, Ui};

use super::super::grid::{self, Footprint, View};

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui() -> Ui {
    Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
}

fn form() -> LaunchForm {
    LaunchForm::from_view(None, &LaunchArgsView::default())
}

/// 1180×760 的宽窗模型（与交付图同一档：`ListDetail`）。
fn model(rows: usize, scan: ScanPhase) -> Model {
    let mut model = Model::new(
        Screen::Instances,
        1180.0,
        760.0,
        Some(NavSection::Instances),
    );
    model.instances = Instances {
        rows: (0..rows)
            .map(|index| Row {
                id: format!("i{index}"),
                name: format!("demo-{index}"),
                supporting: "1.20.4 · Fabric 0.16.5".to_string(),
                status: None,
            })
            .collect(),
        detail: None,
        scan,
        leaving: Vec::new(),
    };
    model
}

/// 渲染一屏实例页，读回**画的那一段**记下的形态。
fn render(view: View<'_>, rows: usize, scan: ScanPhase) -> Footprint {
    let ui = ui();
    let form = form();
    let model = model(rows, scan);
    let metrics = layout::frame_metrics(PaneKit::ListDetail, model.window_width);
    let element = super::view(&ui, metrics, &model, &form, view);
    // 树只是被建出来就走掉：取样在构建期已经发生（`grid::note`），这里要的是那个数。
    drop(element);
    grid::footprint().expect("渲染过一屏之后必须有取样（`grid::note` / `list::column` 的接线）")
}

/// **判据 ①**：`Tiles` 模式下真画出来的列数 == `tile_size.columns()`，三档各不相同。
///
/// ## 变异与期望红（每条只改一处）
///
/// | 变异 | 改哪里 | 期望红在哪句 |
/// |---|---|---|
/// | 网格退回「一列宽卡」 | `instances/grid.rs` 的 `drawn.chunks(columns)` → `drawn.chunks(1)` | `Small` 那一档：`列数 1 ≠ 6` |
/// | 所有尺寸都画 4 列 | `instances/grid.rs` 的 `let columns = size.columns();` → `let columns = 4;` | `Small`（4≠6）与 `Large`（4≠3） |
/// | 渲染端不接 `tile_size` | `pages/instances.rs` 的 `tiles_body` 把 `view.tile_size` 换成 `TileSize::Medium` | 同上两句 |
/// | 取样口报废（不做判据） | 删 `instances/grid.rs` 的 `#[cfg(test)] note(..)` | 「必须有取样」那句 panic |
#[test]
fn the_tiles_view_lays_out_exactly_the_column_count_of_the_tile_size() {
    // 9 条实例：三档的排法各不相同（6+3 / 4+4+1 / 3+3+3），排数与列数一起钉。
    const ROWS: usize = 9;

    let mut measured = Vec::new();
    for size in [TileSize::Small, TileSize::Medium, TileSize::Large] {
        let footprint = render(
            View {
                mode: InstanceView::Tiles,
                tile_size: size,
                query: "",
            },
            ROWS,
            ScanPhase::Ready,
        );
        assert_eq!(
            footprint.columns,
            size.columns(),
            "{size:?} 的网格画成了 {} 列，而 `TileSize::columns()` 是 {}",
            footprint.columns,
            size.columns()
        );
        assert_eq!(
            footprint.grid_rows,
            ROWS.div_ceil(size.columns()),
            "{size:?} 的排数不对（{ROWS} 条实例、每排 {} 个）",
            size.columns()
        );
        measured.push(footprint.columns);
    }

    // 三档**必须**给出三个不同的列数：都画成 4 列时上面每一句仍然自洽，这一句才会红。
    assert_eq!(measured, vec![6, 4, 3], "三档磁贴尺寸没画出三种列数");
}

/// **判据 ③**：`List` 模式下没有网格，且 `tile_size` 对渲染**零影响**。
///
/// ## 变异与期望红
///
/// | 变异 | 改哪里 | 期望红在哪句 |
/// |---|---|---|
/// | 列表模式顺手也画网格 | `pages/instances.rs` 的 `list_body` 里把 `list_column` 换成 `grid::column` | `List` 模式的列数必须是 0（取样成了 6/4） |
/// | 列表模式按尺寸改行高 | `instances/list.rs::column` 里插入 `let _ = tile_size`（任何按尺寸改行数的写法） | `Small` 与 `Large` 两次取样的 `list_rows` 不再相同 |
#[test]
fn the_list_view_draws_no_grid_and_ignores_the_tile_size() {
    let view = |size| View {
        mode: InstanceView::List,
        tile_size: size,
        query: "",
    };

    let small = render(view(TileSize::Small), 7, ScanPhase::Ready);
    let large = render(view(TileSize::Large), 7, ScanPhase::Ready);

    assert_eq!(small.columns, 0, "`List` 模式下取样里不该有网格列数");
    assert_eq!(small.grid_rows, 0, "`List` 模式下没有网格排");
    assert_eq!(
        small.list_rows, 7,
        "七条实例就是七行（没有按尺寸合并/拆分）"
    );
    assert_eq!(
        small, large,
        "`List` 模式下换磁贴尺寸不该改变渲染（两次取样不一致 = 列数影响了列表）"
    );
}

/// **判据（补救项）**：磁贴视图下「还没有实例」那一屏仍走三态，不是一张空网格。
///
/// 抓得住：`tiles_body` 把 `rows.is_empty()` 那一支删掉——那时冷启动首帧画的是「ALL INSTANCES」
/// 列头 + 一片空白（既不说「正在读盘」，也不给「导入已有版本」的出路）。
/// 判据用的是取样：空网格会记 `grid_rows = 0, columns = 0`，而走 `detail_side` 时**没有任何网格
/// 取样**（列表列也不在树上）——于是「上次渲染的取样仍是上一次的值」。
#[test]
fn the_empty_snapshot_still_gets_the_three_state_screen() {
    // 先渲染一屏有内容的网格，把取样写成一个非零形态。
    let filled = render(
        View {
            mode: InstanceView::Tiles,
            tile_size: TileSize::Medium,
            query: "",
        },
        4,
        ScanPhase::Ready,
    );
    assert_eq!(filled.columns, 4);

    // 再渲染「还在扫描」那一屏：不走网格 ⇒ 取样不被改写。
    let _ = render(
        View {
            mode: InstanceView::Tiles,
            tile_size: TileSize::Medium,
            query: "",
        },
        0,
        ScanPhase::Scanning,
    );
    assert_eq!(
        grid::footprint(),
        Some(filled),
        "空快照那一屏画成网格了（三态占位被绕开：这一屏既不说「正在读盘」也没有导入出路）"
    );
}
