//! Compact 档（可用宽 600~675）主版本栏的像素守门：M125 缺陷 2。

use super::{bands, feed, ui, Shot, COMPACT_SHOT};

/// `04-…-compact736.png`（736×760，`ULL_DL_COMPACT_SHOT`）——**Compact 档主版本栏（缺陷 2）**。
///
/// 修前（M123 取证；本文件作者在 736×760 那张图上逐项量过）：主版本栏宽 64，`ListItem`
/// 两侧内边距各 16 ⇒ 文本格 **32**（x=216..247，裁切边 248）：
///
/// | 标签 | 墨迹 | 结论 |
/// |---|---|---|
/// | `1.21` | 26px | 完整（余 6） |
/// | `1.20` | 28px | 完整（余 4） |
/// | `24w` | 29px | 完整（余 3，最紧） |
/// | `All versions` | 画到 x=247（裁切边 248） | **被切**：只剩「All v」 |
/// | `全部版本` | 4 全角字 = 64 > 32 | **被切**：只剩「全部」且顶到边 |
///
/// 即桶标签的余量是 **3~6px**，而「全部版本」这条根本放不下。
///
/// 修后：图标条档的「全部版本」**不画文字**（`rails::major_shape` 的 `Icon` 分支）——
/// 画成文档/清单字形（40×40 `Tonal` 药丸 + 全称 tooltip）。**不是**第一级那个网格：
/// 两条 64 栏相邻，同字形 + 同药丸会让「全部类型」与「全部版本」像素级同形（r1 复审 P3-1）。
///
/// 抓得住：①首项是 **40×40 的图标药丸**（不是 64 宽 × 44 高的通栏文字行）——谁把它改回
/// 「画文字」（全称或短标签都会是通栏 44 行）这条立刻红；②药丸在 64 栏里**居中**（左右各 12）。
/// 抓不住：图标字形对不对、tooltip 长什么样（静态图拍不到 tooltip；字形要看图）。
///
/// ## 为什么不用「文字墨迹右边界 ≤ 栏右内边界」这条（复审建议的那条）
///
/// 它**在坏图上也是绿的**：修前那张图实测首项文字墨迹 x=216..247，而栏右内边界是
/// `200 + 64 − 16 = 248` ⇒ 247 ≤ 248 成立（只剩 1px 余量）——「All **v**」明明被裁掉了，
/// 断言却过。这条比 r1 报告里写的更极端：差 1px 而不是 4px。
/// 原因是 `ListItem` 的 `clip(true)` 让墨迹**永远**停在裁切边之前，边界的比较量不出「被裁」。
/// 所以这里改钉**形状**：文字行是 64×44 的通栏带，图标按钮是 40×40 的居中药丸，两者像素可分。
///
/// **中文为什么没有同档图**：本仓的截图通道 `pages::demo_model` 把 `Model::new(..)` 的
/// `lang` 写死成 `Lang::En`（`pages/model.rs:120`），`ULL_LANG` 只作用到 app 的 `App`，
/// 拍不出中文页面 —— 所以中文的结论走「数据面断言 + 算术」：
/// `tests/rails.rs` 的 `only_the_first_major_entry_switches_to_an_icon_in_the_icon_rail`
/// （中文与英文走**同一条**图标分支，键判据 `entry.key.is_none()` 与语言无关）与
/// `the_icon_rail_text_box_is_32_and_only_bucket_labels_fit`（「全部版本」= 64 > 32）。
#[test]
fn a_compact_tier_shot_draws_an_icon_for_the_first_major_entry() {
    let Some(path) = feed(COMPACT_SHOT) else {
        return;
    };
    let shot = Shot::load(&path);
    assert_eq!(shot.width, 736, "Compact 档那张必须是 736 宽");

    let bands = bands(736.0);
    let ui = ui();
    let rail_left = bands.major_rail.0;
    let rail_right = rail_left + bands.major_rail.1 as usize;
    let center = rail_left + bands.major_rail.1 as usize / 2;
    // 栏的底色（`MaterialLayer::Card`）：取栏内空白处，不取窗口画布
    let card = shot.at(rail_left + 2, 500);

    // ① 首项的竖向量：40（图标按钮的边长）——文字行会是 44
    let (top, height) = shot
        .first_filled_band(center, 156, 300, card)
        .expect("主版本栏里应当有第一项");
    assert_eq!(
        height as f32,
        ui.metrics().control_height,
        "Compact 档主版本栏首项应当是 40 高的图标按钮（y={top}），实测 {height}px —— \
         回到「ListItem 文字行」就是 44"
    );

    // ② 横向：40 宽的药丸，在 64 栏里居中（左右各 12）
    let (start, len) = shot.fill_run(top + height / 2, rail_left, rail_right, center, card);
    assert_eq!(
        (start, len as f32),
        (rail_left + 12, ui.metrics().control_height),
        "首项应当是 40 宽的居中药丸（栏 {rail_left}..{rail_right}），实测 x={start} 宽 {len} —— \
         64 宽的通栏行说明它又画成文字了"
    );
}
