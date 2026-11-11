//! 外观分组第 ⓪ 段（视觉上排在最前）：**自定义强调色**（M250 ACCENTWIRE）。
//!
//! 为什么这一段值得单开：`appearance.accent_color` 从 M41 起就有键、有 presenter 往返、
//! 有 `ACCENT_PRESETS` 色板，却**没有任何 UI 写者**——造 tokens 的唯一入口恒走
//! `DesignTokens::brand(board)`，用户即使改了配置文件，界面也不会变。接线之后这一段就是那条
//! 链路的手动入口，也是**非法值唯一能被看见的地方**（错行紧贴输入框，不折叠、不写进日志了事）。
//!
//! ## 三件控件对应的键与消息
//!
//! | 控件 | 键 | 消息 |
//! |---|---|---|
//! | 强调色文本框（受控，输入即生效） | `appearance.accent_color` | [`Change::Accent`] |
//! | 「出厂」+ 8 个预设色块 | 同上（写入色板里的 hex） | [`Change::Accent`] |
//! | 非法值错行 | —（不进配置） | 无 |
//!
//! ## 色块为什么画成圆点而不是药丸
//!
//! 9 颗写着十六进制串的药丸会变成一排噪声，而且色相要靠读字获取——色板控件的可见内容**就是**
//! 颜色本身。所以色块 = `ShapeScale::Full` 的实心圆 + 描边；选中态加粗描边（不靠色相区分：
//! 色块底下垫的是它自己的颜色，色相对比不可靠）。

use iced::widget::tooltip::{self, Tooltip};
use iced::widget::{button, column, container, row, text, Space as Gap};
use iced::{Background, Border, Element, Length, Padding, Shadow, Theme as IcedTheme};

use unlucky_design::color::Role;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_i18n::catalog::Key;
use unlucky_i18n::Lang;
use unlucky_presenter::prefs::{AccentError, ACCENT_PRESETS};

use crate::app::components::inputs::{text_field, Field};
use crate::pages::Message;
use crate::theme::{to_iced, Ui, STROKE_PX};

use super::super::accent::Accent;
use super::super::state::State;
use super::super::Change;
use super::{choice, label, note_line, t, tr};

/// 把 ⓪ 段推进 `rows`。
pub(super) fn push<'a>(
    ui: &'a Ui,
    state: &'a State,
    lang: Lang,
    rows: &mut Vec<Element<'a, Message>>,
) {
    let accent = &state.accent;

    // 受控输入框 + 失败态：`Field::error()` 让框本身变成错误色，错行再补「错在哪」。
    let mut field = Field::new(accent.text())
        .label(t(Key::SettingsAppearanceAccent, lang))
        .placeholder("#RRGGBB")
        // P18：回车 = 立即提交（把「打了一半就放着」的 `#12` 亮成红框 + 错行）。
        // **不是**逐键报错：敲字那条路（`Change::Accent`）照旧只在「不可能再变合法」时才亮红。
        .on_submit(accent_commit);
    if accent.error().is_some() {
        field = field.error();
    }
    let input: Element<'a, Message> = text_field(ui, field, |value| {
        Message::Appearance(Change::Accent(value))
    });
    rows.push(
        column![label(ui, t(Key::SettingsAppearanceAccent, lang)), input]
            .spacing(ui.space(Space::X1))
            .width(Length::Fill)
            .into(),
    );

    rows.push(presets(ui, accent, lang));

    // 非法值：**结构化的原因**，不是「格式不对」这种没法行动的一句话。
    if let Some(error) = accent.error() {
        let text = reason(lang, error);
        rows.push(note_line(ui, &text, ui.color(Role::Error)));
    }

    // 说明：空 = 出厂板；自定义色只换三族（中性表面与 error 保持基线是 design 既定语义）。
    rows.push(note_line(
        ui,
        t(Key::SettingsAppearanceAccentHint, lang),
        ui.color(Role::OnSurfaceVariant),
    ));
}

/// 回车那一下要发的那条消息——**唯一产地**（M252 / P18）。
///
/// 抽成函数是为了让「回车 = 提交」这件事在源码里只有一个写法：`section/accent.rs` 把它交给
/// `Field::on_submit`，测试按它钉住「回车走的是 `AccentCommit` 而不是又一条 `Accent(原文)`」
/// （后者是**逐键**路径，会把已经亮出来的错重新吞回去）。
fn accent_commit() -> Message {
    Message::Appearance(Change::AccentCommit)
}

/// 「预设」两行：先「标签 + 出厂」，再一排 8 颗色块。
///
/// 为什么分两行：一行放 9 个控件在设置卡片的正文宽度里会被挤成「7 + 1」折行，最后一颗色块
/// 孤零零掉到下一行（出图实测）。分两行是**有意**的排版，不是被挤出来的。
fn presets<'a>(ui: &'a Ui, accent: &'a Accent, lang: Lang) -> Element<'a, Message> {
    let current = accent.persist_value();
    let head = row![
        container(label(ui, t(Key::SettingsAppearanceAccentPresets, lang)))
            .align_y(iced::Alignment::Center),
        choice(
            ui,
            t(Key::SettingsAppearanceAccentFactory, lang),
            current.is_empty(),
            Change::Accent(String::new()),
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(iced::Alignment::Center);

    let swatches: Vec<Element<'a, Message>> = ACCENT_PRESETS
        .iter()
        .map(|hex| swatch(ui, hex, current == *hex))
        .collect();

    column![
        head,
        row(swatches)
            .spacing(ui.space(Space::X2))
            .align_y(iced::Alignment::Center),
    ]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill)
    .padding(Padding::from([ui.space(Space::X2), 0.0]))
    .into()
}

/// 一颗色块：实心圆 + 描边（选中态加粗）。点击写入该预设。
fn swatch<'a>(ui: &'a Ui, hex: &'static str, selected: bool) -> Element<'a, Message> {
    let fill = unlucky_design::color::Color::from_hex(hex).map(to_iced);
    // 直径取控件高度的六成（40 → 24dp）：与药丸按钮同一行时不抢节奏。
    let diameter = ui.metrics().control_height * 0.6;
    let ring = if selected {
        ui.color(Role::OnSurface)
    } else {
        ui.color(Role::OutlineVariant)
    };
    let width = if selected { STROKE_PX * 2.0 } else { STROKE_PX };
    let radius = ui.radius(ShapeScale::Full, diameter);

    let dot: Element<'a, Message> = button(
        Gap::new()
            .width(Length::Fixed(diameter))
            .height(Length::Fixed(diameter)),
    )
    .padding(Padding::from(0.0))
    .style(move |_theme: &IcedTheme, status| button::Style {
        background: fill.map(Background::Color),
        text_color: ring,
        border: Border {
            color: match status {
                button::Status::Hovered | button::Status::Pressed => ui.color(Role::OnSurface),
                _ => ring,
            },
            width,
            radius,
        },
        shadow: Shadow::default(),
        snap: true,
    })
    .on_press(Message::Appearance(Change::Accent(hex.to_owned())))
    .into();

    // 色块本体只有颜色、没有字：补一条可读提示（hex），否则键盘与读屏用户只能靠位置辨色。
    Tooltip::new(dot, hint(ui, hex), tooltip::Position::Top).into()
}

/// 色块的悬停提示：`Overlay` 底 + `BodySmall` 的 hex 原文（与 `pages/agent` 的 tooltip 同一手法）。
fn hint<'a>(ui: &'a Ui, hex: &'a str) -> iced::widget::Container<'a, Message> {
    let style = ui.style(unlucky_design::typography::TypeRole::BodySmall);
    container(
        text(hex.to_owned())
            .size(style.size_px)
            .font(ui.font_of(style))
            .color(ui.color(Role::OnSurface)),
    )
    .padding(Padding::from([ui.space(Space::X1), ui.space(Space::X2)]))
    .style(move |_theme: &IcedTheme| container::Style {
        background: Some(Background::Color(
            ui.layer(unlucky_design::elevation::MaterialLayer::Overlay),
        )),
        border: Border {
            radius: ui.radius_dp(ShapeScale::Small),
            ..Border::default()
        },
        ..container::Style::default()
    })
}

/// 结构化原因 → 一句话（参数由 Rust 侧格式化，词表只认 `{name}`）。
fn reason(lang: Lang, error: AccentError) -> String {
    match error {
        AccentError::Short { digits } => tr(
            lang,
            Key::SettingsAppearanceAccentShort,
            &[("digits", &digits.to_string())],
        ),
        AccentError::Long { digits } => tr(
            lang,
            Key::SettingsAppearanceAccentLong,
            &[("digits", &digits.to_string())],
        ),
        AccentError::Digit { ch } => tr(
            lang,
            Key::SettingsAppearanceAccentDigit,
            &[("ch", &visible(ch))],
        ),
    }
}

/// 把一个字符变成**看得见**的形态：控制字符 / 空白 / 零宽与双向控制符（`Cf`）一律写 `U+XXXX`。
///
/// 为什么不能直接 `ch.to_string()`：用户粘进来的可能是不换行空格、零宽空格或 RTL 覆盖符——
/// 那几个字符在句子里**看不见**，错行会变成「强调色里有非十六进制字符：」后面一片空白，
/// 用户根本不知道自己错在哪。
fn visible(ch: char) -> String {
    if ch.is_alphanumeric() || ch.is_ascii_graphic() {
        ch.to_string()
    } else {
        format!("U+{:04X}", u32::from(ch))
    }
}

#[cfg(test)]
mod tests {
    use super::visible;

    /// 本文件源码（源码结构断言用，与 `pages/layout/source.rs` 同一条纪律）。
    const SOURCE: &str = include_str!("accent.rs");

    /// **M252 / P18 回车接线的判据**：`push` 里有一**行代码**接了 `on_submit`，实参整取等于
    /// `accent_commit`，且那个产地发的确实是 `AccentCommit`（提交）而不是 `Accent(原文)`（敲字）。
    ///
    /// iced 的 `Element` 没有内省接口（见 `pages::layout::source` 的模块文档），所以「回车真的
    /// 按下去」在这里量不到——能钉住的是**它接的是哪条消息**。
    ///
    /// - 抓得住：`.on_submit(..)` 被删掉 **或注释掉**（`source::code_line` 先剥注释，判的是**代码行**）；
    ///   实参被换成同前缀的别的产地（`accent_commit_none`——`call_arguments` 是**整取**，不是子串）；
    ///   接成 `Change::Accent(原文)`（那是**逐键**路径，`retry(false)` 会把已经亮起来的错吞回去）。
    /// - 抓不住：按键真的到达控件（iced 的 `Widget::update`）、以及红框/错行画没画（出图负责）。
    #[test]
    fn the_accent_field_wires_enter_to_commit() {
        let body = crate::pages::layout::source::function_body(SOURCE, "fn push");
        let line =
            crate::pages::layout::source::code_line(body, ".on_submit(").unwrap_or_else(|| {
                panic!("`push` 里没有一行**代码**接 `on_submit`（注释掉也算没有）：\n{body}")
            });

        // 实参**整取**（不是子串）：`accent_commit_none` 这种同前缀产地不许静默通过。
        assert_eq!(
            crate::pages::layout::source::call_arguments(&line, ".on_submit("),
            vec!["accent_commit"],
            "回车必须接 `accent_commit`（唯一产地），实际接线：{line}"
        );

        // 产地本身发什么：先看**代码行**（注释里的字不算），再跑一次拿 `Debug` 文本核形态
        // （`Message` 没有 `PartialEq`）。
        let factory = crate::pages::layout::source::function_body(SOURCE, "fn accent_commit");
        assert!(
            crate::pages::layout::source::code_lines(factory)
                .iter()
                .any(|line| line.contains("Change::AccentCommit")),
            "产地必须发 `Change::AccentCommit`（提交），实际函数体：\n{factory}"
        );
        assert_eq!(
            format!("{:?}", super::accent_commit()),
            format!(
                "{:?}",
                crate::pages::Message::Appearance(super::Change::AccentCommit)
            ),
            "回车那条消息必须是 `AccentCommit`（提交），不是 `Accent(原文)`（敲字）"
        );
    }

    /// 错行里的字符必须**看得见**：不可见字符写成 `U+XXXX`，可见字符原样（r1 小事 2）。
    ///
    /// 抓得住：把 `visible` 改回 `ch.to_string()`——`U+00A0` / `U+200B` 那两条立刻红（错行会变成
    /// 「强调色里有非十六进制字符：」后面一片空白，用户不知道自己错在哪）。
    /// 抓不住：句子本身（词表在 `unlucky-i18n`）。
    #[test]
    fn invisible_characters_are_shown_as_code_points() {
        assert_eq!(visible('\u{a0}'), "U+00A0", "不换行空格看不见");
        assert_eq!(visible('\u{200b}'), "U+200B", "零宽空格看不见");
        assert_eq!(visible('\u{202e}'), "U+202E", "RTL 覆盖符看不见");
        assert_eq!(visible(' '), "U+0020", "普通空格也看不见");
        assert_eq!(visible('z'), "z", "可见字符原样");
        assert_eq!(visible('#'), "#", "ASCII 标点原样");
        assert_eq!(visible('中'), "中", "非 ASCII 可见字符原样");
    }
}
