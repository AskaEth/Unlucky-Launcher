//! **M374 · 导入区落在哪**：落点判据（`stack::pack_place`）、五态覆盖、接线。
//!
//! 从 `m374.rs` 拆出来（同一条 mission，两边都别撞 400 行）：数在 `m374.rs`（视口算式 / 反例 /
//! 限高的数学反驳），**位置**在这里。
//!
//! 这一组是复审 P1-1 的返工：r1 只测了 `Ready`，于是「`Split` + 非 `Ready` 时那块区两头都不在」
//! 这个回归没被抓住 —— 搜索框每敲一下导入区就闪掉，0 结果 / 离线页上一直不在（离线页上它恰好是
//! 唯一还能用的动作：拖本地 `.mrpack` / 粘路径）。

use unlucky_i18n::Lang;
use unlucky_presenter::dto::ContentKind;

use super::super::super::layout::{pack_before_results, BodyMode};
use super::super::super::parts;
use super::super::super::stack::{pack_place, PackPlace};
use super::super::demo;
use super::super::m361::ui;
use super::super::Status;

/// **排法判据**：导入区在滚动区里的位置只由「它是不是纯入口」定，与档位无关（M358 的规则）。
#[test]
fn the_import_block_leads_only_while_the_pack_is_busy() {
    let mut model = demo(ContentKind::Modpack, Lang::En);
    assert!(model.pack.is_entry_only(), "演示夹具是纯入口");
    // 纯入口 ⇒ 排到结果之后（M374：`Split` 档也是 —— M358 那时只改过 `ScrollWhole` 那一档）
    assert!(!pack_before_results(true, model.pack.is_entry_only()));

    // 进行中 / 待决策 / 有结果 ⇒ 排在结果之前（那是用户此刻要做的事）
    model.pack.export.open = true;
    assert!(!model.pack.is_entry_only());
    assert!(pack_before_results(true, model.pack.is_entry_only()));

    // 没有这块区的内容类型恒不在顶部
    assert!(!pack_before_results(false, true));
    assert!(!pack_before_results(false, false));
}

/// **落点判据的整张表**（纯函数，四个入参都是值 —— `tests/m374.rs` r1 的缺口就在这里）：
/// 上一版只测了 `Ready`，于是「`Split` + 非 `Ready` 时那块区两头都不在」这个 P1 没被抓到。
///
/// 复审建议的形状 `pack_above_head(is_modpack, entry_only, mode, ready) -> bool` 在本文件里就是
/// 下面这个局部函数（生产侧用的是 [`PackPlace`] 那张表，`is_above_head` 是它的投影）。
#[test]
fn the_import_block_has_a_place_in_every_status() {
    fn pack_above_head(has_pack: bool, entry_only: bool, mode: BodyMode, ready: bool) -> bool {
        pack_place(has_pack, entry_only, mode, ready).is_above_head()
    }

    // **这一格是 r1 的 P1**：`Split` + 列表没画 ⇒ 必须退回列头之上（否则整块不画）
    assert!(
        pack_above_head(true, true, BodyMode::Split, false),
        "`Split` 档列表没画时，导入区必须退回列头之上（整块消失就是这里漏的）"
    );
    assert_eq!(
        pack_place(true, true, BodyMode::Split, false),
        PackPlace::AboveHead
    );
    assert_eq!(
        pack_place(true, false, BodyMode::Split, false),
        PackPlace::AboveHead
    );

    // Ready：交给列表（本条修好的那一格；`Ready` + `Split` 绝不许是 `AboveHead`）
    assert_eq!(
        pack_place(true, true, BodyMode::Split, true),
        PackPlace::ScrollTail
    );
    assert_eq!(
        pack_place(true, false, BodyMode::Split, true),
        PackPlace::ScrollLead
    );
    assert!(!pack_above_head(true, true, BodyMode::Split, true));

    // `ScrollWhole`（矮窗）：M358 的两档一格都没动
    assert_eq!(
        pack_place(true, true, BodyMode::ScrollWhole, false),
        PackPlace::BelowBody
    );
    assert_eq!(
        pack_place(true, false, BodyMode::ScrollWhole, false),
        PackPlace::AboveHead
    );

    // 没有这块区的内容类型：恒 `Absent`（四条内容里只有整合包有它）
    for mode in [BodyMode::Split, BodyMode::ScrollWhole] {
        for ready in [true, false] {
            assert_eq!(pack_place(false, true, mode, ready), PackPlace::Absent);
            assert_eq!(pack_place(false, false, mode, ready), PackPlace::Absent);
        }
    }
}

/// **四态各自都要能证明「导入区存在」**：用真的 `demo()` 模型 + 真的 `status_body` 判据，
/// 不是「我读了源码觉得它在」。
///
/// `list_drawn` 的判据必须与 `view.rs` 完全一致（`status_body(..).is_none()`），所以这里真调用它：
/// 要是哪天 `status_body` 多给一个态返回 `None`，这条会立刻红（`pack_place` 会把区交给一个
/// 根本没画的列表 ⇒ 又回到 r1 的 P1）。
#[test]
fn every_status_keeps_the_import_block_for_modpacks() {
    let ui = ui();
    let catalog = Lang::En.catalog();

    for status in [
        Status::Ready,
        Status::Loading,
        Status::Empty,
        Status::Failed("network down".to_string()),
        Status::Offline("connect timed out".to_string()),
    ] {
        let mut model = demo(ContentKind::Modpack, Lang::En);
        model.status = status.clone();
        let ready = parts::status_body(&ui, catalog, &model).is_none();
        assert_eq!(
            ready,
            status == Status::Ready,
            "{status:?} 的「列表在画吗」判据不对"
        );

        let place = pack_place(true, model.pack.is_entry_only(), BodyMode::Split, ready);
        assert_ne!(place, PackPlace::Absent, "{status:?} 态下导入区整块消失");
        // `Split` 档：只有 `Ready` 交给列表，其余一律回到列头之上
        assert_eq!(
            place == PackPlace::AboveHead,
            !ready,
            "{status:?} 态的落点不对（{place:?}）"
        );
    }
}

/// **接线**：主列必须真的按落点表画，别只把纯函数写出来没人用。
///
/// `Element` 不可内省，所以钉源码：落点只由 `stack::pack_place` 一次算出，`Ready` 的两格走滚动区
/// （`ScrollLead` / `ScrollTail`），列头之上的那一格走 `is_above_head()`；`parts.rs` 把三样块拼进
/// **同一个** `scrollable`。
#[test]
fn the_main_column_routes_the_import_block_into_the_list_scroll() {
    let view = include_str!("../../view.rs");
    for needle in [
        "stack::pack_place(is_modpack, model.pack.is_entry_only(), mode, ready)",
        "let ready = status.is_none();",
        "parts::pack_block(ui, catalog, model)",
        "if place.is_above_head()",
        "stack::PackPlace::BelowBody",
    ] {
        assert!(
            view.contains(needle),
            "`pages/content/view.rs` 里少了 `{needle}`：导入区的落点有一处没接线"
        );
    }
    assert!(
        view.contains("stack::PackPlace::ScrollLead")
            && view.contains("stack::PackPlace::ScrollTail"),
        "两种滚动区落点都必须画出来（少一个就等于某种状态下整块消失）"
    );

    let parts = include_str!("../../parts.rs");
    assert!(
        parts.contains("pub(super) fn pack_block"),
        "导入区在滚动区里的那一块（分隔线 + 区本身）必须只此一份"
    );
    // 复审 P2-2：滚动条盖在内容上，那块区必须把 `SCROLLBAR_WIDTH + SCROLLBAR_MARGIN` 让出来
    assert!(
        parts.contains("frame::SCROLLBAR_WIDTH + frame::SCROLLBAR_MARGIN"),
        "导入区右边必须给滚动条留出那 8px（不然进度行的右对齐标签被压住一个字）"
    );
}
