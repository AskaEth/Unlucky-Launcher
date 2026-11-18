//! **列表增删轨道**的守卫（M305）：差分、退场钉位、同一条时钟、`NONE` 退化、空闲不烧帧。
//!
//! 判据按 T37/T38 写：从**驱动源**（`Pump::observe_rows` + `Pump::advance`）连拍，不等真实时间；
//! 每条都写清「抓得住什么 / 抓不住什么」。
//!
//! ## 变异账（T41：只改一处 ⇒ 期望红在哪条用例哪句断言）
//!
//! | 变异（只改这一处） | 期望红在哪 |
//! |---|---|
//! | `pump/rows.rs` 的 `Rows::drawn` 里删掉「插入新 id」那一段（退场行照旧钉位） | `an_exit_row_stays_in_place_until_its_leg_finishes` 的「还没走完时 b 必须还在原位」 |
//! | `pump/rows.rs` 的 `Rows::expire` 里删掉 `self.tracks.retain(..)`（走完不删轨道） | `frames_after_the_leg_finishes_are_not_requested`（`advance` 一直为真、`is_animating` 恒真） |
//! | `pump/rows.rs` 的 `frame` 里把 `track.sample(..)` 换成 `track.to`（**改回硬跳**） | `an_enter_row_climbs_and_an_exit_row_descends_on_the_same_clock` 的中间帧断言（可见度要么 0 要么 1） |
//! | `pump/rows.rs` 的 `observe_rows` 里删掉 `scale.is_off()` 那一支 | `no_scale_lands_every_row_on_the_final_state`（`samples()` 非 0、`needs_frames()` 为真） |
//! | `pump/rows.rs` 的 `retarget` 里 `slots` 用 `index` 而不是 `min(index, max_stagger)` | `the_stagger_is_capped_so_a_long_list_does_not_wait_forever` |
//! | `pump/rows.rs` 的 `visibility_of` 里把「按当前采样」换回 `track.from` | `deleting_a_row_mid_enter_does_not_jump_to_zero`（r1 P2-1） |
//! | `pump/rows.rs` 的 `observe_rows` 里删掉「纯重排当场归位」那一段 | `a_pure_reorder_lands_on_the_new_order_at_once`（r1 P2-3） |
//! | `pump/lists.rs` 的 `Lists::of_mut` 两个分支返回同一个字段（两张列表共用一份清单） | `the_two_lists_keep_their_own_order_and_tracks` 的「版本列表的变化不许污染实例列表的清单」 |

use super::super::pump::List;
use super::super::pump::Pump;
use super::super::pump::RowFrame;
use super::super::transition::row_spec;
use super::super::*;
use unlucky_design::motion::MotionScale;

fn standard() -> Pump {
    let mut pump = Pump::new();
    pump.configure(MotionScale::STANDARD, None);
    pump
}

/// 只看某个 id 的可见度。
fn visibility(pump: &Pump, id: &str) -> f32 {
    pump.row_frame(List::Versions, id).visibility
}

/// 一路采样到轨道结束（每帧 16ms，上限 20s），返回采到的可见度序列。
fn run(pump: &mut Pump, id: &str) -> Vec<f32> {
    let mut out = vec![visibility(pump, id)];
    for _ in 0..1250 {
        if !pump.is_animating() {
            break;
        }
        pump.advance(FRAME_MS);
        out.push(visibility(pump, id));
    }
    out
}

/// **进 / 出两腿读同一个时间源**，且中间帧真的是中间值（不是硬跳）。
///
/// 抓得住：①把 `frame` 改成硬跳（`track.to`）——中间帧会全是 0/1；②进/出各用一条时钟
/// （例如退场读 `now_ms`、进场读 `elapsed_ms`）——两边算出来的相位对不上。
/// 抓不住：观感（出图在 `DevDocs/artifacts/m305/`）。
#[test]
fn an_enter_row_climbs_and_an_exit_row_descends_on_the_same_clock() {
    let mut pump = standard();
    // 先对表。"x" 与 "d" 都在**第 0 位**：错位档为 0 ⇒ 两条腿同一拍起跑（这才是「同一条时钟」）。
    assert!(!pump.observe_rows(List::Versions, &["x", "b"]));
    // 一次变化里同时有进（d）与出（x）
    assert!(pump.observe_rows(List::Versions, &["d", "b"]));

    let spec = row_spec();
    // 第一帧（起点）：进场 0、退场 1
    assert_eq!(visibility(&pump, "d"), 0.0, "进场起点必须是 0");
    assert_eq!(visibility(&pump, "x"), 1.0, "退场起点必须是 1");
    assert_eq!(visibility(&pump, "b"), 1.0, "没变的行必须纹丝不动");

    pump.advance(FRAME_MS);
    let now = pump.now_ms();
    let enter_ms = pump.row_leg_ms(true);
    let exit_ms = pump.row_leg_ms(false);
    let enter_expected = spec.enter.easing.sample((now / enter_ms) as f32);
    let exit_expected = 1.0 - spec.exit.easing.sample((now / exit_ms) as f32);
    assert!(
        (visibility(&pump, "d") - enter_expected).abs() < 1.0e-5,
        "进场的中间帧应当是同一条时间轴上的采样：实得 {} 期望 {enter_expected}",
        visibility(&pump, "d")
    );
    assert!(
        (visibility(&pump, "x") - exit_expected).abs() < 1.0e-5,
        "退场的中间帧应当是同一条时间轴上的采样：实得 {} 期望 {exit_expected}",
        visibility(&pump, "x")
    );
    assert!(
        now > 0.0 && now < enter_ms.max(exit_ms),
        "这一拍必须落在「两边都还没走完」的窗口里（now={now}）"
    );

    // 覆盖「状态已切换但没有任何事件」的时段：一条消息都不再来，只推进时间轴。
    let enter_frames = run(&mut pump, "d");
    assert!(enter_frames.len() > 4, "进场要跨多帧：{enter_frames:?}");
    assert!(
        enter_frames.windows(2).all(|pair| pair[1] >= pair[0]),
        "进场必须单调往上：{enter_frames:?}"
    );
    assert_eq!(*enter_frames.last().unwrap(), 1.0, "收尾必须严格到 1");
}

/// 退场行**走完才移除**：这一段里它还在画的顺序里，且下面那些行不许提前上移。
///
/// 抓得住：把退场行在 observe 当拍就从 `drawn` 里摘掉（那就是整表瞬间重排闪跳）。
/// 抓不住：收尾那一帧的像素（高度收到 0 之后才删，出图证明）。
#[test]
fn an_exit_row_stays_in_place_until_its_leg_finishes() {
    let mut pump = standard();
    // 退场行放在第 0 位（错位档 0）：这条用例只看「钉位」与「走完才删」，不看错位。
    pump.observe_rows(List::Versions, &["b", "a", "c"]);
    pump.observe_rows(List::Versions, &["a", "c"]);

    assert_eq!(
        pump.rows_drawn(List::Versions),
        vec!["b", "a", "c"],
        "退场腿还在跑时 b 必须留在原位（否则 a/c 会当场跳上来）"
    );
    // 推进半程：还在
    for _ in 0..3 {
        pump.advance(FRAME_MS);
    }
    assert!(visibility(&pump, "b") < 1.0, "前提：退场真的在走");
    assert_eq!(
        pump.rows_drawn(List::Versions),
        vec!["b", "a", "c"],
        "半程时顺序不许变"
    );

    // 走完之后：b 才消失，且顺序归位
    for _ in 0..1250 {
        if !pump.is_animating() {
            break;
        }
        pump.advance(FRAME_MS);
    }
    assert_eq!(
        pump.rows_drawn(List::Versions),
        vec!["a", "c"],
        "退场走完才移除"
    );
    assert!(
        !pump.is_animating(),
        "轨道到期必须当场删（否则空闲还在要帧）"
    );
}

/// 新行插在**它该在的位置**，不是在末尾追加。
#[test]
fn an_enter_row_lands_between_its_neighbours() {
    let mut pump = standard();
    pump.observe_rows(List::Versions, &["a", "c"]);
    pump.observe_rows(List::Versions, &["a", "b", "c"]);
    assert_eq!(pump.rows_drawn(List::Versions), vec!["a", "b", "c"]);
}

/// **结束后一帧都不再要**（T38 ④ 的算术那一半）：`advance` 返回假、`samples` 不再增长。
///
/// 抓得住：到期不删轨道（`advance` 会永远为真 ⇒ 空闲烧帧）。
/// 抓不住：`App::view_dirty`（那是 app 侧的门控，不在这条用例的能力范围里）。
#[test]
fn frames_after_the_leg_finishes_are_not_requested() {
    let mut pump = standard();
    pump.observe_rows(List::Versions, &["a"]);
    pump.observe_rows(List::Versions, &["a", "b"]);
    for _ in 0..1250 {
        if !pump.is_animating() {
            break;
        }
        pump.advance(FRAME_MS);
    }
    assert!(!pump.is_animating(), "前提：已经收尾");
    let samples = pump.samples();
    for _ in 0..10 {
        assert!(!pump.advance(FRAME_MS), "收尾后 `advance` 不许再报推进");
    }
    assert_eq!(pump.samples(), samples, "收尾后一帧都不许再采样");
}

/// **`MotionScale::NONE` 严格落到最终状态**：可见度 1.0、偏移 0.0、零采样、零订阅。
///
/// 这是任务里那条硬要求的断言版：「值仍是最终值，不得停在中途」。
#[test]
fn no_scale_lands_every_row_on_the_final_state() {
    let mut pump = Pump::new();
    pump.configure(MotionScale::NONE, None);
    pump.observe_rows(List::Versions, &["a", "b"]);
    assert!(
        !pump.observe_rows(List::Versions, &["a", "b", "c"]),
        "关掉动效不许起轨道"
    );
    assert!(!pump.is_animating());
    assert_eq!(pump.samples(), 0);

    for id in ["a", "b", "c"] {
        let frame = pump.row_frame(List::Versions, id);
        assert_eq!(
            frame,
            RowFrame::REST,
            "{id} 在 NONE 档必须是静息（可见度 1、偏移 0），实得 {frame:?}"
        );
    }
    assert_eq!(
        pump.rows_drawn(List::Versions),
        vec!["a", "b", "c"],
        "NONE 不留退场幽灵"
    );

    // 反向（删一行）同样是瞬时的
    pump.observe_rows(List::Versions, &["a", "c"]);
    assert_eq!(pump.rows_drawn(List::Versions), vec!["a", "c"]);
    assert_eq!(pump.samples(), 0);
}

/// 错位有上限：长列表不许「越往后越等」。
///
/// 抓得住：`retarget` 里直接用 `index` 当错位档（50 行时最后一行要等 2.5s）。
#[test]
fn the_stagger_is_capped_so_a_long_list_does_not_wait_forever() {
    let mut pump = standard();
    let before: Vec<String> = Vec::new();
    let ids: Vec<String> = (0..40).map(|index| format!("v{index}")).collect();
    let _ = before;
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    pump.observe_rows(List::Versions, &[]);
    pump.observe_rows(List::Versions, &refs);

    let spec = row_spec();
    let stagger = f64::from(MotionScale::STANDARD.ms(spec.stagger));
    let cap = f64::from(spec.max_stagger) * stagger + f64::from(spec.enter.duration.ms());
    let mut frames = 0;
    for _ in 0..1250 {
        if !pump.is_animating() {
            break;
        }
        pump.advance(FRAME_MS);
        frames += 1;
    }
    let total_ms = f64::from(frames) * FRAME_MS;
    assert!(
        total_ms <= cap + FRAME_MS * 2.0,
        "40 行的总时长 {total_ms}ms 超过「错位封顶 + 一条进场腿」的上界 {cap}ms"
    );
}

/// 同一份清单再报一次是**空操作**（视图重建 / 截图 harness 每帧重建模型都靠这条）。
#[test]
fn observing_the_same_list_twice_is_a_no_op() {
    let mut pump = standard();
    pump.observe_rows(List::Versions, &["a", "b"]);
    assert!(pump.observe_rows(List::Versions, &["a", "b", "c"]));
    pump.advance(FRAME_MS);
    let mid = visibility(&pump, "c");
    assert!(
        !pump.observe_rows(List::Versions, &["a", "b", "c"]),
        "同一份清单不许重起"
    );
    assert_eq!(visibility(&pump, "c"), mid, "进度不许被重置");
}

/// **进场走到一半就被删的行不许当场消失**（r1 P2-1）。
///
/// 场景：搜索框连打 / 两次分类点选间隔 < 200ms —— 上一次的进场还没走完，这一行的 id 就没了。
/// 拿 `track.from`（进场时是 0）当退场起点会得到「0 → 0」：可见度与高度当拍归零、下面整列上跳，
/// 正是任务①禁的闪跳。反方向（退场中途回来）本来就接当前采样，两个方向必须同一条口径。
///
/// 抓得住：`visibility_of` 用 `track.from` 而不是当前采样。
/// 抓不住：错位那几行的相位（另有 `the_stagger_is_capped_…` 盯着）。
#[test]
fn deleting_a_row_mid_enter_does_not_jump_to_zero() {
    let mut pump = standard();
    // b 在**第 0 位**：错位档 0，进场立刻开始。
    pump.observe_rows(List::Versions, &["a"]);
    assert!(pump.observe_rows(List::Versions, &["b", "a"]));
    for _ in 0..3 {
        pump.advance(FRAME_MS);
    }
    let mid = visibility(&pump, "b");
    assert!(mid > 0.0 && mid < 1.0, "前提：进场走到一半（{mid}）");
    let height_before = pump.row_frame(List::Versions, "b").visibility;

    // 这一行在进场中途被删掉
    assert!(pump.observe_rows(List::Versions, &["a"]));
    let after = visibility(&pump, "b");
    assert_eq!(
        after, height_before,
        "退场起点必须是**当前**可见度（{height_before}），不许当场归零"
    );
    assert!(after > 0.0, "这一行还在画、还必须看得见：{after}");
    // 之后才慢慢退
    pump.advance(FRAME_MS);
    assert!(visibility(&pump, "b") < after, "接着往下退");
}

/// **纯重排当场落到新序**（r1 P2-3）：同一批 id 换顺序时不建轨道，但画的顺序必须立刻对齐。
///
/// 抓得住：`observe_rows` 少了「没有进/出就把 `order` 对齐 `current`」那一段 —— 那时没有轨道
/// ⇒ `advance` 早退 ⇒ `expire` 不跑 ⇒ `order` 永远停在旧序（`Rows::expire` 是它唯一的归位处）。
#[test]
fn a_pure_reorder_lands_on_the_new_order_at_once() {
    let mut pump = standard();
    pump.observe_rows(List::Versions, &["a", "b", "c"]);
    pump.observe_rows(List::Versions, &["c", "b", "a"]);
    assert!(!pump.is_animating(), "纯重排不建轨道");
    assert_eq!(
        pump.rows_drawn(List::Versions),
        vec!["c", "b", "a"],
        "没有轨道就没有 expire，顺序必须当场归位"
    );
}

/// 退场中途又回列表：接着当前进度往回走，**不许跳回 0**（错位入也不许闪一下）。
#[test]
fn a_row_that_returns_mid_exit_does_not_jump() {
    let mut pump = standard();
    // b 在第 0 位（错位档 0）：三帧之后必定走到中途。
    pump.observe_rows(List::Versions, &["b", "a"]);
    pump.observe_rows(List::Versions, &["a"]);
    for _ in 0..3 {
        pump.advance(FRAME_MS);
    }
    let low = visibility(&pump, "b");
    assert!(low > 0.0 && low < 1.0, "前提：退场走到一半（{low}）");

    pump.observe_rows(List::Versions, &["b", "a"]);
    assert_eq!(visibility(&pump, "b"), low, "重新回来必须接着当前进度");
    pump.advance(FRAME_MS);
    let later = visibility(&pump, "b");
    assert!(later > low, "回来之后必须往 1 走（{low} → {later}）");
}

/// **两张列表各记各的账**（M313）：喂版本列表不许把实例列表的行判成退场，反过来也一样。
///
/// 为什么要有这条：M305 的 `Rows` 只有**一份**清单。实例页接上来之后，两张列表同一拍各报一次
/// 真值清单——若还共用一份账，第二张一喂进来，第一张的所有行立刻被判成「离场」，整表闪跳、
/// 轨道永远停不下来。`pump/lists.rs` 就是为此存在的。
///
/// 抓得住：`Pump::observe_rows` 里把 `self.rows.of_mut(list)` 改回「恒用第一份」（或 `Lists::of_mut`
/// 两个分支返回同一个字段）——本用例第一段断言立刻红（实例列表的清单被版本列表的 id 覆盖）。
/// 抓不住：两张列表**画出来**对不对（那是页面接线，归 `pages/instances/tests/motion.rs`）。
#[test]
fn the_two_lists_keep_their_own_order_and_tracks() {
    let mut pump = standard();
    assert!(!pump.observe_rows(List::Versions, &["v1", "v2", "v3"]));
    assert!(!pump.observe_rows(List::Instances, &["i1", "i2"]));

    // 只动版本列表：实例列表的清单与轨道一条都不许受影响。
    assert!(pump.observe_rows(List::Versions, &["v1", "v3"]));
    assert_eq!(
        pump.rows_drawn(List::Instances),
        vec!["i1", "i2"],
        "版本列表的变化不许污染实例列表的清单"
    );
    assert!(
        !pump.rows_animating(List::Instances),
        "实例列表没有轨道在跑"
    );
    assert_eq!(
        pump.row_frame(List::Instances, "i2").visibility,
        1.0,
        "实例列表的每一行都得是静息"
    );
    assert_eq!(
        pump.rows_drawn(List::Versions),
        vec!["v1", "v2", "v3"],
        "版本列表自己的退场行照旧钉在原位"
    );

    // 反向：动实例列表，版本列表的轨道照旧按自己的时钟走。
    // 先让版本列表那条退场腿走两帧（错位档 ≠ 0 的行有 50ms 前置延迟，第一帧还在 1.0）。
    for _ in 0..4 {
        pump.advance(FRAME_MS);
    }
    let before = pump.row_frame(List::Versions, "v2").visibility;
    assert!(
        before > 0.0 && before < 1.0,
        "前提：v2 正在退场（{before}）"
    );
    assert!(pump.observe_rows(List::Instances, &["i1"]));
    assert_eq!(
        pump.rows_drawn(List::Instances),
        vec!["i1", "i2"],
        "实例列表的退场行钉原位"
    );
    assert!(
        (pump.row_frame(List::Versions, "v2").visibility - before).abs() < f32::EPSILON,
        "实例列表的变化不许推动版本列表的时钟"
    );

    // 两张都走完之后各自归位（`order` 与真值逐项相同）。
    for _ in 0..200 {
        if !pump.is_animating() {
            break;
        }
        pump.advance(FRAME_MS);
    }
    assert_eq!(pump.rows_drawn(List::Versions), vec!["v1", "v3"]);
    assert_eq!(pump.rows_drawn(List::Instances), vec!["i1"]);
}
