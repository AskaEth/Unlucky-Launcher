//! 「还没选版本」那一态（`ULL_DL_DEMO` 不设 = `ready`）的像素守门：r1 复审 P2-1。
//!
//! ## 为什么要单独守这一态
//!
//! M125 r1 的 `TailShare::of` **只判档位**，于是 chips 档的「刚进页面」也被五五分：
//! 面板里只有一行提示（`PanelState::Hint`，≈30px），却拿走一半高度 ⇒ 700×760 下
//! 只剩 **4 整行**可读、下半块留 ≈160px 空画布（main 那一态是 **7 整行**：不是 8，
//! 第 8 条墨迹其实是 `Pick a version, then install` 那句 hint，横向范围对不上版本号，
//! 而且 700×760 上放 8 行要 352px、实测只剩 309~322px。r2 修回 7 整行）。
//! 同目录 `chips.rs` 只拍「选中 + 选项区展开」，正是这个盲区把回归放了过去。
//!
//! 现在的判据：`Chips` **且**面板真占高度（`Options` / `Install`）才共享；
//! `Hint` 在任何档位都钉死（见 `rows::TailShare::of`）。
//!
//! ```text
//! # 不设 ULL_DL_DEMO（= ready）出 700×760 那张
//! cargo run -p unlucky-app-iced --example pages_demo -- \
//!   --screen=download --width=700 --height=760 --screenshot=ready700.png \
//!   --screenshot-theme=light --software-renderer
//! ULL_DL_READY_SHOT=ready700.png cargo test -p unlucky-app-iced --lib options_shot
//! ```
//!
//! 抓得住：①列表区 **≥ 6 整行**（数版本号那列的墨迹条数，行距 44）；②尾部（提示那条
//! 分隔线）在 6 行之后——五五分时它落在第 4 行下方，两条都立刻红。
//! 抓不住：提示文案本身、hover/tooltip 层（静态图拍不到）；也不管「选中且展开」那一态
//!（那是 `chips.rs` 的活）。

use super::{bands, feed, ui, Shot, READY_SHOT};

/// `ready` 那一态（700×760）：8 个版本里至少 6 行要在屏，尾部提示不许把列表压扁。
///
/// 修前实测（M125 r1，`TailShare::of` 只判档位的版本）：列表区 `374..559` = 185px = 4 行，
/// 版本号那列只有 4 条墨迹；面板那半块 185px 里只有一行 `Pick a version, then install`，
/// 其余是空白 —— 这条断言在那种图上是红的（4 < 6）。
#[test]
fn a_ready_shot_keeps_the_whole_visible_list() {
    let Some(path) = feed(READY_SHOT) else {
        return;
    };
    let shot = Shot::load(&path);
    assert_eq!(shot.width, 700, "这条只认 700×760 那张（chips 档）");

    let bands = bands(700.0);
    let canvas = shot.at(bands.list.0 + 2, 12);
    let metrics = ui().metrics();
    let right = bands.list.0 + bands.list.1 as usize - 2;

    // ① 第一行版本号的墨迹带（ready 态没有选中行，所以量的是**文本**而不是底色；
    //    x 取 `+100` 落在版本号那几个字上——`+60` 会落进「前置下载图标」与文字之间的空档）
    let (first_ink, _) = shot
        .first_filled_band(bands.list.0 + 100, 360, 440, canvas)
        .expect("列表列里应当有版本行");

    // ② 尾部（提示）上沿的分隔线：它上面的高度才是列表拿到的
    let rule = shot
        .first_rule_y(bands.list.0, right, first_ink, canvas)
        .expect("找不到尾部那条分隔线（列表区的下界量不出来）");

    // ③ 数得出几个版本号：8 行里至少 6 行在屏
    let rows = shot.ink_bands(
        bands.list.0 + 50,
        bands.list.0 + 140,
        first_ink,
        rule,
        canvas,
    );
    assert!(
        rows >= 6,
        "ready 态只读得出 {rows} 个版本号（要求 ≥ 6 个，演示数据共 8 个）：\
         面板那一行提示把列表对半切了（列表区 y={first_ink}..{rule}，\
         共 {}px，行高 {}）",
        rule - first_ink,
        metrics.row_height
    );
    assert!(
        (rule - first_ink) as f32 >= 6.0 * metrics.row_height - metrics.row_height / 2.0,
        "ready 态的列表区只有 {}px（y={first_ink}..{rule}），不足 6 整行",
        rule - first_ink
    );
}
