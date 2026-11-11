//! Compact 档（可用宽 600~675）的像素守门：M125 缺陷 2（主版本栏）+ M238（面板首屏 / 阴影越界）。
//!
//! M238 两条都钉在**同一张 736×760 图**上（`04-…-compact736.png`）：
//! ①六枚 loader chip 整枚在屏（修前被面板预算挤得只剩 10px 高的上缘）；
//! ②版本菜单的 L2 阴影不许漏出滚动视口、压暗下面钉死区的实例名（修前浅色画面布 `(249,249,250)`
//! 被压成 `(231,231,232)`，−7.2%）。

use unlucky_design::color::Role;

use super::{bands, feed, ui, Shot, COMPACT_SHOT};

/// iced 的 `Color` → 截图里的 8 位 RGB（tiny-skia 侧就是 `round(分量 × 255)`，截图不带 α）。
fn rgb(color: iced::Color) -> [u8; 3] {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    [channel(color.r), channel(color.g), channel(color.b)]
}

/// `y` 这一行里有多少段（每段 ≥ [`PILL_RUN_MIN`]）与 `color` 同色的连续像素。
///
/// 用来区分「一排药丸的描边」与「一条通栏分隔线」：两者可能是同一个颜色（分隔线也用
/// `OutlineVariant`），但药丸是**多段**、分隔线是**一段通到底**。
fn pill_runs(shot: &Shot, y: usize, left: usize, right: usize, color: [u8; 3]) -> usize {
    let (mut runs, mut run) = (0, 0);
    for x in left..right {
        if shot.at(x, y) == color {
            run += 1;
        } else {
            if run >= PILL_RUN_MIN {
                runs += 1;
            }
            run = 0;
        }
    }
    if run >= PILL_RUN_MIN {
        runs += 1;
    }
    runs
}

/// 药丸描边那一段的最小长度：32 高的圆角药丸（半径 16）在顶边/底边留下的直线段约 28~68px，
/// 取 16 足以跨过圆角收缩，又能甩掉侧边那 1px 竖线。
const PILL_RUN_MIN: usize = 16;

/// `y` 这一行在 `[left, right)` 里是否**整行同色**；是则给出那个颜色。
fn uniform_row(shot: &Shot, y: usize, left: usize, right: usize) -> Option<[u8; 3]> {
    let color = shot.at(left, y);
    (left..right)
        .all(|x| shot.at(x, y) == color)
        .then_some(color)
}

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

/// **M238 ①**：736×760 下六枚 loader chip **整枚**在面板滚动视口里（不是一条 10px 的缝）。
///
/// 修前实测（M238，`before-736x760-*.png`）：面板拿 286px、滚动视口只剩 **54px**
/// （head 40 + 间距 4 之后轮到 chips 只剩 10px），六枚 chip 的下边框一个像素都不在屏；
/// 修后（预算按面板内容推导，`rows/tail.rs::panel_budget`）：视口 **98px**，chip 描边
/// 上下各一条（y=462 / y=493，高 31px）。
///
/// 抓得住：面板预算退回「死的 300」（视口 54 ⇒ 下边框消失，只剩上边框那一条描边行）、
/// 或 chip 被任何东西裁掉下缘（判据是「**药丸描边**行至少有两条、且相距 ≥ 28」——圆角药丸的
/// 上/下边框各是一排 ≥16px 的多段同色像素，裁掉任何一条都当场红；分隔线虽然同色但是**通栏
/// 一段**，用段数把两者分开）。
/// 抓不住：chip 的文案有没有被**横向**裁掉（要看字形宽度）、六枚到底是不是一行（那是
/// `rails.rs` 的单测）、hover/焦点态。
#[test]
fn a_compact_tier_shot_draws_the_whole_chip_row() {
    let Some(path) = feed(COMPACT_SHOT) else {
        return;
    };
    let shot = Shot::load(&path);
    let bands = bands(736.0);
    let ui = ui();
    let left = bands.list.0 + 8;
    let right = bands.list.0 + bands.list.1 as usize - 8;

    // 未选中的 chip 有 1px `OutlineVariant` 描边（选中项无描边，见 `selection/paint/chip.rs`）
    let edge = rgb(ui.color(Role::OutlineVariant));
    let strokes: Vec<usize> = (400..560)
        .filter(|y| pill_runs(&shot, *y, left, right, edge) >= 2)
        .collect();
    assert!(
        strokes.len() >= 2,
        "chip 的描边行只有 {} 条（{strokes:?}）：六枚 chip 被面板视口裁成一条缝 \
         —— 下边框一个像素都不在屏，用户看不出「下面还有加载器 chip」",
        strokes.len()
    );
    let (top, bottom) = (strokes[0], strokes[strokes.len() - 1]);
    assert!(
        bottom - top >= 28,
        "chip 的上下描边相距只有 {}px（y={top}..{bottom}）：chip 本体 32 高，\
         被裁的话两个描边行会挨在一起",
        bottom - top
    );
}

/// **M238 ②**：版本菜单的 L2 阴影不许画到滚动视口之外（压暗下面钉死区的实例名）。
///
/// 修前实测（浅色）：菜单容器 `(238,238,239)` 底下那条 8px 缝、以及实例名框内的空白带
/// 全被压成 `(231,231,232)`（−7.2%，与 M222/M227 视觉审记的「实例名被压暗 6.9%」同一处），
/// 深色画面布 `(15,21,15)` 被压成 `(14,20,14)`。压暗带的**长度跟布局有关**：修前的旧预算下
/// 是菜单底往下 54px 一整条；只把预算修好、阴影留着时缩到 8px（= blur 6 + offset 2 那圈）。
/// 所以两条修法各管一半，判据按「缝 + 框内都必须纯画布色」写，两种布局都抓得住。
/// 根因在 `iced_tiny_skia` 0.14：quad 阴影用 `draw_pixmap(.., mask: None)` 画
/// （`engine.rs:136-143`），绕过裁切掩码；`iced_wgpu` 那条走整层 scissor（`quad.rs:96-101`），
/// 于是同一棵树在 CPU / GPU 上画出来不一样。修法 = 不在这条滚动链里画阴影（`Menu::flat`）。
///
/// 抓得住：`Menu::flat` 被撤（阴影回来 ⇒ 缝与框内变成混合色）、α 被调大调小（只要非 0 就红）、
/// 或者有人把菜单挪出滚动区又不裁（`uniform_row` 的定位仍会命中实例名框）。
/// 抓不住：**滚动中**阴影会不会跟出来——静态图只有一帧，`pages::harness` 目前没有
/// `scroll_to` 的派发通道（`rows.rs::PANEL_SCROLL_ID` 的注释是「拿到了就能派」，实际没人写），
/// 所以「滚到中部/底部」这三态在本 mission 无法出图，只能靠「阴影根本不画」从机制上排除。
#[test]
fn a_compact_tier_shot_keeps_the_menu_shadow_inside_the_scroll_viewport() {
    let Some(path) = feed(COMPACT_SHOT) else {
        return;
    };
    let shot = Shot::load(&path);
    let bands = bands(736.0);
    let left = bands.list.0 + 8;
    let right = bands.list.0 + bands.list.1 as usize - 8;
    let canvas = shot.at(bands.list.0 + 2, 12);

    // 实例名框的两条通栏 1px 描边：整行同色，且「上 3 行」与「下 3 行」互为同色、又与它自己不同
    // ——通栏细线才满足这个形状；菜单块的边界不满足（块内 3 行是选中项底色）。
    let hairline = |y: usize| -> bool {
        uniform_row(&shot, y, left, right).is_some_and(|color| {
            matches!(
                (uniform_row(&shot, y - 3, left, right), uniform_row(&shot, y + 3, left, right)),
                (Some(above), Some(below)) if above == below && color != above
            )
        })
    };
    let hairlines: Vec<usize> = (shot.height / 2..shot.height - 16)
        .filter(|y| hairline(*y))
        .collect();
    assert!(
        hairlines.len() >= 2,
        "找不到实例名输入框的上下描边（命中 {hairlines:?}）：这条断言要靠它定位「钉死区」"
    );
    let field_top = hairlines[hairlines.len() - 2];
    let field_bottom = hairlines[hairlines.len() - 1];
    assert!(
        field_bottom > field_top + 40,
        "实例名框只有 {}px 高（y={field_top}..{field_bottom}）：它被挤扁了，先修那一处再看阴影",
        field_bottom - field_top + 1
    );

    // ① 框上沿往上 8px = 「滚动视口 → 钉死区」的缝：必须**纯画布色**
    let middle = (left + right) / 2;
    for y in field_top - 8..field_top {
        for x in [left + 8, middle, right - 16] {
            assert_eq!(
                shot.at(x, y),
                canvas,
                "y={y} x={x} 是 {:?} 而不是画布色 {canvas:?}：菜单的 L2 阴影漏出了滚动视口，\
                 压暗了视口下面那条 8px 的缝（M238 修前实测 −7.2%）",
                shot.at(x, y)
            );
        }
    }

    // ② 框内标签以下的空白带也不许被压暗（修前那里是 231 而不是 249）
    for y in field_top + 36..field_bottom - 8 {
        assert_eq!(
            shot.at(right - 24, y),
            canvas,
            "y={y} 的实例名框内是 {:?} 而不是画布色 {canvas:?}：阴影画到了框里面",
            shot.at(right - 24, y)
        );
    }
}
