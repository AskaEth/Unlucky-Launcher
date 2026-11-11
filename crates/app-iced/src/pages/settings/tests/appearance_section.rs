//! 「Appearance」分组自己的断言（M100 **新增**：`settings/tests.rs` 那 7 条全是跨 section 的，
//! 一条 section 断言都没有；这 3 条是新增，不是搬移）。
//!
//! **归 language+appearance 线**：这是设置页唯一接了真控件的一组，真控件实现全在
//! `pages/appearance/section/`；本文件只守「注册表这一侧的接线」。
//! 本组已经把 `body` 接上了；`tests/mod.rs` 那条「只有一组有真控件」的**聚合绊线已在 M101 r2 删除**
//! （被四条 per-group 断言完全蕴含——代价是「唯一性」从此没人守，那段说明写在 `tests/mod.rs` 同处）。
//!
//! M179 `I18NSEAM` 加的第 4 条守的是**语言真的进到了渲染层**（此前这一组在中文界面上 0/27 条本地化）。

use super::super::appearance;
use super::super::{demo_groups, view, Language, States};
use unlucky_design::color::SchemeFlavor;
use unlucky_design::icons::IconKind;
use unlucky_i18n::catalog::Key;
use unlucky_i18n::{Lang, LangChoice};
use unlucky_presenter::prefs::{AppearanceBase, AppearancePrefs};

use crate::cli::Backend;
use crate::pages::appearance::{probe, State as AppearanceState};
use crate::pages::layout::{frame_metrics, PaneKit};
use crate::pages::Update;
use crate::theme::{Families, Ui};

/// 这一组的**演示行**（没有外观运行期状态时才画）。
///
/// 抓得住：四条线并行合并时把别人的行粘到本文件（四份 `GROUP` 长得几乎一样）；
/// 漏掉/多一条行；标签与值错位。
/// 抓不住：真控件那一屏（那是 `pages/appearance` 的事，靠出图判）。
#[test]
fn the_appearance_group_shows_density_corner_motion_and_text_scale() {
    let rows: Vec<(&str, &str)> = appearance::GROUP.rows.to_vec();
    assert_eq!(
        rows,
        vec![
            ("Density", "Comfortable"),
            ("Corner style", "Follow system"),
            ("Motion", "Full"),
            ("Text scale", "100%"),
        ]
    );
}

/// 左列图标（`IconKind::Sidebar`）。
#[test]
fn the_appearance_group_keeps_its_icon() {
    assert_eq!(appearance::GROUP.icon, IconKind::Sidebar);
}

/// **本组的绊线**：外观分组必须一直是「挂了真控件」的那一组。
///
/// 抓得住：language+appearance 线把 `body` 换回 `None`（真控件消失、退回演示行，而截图对这种
/// 「只在有外观状态时出现」的差别不一定抓得住）。
/// 抓不住：`body` 到底指向哪个渲染器——**函数指针相等性断言不了**（rustc
/// `unpredictable_function_pointer_comparisons`：fn 地址不保证唯一，比出来的结果没有意义），
/// 所以「指向 `pages::appearance::section`」这件事只能由
/// `settings/appearance.rs` 那一行源码 + 两张图的 sha256 不同来担保。
#[test]
fn the_appearance_group_keeps_a_real_control_body() {
    assert!(
        appearance::GROUP.body.is_some(),
        "外观分组必须是挂了真控件的那一组（`settings/appearance.rs` 里是 `body: Some(body)`，\
         `body` 是本文件那个本地 fn；具体渲染器由那一行源码 + 截图担保，函数指针相等性断言不了）"
    );
}

// --------------------------------------------------------------- 语言（M179 I18NSEAM）

/// **本组的语言绊线**：中文界面下，外观分组渲染出来的字里**必须含汉字**。
///
/// 走的是真路：`settings::view`（真语言分发）→ `Group.body(lang)` → `pages::appearance::section`
/// → `section::t(key, lang)`；取词口 `t` 把每条译文记进 `pages::appearance::probe`，测试断言这批字。
///
/// 为什么不在 `Element` 上遍历：iced 0.14 **没有**可遍历控件树的公开接口（`Widget::children()`
/// 返回的是状态 `Tree`；走 `operate` 要 `Tree` + 布局 + `Renderer`，单测里没有渲染器）。取证点
/// 因此退到「字进树之前」的唯一取词口，代价是**看不见像素**——字体能不能画出汉字要截图判
/// （M179 报告「没验的部分」列了这条）。
///
/// 抓得住：① `section::t` 退回写死 `Lang::En`（M179 之前正是这一行 ⇒ 中文界面上这一组 0/27
/// 条本地化）；② `settings::view` 不把 `language.effective` 递给 `Group.body`（语言到不了渲染层）；
/// ③ 五个 key 的译文在金表里丢了中文。
/// 抓不住：本组其余文案仍是英文**字面量**（没有 key，M172 口径的 27 条里占 22 条）——这条守的是
/// 语言的**管子**，不是文案；补文案是下一条 mission（M179 报告逐行列了那些字面量）。
#[test]
fn the_appearance_group_speaks_chinese_when_the_interface_does() {
    let zh = appearance_texts(Lang::ZhCn);
    assert!(
        !zh.is_empty(),
        "外观分组一条词表文案都没取到：渲染路径没走到 `section::t`（语言管子断了？）"
    );
    assert!(
        zh.iter()
            .any(|(key, _)| *key == Key::SettingsAppearanceCorner),
        "渲染路径没画到「窗口圆角」那一行：取词记录里没有它的 key，{zh:?}"
    );

    let han: Vec<&str> = zh
        .iter()
        .map(|(_, text)| *text)
        .filter(|text| text.chars().any(is_han))
        .collect();
    assert!(
        !han.is_empty(),
        "中文界面下外观分组渲染出来的字全是非汉字（`section::t` 没跟随语言）：{zh:?}"
    );

    // 反向：英文界面下同一批字里不许出现汉字（否则「含汉字」可能来自写死的中文，而不是语言管子）。
    let en = appearance_texts(Lang::En);
    assert!(
        en.iter().all(|(_, text)| !text.chars().any(is_han)),
        "英文界面下外观分组出现了汉字：{en:?}"
    );
    let corner = |texts: &[(Key, &'static str)]| {
        texts
            .iter()
            .find(|(key, _)| *key == Key::SettingsAppearanceCorner)
            .map(|(_, text)| *text)
    };
    assert_ne!(
        corner(&zh),
        corner(&en),
        "同一个 key 在中英两种语言下取到了同一条译文：语言没生效"
    );
}

/// 画一帧设置页（选中「外观」分组）并取走这一帧从词表取到的全部译文。
fn appearance_texts(lang: Lang) -> Vec<(Key, &'static str)> {
    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    let ui = Ui::new(SchemeFlavor::Light, FAMILIES);
    let state = AppearanceState::new(
        &AppearancePrefs::factory(),
        &AppearanceBase::factory(),
        Backend::Auto,
        SchemeFlavor::Light,
    );
    assert_eq!(
        state.selected_group,
        crate::pages::appearance::APPEARANCE_GROUP,
        "工厂态默认选中「外观」分组（否则这一帧根本画不到本组）"
    );
    // 前面几条用例不取词，这里先清一次，避免把别的渲染留下的记录算进来。
    probe::take();

    let metrics = frame_metrics(PaneKit::Supporting, 1180.0);
    let update = Update::default();
    let _tree = view(
        &ui,
        metrics,
        States {
            appearance: Some(&state),
            groups: demo_groups(),
            settings: &[],
        },
        Language {
            choice: LangChoice::System,
            effective: lang,
        },
        &update,
    );

    probe::take()
}

/// 有没有汉字（判据与 `app/view/tests/launch.rs` 的 `is_cjk` 同一段码位）。
fn is_han(ch: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&ch)
}
