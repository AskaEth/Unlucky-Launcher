//! 内容库页的守门（M109）：演示数据、四类内容、页面自留状态的默认值。
//!
//! 视图模型的判定在 `model/tests.rs`，栏几何在 `layout.rs`，这里只管「演示数据真的装得满」——
//! 出图靠它，装不满就说明 `demo()` 与 `Content` 的字段对不上。

use unlucky_i18n::Lang;
use unlucky_presenter::dto::ContentKind;

use super::model::{PageState, Status};
use super::{demo, Content};

#[test]
fn the_page_state_starts_on_mods_with_nothing_selected() {
    let page = PageState::default();
    assert_eq!(page.kind, ContentKind::Mod);
    assert!(page.selected.is_none());
    assert!(page.target.is_none());
    assert!(page.draft.is_empty());
}

#[test]
fn every_kind_gets_a_full_demo_screen() {
    // 四种内容各出一屏（状态矩阵的前 8 张靠这一条）
    for kind in ContentKind::ALL {
        let model: Content = demo(kind, Lang::En);
        assert_eq!(model.kind, kind, "{} 的演示数据换了类型", kind.slug());
        assert_eq!(model.status, Status::Ready, "{}", kind.slug());
        assert_eq!(model.hits.len(), 4, "{} 的命中项不足", kind.slug());
        assert!(
            model.selected.is_some(),
            "演示数据要选中一条（详情面板才有内容）"
        );
        assert!(model.total > 0);
        assert!(model.has_more, "演示数据要能画「加载更多」");
        // 整合包**不**在这一条里断（它天生不可装：`ContentKind::Modpack.dir()` 是 `None`），
        // 由下一条用例专钉。
        if kind != ContentKind::Modpack {
            assert!(
                model.can_install(),
                "默认那一屏应当是「能装」的（{}）",
                kind.slug()
            );
        }
    }
}

#[test]
fn the_modpack_screen_is_browsable_but_never_installable() {
    // 整合包：检索能看，装入一律禁用（M111 未落地）——演示图上必须看得到原因
    let model = demo(ContentKind::Modpack, Lang::En);
    assert!(model.selected.is_some());
    assert!(!model.can_install());
    assert_eq!(
        model.blocker_reasons(Lang::En.catalog()).len(),
        1,
        "整合包只有一条原因（导入管线未落地）"
    );
}

#[test]
fn the_installable_kinds_block_only_when_the_target_disagrees() {
    for kind in [
        ContentKind::Mod,
        ContentKind::Resourcepack,
        ContentKind::Shader,
    ] {
        let model = demo(kind, Lang::En);
        assert!(
            model.blocker_reasons(Lang::En.catalog()).is_empty(),
            "{} 的默认演示项应当可装",
            kind.slug()
        );
    }
}

#[test]
fn the_demo_selection_is_always_one_of_the_hits() {
    for kind in ContentKind::ALL {
        let model = demo(kind, Lang::En);
        let selected = model.selected.clone().expect("有选中项");
        assert!(
            model.hits.iter().any(|hit| hit.id == selected),
            "{} 的选中项不在命中列表里（详情面板会是空的）",
            kind.slug()
        );
    }
}
