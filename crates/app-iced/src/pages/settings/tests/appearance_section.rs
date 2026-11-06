//! 「Appearance」分组自己的断言（M100 **新增**：`settings/tests.rs` 那 7 条全是跨 section 的，
//! 一条 section 断言都没有；这 3 条是新增，不是搬移）。
//!
//! **归 language+appearance 线**：这是设置页唯一接了真控件的一组，真控件实现全在
//! `pages/appearance/section.rs`；本文件只守「注册表这一侧的接线」。
//! 本组已经把 `body` 接上了；`tests/mod.rs` 那条「只有一组有真控件」的**聚合绊线已在 M101 r2 删除**
//! （被四条 per-group 断言完全蕴含——代价是「唯一性」从此没人守，那段说明写在 `tests/mod.rs` 同处）。

use super::super::appearance;
use unlucky_design::icons::IconKind;

/// 这一组的**演示行**（没有外观运行期状态时才画）。
///
/// 抓得住：四条线并行合并时把别人的行粘到本文件（四份 `GROUP` 长得几乎一样）；
/// 漏掉/多一条行；标签与值错位。
/// 抓不住：真控件那一屏（那是 `pages/appearance` 的事，靠出图判）。
#[test]
fn the_appearance_group_shows_density_corner_motion_and_text_scale() {
    let rows: Vec<(&str, &str)> = appearance::GROUP.rows.to_vec();
    assert_eq!(
        rows,
        vec![
            ("Density", "Comfortable"),
            ("Corner style", "Follow system"),
            ("Motion", "Full"),
            ("Text scale", "100%"),
        ]
    );
}

/// 左列图标（`IconKind::Sidebar`）。
#[test]
fn the_appearance_group_keeps_its_icon() {
    assert_eq!(appearance::GROUP.icon, IconKind::Sidebar);
}

/// **本组的绊线**：外观分组必须一直是「挂了真控件」的那一组。
///
/// 抓得住：language+appearance 线把 `body` 换回 `None`（真控件消失、退回演示行，而截图对这种
/// 「只在有外观状态时出现」的差别不一定抓得住）。
/// 抓不住：`body` 到底指向哪个渲染器——**函数指针相等性断言不了**（rustc
/// `unpredictable_function_pointer_comparisons`：fn 地址不保证唯一，比出来的结果没有意义），
/// 所以「指向 `pages::appearance::section`」这件事只能由
/// `settings/appearance.rs` 那一行源码 + 两张图的 sha256 不同来担保。
#[test]
fn the_appearance_group_keeps_a_real_control_body() {
    assert!(
        appearance::GROUP.body.is_some(),
        "外观分组必须是挂了真控件的那一组（`settings/appearance.rs` 里是 `body: Some(body)`，\
         `body` 是本文件那个本地 fn；具体渲染器由那一行源码 + 截图担保，函数指针相等性断言不了）"
    );
}
