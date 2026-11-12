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
//! 按下标分发），`GROUP.body` 仍是 `None`——那个槽位现在带**生效语言**（`Lang`，M179）但仍是
//! `States` + 语言，装不下「当前语言档位」（`LangChoice`）。所以 M101 r2 的
//! 「本组接真控件只需改两个文件」这条口径，在语言行上**仍然是两个文件**。

use super::super::general;
use unlucky_design::icons::IconKind;
use unlucky_i18n::LangChoice;
use unlucky_i18n::{Key, Lang};

/// 这一组的**静态演示行**：语言（M106）与主题（M229）都已经换成真控件，所以只剩两行。
///
/// 抓得住：①四条线并行合并时把别人的行粘到本文件；②语言/主题被「顺手」加回静态表——
/// 那等于把用户看到的假值又写回去（它们的值必须跟着语言/档位变，静态表做不到）。
/// 抓不住：真控件画出来长什么样（那是截图验收的事，见 `DevDocs/artifacts/m229/`）。
#[test]
fn the_general_group_keeps_only_the_two_rows_without_a_backend() {
    let rows: Vec<(&str, &str)> = general::GROUP.rows.to_vec();
    assert_eq!(
        rows,
        vec![
            ("Startup page", "Instances"),
            ("Close button", "Exit to tray")
        ]
    );
}

/// **M168（D28 后半）**：中文字体下载状态行的四种状态都得有话说，且两套目录都不许 `⟪missing⟫`。
///
/// 抓得住：①只画「下载成功」一种（失败的用户会以为程序坏了）；②失败态的文案写成「保持英文」
/// （D28 的实际行为是**继续用系统字体**，界面语言不变）；③占位符没被替换（用户会看到 `{family}`）；
/// ④新 key 只进了英文表（中文界面上出现 `⟪missing⟫`）。
/// 抓不住：真下载时的样子（那要联网）、字体有没有真的出字（截图 + 真机，见报告）。
#[test]
fn the_font_status_row_speaks_in_four_states_and_two_languages() {
    use crate::fonts::install::{CjkInstall, FetchClass};

    let states = [
        CjkInstall::Downloading { bytes: 17_772_300 },
        CjkInstall::Ready {
            family: "Noto Sans SC".to_owned(),
            from_cache: true,
        },
        CjkInstall::Failed {
            class: FetchClass::Network,
            message: "timeout".to_owned(),
        },
        CjkInstall::System {
            family: "Microsoft YaHei".to_owned(),
        },
        CjkInstall::NotNeeded,
        CjkInstall::Unprobeable,
        CjkInstall::Unknown,
    ];

    for lang in Lang::ALL {
        let catalog = lang.catalog();
        assert!(
            catalog.get(Key::FontCjkStatusLabel).is_some(),
            "{} 目录里缺中文字体那一行的标签",
            lang.tag()
        );
        for state in &states {
            let (value, hint) = general::font_status(&catalog, state);
            assert!(!value.is_empty(), "{state:?} 没话说");
            assert!(!value.contains('{'), "{state:?} 占位符没替换：{value}");
            assert!(
                !value.contains(unlucky_i18n::catalog::MISSING),
                "{state:?} 在 {} 目录里缺译：{value}",
                lang.tag()
            );
            if let Some(hint) = hint {
                assert!(!hint.contains('{'), "{state:?} 的提示没替换：{hint}");
            }
        }
    }

    // 失败态必须说清三类原因之一（而不是一句「失败了」）。
    let zh = Lang::ZhCn.catalog();
    for (class, reason) in [
        (FetchClass::Network, "网络不通"),
        (FetchClass::Integrity, "镜像内容校验不通过"),
        (FetchClass::Disk, "写不进数据目录"),
    ] {
        let (value, _) = general::font_status(
            &zh,
            &CjkInstall::Failed {
                class,
                message: "x".to_owned(),
            },
        );
        assert!(value.contains(reason), "失败原因没进文案：{value}");
    }

    // 下载中写的是**真实**字节数（17 772 300 B → 16.9 MiB），不是假百分比。
    let (value, _) = general::font_status(&zh, &CjkInstall::Downloading { bytes: 17_772_300 });
    assert!(
        value.contains("16.9 MB"),
        "17 772 300 B ≈ 16.9 MiB：{value}"
    );
    assert!(!value.contains('%'), "没有真进度就不许写百分比：{value}");
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
/// 而 `Group.body` 那个槽位只拿到 `States` 与**生效语言**（`Lang`，M179）——装不下「当前语言档位」。
/// 谁把语言行塞进 `body`，就得先给它换签名（那会改到别人那个组的文件），这条断言逼他先想一遍。
#[test]
fn the_general_group_does_not_use_the_appearance_body_slot() {
    assert!(
        general::GROUP.body.is_none(),
        "General 的真控件走 pages::Language 入口（见 settings/mod.rs 的 GENERAL_GROUP），不是 body"
    );
}

/// **M188：日志文件夹那一行**的文案必须与实现一致（用户投诉「找不到崩溃日志」的直接判据）。
///
/// 抓得住：①文案里的路径写错（写成 `data/logs`、`cache/game`）；②滚动上限与
/// `core::launch::launcher_log::MAX_BYTES` 脱钩（文案说 1 MiB、实现改成 4 MiB）；
/// ③旧目录被静默改成「已迁移」（那会让用户以为旧日志没了）；
/// ④某条 key 只进了英文表（中文界面上是 `⟪missing⟫`）。
/// 抓不住：界面画出来长什么样（截图验收管那一层）。
#[test]
fn the_logs_row_names_the_real_paths_and_the_real_limit() {
    use unlucky_core::launch::launcher_log;
    use unlucky_core::platform::layout::{LauncherLayout, GAME_LOG_DIR, LOGS_DIR};

    // 实现侧的真值：分区名与上限都只从 core 来。
    assert_eq!(LOGS_DIR, "logs");
    assert_eq!(GAME_LOG_DIR, "game");
    assert_eq!(launcher_log::MAX_BYTES, 1024 * 1024, "文案说 1 MiB");
    let layout = LauncherLayout::new(std::path::Path::new("/tmp/ull-home"));
    assert_eq!(
        layout.game_logs(),
        std::path::Path::new("/tmp/ull-home/logs/game")
    );

    for lang in Lang::ALL {
        let catalog = lang.catalog();
        let hint = catalog.text(Key::SettingsLogsFolderHint);
        for needle in [
            "launcher.log",
            "1 MiB",
            &format!("{LOGS_DIR}/{GAME_LOG_DIR}/"),
            "data/agent/sessions",
        ] {
            assert!(
                hint.contains(needle),
                "{} 缺 `{needle}`：{hint}",
                lang.tag()
            );
        }
        assert!(!hint.contains('⟪'), "{} 目录里缺词：{hint}", lang.tag());
        assert!(
            !catalog.text(Key::SettingsLogsFolder).contains('⟪'),
            "{} 缺「日志文件夹」：{}",
            lang.tag(),
            catalog.text(Key::SettingsLogsFolder)
        );
        assert!(!catalog.text(Key::SettingsLogsCopy).contains('⟪'));
        assert!(!catalog.text(Key::SettingsLogsUnknown).contains('⟪'));
    }
}
