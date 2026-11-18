//! **主题交叉淡入**的守卫（M313）：两块板之间插值、复用既有腿、`NONE` 瞬时到位、换到一半再换。
//!
//! 判据按 T37/T41 写：从**驱动源**（`Pump::observe_theme` + `Pump::advance`）连拍，不等真实时间；
//! 每条都写清「抓得住什么 / 抓不住什么」。
//!
//! ## 变异账（T41：只改一处 ⇒ 期望红在哪条用例哪句断言）
//!
//! | 变异（只改这一处） | 期望红在哪 |
//! |---|---|
//! | `pump/theme.rs` 的 `crossfade` 开头加 `if true { return to; }`（**改回硬切**） | `a_switch_crossfades_through_a_middle_board_and_lands_on_the_target` 的「中间帧必须与两端都不同」 |
//! | `pump/theme.rs` 的 `Theme::observe` 里删掉 `if from == to \|\| off { return false; }` 的 `off` 那一半 | `no_scale_switches_instantly_to_the_final_board`（`is_animating()` 为真、`samples()` 非 0） |
//! | `pump/theme.rs` 的 `Theme::frame` 里把「没有轨道 ⇒ 终点板」改成 `None` | 同上那条的「落到位之后采样仍给终点板」 |
//! | `pump/theme.rs` 的 `observe_theme` 里把 `modal_spec().enter` 换成 `page_spec(Kind::Peer…).enter` | `a_switch_crossfades_…` 的「帧数必须等于模态进场腿」 |
//! | `pump/theme.rs` 的 `Theme::expire` 里把腿删掉的同时把 `self.to` 也清掉 | `a_switch_crossfades_…` 的末帧断言（`scheme != to`，界面会停在上次采到的中间值） |

use unlucky_design::color::{Role, Scheme, SchemeFlavor};
use unlucky_design::motion::MotionScale;

use super::super::pump::Pump;
use super::super::transition::modal_spec;
use super::super::*;

fn standard() -> Pump {
    let mut pump = Pump::new();
    pump.configure(MotionScale::STANDARD, None);
    pump
}

/// 两个端点之间的「严格中间」：某个角色在两端**都不同**、而中间帧与两端也**都不同**。
fn is_between(mid: Scheme, from: Scheme, to: Scheme) -> bool {
    Role::ALL.iter().any(|role| {
        let (a, b, m) = (from.get(*role), to.get(*role), mid.get(*role));
        a != b && m != a && m != b
    })
}

/// **换主题要走中间帧、末帧严格落在目标板上**（塔的判据：中间帧颜色与两端都不同）。
///
/// 抓得住：①硬切（`crossfade` 直接返回 `to`）——中间帧断言红；②末帧没落到位 / 停在中途
/// （轨道删掉顺手清掉 `to`）——末帧断言红；③换腿（用页面切换的 300ms 腿）——帧数断言红。
/// 抓不住：观感与真机像素（那要出图，见 `DevDocs/artifacts/m313/`）。
#[test]
fn a_switch_crossfades_through_a_middle_board_and_lands_on_the_target() {
    let mut pump = standard();
    let light = Scheme::baseline(SchemeFlavor::Light);
    let dark = Scheme::baseline(SchemeFlavor::Dark);
    assert!(pump.observe_theme(light, dark), "换主题必须起跑");

    // 起手那一帧**逐位**等于旧板：换主题的第一眼不许有台阶。
    let first = pump.theme_frame().expect("起了账就有采样");
    assert_eq!(first.scheme, light, "第一帧必须还是旧板");
    assert_eq!(first.progress, 0.0);
    assert!(first.running, "刚起跑时还要续帧");

    // 连拍到停：帧数由**模态进场腿**（`short4` 200ms）决定，不是别处的常数。
    let leg_ms = f64::from(modal_spec().enter.duration.ms());
    let expected_frames = (leg_ms / FRAME_MS).ceil() as u64;
    let mut frames = 0;
    let mut saw_between = false;
    while pump.is_animating() {
        pump.advance(FRAME_MS);
        frames += 1;
        let frame = pump.theme_frame().expect("采样口不许中途变 None");
        if frame.running && is_between(frame.scheme, light, dark) {
            saw_between = true;
        }
    }
    assert!(
        saw_between,
        "中间帧必须与两块板**都不同**（这一句红 = 改回硬切了）"
    );
    assert_eq!(
        frames, expected_frames,
        "淡入必须走模态进场那条腿（{leg_ms}ms），实测 {frames} 帧"
    );

    // 落到位：终点板 + 不再要帧（轨道被删掉了，但记的终点还在）。
    let landed = pump.theme_frame().expect("落位之后仍要给出终点板");
    assert_eq!(landed.scheme, dark, "末帧必须严格等于目标板");
    assert_eq!(landed.progress, 1.0);
    assert!(!landed.running);
    assert!(!pump.is_animating(), "落位之后不许再占帧");
    assert!(!pump.advance(FRAME_MS), "没有轨道时推进必须是空操作");
}

/// **`MotionScale::NONE`（含「减少动效」）⇒ 瞬时切到最终值，不许停在中途。**
///
/// 抓得住：把 `Theme::observe` 的 `off` 分支删掉（会插一条 0 时长轨道：`is_animating()` 为真、
/// 采样停在中途）；或把「没有轨道 ⇒ 终点板」改成 `None`（调用方保持旧板，颜色永远不变）。
/// 抓不住：系统「减少动效」开关有没有真的读到（那是 `platform::motion` 的用例）。
#[test]
fn no_scale_switches_instantly_to_the_final_board() {
    let mut pump = Pump::new(); // 出厂即 NONE
    assert!(pump.scale().is_off());
    let light = Scheme::baseline(SchemeFlavor::Light);
    let dark = Scheme::baseline(SchemeFlavor::Dark);
    assert!(!pump.observe_theme(light, dark), "关掉动效不许起轨道");

    let frame = pump.theme_frame().expect("关掉动效也要给出最终板");
    assert_eq!(frame.scheme, dark, "必须是**最终**值，不是旧板");
    assert_eq!(frame.progress, 1.0, "不许停在中途");
    assert!(!frame.running);
    assert!(!pump.is_animating(), "一帧都不许订阅");
    for _ in 0..60 {
        assert!(!pump.advance(FRAME_MS), "关掉动效没有任何可推进的");
    }
    assert_eq!(pump.samples(), 0, "NONE 档的帧采样次数必须是 0");

    // 反向（深 → 浅）同样瞬时到位：不是「第一次切对了」的巧合。
    assert!(!pump.observe_theme(dark, light));
    assert_eq!(pump.theme_frame().expect("有账").scheme, light);
}

/// **换到一半再换**：新的起手板就是刚才那一刻的屏幕上那块（调用方传进来的），轨道被取代、不叠加。
///
/// 抓得住：把 `Theme::observe` 改成「push 一条新轨道」或「只在没有轨道时记账」（第二次换主题
/// 会被静默忽略 ⇒ 采样仍往第一次的终点走）。
/// 抓不住：连续两次换主题的**观感**（那是出图的事）。
#[test]
fn a_second_switch_mid_flight_takes_over_from_where_it_is() {
    let mut pump = standard();
    let light = Scheme::baseline(SchemeFlavor::Light);
    let dark = Scheme::baseline(SchemeFlavor::Dark);
    let accent = Scheme::from_accent(
        SchemeFlavor::Light,
        unlucky_design::color::Color::rgb(0x00, 0x6A, 0x5C),
    );

    pump.observe_theme(light, dark);
    for _ in 0..5 {
        pump.advance(FRAME_MS);
    }
    let mid = pump.theme_frame().expect("半路有采样").scheme;
    assert!(is_between(mid, light, dark), "半路先得真的是中间值");

    // 用户在淡入途中又按了一次：起手板 = 屏幕上那块（`mid`），终点换成新板。
    assert!(pump.observe_theme(mid, accent), "中途再换必须重新起跑");
    let restarted = pump.theme_frame().expect("有账");
    assert_eq!(
        restarted.scheme, mid,
        "新的第一帧必须等于上一次的中间值（不跳变）"
    );
    assert_eq!(restarted.progress, 0.0);

    let mut frames = 0;
    while pump.is_animating() {
        pump.advance(FRAME_MS);
        frames += 1;
        assert!(frames < 200, "轨道不许永远跑不完");
    }
    assert_eq!(
        pump.theme_frame().expect("有账").scheme,
        accent,
        "落在新的终点上"
    );
}
