//! 状态层过渡的判据（M257 / A8）：**帧数 + 端点精确 + 中间态严格单调**，外加两条硬契约。
//!
//! 两条硬契约（塔在 M257 briefing 里点名的）：
//!
//! 1. `MotionScale::NONE` 下进度**严格**等于终值（独立用例，不靠别的用例顺带覆盖）；
//! 2. 「鼠标停住不动」的那段时间里过渡必须**自己走完并停在终值**——本文件用「只推时钟、不喂事件」
//!    的形态抓「停在 progress 0」这个具体故障。

use super::*;
use crate::app::components::motion::FRAME_MS;
use crate::theme::to_iced;
use unlucky_design::color::{BrandBoard, Role, SchemeFlavor};
use unlucky_design::motion::MotionScale;

const STANDARD: MotionScale = MotionScale::STANDARD;
const NONE: MotionScale = MotionScale::NONE;

/// 一条全新的过渡（每个用例开头都从干净时钟起跑）。
fn fresh() -> Layer {
    reset();
    Layer::new()
}

/// **悬停：50ms 内走到 1.0，端点精确，中间严格单调——且**不靠任何事件**。**
///
/// 这条用例直接对着塔的硬判据写：*「鼠标停住不动的整段时间里，过渡必须走完并停在终值」*。
/// 实现里唯一驱动它的是 [`advance`]（`motion::advance` 每帧调），所以「停在 progress 0」
/// （= 起跑写在 draw 侧、没有帧订阅）在这里会立刻红。
///
/// 抓得住：①把 `short1`(50) 改成 0（进度当帧就是 1.0 ⇒ 「第一帧必须是 0」的断言红）；
/// ②把 `advance` 的时钟加去掉（进度永远 0 ⇒ 终值断言红）；③把终止条件删掉（`is_animating` 恒真）。
/// 抓不住：真实帧率（那由 `window::frames()` 决定）、hover 的观感。
#[test]
fn hovering_fades_in_over_the_token_duration_without_any_further_events() {
    let mut layer = fresh();
    assert_eq!(layer.progress(), 1.0, "静息层：进度是终值（= 今天的行为）");

    assert!(
        layer.retarget(Interaction::Hovered, STANDARD),
        "目标变了必须起跑"
    );
    assert_eq!(layer.progress(), 0.0, "第一帧必须还在静息端（端点精确）");
    assert!(is_animating(), "起跑之后同一拍就该要帧");

    let mut values = vec![layer.progress()];
    let mut frames = 0;
    while is_animating() {
        advance_by(FRAME_MS);
        values.push(layer.progress());
        frames += 1;
        assert!(frames < 60, "过渡没停，疑似终止条件失效");
    }

    // 50ms / 16ms ≈ 3~4 帧（含到期后多要的那一帧）。
    assert!(
        (3..=5).contains(&frames),
        "50ms 应落在 3~5 帧，实测 {frames} 帧"
    );
    assert_eq!(values.last().copied(), Some(1.0), "终值必须**严格**是 1.0");
    for pair in values.windows(2) {
        assert!(
            pair[1] >= pair[0],
            "淡入必须单调不减：{:?} → {:?}",
            pair[0],
            pair[1]
        );
    }
    let distinct: Vec<_> = values.iter().filter(|v| **v > 0.0 && **v < 1.0).collect();
    assert!(
        !distinct.is_empty(),
        "必须有可判别的中间态（否则就是硬切）：{values:?}"
    );
    assert!(!is_animating(), "走完必须退订帧");
    assert_eq!(layer.progress(), 1.0, "没有轨道时进度恒为终值");
}

/// **`MotionScale::NONE` = 严格等于终值**（独立用例；不靠上一条顺带覆盖）。
///
/// 抓得住：把 `retarget` 里的 `NONE ⇒ ms = 0` 删掉（那会跑一遍 50ms ⇒ 首帧进度是 0.0，本用例红）；
/// 或把 NONE 实现成「很快走完」（同样红：本用例断言的是**当帧就到位**，不是「几帧后到位」）。
/// 抓不住：`NONE` 之外的档位（那是别的用例的事）。
#[test]
fn motion_scale_none_has_no_intermediate_state_at_all() {
    let mut layer = fresh();
    assert!(layer.retarget(Interaction::Hovered, NONE));
    assert_eq!(layer.progress(), 1.0, "NONE 下当帧就是终值（严格等于 1）");
    assert!(!is_animating(), "NONE 下不许要帧（不是「很快走完」）");

    assert!(layer.retarget(Interaction::Enabled, NONE));
    assert_eq!(layer.progress(), 1.0, "移出同样是当帧终值");
    assert!(!is_animating());
}

/// 同态重复不改目标（否则每次 draw 都会重启一条腿 ⇒ 过渡被无限拉长）。
///
/// 抓得住：把 `retarget` 的 `state == self.to` 早退删掉。
#[test]
fn retargeting_the_same_state_is_a_no_op() {
    let mut layer = fresh();
    assert!(layer.retarget(Interaction::Hovered, STANDARD));
    advance_by(FRAME_MS);
    let mid = layer.progress();
    assert!(!layer.retarget(Interaction::Hovered, STANDARD));
    assert_eq!(layer.progress(), mid, "同一目标的重复报告不许重置进度");
}

/// 按下比悬停更「强」：从悬停转按下走 `ENTER` 那条腿（起跑端是**上一个目标**，不是静息）。
///
/// 抓得住：把 `retarget` 里的 `self.from = self.to` 写成 `self.from = Interaction::Enabled`
/// （按下瞬间会先掉回静息色一帧——状态层闪一下），本用例断言 `from` 是 `Hovered`。
#[test]
fn pressing_starts_from_the_previous_target_not_from_rest() {
    let mut layer = fresh();
    layer.retarget(Interaction::Hovered, STANDARD);
    // 不推进：模拟「悬停刚开始就按下」（鼠标点得比淡入还快）。
    assert!(layer.retarget(Interaction::Pressed, STANDARD));
    let (from, to, p) = current_from(&layer);
    assert_eq!(from, Interaction::Hovered, "起跑端必须是上一个目标");
    assert_eq!(to, Interaction::Pressed);
    assert_eq!(p, 0.0, "新的一条腿从 0 开始");
}

/// 把层的两端读出来（`Layer` 的字段是私有的，这里经 [`with_current`] 走公开路径）。
fn current_from(layer: &Layer) -> (Interaction, Interaction, f32) {
    with_current(*layer, || current().expect("刚压进去的层必须在栈顶"))
}

/// 两个不透明令牌色的插值：**端点逐位相等**，中点严格落在两者之间。
///
/// ## 端点为什么是角色，而不是 `Color::from_rgb(…)` 写死的数值
///
/// 这两行原先随手写了 `from_rgb(0.1, 0.2, 0.3)` / `from_rgb(0.7, 0.8, 0.9)`，撞上
/// `tests/no_hardcoded_tokens.rs` 的色值守门（它扫 `src/**`，测试模块也算在里面）。修法是
/// **改测试、不动守门**：端点取状态层真正会插值的那一对——容器 [`Role::Surface`] 与内容
/// [`Role::OnSurface`]——经产品同一条令牌路径（`Scheme::get` → [`to_iced`]）翻成 iced 色。
///
/// ⚠️ 本文件里连**注释**都不许出现「构造名 + 数字」的写法：守门按行做子串匹配，不区分代码与
/// 文档（`the_gate_itself_catches_a_planted_violation` 也把这条钉住了）。要举例就写 `from_rgb(…)`。
///
/// ## 为什么这不算「削弱守卫」（将来别顺手把测试全排除）
///
/// 守卫的规则是「组件层的颜色一律来自 `unlucky_design` 的 `Role`」，它不区分测试代码与产品
/// 代码——也不该区分：测试里的色字面量**就是**组件层源码里长出来的一串数字，例外一旦开在
/// 「测试」名下，下一个「临时」的 `from_rgb` 只会多一个借口。所以这里不削弱、不豁免、不给守卫
/// 加任何例外：**扫描范围一行没改**，只是把被它正确抓到的那两行换成令牌色。
///
/// 反过来说，按路径排除 `*/tests.rs` 也排不干净：`#[cfg(test)] mod tests` 与产品代码**同文件**
/// （本模块就是 `layer.rs` 末尾的 `#[cfg(test)] mod tests;`），路径分不开「同文件里的测试」与
/// 「同文件里的产品代码」；而任何文件都能被改名叫 `tests.rs` 来藏字面量。结论：守卫保持全量
/// 覆盖，误判在**被判的那一侧**修。
///
/// ## 判据（三条一条不少）
///
/// 1. 端点逐位相等（`p = 0 / 1` 直接返回原值，不许「差不多」）；
/// 2. 中点**严格**落在两端之间（每个通道用两端现算的 `min`/`max`，不是抄来的数字）；
/// 3. α 全程 1.0（D27 的「两个后端逐像素一致」就靠这条守着）。
///
/// 浅/深两套品牌板各跑一遍，判据值全部现算 ⇒ 换板、换品牌色都不会让这条用例变哑。
///
/// 抓得住：①把端点写成「差不多」（`p == 0` 时返回插值结果 0.999…）——逐位断言红；
/// ②引入 α（`a` 不再是 1.0）——第 3 条红；③插值写成线性之外的东西（端点仍对、中点越界）——第 2 条红。
/// 抓不住：色值本身好不好看（那是角色映射的事）。
#[test]
fn blend_hits_both_endpoints_bit_for_bit() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let from = board_color(flavor, Role::Surface);
        let to = board_color(flavor, Role::OnSurface);

        // 「严格介于」的前提：两端不透明，且每个通道真的不同（分量可排序）。
        assert_eq!(
            (from.a, to.a),
            (1.0, 1.0),
            "{flavor:?}：Surface / OnSurface 必须都不透明（D27）"
        );
        assert!(
            differs_in_every_channel(from, to),
            "{flavor:?}：两端有通道相同（{from:?} / {to:?}），中点不可能严格介于——换一对角色"
        );

        assert_eq!(
            blend(from, to, 0.0),
            from,
            "{flavor:?}：p = 0 必须逐位等于起点"
        );
        assert_eq!(
            blend(from, to, 1.0),
            to,
            "{flavor:?}：p = 1 必须逐位等于终点"
        );

        for p in [0.25, 0.5, 0.75] {
            let mid = blend(from, to, p);
            for (name, value, start, end) in [
                ("r", mid.r, from.r, to.r),
                ("g", mid.g, from.g, to.g),
                ("b", mid.b, from.b, to.b),
            ] {
                let (lo, hi) = (start.min(end), start.max(end));
                assert!(
                    value > lo && value < hi,
                    "{flavor:?} p = {p}：{name} 必须**严格**落在两端之间（{lo} < {value} < {hi}）"
                );
            }
            assert_eq!(mid.a, 1.0, "中间态也不许引入半透明（D27）");
        }
    }
}

/// 品牌板上某个角色的 iced 颜色（与产品同一条令牌路径）。
fn board_color(flavor: SchemeFlavor, role: Role) -> iced::Color {
    to_iced(BrandBoard::for_flavor(flavor).scheme().get(role))
}

/// 三个通道是否都不同——只有全不同，「严格介于」才有定义。
fn differs_in_every_channel(from: iced::Color, to: iced::Color) -> bool {
    from.r != to.r && from.g != to.g && from.b != to.b
}
