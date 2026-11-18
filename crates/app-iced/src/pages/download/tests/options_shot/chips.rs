//! Chips 档（窗口 < 736、可用宽 < 600）的像素守门：M125 缺陷 1。

use super::{bands, feed, ui, Shot, CHIPS_SHOT};

/// `02-…-chips700.png`（700×760，`ULL_DL_CHIPS_SHOT`）——**M125 缺陷 1 的回归守门**。
///
/// 修前（M123 取证，2026-11-05，tiny-skia，700×760，`ULL_DL_DEMO=options-fabric`）：
/// chips 档多出两行 chip，选项区又整块钉在列表下面（316px），于是列表列只剩一条 **18px** 的缝，
/// 选中行的底色带实测 19px（行高 44）——版本号一个字读不出来，用户换不了版本。
/// 修法见 `rows::TailShare`：chips 档改成「面板先保预算、列表按整行吃剩下的」
/// （M227 起是 `TailShare::Narrow` + `rows/tail.rs::CHIPS_PANEL_BUDGET`，700/720×760 = 3 整行）。
///
/// ```text
/// ULL_DL_CHIPS_SHOT=02-options-fabric-en-light-chips700.png \
///   cargo test -p unlucky-app-iced --lib options_shot
/// ```
///
/// 抓得住：①首行版本行的底色带 = 44（整行在屏）；②**列表区 ≥ 3 整行**（面板上沿那条通栏
/// 分隔线就是下界）——这是窄档预算的硬下限：面板钉死区若重新钉住版本菜单（M227 之前的样子），
/// 它在 459px 里只能再留 1 整行给列表；③面板头部在屏：断言在面板上沿分隔线下方 96px 的窗口
/// （分隔线 514 ⇒ 窗口 518–610）里找一条 ≥ 80px 的同色带，窄档实测命中 **y=550、宽 94px**，
/// 那是**实心「安装」按钮的药丸本身**——版本菜单容器的上沿在 636–639，落在窗口之外，与本条无关
/// （r2 复审订正：旧注释把「菜单容器的满宽底边」也列成可能来源，那是错的）。
/// 「能换版本」与「能看见选项面板」是这条断言的成对验收。
///
/// 抓不住：文本有没有被**横向**裁掉（那要看字形宽度，见 `tests/rails.rs` 的标签断言与
/// 报告里的量测表）、hover/tooltip 层（静态图拍不到）、面板内部滚动到底后长什么样
/// （静态图只有一帧，只能拍到 scroll offset 0）——所以这里**不**断言「面板没被窗口底边切」：
/// chips 档的面板本来自己可滚，实例名钉在面板底部。**M227 起**滚动区首屏装
/// head + 六枚 chip + 说明行（104px），版本菜单露在折线下方、滚得到。
#[test]
fn a_chips_tier_shot_keeps_a_whole_version_row_readable() {
    let Some(path) = feed(CHIPS_SHOT) else {
        return;
    };
    let shot = Shot::load(&path);
    let bands = bands(700.0);
    let canvas = shot.at(bands.list.0 + 2, 12);
    let metrics = ui().metrics();

    // ① 首行整行 44：这是 M123 原本写的那条（修好之后解 ignore，不许改期望值迁就现状）
    let (row_top, row_height) = shot
        .first_filled_band(bands.list.0 + 60, 360, 440, canvas)
        .expect("列表列里应当有版本行");
    assert_eq!(
        row_height as f32, metrics.row_height,
        "Chips 档的版本行被压成 {row_height}px（应为 {}）",
        metrics.row_height
    );

    // ② 版本列表那一块至少 3 整行高：尾部面板上沿的通栏分隔线就是列表的下界
    let right = bands.list.0 + bands.list.1 as usize - 2;
    let rule = shot
        .first_rule_y(bands.list.0, right, row_top, canvas)
        .expect("找不到尾部面板上沿的分隔线（列表区的下界量不出来）");
    let rows_px = rule.saturating_sub(row_top) as f32;
    assert!(
        rows_px >= 3.0 * metrics.row_height,
        "Chips 档的列表区只有 {rows_px}px（y={row_top}..{rule}），不足 3 整行（{}）：\
         尾部面板把列表压扁了",
        3.0 * metrics.row_height
    );

    // ③ 面板在屏内：分隔线下面紧接着就是面板的头（那条实心「安装」按钮 = 一条 ≥ 80px 的实色带）
    let (y, run) = shot.longest_run(bands.list.0, right, rule + 4, rule + 96, canvas);
    assert!(
        run >= 80,
        "选项面板没画出来：分隔线 y={rule} 下方 96px 里最长只有 {run}px 的同色带（y={y}），\
         看不到那条实心「安装」按钮"
    );

    // ④ 面板确实占了下半块（列表区的下界以上都归列表，以下归面板）
    let last = shot
        .last_ink_row(bands.list.0, right, canvas)
        .expect("列表列里应当有内容");
    assert!(
        last > rule + 100,
        "选项面板没有占到下半块：最后一行墨迹在 y={last}，面板上沿在 y={rule}"
    );
}
