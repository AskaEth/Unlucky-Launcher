//! 三栏几何的判据：**报告里的数值表就是这张表**，改任何一个门槛或栏宽都会在这里红。
//!
//! 每条都写清「抓得住什么 / 抓不住什么」——这里只测纯函数，**像素**由 `DevDocs/artifacts/m107/`
//! 的图与报告的量测表负责。

use super::*;
use crate::pages::layout::available_width;

/// 报告里那张数值表的**逐行快照**（窗口 → 页面拿到多宽 → 可用宽 → 档 → 三列）。
///
/// 抓得住：任何一处门槛 / 栏宽 / 间距被改动（表里 7 行的档位或列宽立刻对不上）。
/// 抓不住：改完之后画面**看起来**是否还行（那要出图，见报告）。
#[test]
fn the_numeric_table_matches_the_report() {
    // (页面拿到的宽, 期望档, 类型栏, 主版本栏, 列表)
    let table: [(f32, VersionRailTier, f32, f32, f32); 7] = [
        (1180.0, VersionRailTier::Wide, 140.0, 140.0, 716.0),
        (1132.0, VersionRailTier::Wide, 140.0, 140.0, 668.0),
        (852.0, VersionRailTier::Narrow, 64.0, 140.0, 464.0),
        (1024.0, VersionRailTier::Wide, 140.0, 140.0, 560.0),
        (900.0, VersionRailTier::Narrow, 64.0, 140.0, 512.0),
        (880.0, VersionRailTier::Narrow, 64.0, 140.0, 492.0),
        (700.0, VersionRailTier::Chips, 0.0, 0.0, 564.0),
    ];

    for (page_width, tier, type_rail, major_rail, list) in table {
        let rails = VersionRails::of(available_width(page_width));
        assert_eq!(rails.tier, tier, "{page_width} 的档位");
        assert_eq!(rails.type_rail, type_rail, "{page_width} 的类型栏");
        assert_eq!(rails.major_rail, major_rail, "{page_width} 的主版本栏");
        assert_eq!(rails.list, list, "{page_width} 的列表列");
    }
}

/// 三栏档里，列表列**永远**不低于行宽下限 424（任务里那条硬约束：版本行不许折行）。
///
/// 抓得住：有人把门槛算小了（比如把 `Compact` 档门槛写成 560，列表会掉到 384 以下，
/// `1.21.9-pre3 · Snapshot · 2026-09-20 · Install` 这一行就得折）。
/// 抓不住：折行到底发生在第几个字符（那要 FreeType 量宽 + 出图）。
#[test]
fn every_three_column_tier_keeps_the_list_at_the_reading_floor() {
    let floor = version_list_min_width();
    let mut sweep = 0;
    // 4px 栅格扫一遍：从 480 到 2400 的可用宽
    let mut available = 480.0_f32;
    while available <= 2400.0 {
        let rails = VersionRails::of(available);
        if rails.tier.is_three_column() {
            assert!(
                rails.list + f32::EPSILON >= floor,
                "可用宽 {available}（{} 档）的列表列 {} 低于行宽下限 {floor}",
                rails.tier.name(),
                rails.list
            );
            sweep += 1;
        }
        available += 4.0;
    }
    assert!(sweep > 100, "扫描没跑到（只命中 {sweep} 档）");
}

/// 档位随可用宽**单调**下沉：变窄只可能降档，不会「窄一点反而三栏都展开」。
#[test]
fn the_tiers_only_ever_step_down_as_the_width_shrinks() {
    let order = |tier: VersionRailTier| match tier {
        VersionRailTier::Wide => 0,
        VersionRailTier::Narrow => 1,
        VersionRailTier::Compact => 2,
        VersionRailTier::Chips => 3,
    };

    let mut previous = VersionRails::of(4096.0);
    let mut available = 4092.0_f32;
    while available >= 200.0 {
        let rails = VersionRails::of(available);
        assert!(
            order(rails.tier) >= order(previous.tier),
            "{available} 反而比更宽时更「展开」：{:?} → {:?}",
            previous.tier,
            rails.tier
        );
        previous = rails;
        available -= 4.0;
    }
}

/// 三个门槛都是**算出来的**，不是拍的：把它们与文档里的公式对上。
#[test]
fn the_three_thresholds_are_derived() {
    assert_eq!(wide_min_available(), 888.0, "1024 窗宽换算的可用宽");
    assert_eq!(
        narrow_min_available(),
        64.0 + 24.0 + 140.0 + 24.0 + 424.0,
        "类型栏收成图标条后三栏仍成立的最小可用宽"
    );
    assert_eq!(
        compact_min_available(),
        64.0 + 24.0 + 64.0 + 24.0 + 424.0,
        "两条栏都收成 64 时的最小可用宽"
    );
    assert!(compact_min_available() < narrow_min_available());
    assert!(narrow_min_available() < wide_min_available());

    // 门槛上的两个采样点：刚好到就进这一档，差 4px 就退一档
    assert_eq!(
        VersionRails::of(narrow_min_available()).tier,
        VersionRailTier::Narrow
    );
    assert_eq!(
        VersionRails::of(narrow_min_available() - 4.0).tier,
        VersionRailTier::Compact
    );
    assert_eq!(
        VersionRails::of(compact_min_available()).tier,
        VersionRailTier::Compact
    );
    assert_eq!(
        VersionRails::of(compact_min_available() - 4.0).tier,
        VersionRailTier::Chips
    );
}

/// 右栏展开（M104 的 `content_width`：窗口 − 320 − 拖拽缝 8）之后**仍是三栏**，且列表够宽。
///
/// 这条钉的是任务里那个场景：1180 窗宽开右栏，用户还能看见两条侧栏 + 一个不折行的版本列表。
/// 抓得住：有人把 `Narrow` 门槛抬到 716 以上（那会让右栏一展开就掉进 chips）。
/// 抓不住：`content_width` 自己算错（那是 M104 的测试）。
#[test]
fn the_agent_rail_open_at_1180_keeps_three_columns() {
    // M104 `content_width` = 窗口 − 右栗 320 − 拖拽缝 8
    let page_width = 1180.0 - 320.0 - 8.0;
    let rails = VersionRails::of(available_width(page_width));

    assert_eq!(page_width, 852.0);
    assert_eq!(rails.tier, VersionRailTier::Narrow);
    assert!(rails.tier.is_three_column(), "右栏展开不许把侧栏收掉");
    assert_eq!(rails.list, 464.0, "列表列 464（任务书写的 492 漏扣了 96）");
    assert!(rails.list >= version_list_min_width());
}
