//! **M341**：预检屏的两段注记真的被画出来了（结构级证据，顺着真控件树读回）。
//!
//! 为什么模型层的断言不够：`model/tests/pick.rs` 只证明「模型把两档切对了」，证明不了
//! **`view/pick.rs` 把这两段 `push` 进了那一块**——删掉那两行 `push`，模型层用例一条都不红，
//! 而用户还是什么都看不见（这正是 M341 要修的那个洞）。所以这里照 M333 那套手法：段自己往
//! `Tree::state` 交一份 [`marker::SectionMark`]，用例顺着 `Tree::new(..)` 把「这一帧画了哪几段、
//! 各挂几条」读回来。
//!
//! | 用例 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`the_pick_screen_paints_both_note_sections`] | 两段都进了树、段名对不对、各挂几条（删掉任一 `push` 即红） | 段头那句**文案**、位置与像素（`DevDocs/artifacts/m341/` 的图守） |
//! | [`an_empty_notes_list_paints_no_section_at_all`] | `notes` 为空时两段**都不进树**（判据③的界面侧证据：空值不画标签） | —— |
//! | [`the_pick_fixture_carries_leveled_notes`] | 出图夹具真的带着**两档**注记（夹具丢了它，截图会静静退回"没有注记"那张） | 它画出来好不好看 |

use unlucky_i18n::Lang;

use super::marker;
use super::pack_in;

/// 判据①③的界面侧证据：`pick` 那一态画出两段，`pick_no_notes` 一段都不画。
///
/// **变异证明**：删掉 `view/pick.rs` 里 `block = block.push(marker::tagged(issue_section(..)))`
/// 任一行 ⇒ 那一段不再进树 ⇒ 对应的 `marks` 断言立刻红。
#[test]
fn the_pick_screen_paints_both_note_sections() {
    let marks = super::painted_sections("pick");
    let names: Vec<&str> = marks.iter().map(|mark| mark.mark.name).collect();
    assert_eq!(
        names,
        vec![marker::PICK_WARNINGS, marker::PICK_NOTES],
        "预检屏要画两段，且「要处理」排在「只是告知」前头：{marks:?}"
    );
    // 段头 + 每条一行 ⇒ 树里的直接子节点数 = 1 + 条数。
    assert_eq!(marks[0].mark.rows, 2, "夹具是两条告警");
    assert_eq!(
        marks[0].lines, 3,
        "两条告警 ⇒ 段头 + 两行，少一行就是某条没画"
    );
    assert_eq!(marks[1].mark.rows, 1, "夹具是一条注记");
    assert_eq!(marks[1].lines, 2, "一条注记 ⇒ 段头 + 一行");
}

/// 判据③（界面侧）：注记为空 ⇒ **两个段头都不出现**（连控件都不进树）。
///
/// 抓得住：`view/pick.rs` 把 `let Some(title) = title else { continue }` 去掉、改成无条件画段头
/// ⇒ 空态那张也会收到两段 ⇒ 本条红。空字符串标题那种半吊子画法同样会红（段还是进了树）。
///
/// **必须先断「这一屏真建出来了」**（r1 复审 p2-2）：`pack_in` 对不认识的状态名走 `_ => {}`，
/// 那时模型里 `pick` 是 `None`、树里也没有段 ⇒ 空表**天然成立**，判据③会变成恒真。
/// 所以先断 `pick.is_some()`，再用**同一个模型**去画（`painted_model`），两句话说的才是同一屏。
#[test]
fn an_empty_notes_list_paints_no_section_at_all() {
    let (model, _page) = pack_in("pick_no_notes", Lang::En);
    assert!(
        model.pick.is_some(),
        "这一态必须真画出冲突清单：`pick` 为 `None` 时「两个段头都不出现」是天然成立的，判据③就恒真了"
    );
    let marks = super::painted_model(&model);
    assert!(
        marks.is_empty(),
        "注记为空时树里一段都不许有（空段头是本仓反复收的那个毛病）：{marks:?}"
    );

    // 对照：同一份夹具带注记时是有段的——所以上面那条抓的是「空态没画」，不是「这里恒为空」。
    assert_eq!(super::painted_sections("pick").len(), 2);
}

/// 出图夹具（M341）：`pick` 这一态真的带着**两档**注记，段标题的计数来自级别而不是总条数。
///
/// 不再只断「非 idle」：夹具哪天把 `pick::notes()` 换成空表，`m341/` 那张图会静静变成
/// 「预检屏没有注记」，而上面两条用例里只有第二条会红（它对的是另一个夹具态）。
#[test]
fn the_pick_fixture_carries_leveled_notes() {
    let (model, _page) = pack_in("pick", Lang::ZhCn);
    let pick = model.pick.expect("pick 这一态要有冲突清单");
    assert_eq!(
        pick.warnings_title.as_deref(),
        Some("要处理：2 条"),
        "两段注记的计数各算各的：告警档 2 条（不是 3 条，也不是混装的 3 条）"
    );
    assert_eq!(
        pick.notes_title.as_deref(),
        Some("只是告知：1 条"),
        "注记档 1 条"
    );
    assert!(
        pick.warnings
            .iter()
            .all(|line| !line.contains("Modrinth 补全了")),
        "注记档那句不许混进「要处理」段：{:?}",
        pick.warnings
    );
}
