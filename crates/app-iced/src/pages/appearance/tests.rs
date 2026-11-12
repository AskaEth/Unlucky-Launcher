//! `pages::appearance` 的单测：`Change` 的落盘映射（M93）+ **中文界面文案的取证守卫**（M180）。
//!
//! 为什么是独立文件：`mod.rs` 已经 220 行上下，而 M180 要在模块里加三条取证用例。搬移口径与
//! `background/tests.rs` 一致——既有两条用例**一字未动**，只是从 `mod.rs` 的行内 `mod tests` 挪到这里。
//!
//! ## M180 的两条取证守卫，它们「抓得住什么」
//!
//! iced 0.14 没有可遍历控件树的公开接口（见 [`super::probe`] 的模块文档），所以取证点只能是
//! 取词口 `section::t` / `section::tr`。于是这一对守卫分工如下：
//!
//! | 守卫 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`the_appearance_card_speaks_chinese`] | 中文界面下取到的每一条文案都是纯 ASCII（= 有人把某条 key 的英文写回中文表 / 中文表里放英文原句） | 硬编码字面量（它压根不过取词口） |
//! | [`every_appearance_card_key_is_drawn`] | 某一处控件被改回**写死英文**：那条 key 不再被取，集合里就少一个元素 | 新加一条字面量（新 key 还没有，集合不看它） |
//!
//! 两条合起来才等于「中文界面下外观卡片里不再出现英文字面量」——单看任一条都有一个真会发生的
//! 漏网口，所以这里刻意写两条。

use super::{Change, APPEARANCE_GROUP};
use unlucky_i18n::catalog::Key;
use unlucky_i18n::Lang;
use unlucky_presenter::prefs::{keys, BgFill, PaletteSource};

mod accent;

/// 抓得住：每条改动都点名到自己那个 schema 键（写错键 = 静默写到别的配置项上）。
/// 抓不住：这些键是否真的在 core 的 schema 白名单里（那由 presenter/core 的测试守）。
#[test]
fn every_change_names_its_own_schema_key() {
    assert_eq!(APPEARANCE_GROUP, 1, "左列第 1 项是 Appearance");
    let cases = [
        (Change::WindowOpacity(0.8), keys::WINDOW_OPACITY),
        (Change::PanelAlpha(0.6), keys::PANEL_ALPHA),
        (Change::BgEnabled(true), keys::BG_IMAGE_ENABLED),
        (Change::BgImage("D:/x.png".to_owned()), keys::BG_IMAGE),
        (Change::BgFill(BgFill::Tile), keys::BG_FILL),
        (Change::BgDim(0.5), keys::BG_DIM),
        (
            Change::PaletteSource(PaletteSource::Wallpaper),
            keys::PALETTE_SOURCE,
        ),
        (
            Change::Corner(unlucky_presenter::prefs::WindowCorner::Square),
            keys::WINDOW_CORNER,
        ),
    ];
    for (change, key) in cases {
        assert_eq!(change.key(), Some(key), "{change:?}");
        assert!(change.value().is_some());
    }
    assert_eq!(Change::SelectGroup(2).key(), None, "切分组不落盘");
    assert_eq!(Change::SelectGroup(2).value(), None);
    // P18：回车提交不产生值，但它是强调色那一格的改动（`on_appearance` 据此再写一次上一次合法值）
    assert_eq!(
        Change::AccentCommit.key(),
        Some(keys::ACCENT_COLOR),
        "回车提交也要点名到自己那个 schema 键"
    );
    assert_eq!(
        Change::AccentCommit.value(),
        None,
        "回车不产生新值（落盘一直取上一次合法值）"
    );
}

/// 抓得住：落盘字符串必须是 schema 认识的词表（枚举档写错就整条配置读回默认值）。
/// 抓不住：数值是否在范围内（钳位在 core 侧，越界写入会被 schema 拒绝）。
#[test]
fn persisted_values_use_the_schema_vocabulary() {
    assert_eq!(
        Change::BgFill(BgFill::Contain).value().as_deref(),
        Some("contain")
    );
    assert_eq!(
        Change::PaletteSource(PaletteSource::BackgroundImage)
            .value()
            .as_deref(),
        Some("background-image")
    );
    assert_eq!(Change::BgEnabled(false).value().as_deref(), Some("false"));
    assert_eq!(Change::WindowOpacity(0.8).value().as_deref(), Some("0.8"));
}

// --------------------------------------------------------------- 文案（M180 I18NAPPEAR）

/// 画一帧外观卡片，取走这一帧从词表取到的**全部**字：`(key, 文案)`。
///
/// 三档状态各画一帧：
/// - [`Case::Factory`]：auto 档、没设背景图——覆盖常驻的标签 / 选项 / 说明；
/// - [`Case::Cpu`]：CPU 档——覆盖「强制 100%」那一行与软件渲染说明（这两条只在降级态出现）；
/// - [`Case::Dim`]：整窗 α 调低——覆盖「圆角暂时不生效」那条 notice。
///
/// 两条取词路径都要收：`t()` 的 `&'static str` 与 `tr()` 替换完占位符的整句（后者不进前者的记录）。
fn appearance_texts(lang: Lang, case: Case) -> Vec<(Key, String)> {
    use unlucky_design::color::SchemeFlavor;
    use unlucky_presenter::prefs::{AppearanceBase, AppearancePrefs};

    use super::probe;
    use crate::cli::Backend;
    use crate::theme::{Families, Ui};

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    let (backend, alpha) = match case {
        Case::Factory | Case::BadAccent => (Backend::Auto, 1.0),
        Case::Cpu => (Backend::Cpu, 1.0),
        Case::Dim => (Backend::Gpu, 0.5),
    };
    let mut state = super::State::new(
        &AppearancePrefs::factory(),
        &AppearanceBase::factory(),
        backend,
        SchemeFlavor::Light,
    );
    if alpha < 1.0 {
        state.apply(Change::WindowOpacity(alpha));
    }
    if matches!(case, Case::BadAccent) {
        // M250：错行只在失败态出现，所以「三档状态合起来覆盖本页全部 key」需要这一档。
        state.apply(Change::Accent("zzzzzz".to_owned()));
    }

    let ui = Ui::new(SchemeFlavor::Light, FAMILIES);
    // 前面几条用例可能留了记录：本线程的取证每次都从清空开始。
    probe::take();
    probe::take_rendered();

    let _tree = super::section(&ui, &state, lang);

    let mut texts: Vec<(Key, String)> = probe::take()
        .into_iter()
        .map(|(key, text)| (key, text.to_owned()))
        .collect();
    texts.extend(probe::take_rendered());

    // 复审 L-2：`{name}` 换完还留着花括号 ⇒ 某处参数名与模板不一致（`("floor", …)` 写成
    // `("flor", …)` 时句子会原样显示「下限 {floor}」，而「整句非 ASCII」的守卫看不出来）。
    // 三档状态合起来覆盖本页全部 `tr()` 调用，所以这条断言是全覆盖的。
    for (key, text) in &texts {
        assert!(
            !text.contains('{'),
            "{case:?}：{} 渲染后还留着占位符「{text}」——`tr()` 的参数名与模板对不上",
            key.name()
        );
    }
    texts
}

/// 画哪一档状态。
#[derive(Debug, Clone, Copy)]
enum Case {
    /// 工厂态（GPU / auto）。
    Factory,
    /// CPU 降级态（面板 α 与玻璃被强制）。
    Cpu,
    /// 整窗 α 调低（圆角暂时不生效）。
    Dim,
    /// 强调色填了非法值（M250：错行 + 输入框失败态）。
    BadAccent,
}

/// 这一页**常驻**的 key（三档状态合起来必须一个不缺地取到）。
///
/// 为什么是手写清单而不是「`Key::ALL` 里所有 `SettingsAppearance*`」：诊断一族（读盘失败 / 配色回落 /
/// 小图平铺）取决于运行时状态，单测里要让它们出现得造磁盘上的坏图；那些由
/// `background/tests.rs` 与 `palette.rs` 的单测按 key 逐条钉（见各自的用例注释）。
const CARD_KEYS: &[Key] = &[
    Key::SettingsAppearanceAccent,
    Key::SettingsAppearanceAccentPresets,
    Key::SettingsAppearanceAccentFactory,
    Key::SettingsAppearanceAccentHint,
    Key::SettingsAppearanceAccentDigit,
    Key::SettingsAppearanceCorner,
    Key::SettingsAppearanceCornerSystem,
    Key::SettingsAppearanceCornerSquare,
    Key::SettingsAppearanceCornerHint,
    Key::SettingsAppearanceCornerSuspended,
    Key::SettingsAppearanceWindowOpacity,
    Key::SettingsAppearanceWindowOpacityHint,
    Key::SettingsAppearancePanelOpacity,
    Key::SettingsAppearancePanelOpacityHint,
    Key::SettingsAppearancePanelOpacityForcedValue,
    Key::SettingsAppearancePanelOpacityForcedHint,
    Key::SettingsAppearanceBackgroundImage,
    Key::SettingsAppearanceOff,
    Key::SettingsAppearanceOn,
    Key::SettingsAppearanceImagePath,
    Key::SettingsAppearancePaletteLevel,
    Key::SettingsAppearanceBgFill,
    Key::SettingsAppearanceBgFillCover,
    Key::SettingsAppearanceBgFillContain,
    Key::SettingsAppearanceBgFillTile,
    Key::SettingsAppearanceBgDim,
    Key::SettingsAppearanceBgDimHint,
    Key::SettingsAppearancePaletteSource,
    Key::SettingsAppearancePaletteBackground,
    Key::SettingsAppearancePaletteWallpaper,
    Key::SettingsAppearancePalettePreset,
];

/// **M180 守门 ①**：中文界面下，外观卡片里取到的每一条文案都不是纯 ASCII。
///
/// 抓得住：某条 key 的中文译文被换成英文原句（中文表里出现纯 ASCII 的一句）；中文译文写成
/// `PNG` / `Preset` 这种专名；`tr()` 那条路径把数字拼进英文模板（比如 `Colours: preset`）。
/// 抓不住：硬编码字面量——它不过取词口，由 [`every_appearance_card_key_is_drawn`] 与
/// `catalog/settings/appearance.rs` 的译文守卫合力守。
///
/// 变异证明：把 `SettingsAppearanceOff` 的中文改成 `"Off"` ⇒ 本条立刻红。
#[test]
fn the_appearance_card_speaks_chinese() {
    // 白名单：技术专名允许是纯 ASCII（现在一条都没有——`%` / 数字不在文案里，它们是参数）。
    const ASCII_OK: &[&str] = &[];

    for case in [Case::Factory, Case::Cpu, Case::Dim, Case::BadAccent] {
        let pairs = appearance_texts(Lang::ZhCn, case);
        assert!(
            pairs.len() >= 20,
            "{case:?}：取词口只记到 {} 条文案，渲染路径没走到底（语言管子断了？）",
            pairs.len()
        );
        for (key, text) in &pairs {
            if ASCII_OK.contains(&text.as_str()) {
                continue;
            }
            assert!(
                !text.is_ascii(),
                "{case:?}：中文界面下外观卡片里出现了纯 ASCII 的文案「{text}」（{}）\
                 ——写死英文，或中文表里放了英文原句",
                key.name()
            );
        }
    }
}

/// **M180 守门 ②（漏项）**：这一页每一处控件都必须从词表取词。
///
/// 抓得住：把某处 `t(Key::X, lang)` / `tr(lang, Key::X, …)` 改回写死的英文字面量——那条 key 不再
/// 出现在取词记录里，集合比较当场红（这正是 M172 口径「22 处硬编码」的复发形态）。
/// 抓不住：新加一条写死英文（它没有 key，也就没有「少了哪条」）。
///
/// 变异证明：把 `bg_image.rs` 里 `t(Key::SettingsAppearanceOff, lang)` 换回 `"Off"` ⇒ 本条红。
#[test]
fn every_appearance_card_key_is_drawn() {
    let mut seen: Vec<Key> = Vec::new();
    for case in [Case::Factory, Case::Cpu, Case::Dim, Case::BadAccent] {
        seen.extend(
            appearance_texts(Lang::ZhCn, case)
                .into_iter()
                .map(|(key, _)| key),
        );
    }

    let missing: Vec<&str> = CARD_KEYS
        .iter()
        .filter(|key| !seen.contains(key))
        .map(|key| key.name())
        .collect();
    assert!(
        missing.is_empty(),
        "这些 key 一帧都没取到（控件被改回写死英文，或整段没画）：{missing:?}"
    );
}

/// 英文界面下不许出现汉字（否则「含汉字」可能来自写死的中文，而不是词表）。
#[test]
fn the_appearance_card_keeps_its_english_baseline() {
    for case in [Case::Factory, Case::Cpu, Case::Dim, Case::BadAccent] {
        let texts = appearance_texts(Lang::En, case);
        let with_han: Vec<&(Key, String)> = texts
            .iter()
            .filter(|(_, text)| text.chars().any(is_han))
            .collect();
        assert!(
            with_han.is_empty(),
            "{case:?}：英文界面下外观卡片出现了汉字：{with_han:?}"
        );
    }
}

/// 有没有汉字（与 `pages/settings/tests/appearance_section.rs` 的 `is_han` 同一段码位）。
fn is_han(ch: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&ch)
}
