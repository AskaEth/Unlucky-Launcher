//! 帧泵与过渡表的守卫（M192 建，M256 扩）。全部用**本地** `Pump` 摆时间轴：不起窗口、
//! 不等真实时间，所以不会假绿。
//!
//! 接线级的守卫（真消息 / 真订阅表）在 `app/subscription.rs`：这一组只管算术与表。
//!
//! 每条用例都写清「抓得住什么 / 抓不住什么」（T27 之后的纪律）。判据是「泵帧数 + 端点 +
//! 单调性」，不是像素比对：像素只能证明"某一张图有没有偏移"，证明不了"中间有 19 个相位"。
//!
//! 模态 / 系统偏好 / 降级那一半在 [`modal`]。

mod modal;
mod timeline;

use super::pump::Pump;
use super::transition::{page_spec, Kind, ModalFrame, PageFrame};
use super::*;
use unlucky_design::motion::MotionScale;

/// 标准档、没有钉相位的泵（出厂态是 `NONE`，见 [`Pump::new`]）。
fn standard() -> Pump {
    let mut pump = Pump::new();
    pump.configure(MotionScale::STANDARD, None);
    pump
}

/// 按「每帧 16 ms」推进到没有轨道为止，返回推进了多少帧（上限 1250 帧 = 20 s，防死循环）。
fn settle(pump: &mut Pump) -> u64 {
    let mut frames = 0;
    for _ in 0..1250 {
        if !pump.is_animating() {
            break;
        }
        if pump.advance(FRAME_MS) {
            frames += 1;
        }
    }
    frames
}

/// 一路采样所有中间态（含终点），用完自己 settle。
fn samples(pump: &mut Pump) -> Vec<PageFrame> {
    let mut out = vec![pump.page_frame()];
    while pump.is_animating() {
        pump.advance(FRAME_MS);
        out.push(pump.page_frame());
    }
    out
}

// ------------------------------------------------------------------ 规格表

/// 规格**全部**来自 design 的令牌表，且落在 M3E 的短档（150~300ms）。
///
/// 抓得住：谁把某档改成 400ms 以上（"更显眼"的诱惑），或把幅度改成非 4 的倍数。
/// 抓不住：观感好不好——那要截图（D30），不在这条用例的能力范围里。
#[test]
fn every_transition_is_built_from_registered_tokens() {
    let kinds = [
        Kind::Boot,
        Kind::Peer { forward: true },
        Kind::Peer { forward: false },
        Kind::Push,
        Kind::Pop,
    ];
    for kind in kinds {
        let spec = page_spec(kind);
        for leg in [spec.enter, spec.exit] {
            assert!(
                (150..=300).contains(&leg.duration.ms()),
                "{}: {}ms 不在 MD3E 短档（150~300）",
                kind.name(),
                leg.duration.ms()
            );
            assert!(leg.easing.sample(0.0).abs() < 1.0e-6);
            assert!((leg.easing.sample(1.0) - 1.0).abs() < 1.0e-6);
        }
        // 进略长于出（M3E：进场要看清，退场只是让位）
        assert!(
            spec.enter.duration.ms() > spec.exit.duration.ms(),
            "{}: 进场必须比退场长（进 {} / 出 {}）",
            kind.name(),
            spec.enter.duration.ms(),
            spec.exit.duration.ms()
        );
        // 幅度落在 4px 栅格上，且退场幅度小于进场幅度
        let (inx, iny) = spec.enter_from;
        let (outx, outy) = spec.exit_to;
        for value in [inx, iny, outx, outy] {
            assert_eq!(value % 4.0, 0.0, "{}: {value} 不在 4px 栅格上", kind.name());
        }
        assert!(
            inx.abs().max(iny.abs()) >= outx.abs().max(outy.abs()),
            "{}: 退场幅度不该大于进场幅度",
            kind.name()
        );
    }
    // 四档的动作方向钉住（谁改方向，报告与出图都要跟着改）
    assert_eq!(
        page_spec(Kind::Peer { forward: true }).enter_from,
        (12.0, 0.0)
    );
    assert_eq!(page_spec(Kind::Peer { forward: true }).exit_to, (-8.0, 0.0));
    assert_eq!(
        page_spec(Kind::Peer { forward: false }).enter_from,
        (-12.0, 0.0)
    );
    assert_eq!(page_spec(Kind::Push).enter_from, (0.0, 16.0));
    assert_eq!(page_spec(Kind::Push).exit_to, (0.0, -8.0));
    assert_eq!(page_spec(Kind::Pop).enter_from, (0.0, -16.0));
}

// ------------------------------------------------------------------ 页面切换

/// **进入是 12~20 帧、落在端点、中途有严格中间态**（塔给的判据原文）。
///
/// 抓得住：①把时长改到 300ms 以外（帧数跑出窗口）；②把 `enter_offset` 改成直接返回 0
/// （"硬跳"，中间态消失）；③把末帧没归零（`PageFrame::STILL` 的端点断言红）。
/// 抓不住：这一帧画出来好不好看（截图的事）。
#[test]
fn a_page_enters_over_twelve_to_twenty_frames_and_lands_on_the_endpoint() {
    let mut pump = standard();
    assert!(pump.observe(Some(2), Some(1), None), "首次观察要起跑 Boot");
    let frames = settle(&mut pump);
    assert!(
        (12..=20).contains(&frames),
        "进入应落在 12~20 帧，实测 {frames} 帧"
    );

    // 重跑一次，逐帧看端点与单调性
    pump.observe(Some(3), Some(1), None); // 同级换页：medium2 = 300ms
    let trace = samples(&mut pump);
    assert!(trace.len() >= 12, "中间态太少：{}", trace.len());
    assert_eq!(
        trace.last().copied(),
        Some(PageFrame::STILL),
        "末帧必须归位"
    );
    assert_eq!(trace[0].enter, (12.0, 0.0), "首帧必须满幅（向右推进）");
    assert!(
        trace
            .iter()
            .skip(1)
            .any(|f| f.enter.0 > 0.0 && f.enter.0 < 12.0),
        "中途必须存在**严格介于两端之间**的相位"
    );
    // 单调收敛到 0（缓动是单调曲线，取整也不会反弹）
    let mut previous = trace[0].enter.0;
    for frame in trace.iter().skip(1) {
        assert!(
            frame.enter.0 <= previous,
            "位移必须单调归零：{previous} → {}",
            frame.enter.0
        );
        previous = frame.enter.0;
    }
}

/// **旧页反向退场**：同一拍里新页从 +12 归零、旧页向 −8 走；退场腿更短，先结束。
///
/// 抓得住：把 `exit` 位移删掉/写成同向（"只挪新页"），或让退场腿和进场腿一样长。
/// 抓不住：旧页**内容**是否正确（那是 `pages::page_with_settings_transition` 的职责，
/// 归 `app` 层的用例）。
#[test]
fn the_old_page_moves_the_other_way_and_leaves_first() {
    let mut pump = standard();
    pump.observe(Some(2), Some(1), None);
    settle(&mut pump);

    assert!(pump.observe(Some(3), Some(1), None));
    let first = pump.page_frame();
    assert_eq!(first.enter, (12.0, 0.0));
    assert_eq!(first.exit, (0.0, 0.0), "退场起点是 0");
    assert!(first.exiting, "退场腿刚开始");

    let mut saw_half = false;
    let mut exit_finished_at = None;
    let mut frames = 0;
    while pump.is_animating() {
        frames += 1;
        pump.advance(FRAME_MS);
        let frame = pump.page_frame();
        if frame.exit.0 < 0.0 && frame.exit.0 > -8.0 {
            saw_half = true;
        }
        if !frame.exiting && exit_finished_at.is_none() {
            exit_finished_at = Some(frames);
        }
    }
    assert!(saw_half, "旧页必须有严格中间态（不是一步到位）");
    let exit_frames = exit_finished_at.expect("退场腿必须在进场腿之前结束");
    assert!(
        exit_frames < frames,
        "退场（{exit_frames} 帧）必须比进场（{frames} 帧）先结束"
    );
}

/// **方向由导航深度决定**：同级换页走水平、进深层走纵向。
///
/// 抓得住：把四档动作表合并成一档（"都往右下飘"），或把 `forward` 判据反了（回到左边却向右推）。
/// 抓不住：导航表本身的顺序对不对（那是 `pages::Screen::ALL` / `screen_key` 的事）。
#[test]
fn direction_follows_navigation_depth() {
    let mut pump = standard();
    pump.observe(Some(1), Some(0), None);
    settle(&mut pump);

    // 向右推进（屏键 1 → 3）
    pump.observe(Some(3), Some(1), None);
    assert_eq!(pump.kind(), Some(Kind::Peer { forward: true }));
    assert_eq!(pump.page_frame().enter, (12.0, 0.0), "同级往右 = 水平位移");
    settle(&mut pump);

    // 向左退回（3 → 1）
    pump.observe(Some(1), Some(0), None);
    assert_eq!(pump.kind(), Some(Kind::Peer { forward: false }));
    assert_eq!(pump.page_frame().enter, (-12.0, 0.0), "同级往左 = 镜像");
    settle(&mut pump);

    // 进深层（同屏：列表 → 详情）
    pump.observe(Some(1), Some(0), Some("instance-7"));
    assert_eq!(pump.kind(), Some(Kind::Push));
    let push = pump.page_frame();
    assert_eq!(push.enter, (0.0, 16.0), "进深层 = 纵向（从下方上浮）");
    assert_eq!(push.exit, (0.0, 0.0));
    settle(&mut pump);

    // 出深层（详情 → 列表）
    pump.observe(Some(1), Some(0), None);
    assert_eq!(pump.kind(), Some(Kind::Pop));
    assert_eq!(pump.page_frame().enter, (0.0, -16.0), "出深层必须镜像");
}

/// **左右方向看导航轨序，不看屏键**（复审 P2-1 的守卫）。
///
/// 两处顺序不同：`Screen::ALL` 是 `… Content / Settings / Logs`，导航轨是 `… Logs / Settings / Ai`。
/// 用屏键判方向会让 **Logs↔Settings 镜像**。
///
/// 抓得住：把 `Pump::observe` 的方向改回「比较屏键」（`new > old`）——本用例里
/// Settings(键 4 / **序 3**) → Logs(键 5 / **序 2**) 会被判成向右推进，第一条断言立即红。
/// 也抓得住「谁调了 `Screen::ALL` 的顺序」：这里**只引用轨序**，与那张表的声明顺序解耦。
/// 抓不住：导航轨本身的视觉位置（那是 `shell/nav.rs` 的几何，归它的守卫）。
#[test]
fn direction_comes_from_the_nav_rail_not_the_screen_table() {
    let mut pump = standard();
    pump.observe(Some(4), Some(3), None); // Settings：屏键 4、轨序 3
    settle(&mut pump);

    // 轨序 3 → 2（向左）。若按屏键 4 → 5，会被判成向右 —— 那正是被修的 bug。
    pump.observe(Some(5), Some(2), None);
    assert_eq!(
        pump.kind(),
        Some(Kind::Peer { forward: false }),
        "Settings → Logs 在导航轨上是向左"
    );
    assert_eq!(pump.page_frame().enter, (-12.0, 0.0));
    settle(&mut pump);

    // 反向：轨序 2 → 3 = 向右
    pump.observe(Some(4), Some(3), None);
    assert_eq!(pump.kind(), Some(Kind::Peer { forward: true }));
    assert_eq!(pump.page_frame().enter, (12.0, 0.0));
}

/// **`MotionScale::NONE` 下位移与缩放严格为 0**（不是"很小"）。
///
/// 抓得住：把 `play_page` 的 `is_off()` 分支删掉（变成跑一遍 0 时长动画）——那时 `frame()`
/// 会短暂给出非 0 的位移、`samples` 会增长、帧订阅次数不再是 0。
/// 抓不住：OS 偏好有没有真的读到（那是 `platform::motion` 的用例与真机取证）。
#[test]
fn none_scale_is_strictly_zero_not_merely_small() {
    let mut pump = Pump::new(); // 出厂即 NONE
    assert!(pump.scale().is_off());

    // 首屏 / 换页 / 进退层：三次状态变化都不许留下轨道，位移必须 `== 0.0`（不是 `abs() < eps`）
    for (screen, rank, detail) in [
        (Some(1), Some(0), None),
        (Some(2), Some(1), None),
        (Some(2), Some(1), Some("instance-7")),
        (Some(2), Some(1), None),
    ] {
        assert!(!pump.observe(screen, rank, detail), "关掉动效时不许起跑");
        let frame = pump.page_frame();
        assert_eq!(frame.enter.0, 0.0);
        assert_eq!(frame.enter.1, 0.0);
        assert_eq!(frame.exit.0, 0.0);
        assert_eq!(frame.exit.1, 0.0);
        assert!(!frame.exiting, "NONE 档不许画旧页");
    }

    // 模态：进场直接到 1.0（缩放严格 1.0），退场直接关
    assert!(!pump.observe_modal(true), "NONE 档不起模态轨道");
    assert_eq!(
        pump.modal_frame(),
        ModalFrame {
            visible: true,
            progress: 1.0
        }
    );
    assert!((modal_scale(1.0) - 1.0).abs() < 1.0e-6, "缩放严格 1.0");
    assert!(pump.begin_modal_close(), "NONE 档必须**当场**关，不留退场");
    assert_eq!(pump.modal_frame(), ModalFrame::HIDDEN);

    assert!(!pump.is_animating());
    for _ in 0..120 {
        assert!(!pump.advance(FRAME_MS), "NONE 档没有任何可推进的");
    }
    assert_eq!(pump.samples(), 0, "NONE 档的帧采样次数必须是 0");
}

// ------------------------------------------------------------------ 触发判据

/// 重复绘制**永远不会**重新起跑（否则每帧都重启动画 = 永远停不下来）。
///
/// 抓得住：把「标识没变 ⇒ 不起跑」的判断写错（比如每个 view 都 `play_page`）——30 次重复绘制后
/// `is_animating()` 仍为真、帧泵永远停不下来，本条红。
/// 抓不住：真实屏幕上"重画是否真的只是重画"（那要看 iced 的订阅表，归 `app::subscription` 的用例）。
#[test]
fn repeated_paints_never_restart_the_animation() {
    let mut pump = standard();
    pump.observe(Some(3), Some(1), Some("instance-9"));
    settle(&mut pump);

    for _ in 0..30 {
        assert!(!pump.observe(Some(3), Some(1), Some("instance-9")));
        assert_eq!(pump.page_frame(), PageFrame::STILL);
    }
    assert!(!pump.is_animating());
}

/// 不归 pages 画的屏（账户 / AI 独立页）**不起跑**；回到某一屏照样跑。
///
/// 抓得住：把 `screen == None` 也当成一次换页起跑（会白跑几百毫秒的帧却没人采样）。
/// 抓不住：账户页 / AI 页自己的渲染（它们不走 `pages::render`，不归本 pump 管）。
#[test]
fn screens_rendered_elsewhere_never_start_a_track() {
    let mut pump = standard();
    pump.observe(Some(1), Some(0), None);
    settle(&mut pump);

    assert!(
        !pump.observe(None, None, None),
        "没有 pages 树可动 ⇒ 不许白开帧"
    );
    assert!(!pump.is_animating());

    assert!(
        pump.observe(Some(1), Some(0), None),
        "从账户页回来仍然是换页"
    );
    assert_eq!(pump.kind(), Some(Kind::Boot), "没有旧页可退 ⇒ 按首屏方向");
}

/// **过渡中途再换页要能重入**（塔的门槛 D）：新的进场、旧的立刻被取代，不叠加、不卡死。
///
/// 抓得住：把「新轨道取代旧轨道」改成 push（位移会叠加成 24px）、或让旧轨道继续占帧
/// （帧数会明显超出 12~20 那一档）。
/// 抓不住：`App::previous_view` 的丢弃（那条在 `app/update/motion_sync.rs` 的用例里）。
#[test]
fn a_second_navigation_mid_flight_takes_over_cleanly() {
    let mut pump = standard();
    pump.observe(Some(1), Some(0), None);
    settle(&mut pump);

    pump.observe(Some(2), Some(1), None); // 往右推进
    for _ in 0..4 {
        pump.advance(FRAME_MS); // 走 3~5 帧就再点一次
    }
    let mid = pump.page_frame();
    assert!(
        mid.enter.0 < 12.0 && mid.enter.0 > 0.0,
        "第一次过渡还在半路"
    );

    pump.observe(Some(1), Some(0), None); // 立刻退回左边
    let restarted = pump.page_frame();
    assert_eq!(restarted.enter, (-12.0, 0.0), "新方向立刻生效（满幅起点）");
    assert_eq!(
        restarted.exit,
        (0.0, 0.0),
        "旧页从 0 重新开始，不与上一次叠加"
    );

    let frames = settle(&mut pump);
    assert!(
        (12..=20).contains(&frames),
        "重入之后仍要在 12~20 帧内停下，实测 {frames}"
    );
    assert_eq!(pump.page_frame(), PageFrame::STILL);
}
