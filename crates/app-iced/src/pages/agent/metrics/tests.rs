//! 布局数学的数值判据（M104 任务第 2 条那张表就长在这里）。
//!
//! 每条断言都写清「抓得住什么 / 抓不住什么」——纯函数能钉住算式，**钉不住**像素与手感。

use crate::pages::layout::{self, PaneKit};

use crate::shell::agent::RailState;

use super::{
    clamp_width, content_width, effective_state, layout_for, magnet_width, max_rail_width,
    min_content_width, COLLAPSED_WIDTH, DEFAULT_EXPANDED_WIDTH, MAGNET_RADIUS, PRESETS,
    RESIZE_GUTTER,
};

/// **布局数学的核心表**（1180 / 1024 / 900 / 700，任务第 2 条点名要的四个宽度）。
///
/// 抓得住：右栗占宽之后中栗是**收缩**的（`content = window − rail − gutter`），以及页面布局系统
/// 拿到的列宽确实跟着变（1180 展开时实例页详情列 424 → 356）。
/// 抓不住：真实的像素宽度（那要量图），也抓不住「右栗和外框之间有没有缝」这种观感问题。
#[test]
fn the_column_table_matches_the_mission_widths() {
    // (窗口, 档位, 右栗面板, 拖拽缝, 中栗, 实例详情列, 单列页列宽)
    //
    // 第三列是**面板宽度**，前两列的「展开档」用默认锚 320；360 那一行显式给值（见 `set_expanded`）。
    let expect = [
        (1180.0, RailState::Detached, 0.0, 0.0, 1180.0, 424.0, 424.0),
        (
            1180.0,
            RailState::Collapsed,
            48.0,
            0.0,
            1132.0,
            424.0,
            424.0,
        ),
        (1180.0, RailState::Expanded, 320.0, 8.0, 852.0, 356.0, 424.0),
        // 360 档：中栗 812 掉到 medium 档 ⇒ 实例页退化单列（不再有两列硬挤）
        (1180.0, RailState::Expanded, 360.0, 8.0, 812.0, 424.0, 424.0),
        (1024.0, RailState::Detached, 0.0, 0.0, 1024.0, 424.0, 424.0),
        (1024.0, RailState::Collapsed, 48.0, 0.0, 976.0, 424.0, 424.0),
        // 1024 展开：中栗 696 掉到 medium 档 ⇒ 实例页退化成单列（不再两列硬挤）
        (1024.0, RailState::Expanded, 320.0, 8.0, 696.0, 424.0, 424.0),
        // 900 收起：中栗 852 仍够 list-detail（几何门槛 816），详情列 356
        (900.0, RailState::Detached, 0.0, 0.0, 900.0, 404.0, 424.0),
        (900.0, RailState::Collapsed, 48.0, 0.0, 852.0, 356.0, 424.0),
        (900.0, RailState::Expanded, 320.0, 8.0, 572.0, 424.0, 424.0),
        // 700 < 进程最小窗宽 880：展开档不成立（中栗会被压到 472 以下），退回收起档
        (700.0, RailState::Expanded, 48.0, 0.0, 652.0, 424.0, 424.0),
    ];

    for (window, rail, width, gutter, content, detail, single) in expect {
        // 展开档用「锚 320」或表里显式给的更宽档（360）——收起/拆出的行也给 320，不影响结果
        let expanded = if width > DEFAULT_EXPANDED_WIDTH {
            width
        } else {
            DEFAULT_EXPANDED_WIDTH
        };
        let laid_out = layout_for(window, rail, expanded);
        assert_eq!(
            (laid_out.rail, laid_out.gutter, laid_out.content),
            (width, gutter, content),
            "{window} / {} 的横向预算不对",
            rail.name()
        );
        assert_eq!(laid_out.left, layout::rail_width(), "左轨宽度只有一个真源");

        // 页面布局系统吃的就是这个 content 宽：实例页（list-detail）与单列页各算一次
        let instances = layout::frame_metrics(PaneKit::ListDetail, content);
        assert_eq!(
            instances.column,
            detail,
            "{window} / {} 的实例详情列宽不对",
            rail.name()
        );
        let page = layout::frame_metrics(PaneKit::Single, content);
        assert_eq!(
            page.column,
            single,
            "{window} / {} 的单列页列宽不对",
            rail.name()
        );
    }
}

/// **不溢出**的硬判据：窗口 ≥ 最小窗宽时，任何档位、任何预设，中栗都不低于 472。
///
/// 抓得住：有人把 [`min_content_width`] 调小、或让 [`content_width`] 忘了扣右栗/拖拽缝。
/// 抓不住：`row!` 的兄弟顺序写错（那要出图看）。
#[test]
fn the_middle_column_never_drops_below_the_usable_width() {
    assert_eq!(
        min_content_width(),
        472.0,
        "472 = 行宽上限 424 + 两侧页边距 48"
    );

    for window in [880.0, 900.0, 1024.0, 1180.0, 1440.0, 2560.0] {
        for rail in RailState::ALL {
            for preset in PRESETS {
                let width = content_width(window, rail, preset);
                assert!(
                    width >= 472.0,
                    "{window} / {} / 预设 {preset} 把中栗压到 {width}",
                    rail.name()
                );
            }
        }
    }

    // 边界：最小窗宽 880 上，展开档被夹到上限 312（880 − 88 − 472 − 8），中栗留 560
    assert_eq!(max_rail_width(880.0), 312.0);
    assert_eq!(content_width(880.0, RailState::Expanded, 320.0), 560.0);
}

/// 拖拽：磁铁吸附 + 夹紧。
///
/// 抓得住：①拖动结果落在预设档或 4px 栅格上（不会一像素一像素抖）；②离预设 24px 内被吸住；
/// ③往左拖过头会被夹在 [`max_rail_width`]，而不是把中栗压扁。
/// 抓不住：真实指针手感（`mouse_area::on_move` 在真窗口里的采样率只能人试）。
#[test]
fn the_drag_magnetises_and_refuses_to_squeeze_the_middle() {
    // 1180 窗口：上限 = 1180 − 88（左轨）− 472（中栗下限）− 8（拖拽缝）= 612
    assert_eq!(max_rail_width(1180.0), 612.0);

    // 离 320 只有 6px ⇒ 吸到 320
    assert_eq!(magnet_width(1180.0, 326.0), 320.0);
    // 两档之间（310）：离 320 只有 10px ⇒ 还是吸住
    assert_eq!(magnet_width(1180.0, 310.0), 320.0);
    // 360 之上再没有档位 ⇒ 走 4px 栅格
    assert_eq!(magnet_width(1180.0, 500.0), 500.0);
    assert_eq!(magnet_width(1180.0, 497.0), 496.0);
    // 低于最小档 ⇒ 吸到 280（不是变细）
    assert_eq!(magnet_width(1180.0, 120.0), 280.0);
    // 超过上限 ⇒ 夹在 612（中栗正好 472）
    assert_eq!(magnet_width(1180.0, 900.0), 612.0);
    assert_eq!(
        content_width(1180.0, RailState::Expanded, 612.0),
        1180.0 - 612.0 - 8.0
    );

    // 900 窗口：上限 = 900 − 88 − 472 − 8 = 332 ⇒ 360 档被夹到 332
    assert_eq!(max_rail_width(900.0), 332.0);
    assert_eq!(magnet_width(900.0, 360.0), 332.0);
    // 窄到连 280 都放不下（700）：展开档不成立，返回折叠宽
    assert_eq!(clamp_width(700.0, 320.0), COLLAPSED_WIDTH);
    assert_eq!(
        effective_state(700.0, RailState::Expanded, 320.0),
        RailState::Collapsed
    );
    assert_eq!(
        effective_state(1180.0, RailState::Expanded, 320.0),
        RailState::Expanded
    );
}

/// 三段常量必须互相自洽（改一个忘一个，图上就是「面板 320 但内容按 280 排」）。
#[test]
fn the_rail_constants_stay_consistent() {
    assert_eq!(COLLAPSED_WIDTH, 48.0, "折叠条 = M3 触控目标");
    assert_eq!(
        DEFAULT_EXPANDED_WIDTH, 320.0,
        "展开档 = 令牌 ai_panel_width"
    );
    assert_eq!(PRESETS, [280.0, 320.0, 360.0]);
    assert_eq!(RESIZE_GUTTER, 8.0);
    assert_eq!(MAGNET_RADIUS, 24.0);

    assert!(
        PRESETS.contains(&DEFAULT_EXPANDED_WIDTH),
        "锚档必须在预设里"
    );

    // 下面两条用**变量**而不是常量表达式：clippy 的 `assertions_on_constants` 会把
    // 「拿常量断言常量」判成无用断言，而这里断的正是两个常量之间的关系。
    let shortest_rail = COLLAPSED_WIDTH;
    let narrowest_panel = PRESETS[0];
    assert!(shortest_rail < narrowest_panel, "折叠比最小展开还窄");

    // 磁铁半径 > 档位间距的一半 ⇒ 280~360 之间没有「吸不住」的空档
    let magnet = MAGNET_RADIUS;
    let half_step = (PRESETS[1] - PRESETS[0]) / 2.0;
    assert!(magnet > half_step);

    // 三档名字是取证开关的取值，改名等于改命令行契约
    for rail in RailState::ALL {
        assert_eq!(RailState::from_name(rail.name()), Some(rail));
    }
    assert_eq!(RailState::from_name("open"), None);
}
