//! 强调色接线的两条判据（M250 ACCENTWIRE）：判据一「两套配置下同一组件色值不同」、
//! 判据二「非法值拿到结构化原因且不静默回退」。
//!
//! 为什么从 `tests.rs` 再拆一层：这两条用例加起来 150 行，`tests.rs` 会越过 400 行硬线
//! （M180 把它从 `mod.rs` 拆出来就是为了这个）。判据本身与写进 `tests.rs` 的版本逐字一致，
//! 只是换了落点。

use super::Change;

/// 青绿（`00 6A 5C`）由 design 的 `Color` 拼出来：本 crate 的 `src/**` 不许出现十六进制色值
/// 字面量（`tests/no_hardcoded_tokens.rs` 把测试文件也扫在内——那条门禁是对的，色值只能来自令牌）。
const TEAL: unlucky_design::color::Color = unlucky_design::color::Color::rgb(0x00, 0x6A, 0x5C);

/// 判据一：`accent_color` 为空 与 为青绿（见 [`TEAL`]）时，**同一组件取到的 primary 类角色色值必须不同**。
///
/// 取色点是 `progress::linear_colors`（进度条指示条 = `Role::Primary`，实心按钮的容器也是它）：
/// 断言的是**取到的值**，不是「没 panic」——把接线改回「恒走出厂板」，两边相等当场红。
///
/// 抓得住：① 接线不存在（两套 Ui 同色）；② 只改了 `Primary` 而漏掉 secondary/tertiary；
/// ③ 有人把 `Scheme::from_accent` 的中性基准换掉（下面那条 `assert_eq` 会红）。
/// 抓不住：像素（样式闭包被别处覆盖）——那是出图与 `components/**` 的 probe 用例的事。
#[test]
fn custom_accent_changes_the_component_colours_it_reaches() {
    use crate::app::components::feedback::progress;
    use crate::cli::Backend;
    use crate::theme::{Families, Ui};
    use unlucky_design::color::{Role, Scheme, SchemeFlavor};
    use unlucky_presenter::prefs::{AppearanceBase, AppearancePrefs};

    use super::super::State;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    let mut state = State::new(
        &AppearancePrefs::factory(),
        &AppearanceBase::factory(),
        Backend::Gpu,
        SchemeFlavor::Light,
    );
    // 空配置 = 出厂板（`rebuild_ui` 的 `(None, None)` 分支同一条路）
    assert!(
        state.accent.color().is_none(),
        "空串是「用出厂板」，不该被解析成自定义色"
    );
    let factory = Ui::new(SchemeFlavor::Light, FAMILIES);

    // 填青绿 = `rebuild_ui` 的 `(Some(accent), _)` 分支
    state.apply(Change::Accent(TEAL.to_hex()));
    assert_eq!(state.accent.error(), None, "合法 hex 不该报错");
    let accent = state.accent.color().expect("合法 hex 必须解析成颜色");
    let custom = Ui::with_accent(SchemeFlavor::Light, accent, FAMILIES);

    let (_, factory_primary) = progress::linear_colors(&factory);
    let (_, custom_primary) = progress::linear_colors(&custom);
    assert_eq!(factory_primary, factory.color(Role::Primary));
    assert_eq!(custom_primary, custom.color(Role::Primary));
    assert_ne!(
        factory_primary, custom_primary,
        "同一组件的 primary 类角色在两套配置下同色 ⇒ 强调色没接线"
    );

    // 强调族逐条跟着变（不只 primary）。
    //
    // `OnPrimary` 刻意**不在**清单里：深浅两套方案的 on-primary 都是 tone 100/20 的白/深色，
    // 强调色只换色相，这一格在两套方案下同为纯白——令牌表里两格同值是合法的，钉它会假红。
    // 「强调族真的换了色相」由下面三个色相承载角色负责：Primary / PrimaryContainer / Tertiary。
    let baseline = Ui::from_scheme(Scheme::baseline(SchemeFlavor::Light), FAMILIES);
    for role in [
        Role::Primary,
        Role::PrimaryContainer,
        Role::Secondary,
        Role::Tertiary,
    ] {
        assert_ne!(
            custom.color(role),
            baseline.color(role),
            "{role:?} 没跟着强调色走"
        );
    }
    // **中性表面与 error 保持基线**：`Scheme::from_accent` 的既定语义（design 侧登记，不是没接线）
    for role in [
        Role::Surface,
        Role::SurfaceContainerHighest,
        Role::OnSurface,
        Role::Outline,
        Role::Error,
        Role::OnError,
    ] {
        assert_eq!(
            custom.color(role),
            baseline.color(role),
            "{role:?} 被强调色染了：`Scheme::from_accent` 只换 primary/secondary/tertiary 三族"
        );
    }
}

/// 判据二：非法值必须拿到**结构化的原因**，生效色不回退、不写盘，tokens 仍可构造（不崩）。
///
/// 抓得住：把非法分支改成「静默回退出厂色」——`color()` 会变成 `None`、`persist_value()` 会变成
/// 空串，三条断言当场红；把错误吞成 `bool`——`error()` 不再是 `Some(..)`，也红。
/// 抓不住：错行在屏幕上到底画没画出来（`every_appearance_card_key_is_drawn` 的 `BadAccent` 档
/// 证明取词口被走到；像素由出图负责）。
#[test]
fn illegal_accent_reports_a_reason_and_never_silently_falls_back() {
    use crate::cli::Backend;
    use crate::theme::{Families, Ui};
    use unlucky_design::color::{Role, SchemeFlavor};
    use unlucky_presenter::prefs::{AccentError, AppearanceBase, AppearancePrefs};

    use super::super::State;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    let mut state = State::new(
        &AppearancePrefs::factory(),
        &AppearanceBase::factory(),
        Backend::Gpu,
        SchemeFlavor::Light,
    );
    state.apply(Change::Accent(TEAL.to_hex()));
    let good = state.accent.color().expect("先放一个合法值");
    let good_primary = Ui::with_accent(SchemeFlavor::Light, good, FAMILIES).color(Role::Primary);

    // 超长串由合法值拼出来：`src/**` 的色值字面量门禁把注释与测试也算在内（那条门禁是对的）。
    //
    // 三档里只有后两档**当场**亮：`#12` 是「还可能是合法前缀」（用户可能正打到一半），它的
    // 亮灯时机由 `accent_errors_only_appear_when_they_are_actionable` 与 `commit` 覆盖。
    let long = format!("{}00112233", TEAL.to_hex());
    for (text, expected) in [
        ("zzzzzz".to_owned(), AccentError::Digit { ch: 'z' }),
        (long, AccentError::Long { digits: 14 }),
    ] {
        state.apply(Change::Accent(text.clone()));
        assert_eq!(
            state.accent.error(),
            Some(expected),
            "{text}: 不可能变合法的输入必须当场亮且原因结构化（不是一句 bool / 一句「格式不对」）"
        );
        assert_eq!(
            state.accent.color(),
            Some(good),
            "{text}: 静默回退出厂色了（非法输入不该改生效色）"
        );
        assert_eq!(
            state.accent.persist_value(),
            TEAL.to_hex(),
            "{text}: 非法值写进配置了"
        );
        // tokens 仍可构造，取到的还是上一次合法色
        let ui = Ui::with_accent(SchemeFlavor::Light, state.accent.color().unwrap(), FAMILIES);
        assert_eq!(ui.color(Role::Primary), good_primary);
    }

    // `#12`：输入中不亮，**提交**后必须亮（否则「非法值可见」会在短输入上落空）
    state.apply(Change::Accent("#12".to_owned()));
    assert_eq!(state.accent.error(), None, "#12: 输入中先不亮");
    state.accent.commit();
    assert_eq!(
        state.accent.error(),
        Some(AccentError::Short { digits: 2 }),
        "#12: 提交后必须亮且原因正确"
    );
    assert_eq!(state.accent.color(), Some(good), "#12: 提交也不许改生效色");

    // 空串是**合法**的「回出厂板」，不是错误
    state.apply(Change::Accent(String::new()));
    assert_eq!(state.accent.error(), None, "空串不是错误");
    assert!(state.accent.color().is_none(), "空串 = 出厂板");
    assert_eq!(state.accent.persist_value(), "");

    // 小写、缺 `#` 都吃得下；落盘一律规范化成大写的 `#RRGGBB`
    state.apply(Change::Accent(TEAL.to_hex().to_lowercase()));
    assert_eq!(state.accent.persist_value(), TEAL.to_hex());
    state.apply(Change::Accent(
        TEAL.to_hex().trim_start_matches('#').to_owned(),
    ));
    assert_eq!(state.accent.persist_value(), TEAL.to_hex());
}

/// **p2-3**：错行与 `Field::error()` 的出现时机 —— 只在「提交」或「不可能再变合法」时。
///
/// 抓得住：把 `Accent::retry` 的延后逻辑删掉（所有错都立刻亮）⇒ 第一段当场红（手打青绿
/// 的每一个前缀都会亮）；把 `commit` 改成空实现 ⇒ 第二段红（提交后不亮，等于「非法值不可见」）；
/// 把配置读回也走延后 ⇒ 第三段红。
/// 抓不住：像素（红框与红字到底画没画）——那是出图与 `every_appearance_card_key_is_drawn` 的事。
#[test]
fn accent_errors_only_appear_when_they_are_actionable() {
    use crate::cli::Backend;
    use unlucky_design::color::SchemeFlavor;
    use unlucky_presenter::prefs::{AccentError, AppearanceBase, AppearancePrefs};

    use super::super::State;

    let factory = AppearancePrefs::factory();
    let mut state = State::new(
        &factory,
        &AppearanceBase::factory(),
        Backend::Gpu,
        SchemeFlavor::Light,
    );

    // ① 手打青绿的过程：每个**合法前缀**都不许亮红（否则一次输入闪 5 次）
    for prefix in ["#", "#0", "#00", "#006", "#006A", "#006A5"] {
        state.apply(Change::Accent(prefix.to_owned()));
        assert_eq!(
            state.accent.error(),
            None,
            "{prefix:?}: 还没打完就亮红了（手打一个合法色会闪好几次）"
        );
        assert!(
            state.accent.color().is_none(),
            "{prefix:?}: 没打完的输入不该生效"
        );
    }
    state.apply(Change::Accent(TEAL.to_hex()));
    assert_eq!(state.accent.error(), None);
    assert!(state.accent.color().is_some(), "打完了就该生效");

    // ② 输入中留着不打完 ⇒ 不亮；**提交**（点到别的外观控件）⇒ 亮且原因正确
    state.apply(Change::Accent("#12".to_owned()));
    assert_eq!(state.accent.error(), None, "输入中先不亮");
    state.apply(Change::PanelAlpha(0.8));
    assert_eq!(
        state.accent.error(),
        Some(AccentError::Short { digits: 2 }),
        "离开这一格之后必须亮，否则非法值就不可见了"
    );

    // ③ 配置读回（`Accent::new`）⇒ **当场**亮，不等用户来点
    let bad = AppearancePrefs {
        accent_color: "#12".to_owned(),
        ..AppearancePrefs::factory()
    };
    let loaded = State::new(
        &bad,
        &AppearanceBase::factory(),
        Backend::Gpu,
        SchemeFlavor::Light,
    );
    assert_eq!(
        loaded.accent.error(),
        Some(AccentError::Short { digits: 2 }),
        "配置文件里的坏值必须当场可见"
    );
    assert!(loaded.accent.color().is_none(), "坏值不生效，走出厂板");
}

/// **P18 / M252 判据 ③**：回车 = 立即提交（`Change::AccentCommit`）。
///
/// 抓得住：① 把回车实现成「再走一次 `Accent(原文)`」——`retry(false)` 会把已经亮起来的错**吞回去**，
/// 第一段当场红；② 把 `State::apply` 开头那条「非敲字即提交」的规则删掉（回车不再提交），
/// 第一段同样红；③ 回车把非法值写进配置（`persist_value` 变了），第二、三段红。
/// 抓不住：真按下回车（iced 的键路径，见 `section/accent.rs` 的接线用例），也抓不住写盘那一下
/// （`SettingsBridge::persist` 在 `app` 侧）——这里钉的是 `current_values().accent_color`，
/// 也就是**要写出去的那个值**。
#[test]
fn enter_commits_a_half_typed_value_and_persists_only_legal_ones() {
    use crate::cli::Backend;
    use unlucky_design::color::SchemeFlavor;
    use unlucky_presenter::prefs::{AccentError, AppearanceBase, AppearancePrefs};

    use super::super::State;

    let mut state = State::new(
        &AppearancePrefs::factory(),
        &AppearanceBase::factory(),
        Backend::Gpu,
        SchemeFlavor::Light,
    );

    // ① 打了一半就放着：输入中**不**亮（逐键报错的规矩不许被回车改回来）……
    state.apply(Change::Accent("#12".to_owned()));
    assert_eq!(
        state.accent.error(),
        None,
        "输入中先不亮（手打一个合法色不该闪好几次）"
    );
    // ……但回车必须亮，而且**不写盘**（落盘仍是上一次合法值 = 空串 = 出厂板）
    state.apply(Change::AccentCommit);
    assert_eq!(
        state.accent.error(),
        Some(AccentError::Short { digits: 2 }),
        "回车之后 #12 必须亮红（打了一半放着也能被用户主动确认）"
    );
    assert_eq!(
        state.accent.persist_value(),
        "",
        "#12: 回车不许把非法值写盘"
    );
    assert!(state.accent.color().is_none(), "#12: 也不许改生效色");
    assert_eq!(
        state.current_values().accent_color,
        "",
        "写回配置的值仍是上一次合法值"
    );

    // ② 合法值回车：不报错、生效、写盘值就是规范化 hex
    state.apply(Change::Accent(TEAL.to_hex().to_lowercase()));
    state.apply(Change::AccentCommit);
    assert_eq!(state.accent.error(), None, "合法值回车不该冒出错误");
    assert!(state.accent.color().is_some(), "合法值回车后仍生效");
    assert_eq!(
        state.current_values().accent_color,
        TEAL.to_hex(),
        "合法值回车后写回配置的是规范化 hex"
    );

    // ③ 幂等：同一个值连按两次回车不出错、不改值
    state.apply(Change::AccentCommit);
    state.apply(Change::AccentCommit);
    assert_eq!(state.accent.error(), None);
    assert_eq!(state.current_values().accent_color, TEAL.to_hex());
}
