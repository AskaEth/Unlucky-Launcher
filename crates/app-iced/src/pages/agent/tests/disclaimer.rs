//! 「这是模拟助手」的四处文案必须**一起**变（M119 r2 / 复审 P2-1 的验收标准）。
//!
//! ## 为什么这个文件存在
//!
//! 复审 r2 的 P2-1 抓得很准：`simulated` 是真字段，但 r1 只有**标题胶囊**一个消费者，另外三处
//! （副标题 / 输入框占位符 / 空态正文）是**无条件取词**，而它们的译文里写着「模拟 / 没接真实模型」。
//! 于是「接上真 transport 之后四处一起消失」只是一句声明——没有测试钉住它。
//!
//! 现在四处都从 [`Disclaimer`] 取词（`Disclaimer::of(catalog, simulated)`），本文件就是那句声明的
//! 证据：`simulated == true` 四处都提模拟；`simulated == false` 四处**都不提**。
//!
//! 抓得住：谁把某一处改回无条件取词（那就会出现「字段说真、界面说模拟」的反向假话）。
//! 抓不住：文案本身翻得好不好读（那要人看），也不管视图有没有真的用这几个字段
//! （那是 `tests.rs::the_spec_borrows_the_presenter_rail_verbatim` 与出图的活）。

use unlucky_i18n::{Catalog, Key, Lang};

use super::super::Disclaimer;

/// 这句话里有没有「模拟 / 没接真实模型」的意思（中英各一套写法）。
fn mentions_simulated(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("simulated") || lower.contains("no real model") || text.contains("模拟")
}

/// **四处一起出现**（`simulated == true`）：副标题 / 占位符 / 空态正文 / 标题胶囊。
#[test]
fn all_four_disclaimers_appear_when_the_backend_is_simulated() {
    for lang in [Lang::En, Lang::ZhCn] {
        let catalog = Catalog::of(lang);
        let note = Disclaimer::of(catalog, true);
        assert!(note.simulated, "{}：开关要传下去", lang.tag());
        for (place, text) in [
            ("副标题", note.subtitle),
            ("输入框占位符", note.placeholder),
            ("空态正文", note.empty_body),
            // 第四处是标题胶囊：只在 `simulated` 时渲染，文案来自这条 key
            ("标题胶囊", catalog.text(Key::AgentSimulated)),
        ] {
            assert!(
                mentions_simulated(text),
                "{} 的{place}没有说清「这是模拟」：{text:?}",
                lang.tag()
            );
        }
    }
}

/// **四处一起消失**（`simulated == false`）：三处文案都不许再提模拟，胶囊也不该被画。
///
/// 这条就是复审要的那一句验收：接上真 transport 那天，界面不会再反向地声明「没接真实模型」。
#[test]
fn no_disclaimer_survives_when_a_real_backend_is_wired() {
    for lang in [Lang::En, Lang::ZhCn] {
        let catalog = Catalog::of(lang);
        let note = Disclaimer::of(catalog, false);
        assert!(!note.simulated, "{}：开关要传下去", lang.tag());
        for (place, text) in [
            ("副标题", note.subtitle),
            ("输入框占位符", note.placeholder),
            ("空态正文", note.empty_body),
        ] {
            assert!(
                !mentions_simulated(text),
                "{} 的{place}在真实态还在说模拟：{text:?}",
                lang.tag()
            );
        }
        // 真实态的三句也得是**有内容的话**，不能靠空串糊过去
        for (place, text) in [
            ("副标题", note.subtitle),
            ("输入框占位符", note.placeholder),
            ("空态正文", note.empty_body),
        ] {
            assert!(!text.trim().is_empty(), "{} 的{place}是空的", lang.tag());
        }
    }
}

/// 两态必须是**两套不同的文案**（同一个 key 两边都用就退化成「永远说模拟」或「永远不说」）。
#[test]
fn the_two_states_use_different_keys() {
    let catalog = Catalog::of(Lang::En);
    assert_ne!(
        catalog.text(Key::AgentSubtitle),
        catalog.text(Key::AgentSubtitleReal)
    );
    assert_ne!(
        catalog.text(Key::AgentPlaceholder),
        catalog.text(Key::AgentPlaceholderReal)
    );
    assert_ne!(
        catalog.text(Key::AgentEmptyBody),
        catalog.text(Key::AgentEmptyBodyReal)
    );
}
