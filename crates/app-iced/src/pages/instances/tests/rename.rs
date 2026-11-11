//! M252 / P18：行内重命名的**回车**接线。
//!
//! iced 的 `Element` 没有内省接口（见 `pages::layout::source` 的模块文档），所以「真按下回车会
//! 发生什么」在单测里量不到。这里能钉住的两件事：
//!
//! | 断言 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`enter_takes_the_same_message_as_the_rename_button`] | 「回车」与「点改名」共用的那条消息被换成别的（取消 / 重开…）；空名 / 原名不变 / 非法字符那几道校验因此**自动**对回车成立——它们挂在同一个 Message 后面 | 校验本身对不对（在 `app/update/instances.rs` 与 presenter，本 mission 一字未改） |
//! | [`the_rename_field_wires_enter`] | `rename_row` 里那枚输入框的 `.on_submit(..)` 被删掉 / 接成别的（T27 反例） | 回车键真的到达控件（那是 iced 的 `Widget::update`） |
//!
//! 也就是说：判据①「回车真的提交一次」里，**「提交的是什么」被钉住了，「按下去会不会到这儿」没有**
//! （理由与替代验证写在 M252 报告的「没验的部分」一节）。

use crate::pages::layout::source;

use super::super::saves::commit_rename;
use super::super::state::Message as PageMessage;

/// 回车与「改名」按钮共用同一个消息产地 ⇒ 走同一套校验。
///
/// `PageMessage` 只有 `Debug + Clone`（没有 `PartialEq`，那些枚举不在本 mission 的 scope），
/// 所以这里比 `Debug` 文本：枚举的 `Debug` 打印变体名与字段，变体不同必然文本不同。
///
/// 这条只钉「产地生产什么」；**「两条路都用了这个产地」由 [`the_rename_field_wires_enter`] 钉**
/// （r1 复审 P2-1：上一版把两件事写进这里，而实际覆盖只在文本型断言里）。
#[test]
fn enter_takes_the_same_message_as_the_rename_button() {
    assert_eq!(
        format!("{:?}", commit_rename()),
        format!("{:?}", PageMessage::CommitRenameWorld),
        "回车必须发「提交重命名」，不能是取消/重开/逐键输入"
    );
    assert!(
        !format!("{:?}", commit_rename()).contains("Cancel"),
        "回车不许接到取消上（那会让用户按回车丢掉草稿）"
    );
}

/// 回车与「改名」按钮**在源码里**都接 `commit_rename()`，且都没接第二种页面意图。
///
/// 判据是**代码行**（`source::code_line` 先剥注释），所以下面这些改法都会红：
/// - 把 `.on_submit(..)` 那一行删掉，或注释掉（`//` / `/*` 独占一行的块注释）；
/// - 把它接成别的页面意图（取消 / 重开 / 逐键输入）——按钮那一行同样不许；
/// - 让两条路走**不同**的产地（例如按钮仍写 `PageMessage::CommitRenameWorld` 字面量）：
///   那时按钮那行不含 `commit_rename()`，第二条断言红。
///
/// 抓不住：回车键真的到达控件（那是 iced 的 `Widget::update`，见模块文档）；同一行里写完的
/// 等价接线（本仓构建链一律一行一条）。
#[test]
fn the_rename_field_wires_enter() {
    let body = source::function_body(include_str!("../saves/rows.rs"), "fn rename_row");
    let enter = source::code_line(body, ".on_submit(").unwrap_or_else(|| {
        panic!("`rename_row` 里没有一行**代码**接 `on_submit`（注释掉也算没有）：\n{body}")
    });
    assert!(
        enter.contains("commit_rename()"),
        "回车必须接 `commit_rename()`（与「改名」按钮同一个产地）：{enter}"
    );

    let button = source::code_lines(body)
        .into_iter()
        .find(|line| line.contains("saves_rename_confirm()"))
        .unwrap_or_else(|| panic!("找不到「改名」按钮那一行：\n{body}"));
    assert!(
        button.contains("commit_rename()"),
        "「改名」按钮必须用同一个产地（RenameWorld 只有一处写法）：{button}"
    );

    // P2-1：两条路都只许这一个页面意图——接成别的（取消/重开/逐键输入）就是行为变了。
    for line in [&enter, &button] {
        for other in [
            "CancelRenameWorld",
            "StartRenameWorld",
            "RenameWorldChanged",
            "BackupWorld",
            "RequestDeleteWorld",
        ] {
            assert!(
                !line.contains(other),
                "回车/按钮接成了别的页面意图（{other}）：{line}"
            );
        }
    }
}
