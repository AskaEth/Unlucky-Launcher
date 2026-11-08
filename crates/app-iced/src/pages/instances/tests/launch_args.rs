//! 「启动参数」区（M157）的**门限**判据：三态 chips 在版本不够时谁可点。
//!
//! 为什么只测一个纯函数：iced 0.14 的 `Element` 没有内省接口，画出来的「禁用态」在测试里
//! 拿不到（`Chip::disabled` 只是把 `on_press` 清成 `None`）。所以把判据抽成
//! [`backend_chip_enabled`] 并在这里钉住——页面那一行必须调它（否则这条绿着也没意义，
//! 守门形状与 `instances/tests.rs` 的源码断言同一取舍）。

use super::super::launch_args::backend_chip_enabled;
use crate::pages::LaunchFacts;

/// 门限满足：三档全可点。
#[test]
fn every_backend_chip_is_clickable_when_the_gate_is_open() {
    for id in ["default", "opengl", "vulkan"] {
        assert!(backend_chip_enabled(true, id), "门限满足时 {id} 必须可点");
    }
}

/// 门限不满足：只有 `default` 可点（它是「不下发」的安全档）。
///
/// 抓得住：把 `default` 一起灰掉（M157 复审 P3 的原状）——那时旧版本实例上写了
/// `graphicsBackend: vulkan` 的用户在 UI 里回不到 `default`，只能「恢复跟随全局」
/// （连带清空 JVM / 游戏参数 / 内存 / Java 四项）。
#[test]
fn the_default_chip_stays_clickable_when_the_gate_is_closed() {
    assert!(
        backend_chip_enabled(false, "default"),
        "门限不满足时 default 也必须可点：它是唯一永远合法的档"
    );
    for id in ["opengl", "vulkan"] {
        assert!(
            !backend_chip_enabled(false, id),
            "门限不满足时 {id} 必须灰掉（下发会被 core 拒绝）"
        );
    }
}

/// 词表与判据不许脱节：`default` 必须真在这份快照的 options 里，否则「安全档」无从点起。
#[test]
fn the_gate_rule_covers_the_demo_option_table() {
    let facts = LaunchFacts {
        options: ["default", "opengl", "vulkan"]
            .iter()
            .map(|id| (*id).to_string())
            .collect(),
        ..LaunchFacts::default()
    };
    assert!(
        facts
            .options
            .iter()
            .any(|id| backend_chip_enabled(false, id)),
        "门限关闭时至少有一档可点（否则三态整体锁死）"
    );
}
