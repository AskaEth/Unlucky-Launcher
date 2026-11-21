//! **启动成败交叉淡入**的守卫（M314 / A6）：帧数 / 端点 / 单调性 / `NONE` 瞬时到位 / 没账时不给采样。
//!
//! 判据按 T37/T41：从**驱动源**（`Pump::observe_launch` + `Pump::advance`）连拍，不等真实时间。
//!
//! ## 变异账（T41：只改一处 ⇒ 期望红在哪条用例哪句断言）
//!
//! | 变异（只改这一处） | 期望红在哪 |
//! |---|---|
//! | `pump/launch.rs` 的 `LaunchTrack::sample` 改成 `1.0`（**改回硬切**） | `a_status_change_crossfades_monotonically_and_lands_on_the_target` 的「中途必须严格单调递增」 |
//! | `pump/launch.rs` 的 `observe` 里删掉 `if off { … }` 那一支 | `no_scale_switches_instantly_to_the_final_tone`（`is_animating()` 为真、`samples()` 非 0） |
//! | `pump/launch.rs` 的 `expire` 里把腿删掉的同时把 `self.tone` 也清掉 | 前一条用例的末帧断言（采样退化成 `None`，界面停在最后采到的中间档） |
//! | `pump/launch.rs` 的 `observe` 里删掉「换了实例就落档」那一段 | `switching_instances_lands_instead_of_dyeing_the_next_one` |
//! | `transition.rs` 的 `status_spec` 把 `Medium1` 换成 `Medium2`（300ms） | `a_status_change_crossfades_…` 的 `assert_eq!(status_spec().duration, DurationToken::Medium1)`（**r1 复审 p2-2**：只靠帧数窗口抓不住——`expected_frames` 与实测帧数由同一个 spec 推出，换 token 两边一起变） |

use unlucky_design::color::Role;
use unlucky_design::motion::{DurationToken, MotionScale};

use super::super::launch::LaunchFrame;
use super::super::transition::status_spec;
use super::super::*;
use crate::app::components::motion;

fn standard() -> Pump {
    let mut pump = Pump::new();
    pump.configure(MotionScale::STANDARD, None);
    pump
}

/// 这一帧采样的颜色**角色对**（两端都给了才算采样到）。
fn tones(frame: LaunchFrame) -> (Role, Role) {
    (frame.from, frame.to)
}

/// **状态行换了档要走中间帧、末帧严格落在目标档上**（塔的判据：帧数 + 端点 + 严格单调）。
///
/// 抓得住：①硬切（`sample` 直接返回 1.0）——单调性那一句红（一个中间帧都没有）；
/// ②末帧没落到位 / 停在中途（`expire` 顺手清掉 `tone`）——末帧断言红；
/// ③换腿（`status_spec` 用 `medium2`）——帧数断言红。
/// 抓不住：观感与真机像素（那要出图，本 mission 没有可用通道，见交付报告的「没验的部分」）。
#[test]
fn a_status_change_crossfades_monotonically_and_lands_on_the_target() {
    let mut pump = standard();
    // 第一拍：启动中（记 OnSurfaceVariant），落档、不起轨道。
    assert!(!pump.observe_launch("alpha", Some(Role::OnSurfaceVariant)));
    let first = pump.launch_frame("alpha").expect("记过账就有采样");
    assert_eq!(
        tones(first),
        (Role::OnSurfaceVariant, Role::OnSurfaceVariant),
        "第一拍直接落档（不许从「无」淡进来）"
    );
    assert_eq!(first.progress, 1.0);
    assert!(!pump.is_animating(), "落档不占帧");

    // 第二拍：失败（红句）。这一拍起跑。
    assert!(
        pump.observe_launch("alpha", Some(Role::Error)),
        "成败切换必须起跑"
    );
    let start = pump.launch_frame("alpha").expect("起了账就有采样");
    assert_eq!(
        tones(start),
        (Role::OnSurfaceVariant, Role::Error),
        "两端角色必须是「上一档 → 这一档」"
    );
    assert_eq!(start.progress, 0.0, "起手那一帧必须还是上一档的颜色");
    assert!(start.running, "刚起跑时还要续帧");

    // 连拍到停：帧数由 `status_spec` 的 medium1 腿决定，不是别处的常数。
    let leg_ms = f64::from(status_spec().duration.ms());
    let expected_frames = (leg_ms / FRAME_MS).ceil() as u64;
    let mut frames = 0;
    let mut last = 0.0_f32;
    while pump.is_animating() {
        pump.advance(FRAME_MS);
        frames += 1;
        let frame = pump.launch_frame("alpha").expect("采样口不许中途变 None");
        if !frame.running {
            // 到期那一帧：腿被删掉了，采样退化成「目标档 + 1.0」（见 `pump/launch.rs`）。
            continue;
        }
        assert_eq!(
            tones(frame),
            (Role::OnSurfaceVariant, Role::Error),
            "中途不许把两端换掉（换了就是另一条腿）"
        );
        assert!(
            frame.progress > last,
            "中途必须**严格单调递增**（{last} → {}）：相等就等于这一帧没在动",
            frame.progress
        );
        last = frame.progress;
        assert!(frames < 200, "轨道不许永远跑不完");
    }
    assert!(
        (15..=16).contains(&frames),
        "淡入必须落在 15~16 帧（250ms / 16ms；实测 {frames} 帧，腿 {leg_ms}ms）——\
         窗口故意收窄到两帧：放宽成 12~20 就抓不住「换错档位」"
    );
    assert_eq!(frames, expected_frames, "帧数必须等于腿本身推出来的帧数");
    // **直接钉令牌**：上面的 `frames == expected_frames` 两边都由同一个 spec 推出来，换 token
    // 会一起变（复审 r1 的 p2-2 就是这么抓到「声明会红、实测全绿」的）。档位必须另外钉死。
    assert_eq!(
        status_spec().duration,
        DurationToken::Medium1,
        "A6 的腿必须是文档 §3 点名的 medium1（250ms）——改成 medium2 时红在这一句"
    );
    assert_eq!(leg_ms, 250.0, "medium1 = 250ms（令牌值，不是本地的数）");

    // 落到位：目标档 + 不再要帧（轨道被删掉了，但记的目标还在）。
    let landed = pump.launch_frame("alpha").expect("落位之后仍要给出目标档");
    assert_eq!(
        tones(landed),
        (Role::Error, Role::Error),
        "末帧必须严格等于目标档"
    );
    assert_eq!(landed.progress, 1.0);
    assert!(!landed.running);
    assert!(!pump.is_animating(), "落位之后不许再占帧");
    assert!(!pump.advance(FRAME_MS), "没有轨道时推进必须是空操作");
    assert_eq!(landed.from, Role::Error, "失败那档是红句");
}

/// **`MotionScale::NONE`（`ULL_MOTION=off` / 截图通道）⇒ 瞬时切到终值，不许停在中途。**
///
/// 抓得住：把 `observe` 的 `off` 分支删掉（会插一条 0 时长轨道：`is_animating()` 为真、
/// 采样停在中途）；或「没有轨道 ⇒ 目标档」改成 `None`（调用方保持旧色，红句永远不出现）。
/// 抓不住：系统「减少动效」开关有没有真的读到（那是 `platform::motion` 的用例）。
#[test]
fn no_scale_switches_instantly_to_the_final_tone() {
    let mut pump = Pump::new(); // 出厂即 NONE
    assert!(pump.scale().is_off());
    assert!(!pump.observe_launch("alpha", Some(Role::OnSurface)), "落档");
    assert!(
        !pump.observe_launch("alpha", Some(Role::Error)),
        "NONE 不许起轨道"
    );

    let frame = pump.launch_frame("alpha").expect("关掉动效也要给出目标档");
    assert_eq!((frame.from, frame.to), (Role::Error, Role::Error));
    assert_eq!(frame.progress, 1.0, "不许停在中途");
    assert!(!frame.running);
    assert!(!pump.is_animating(), "一帧都不许订阅");
    for _ in 0..60 {
        assert!(!pump.advance(FRAME_MS), "关掉动效没有任何可推进的");
    }
    assert_eq!(pump.samples(), 0, "NONE 档的帧采样次数必须是 0");

    // 反向（失败 → 运行中）同样瞬时到位：不是「第一次切对了」的巧合。
    assert!(!pump.observe_launch("alpha", Some(Role::OnSurface)));
    assert_eq!(
        pump.launch_frame("alpha").expect("有账").to,
        Role::OnSurface
    );
}

/// **换实例 = 落档，不串色**：上一实例的红句不许染到新实例上。
///
/// 抓得住：删掉 `observe` 的「换了 key 就落档」那一段（新实例会从 alpha 的失败档淡进来）。
/// 抓不住：真机上连续点两个实例的观感。
#[test]
fn switching_instances_lands_instead_of_dyeing_the_next_one() {
    let mut pump = standard();
    pump.observe_launch("alpha", Some(Role::OnSurface));
    assert!(pump.observe_launch("alpha", Some(Role::Error)));
    for _ in 0..4 {
        pump.advance(FRAME_MS);
    }
    assert!(pump.is_animating(), "前提：alpha 的淡入还在跑");

    // 点开另一个实例（状态行是「运行中」）：必须**落档**，且不保留 alpha 的轨道。
    assert!(
        !pump.observe_launch("beta", Some(Role::OnSurface)),
        "换实例不许起轨道"
    );
    let frame = pump.launch_frame("beta").expect("beta 有自己的账");
    assert_eq!(
        (frame.from, frame.to),
        (Role::OnSurface, Role::OnSurface),
        "新实例的第一帧就是它自己的档（不许从 alpha 的红句过渡过来）"
    );
    assert_eq!(frame.progress, 1.0);
    assert!(!pump.is_animating(), "alpha 的轨道必须被取代，不是叠加");
    assert!(
        pump.launch_frame("alpha").is_none(),
        "账已经换成 beta：拿旧 key 问会给 None（宁可无动效，不许串色）"
    );
}

/// **没走过 `observe` 的模型不给采样**（演示数据 / `pages_demo` / 单测直接构造模型）。
///
/// 抓得住：把 `frame` 的 key 判据删掉（页面会拿到一个「猜出来的」角色对）。
/// 抓不住：页面拿到 `None` 之后画的兜底色（那是 `pages/instances/detail.rs::status_color` 的事）。
#[test]
fn an_unobserved_instance_gets_no_sample() {
    let mut pump = standard();
    assert!(
        pump.launch_frame("nobody").is_none(),
        "没记过账就不许给采样"
    );
    pump.observe_launch("alpha", Some(Role::OnSurface));
    assert!(pump.launch_frame("nobody").is_none(), "别的 key 也不许给");
    assert!(pump.launch_frame("alpha").is_some());
}

/// **状态行出现 / 消失不起轨道**：那是版面变化，没有颜色可插。
///
/// 抓得住：把 `observe` 的「两端都得有状态行」判据删掉（会在 `Some ↔ None` 之间插一条腿，
/// 而 `frame` 在 `tone = None` 时给不出采样 ⇒ 帧订阅白烧 250ms）。
#[test]
fn a_line_appearing_or_disappearing_does_not_start_a_leg() {
    let mut pump = standard();
    pump.observe_launch("alpha", None); // 空闲：没有状态行
    assert!(!pump.observe_launch("alpha", Some(Role::OnSurface)));
    assert!(!pump.is_animating(), "无 → 有：版面长出一行，不是颜色变化");
    assert!(!pump.observe_launch("alpha", None));
    assert!(!pump.is_animating(), "有 → 无：同理");
    assert!(
        pump.launch_frame("alpha").is_none(),
        "没有状态行时不给采样（页面也就不画这一行）"
    );
}

/// 接线（**motion 侧**）：`motion::launch::observe` 起跑之后真的会占帧、并且能落到终值。
///
/// 与 `app/state/refresh/tests.rs` 那条 App 级用例的分工：这条只证明采样口与泵接上了，
/// 那条证明**产品路径**（`refresh_view`）会调它。
#[test]
fn the_public_observe_reaches_the_pump() {
    motion::reset();
    motion::configure_for_test(MotionScale::STANDARD, None);
    assert!(!motion::launch::observe("alpha", Some(Role::OnSurface)));
    assert!(motion::launch::observe("alpha", Some(Role::Error)));
    assert!(motion::needs_frames(), "起了腿就要订阅帧");
    let mid = motion::launch::frame("alpha").expect("采样口接上了");
    assert_eq!((mid.from, mid.to), (Role::OnSurface, Role::Error));
    for _ in 0..200 {
        if !motion::needs_frames() {
            break;
        }
        motion::advance_by(FRAME_MS);
    }
    assert!(!motion::needs_frames(), "落位之后不许再订阅帧");
    assert_eq!(motion::launch::frame("alpha").expect("有账").progress, 1.0);
    motion::reset();
}
