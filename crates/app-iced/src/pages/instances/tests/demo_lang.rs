//! 演示路径的**元信息卡标签**必须跟着语言走（M355 P2-1 的判据）。
//!
//! 起因：M354 的出图取证发现中文档里六张卡恒 `VERSION / LOADER / MEMORY / JAVA / MODS /
//! LAST PLAYED`。根因是 `instances/demo.rs` 的 `catalog()` 写死 `Lang::En`，而译文本就在
//! `catalog/instances.rs` 里躺着（六条 `InstancesMeta*`）——纯属没接线。这跟 M353 修设置页是
//! **同一个形状**（夹具写死语言 ⇒ 中文档的图上混英文），当时的结论也适用：「出图通道的保真度
//! 也是证据的一部分」。
//!
//! | 用例 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`the_demo_cards_follow_the_language`] | `label()` 又写死一种语言（六格会与目录对不上 / 中英同值） | 目录里某条中文被改成英文以外的东西（那只归 `i18n.rs` 的全量对照） |
//! | [`the_harness_feeds_the_cards_the_model_language`] | `demo_model` 传了个别的语言（`Lang::En`），于是 `demo(lang)` 再对也没人喂 | 传的是不是 `model.lang` 的**副本**（`let lang = …` 与 `model.lang` 同一值，语义等价，不判） |
//! | [`the_demo_card_values_stay_fixture_text`] | 有人把夹具值也塞进目录/或反过来把标签塞进 `data.rs` 当数据 | 值本身该不该翻译（那是 i18n 的决定，见 `data.rs` 模块文档） |

use unlucky_i18n::catalog::MISSING;
use unlucky_i18n::{Key, Lang};

use crate::pages::tests::scan;

use super::super::demo;

/// 六张卡的 key，**按 `demo()` 装箱的顺序**（顺序也一起钉：换序会让同排卡底不齐的排版判据漂）。
const CARDS: [Key; 6] = [
    Key::InstancesMetaVersion,
    Key::InstancesMetaLoader,
    Key::InstancesMetaMemory,
    Key::InstancesMetaJava,
    Key::InstancesMetaMods,
    Key::InstancesMetaLastPlayed,
];

/// 演示模型里六张卡的标签（`detail.meta` 的 `label` 一栏）。
fn labels(lang: Lang) -> Vec<String> {
    let detail = demo(lang)
        .detail
        .expect("演示数据恒选中第一条，`detail` 不该是 None");
    detail.meta.iter().map(|cell| cell.label.clone()).collect()
}

/// 有没有汉字（`InstancesMetaJava` 是品牌名，那条单独放行）。
fn has_cjk(text: &str) -> bool {
    text.chars()
        .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch))
}

/// **中文档的六张卡标签是中文**（M355 的核心判据）。
///
/// 抓得住：`demo.rs` 的 `label()` 退回写死 `Lang::En`（六格全变成英文 ⇒ 中英同值，立刻红）；
/// 把某一格接错 key；把中文译文改成英文（`i18n.rs` 的全量对照也管这条，这里再从**出图模型**
/// 这一侧钉一次，因为它才是图上真正画出来的那份）。
///
/// 抓不住：真图上的字形（那要出图，见 `DevDocs/artifacts/instlang/`）。
#[test]
fn the_demo_cards_follow_the_language() {
    let zh = Lang::ZhCn.catalog();
    let en = Lang::En.catalog();
    let (zh_labels, en_labels) = (labels(Lang::ZhCn), labels(Lang::En));

    assert_eq!(
        zh_labels.len(),
        CARDS.len(),
        "演示详情是六张卡（版本/加载器/内存/Java/模组/上次游玩），装箱数变了要回来改这张表"
    );

    for (index, key) in CARDS.iter().enumerate() {
        let name = key.name();
        assert_eq!(
            zh_labels[index],
            zh.text(*key),
            "{name} 的中文档标签不是目录里那一条（M355 之前这里写死英文）"
        );
        assert_eq!(
            en_labels[index],
            en.text(*key),
            "{name} 的英文档标签不是目录里那一条"
        );
        assert_ne!(zh_labels[index], MISSING, "{name} 没有中文译文");
        assert_ne!(
            zh_labels[index], en_labels[index],
            "{name} 中英标签同值（{:?}）⇒ 中文档里这一格永远是英文：这正是 M354 报的 P2-1",
            zh_labels[index]
        );

        // 品牌名例外：`JAVA` → `Java`（`catalog/instances.rs` 的两条口径第 1 条：技术标识不翻译）。
        if *key != Key::InstancesMetaJava {
            assert!(
                has_cjk(&zh_labels[index]),
                "{name} 的中文标签里一个汉字都没有：{:?}",
                zh_labels[index]
            );
        }
    }

    // 顺带钉住 M354 图里那串英文字面量**不再出现**在中文档里（例外是 Java 那个品牌名）。
    for label in &zh_labels {
        assert!(
            !matches!(
                label.as_str(),
                "VERSION" | "LOADER" | "MEMORY" | "MODS" | "LAST PLAYED"
            ),
            "中文档里又出现了英文标签 {label:?}"
        );
    }
}

/// **出图通道真的把语言递进来了**（否则 `demo(lang)` 再对也没人喂）。
///
/// 为什么读源码：`demo_model` 在 `pages/demo.rs`（M357 r2 从 `pages/mod.rs` 搬出），
/// `instances` 这一族扫不到它；而「传参」
/// 这件事在模型层面**看不见**——`demo(Lang::ZhCn)` 自己会把标签翻好，坏的是上游递错了语言。
/// 所以只能断言那个调用点的形状（M353 的 `harness/tests.rs` 用的是同一手法）。
#[test]
fn the_harness_feeds_the_cards_the_model_language() {
    let page = scan::source_at("pages/demo.rs");

    // 只数**代码行**：`demo_model` 的注释里也写着这个调用（那是说明，不是接线）。
    let calls: Vec<&str> = page
        .text
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .filter(|line| line.contains("instances::demo("))
        .collect();

    assert_eq!(
        calls.len(),
        1,
        "`instances::demo` 只该有 `demo_model` 那一个调用点（多出来的那条要一起接语言）：{calls:?}"
    );
    assert!(
        calls[0].contains("instances::demo(lang)"),
        "`demo_model` 要把刚按 `ULL_DL_LANG` 定出来的 `lang` 递给实例页；\
         写死（`instances::demo(Lang::En)`）就是 M355 修掉的那个 bug 的入口"
    );
}

/// **夹具值仍然是夹具**：`data.rs` 里那六样是「用户数据长什么样」的样本，不是界面文案。
///
/// 这一条是给 M355 的改动划边界用的：`label` 现在跟语言走了，但 `value` 不跟——值域
/// （`1.20.4` / `4 GiB` / `148 mods` / `2 hours ago`）没有对应的 key，`data.rs` 的模块文档
/// 说明理由。写这条是防「顺手把值也搬进目录」与「反过来把标签塞回 `data.rs` 当数据」两种漂移。
#[test]
fn the_demo_card_values_stay_fixture_text() {
    let data = scan::source_at("pages/instances/data.rs");

    for key in CARDS {
        let needle = format!("Key::{}", key.name());
        assert!(
            !data.text.contains(&needle),
            "`data.rs` 里不该出现 {needle}：标签是界面文案，住在 `demo.rs` + 目录里"
        );
    }
    assert!(
        data.text.contains("2 hours ago") && data.text.contains("148 mods"),
        "夹具值应当还是那批英文样本（它们没有 key）；真搬进目录了就把这条删掉并说明"
    );
}
