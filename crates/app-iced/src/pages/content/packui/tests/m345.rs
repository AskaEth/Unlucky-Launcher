//! **M345**：「注记多到要收口」那一态是**常驻**夹具，且「收口确实发生了」有可红的结构判据。
//!
//! 为什么要有这一态：M341 的「>3 条收口」只靠一张临时夹具图
//! （`DevDocs/artifacts/m341/NOT-A-STATE-pick-20-notes-temp-fixture-*.png`），拍完就丢 ⇒ 之后改这一屏
//! 的收口与行高都没有可比对的常驻像素。现在它是 `ULL_CONTENT_PACK=pick_many`，出图清单在
//! `DevDocs/artifacts/pickshots/`（`pick-many-zh-light/dark-1180x900.png`）。
//!
//! | 用例 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`the_many_notes_state_crosses_the_fold_threshold`] | 夹具真的带着 13 条注记、两档混合、**总数越过收口线**（`NOTES_ROWS_THRESHOLD`） | 屏上画到第几条、第二段段头有没有露出来（图守）；**段标题里的计数不是收口证据**——那是 `model/report_issue.rs` 按档计数渲染的，`pick` 态 3 条不收口也有计数 |
//! | [`the_many_notes_state_paints_both_sections`] | 两段真进了控件树、各挂 4 / 9 条（删掉任一 `push` 或夹具掉一条即红） | 段头文案、位置与像素 |
//! | [`the_many_notes_state_differs_from_the_plain_pick_state`] | **两侧各算一次**：`pick_many` 的总数越过收口线、`pick` 的总数不越过 ⇒ 夹具退回 `pick::notes()` 时本条**真的红** | 好看不好看 |
//!
//! ## 收口这条判据能红到什么程度
//!
//! 屏上**唯一**能证明「收口发生了」的是「只画前 N 条 + 右侧出现滚条」（常驻图里只画到 4 条告警
//! 与第二段的段头，9 条注记的正文一行都没进视口），不是段标题的计数。
//!
//! **能红**（三条都实测过）：
//! - `NOTES_ROWS_THRESHOLD` → `usize::MAX`：**①③红、②绿**——② 只看 marker 的 `rows` / `lines`，
//!   与「收不收口」这个开关无关，所以它本来就不该在这里红；常驻图与「不收口」那张差三万像素级
//!   （口径与数值见 `DevDocs/artifacts/pickshots/MEASURE-BANDS.md`）；
//! - 夹具的 `notes_many()` 换回 `pick::notes()`（3 条）：**①②③全红**（③是 r2 按复审修的那条：
//!   它现在两侧都取）；
//! - 阈值 → 2：③红（它非恒真）。
//!
//! 这三条都只钉**数据/结构**；「屏上有没有切到字」只有像素能判——**M350 起常驻图在这一项上是
//! 通过的**，判据是像素脚本 `DevDocs/artifacts/pickrow/check_notes_tail.py`（折线由脚本自推：
//! 动作行按钮带顶 − 12；余量 = 折叠行前还剩几行空白，实测中文 2 行、英文 1 行）。
//! 反例：把 cap 压回 172 ⇒ 红（折叠线穿进第二段段头的行框，被切 6 行 / 192 个墨迹像素）。
//! 它**不是**用例：像素要 PNG，而单测里没有渲染器；把它写成 `assert!(true)` 那种结构断言只会骗人。
//! （M345 那一版常驻图在这一项上是**失败的**——折线 648 落在段头行框 632..652 内部。）
//!
//! **抓不住**：收口好不好看、配色对不对、滚条对比度——只有看图能判。

use unlucky_i18n::Lang;

use super::super::model::NOTES_ROWS_THRESHOLD;
use super::{marker, pack_in, painted_sections};

/// 判据①：`pick_many` 这一态的**夹具**真的越过收口线，且两档注记都非空。
///
/// 抓得住（变异）：夹具掉到 3 条以内、或阈值抬到 13 条以上 ⇒ 本条红——那时屏上不再收口，
/// 常驻图与「注记全铺开」那一张就分不出来了。
/// 抓不住：**屏上**实际画到第几条、第二段段头有没有落在视口里（图守）。段标题的计数**不能**
/// 当收口证据（`model/report_issue.rs` 只要该档非空就渲染计数，与收口无关），所以本条不拿它当判据。
#[test]
fn the_many_notes_state_crosses_the_fold_threshold() {
    let (model, _page) = pack_in("pick_many", Lang::ZhCn);
    let pick = model.pick.expect("pick_many 这一态要有冲突清单");
    let total = pick.warnings.len() + pick.notes.len();
    assert_eq!(total, 13, "夹具是 13 条注记（任务书要求 ≥ 6）");
    assert!(
        pick.warnings.len() == 4 && pick.notes.len() == 9,
        "两档要混合：{:?} / {:?}",
        pick.warnings,
        pick.notes
    );
    assert!(
        total > NOTES_ROWS_THRESHOLD,
        "总数必须越过收口线（阈值 {NOTES_ROWS_THRESHOLD}）：不然这一屏铺开画，常驻图就不是「收口」那张"
    );
}

/// 判据②：两段注记**真的画出来了**，各挂 4 / 9 条（段头 + 4 行 = 5 行，段头 + 9 行 = 10 行）。
///
/// 走的是产品路径 `view::section`（不是单独调某一段），与 `tests/m341.rs` 同一套走法。
#[test]
fn the_many_notes_state_paints_both_sections() {
    let marks = painted_sections("pick_many");
    let names: Vec<&str> = marks.iter().map(|mark| mark.mark.name).collect();
    assert_eq!(
        names,
        vec![marker::PICK_WARNINGS, marker::PICK_NOTES],
        "两段都要进树，且「要处理」排在「只是告知」前头：{marks:?}"
    );
    assert_eq!(marks[0].mark.rows, 4, "告警档 4 条");
    assert_eq!(marks[0].lines, 5, "4 条 ⇒ 段头 + 4 行");
    assert_eq!(marks[1].mark.rows, 9, "注记档 9 条");
    assert_eq!(marks[1].lines, 10, "9 条 ⇒ 段头 + 9 行");
}

/// 判据③：**两侧各算一次**——`pick_many` 越过收口线、`pick` 不越过。
///
/// r1 复审 P2-2：上一条版本的③只 `pack_in("pick")`，根本不碰 `pick_many`，所以「夹具退回 3 条」
/// 时它照样绿（实测：①②红、③绿），注释却是「本条先红」——那是假话。现在两侧都取：
///
/// 抓得住（变异）：夹具退回 `pick::notes()`（3 条）⇒ `pick_many` 的总数掉到收口线以内 ⇒ 本条红。
/// 抓不住：`pick` 那一态自己的版式（那是 `m341` 与 `pick-notes-*.png` 的事）。
#[test]
fn the_many_notes_state_differs_from_the_plain_pick_state() {
    let (many, _) = pack_in("pick_many", Lang::ZhCn);
    let many = many.pick.expect("pick_many 这一态要有冲突清单");
    let many_total = many.warnings.len() + many.notes.len();

    let (plain, _) = pack_in("pick", Lang::ZhCn);
    let plain = plain.pick.expect("pick 这一态要有冲突清单");
    let plain_total = plain.warnings.len() + plain.notes.len();

    assert!(
        many_total > NOTES_ROWS_THRESHOLD,
        "`pick_many` 必须越过收口线（阈值 {NOTES_ROWS_THRESHOLD}，实际 {many_total}）：这一条是「夹具退回 3 条」的守门"
    );
    assert!(
        plain_total <= NOTES_ROWS_THRESHOLD,
        "`pick` 必须落在收口线**之内**（阈值 {NOTES_ROWS_THRESHOLD}，实际 {plain_total}）：它那张图要证明「三条以内铺开」"
    );
    assert_eq!(plain_total, 3, "`pick` 那一态是 3 条（M341 的基线）");
    assert!(
        many_total > plain_total,
        "两态必须真的不同（{many_total} vs {plain_total}）"
    );
}
