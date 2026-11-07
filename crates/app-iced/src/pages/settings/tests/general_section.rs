//! 「General」分组自己的断言（M106 `I18NFOLLOW` 重写：语言行换成真控件）。
//!
//! **归 language+appearance 线**。接真控件那天要改**两个**文件（M101 r2 删掉了原先的第 3 处跨
//! section 绊线）：`settings/general.rs` + 本文件 `the_general_group_is_still_demo_rows`。
//!
//! M106（`I18NFOLLOW`）已经接了这一组的语言行。用户报的 bug 有两半：**界面全英文**（探测对了
//! 但没人用）与**设置页写着 `English (System)`**（一行硬编码假值）。本文件的断言守后半：那行
//! 假值不许回来，语言行的三档必须真的能在两套目录里取到词。
//!
//! 接线形状：语言行的正文由 [`general::body`] 画（入口是 `pages::settings` 的 `GENERAL_GROUP`
//! 按下标分发），`GROUP.body` 仍是 `None`——那个槽位的签名是 `fn(&Ui, States) -> Option<Element>`，
//! `States` 只有 `appearance` + `groups` 两格，装不下「当前语言档位」。所以 M101 r2 的
//! 「本组接真控件只需改两个文件」这条口径，在语言行上**仍然是两个文件**。

use super::super::general;
use unlucky_design::icons::IconKind;
use unlucky_i18n::LangChoice;
use unlucky_i18n::{Key, Lang};

/// 这一组的**静态演示行**：语言那一行已经换真控件，所以只剩三行。
///
/// 抓得住：①四条线并行合并时把别人的行粘到本文件；②语言行被「顺手」加回静态表——
/// 那等于把用户看到的假值又写回去（它的值必须跟着语言变，静态表做不到）。
/// 抓不住：真控件画出来长什么样（那是截图验收的事，见 `DevDocs/artifacts/m106/`）。
#[test]
fn the_general_group_keeps_theme_startup_and_close_as_static_rows() {
    let rows: Vec<(&str, &str)> = general::GROUP.rows.to_vec();
    assert_eq!(
        rows,
        vec![
            ("Theme", "Follow system"),
            ("Startup page", "Instances"),
            ("Close button", "Exit to tray"),
        ]
    );
}

/// 左列图标（`IconKind::Settings`）。分组图标是左列的语义标签，换掉就是换观感。
#[test]
fn the_general_group_keeps_its_icon() {
    assert_eq!(general::GROUP.icon, IconKind::Settings);
}

/// **回归绊线（用户原话）**：`"English (System)"` 那条硬编码字面量不许回来。
///
/// 抓得住：①原来那条「这一组还没接真控件」在接上之后没更新（静默变假绿）；
///         ②语言行**退回静态演示行**（哪怕换个说法，只要还写着「English」就是同一个 bug 的复盘）。
/// 抓不住：界面上真控件长什么样、三档选项名对不对（下一条与截图各管一段）。
///
/// 现状（M106）：`body` 仍是 `None`（语言行走 `GENERAL_GROUP` 的按下标分发），但这一组**已经**
/// 有真控件了——所以这里断言的**不再是「还没接」**，而是「不许回到静态行」。
#[test]
fn the_language_row_is_not_a_static_string() {
    for (label, value) in general::GROUP.rows {
        assert_ne!(
            *label, "Language",
            "语言行必须走真控件（pages::Language），不许回到静态演示行"
        );
        assert!(
            !value.contains("English"),
            "静态行里不许再出现写死的语言名：{value}"
        );
    }
}

/// 语言行**三档**，且在**两套目录**里都有词（中文界面不该出现 `⟪missing⟫`）。
///
/// 抓得住：①少一档（用户就没法切回去了）；②标签 key 只加进了中文表（英文界面上一档变成
/// `⟪missing⟫`）；③回显那句的占位符名字与调用点对不上（会原样印出 `{language}`）。
/// 抓不住：译文翻得对不对（金表 + `chinese_catalog_is_actually_chinese` 管那一层）。
#[test]
fn the_language_row_offers_three_choices_in_both_languages() {
    assert_eq!(
        LangChoice::ALL.len(),
        3,
        "跟随系统 / 简体中文 / 英文，少一档就切不回来"
    );

    for lang in Lang::ALL {
        let catalog = lang.catalog();
        for choice in LangChoice::ALL {
            assert!(
                catalog.get(choice.label_key()).is_some(),
                "{} 在 {} 目录里没词",
                choice.label_key().name(),
                lang.tag()
            );
        }

        // 「界面当前使用：⟨语言名⟩」——中文系统下这句必须真的出现「简体中文」。
        let effective = catalog.text(LangChoice::Fixed(*lang).label_key());
        let line = catalog.render(Key::SettingsLanguageEffective, &[("language", effective)]);
        assert!(line.contains(effective), "回显没带上语言名：{line}");
        assert!(!line.contains('{'), "占位符没被替换：{line}");
    }

    // 中文系统 + 跟随系统 ⇒ 回显是「简体中文」（用户报的 bug 的直接判据）。
    let zh = Lang::ZhCn.catalog();
    assert_eq!(
        zh.render(
            Key::SettingsLanguageEffective,
            &[(
                "language",
                zh.text(LangChoice::Fixed(Lang::ZhCn).label_key())
            )]
        ),
        "界面当前使用：简体中文"
    );
}

/// **绊线**：这一组仍不走 `Group.body`。
///
/// 为什么不是「接了真控件」：语言行的入口是 `pages::Language`（`GENERAL_GROUP` 那条分发），
/// 而 `Group.body` 那个槽位的签名是 `fn(&Ui, &pages::appearance::State)`——装不下「当前语言档位」。
/// 谁把语言行塞进 `body`，就得先给它换签名（那会改到另外三组的文件），这条断言逼他先想一遍。
#[test]
fn the_general_group_does_not_use_the_appearance_body_slot() {
    assert!(
        general::GROUP.body.is_none(),
        "General 的真控件走 pages::Language 入口（见 settings/mod.rs 的 GENERAL_GROUP），不是 body"
    );
}
