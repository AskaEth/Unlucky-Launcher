//! 设置页「外观」分组（**真控件**）+ 背景层 Element + 面板层配色。
//!
//! ## 控件与键的对应
//!
//! | 控件 | 键 | 改动消息 |
//! |---|---|---|
//! | 强调色文本框 + 预设色块（M250） | `appearance.accent_color` | [`Change::Accent`] |
//! | 整窗透明度滑块（下限 0.08） | `appearance.window_opacity` | [`Change::WindowOpacity`] |
//! | 窗口圆角（跟随系统 / 直角，M116） | `appearance.window_corner` | [`Change::Corner`] |
//! | 面板透明度滑块（下限 0.4，CPU 档禁用） | `appearance.panel_alpha` | [`Change::PanelAlpha`] |
//! | 背景图开关 | `appearance.bg_image_enabled` | [`Change::BgEnabled`] |
//! | 背景图路径（文本框） | `appearance.bg_image` | [`Change::BgImage`] |
//! | 填充（铺满 / 完整显示 / 平铺） | `appearance.bg_fill` | [`Change::BgFill`] |
//! | 暗化强度（0~0.95） | `appearance.bg_dim` | [`Change::BgDim`] |
//! | 配色来源（三档） | `appearance.palette_source` | [`Change::PaletteSource`] |
//!
//! ## 文件分工（M179 `I18NSEAM`：纯搬移 + 语言接线）
//!
//! 本模块原来是一个 395 行的 `section.rs`，而全仓行数守卫是 400（`components/gate.rs`），
//! 剩 5 行——加不了任何东西。M179 按「一段控件一个文件」切开了它：
//!
//! | 文件 | 是什么 |
//! |---|---|
//! | 本文件（`section/mod.rs`） | **壳**：[`section`] 按顺序拼各段 + [`card`] 卡片 + 行控件（[`label`] / [`slider_row`] / [`choice_row`] / [`choice`] / [`note_line`]）+ 两个取词口 [`t`] / [`tr`] |
//! | [`window`] | ① 整窗透明度 + ② 窗口圆角（含「因整窗 α 不生效」的 notice 与说明段） |
//! | [`panel`] | ③ 面板透明度（CPU 档强制 1.0，值 / 生效值分开显示） |
//! | [`bg_image`] | ④ 背景图开关 + ⑤ 路径 + ⑥ 诊断 + ⑦ 填充 + ⑧ 暗化 |
//! | [`colour_source`] | ⑨ 配色来源（三档） |
//!
//! 拆分口径：**纯搬移**——每段的语句逐字搬过去，只有下面这几处机械改写（M179 报告里用「去空白/
//! 去尾逗号 diff」逐段证明：改完之后与原文件逐字相同，断言一条没动）：
//!
//! 1. `rows = rows.push(…)` → `rows.push(…)`（壳现在传 `&mut Vec<Element>`）；
//! 2. `t(Key::X)` → `t(Key::X, lang)`（本 mission 的语言接线）；
//! 3. `super::notice(…)` → `super::super::notice(…)`（多了一层模块）；
//! 4. 唯一一处非 `Element` 实参：`rows.push(column![…])` → 末尾加 `.into()`（`Vec::push` 不做 `Into`）；
//! 5. rustfmt 因为多出来的 `, lang` 把两处调用换成了多行。
//!
//! ## 语言（M179 接过管子，M180 补齐文案）
//!
//! [`t`] / [`tr`] 是**唯一**两个取词口（一个拿 `&'static str`、一个拿带占位符的 owned 句子），
//! `lang` 从 `settings::view` 的 `Language.effective` → `Group.body` → [`section`] 一路递进来。
//! M179 只把管子接通（当时本组 27 条文案里真正本地化的只有圆角那 5 条）；M180 `I18NAPPEAR` 把
//! 剩下 22 条字面量与 `state.rs` / `background.rs` / `palette.rs` 的运行时诊断全部搬进
//! `catalog/settings/appearance.rs`，于是中文界面上这一页的**卡片文案**没有一句是英文。
//!
//! 两处例外（M180 复审 L-1 记下的口径）：① 写进 `{reason}` 的**原因串**里，来自本仓库自写的
//! 两条（`"no path configured"`、`"unknown image format …"`）与 OS / 解码库给出的文本仍是英文——
//! 它们是不可翻译的第三方或诊断原文；② `Windows 11` / `macOS` / `Linux` / `PNG` / `JPEG` / `BMP`
//! 这些专名。除此之外这一页不出现英文。
//!
//! ## 为什么「值」与「生效值」分开显示
//!
//! CPU 档下面板 α 被强制 1.0（D36），而用户请求值可能是 0.5。滑块位置按**生效值**画、旁边写
//! 「requested 0.50 → forced 1.00」，这样「为什么拖了没反应」在界面上就有答案，而不是只躺在日志里。

use iced::widget::{button, column, container, row};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding, Shadow, Theme as IcedTheme,
};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::catalog::Key;
use unlucky_i18n::Lang;

use crate::pages::{frame, Message};
use crate::theme::Ui;

use super::state::State;
use super::Change;

mod accent;
mod bg_image;
mod colour_source;
mod panel;
mod window;

/// 「外观」分组整块（一张卡）。
///
/// `lang` 是**这一次生效的语言**：`settings::view` 的 `Language.effective`（M179）。壳把各段按
/// **视觉顺序**拼起来；每一段的语句逐字来自拆分前的 `section()`。
pub fn section<'a>(ui: &'a Ui, state: &'a State, lang: Lang) -> Element<'a, Message> {
    let mut rows: Vec<Element<'a, Message>> = Vec::new();

    // M250：强调色排在最前 —— 它是「配色可自定义」这一页最直接的入口（键一直在，控件是新的）。
    accent::push(ui, state, lang, &mut rows);
    window::push(ui, state, lang, &mut rows);
    panel::push(ui, state, lang, &mut rows);
    bg_image::push(ui, state, lang, &mut rows);
    colour_source::push(ui, state, lang, &mut rows);

    card(
        ui,
        column(rows)
            .spacing(ui.space(Space::X1))
            .width(Length::Fill),
    )
}

/// 外观卡片：底色走 `ui.layer`（面板 α 的唯一来源），与其它页的卡片同源。
fn card<'a>(ui: &'a Ui, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let shape = ui.card_shape();
    let background = ui.layer(MaterialLayer::Card);

    container(content)
        .width(Length::Fill)
        .padding(ui.metrics().card_padding)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(background)),
            border: ui.outlined(shape),
            ..container::Style::default()
        })
        .into()
}

// --------------------------------------------------------------- 行与控件
//
// 五个行控件由各段共用，所以留在壳里（`pub(super)` = 只给 `section/` 下的兄弟模块用）。

pub(super) fn label<'a>(ui: &'a Ui, content: &'a str) -> Element<'a, Message> {
    frame::text_role(
        ui,
        TypeRole::BodyMedium,
        false,
        content,
        ui.color(Role::OnSurface),
    )
}

/// 设置页文案的 i18n 取词口（M116 起新条目一律走词表，不再写字面量）。
///
/// `lang` 是**这一次生效的语言**，由 [`section`] 一路递下来（M179 `I18NSEAM` 之前这里按英文目录
/// 取词，于是中文界面上本分组的 27 条文案里真正本地化 0 条）。现在这个函数是全模块
/// **唯一**的取词点：谁硬编码英文，就不会经过这里——M180 `I18NAPPEAR` 已把这一页剩余的 22 处
/// 字面量与全部运行时诊断搬进词表，见 `catalog/settings/appearance.rs` 的模块文档。
pub(super) fn t(key: Key, lang: Lang) -> &'static str {
    let text = lang.catalog().text(key);
    // 测试专用：把「这一帧真的取了哪条译文」记一份（iced 0.14 的 `Element` 没有可遍历控件树的
    // 公开接口，所以「渲染出来的字是不是中文」只能在取词口取证，见 `appearance::probe`）。
    #[cfg(test)]
    super::probe::record(key, text);
    text
}

/// 带占位符的句子（`{name}` 替换后的 owned 版本）。
///
/// 为什么不直接 `lang.catalog().render(...)`：这条路径也要进 [`t`] 那份取证记录
/// （`probe::record_rendered`），否则「中文界面下这一页每一句都不是纯 ASCII」的守卫会漏掉所有
/// 带数字的说明句。数值参数由调用方先格式化好（词表只认 `{name}`，不认 `{:.2}`）。
pub(super) fn tr(lang: Lang, key: Key, args: &[(&str, &str)]) -> String {
    let text = lang.catalog().render(key, args);
    #[cfg(test)]
    super::probe::record_rendered(key, text.clone());
    text
}

/// 两位小数（`{:.2}` 这种格式说明由 Rust 侧做，词表里只留 `{name}`）。
pub(super) fn fixed2(value: f32) -> String {
    format!("{value:.2}")
}

/// 一条 [`Note`]（key + 已格式化的参数）按语言取整句；与 [`tr`] 共用取证口。
pub(super) fn tr_note(lang: Lang, note: &super::background::Note) -> String {
    tr(lang, note.key(), &note.args())
}

pub(super) fn note_line<'a>(ui: &'a Ui, content: &str, color: Color) -> Element<'a, Message> {
    // 诊断文案是**运行时拼出来的**（路径、错误原因），只能走 owned 版本。
    frame::text_role_owned(ui, TypeRole::BodySmall, false, content.to_owned(), color)
}

/// 一行「标签 + 值 + 滑块 + 说明」。
pub(super) fn slider_row<'a>(
    ui: &'a Ui,
    title: &'a str,
    value: String,
    hint: String,
    enabled: bool,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    let value_color = if enabled {
        ui.color(Role::OnSurface)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };

    let head = row![
        container(label(ui, title))
            .width(Length::Fill)
            .align_y(Alignment::Center),
        frame::text_role_owned(ui, TypeRole::LabelLarge, true, value, value_color),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center);

    column![
        head,
        control,
        frame::text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            hint,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .padding(Padding::from([ui.space(Space::X2), 0.0]))
    .into()
}

/// 一行「标签 + 一排互斥按钮」。
pub(super) fn choice_row<'a>(
    ui: &'a Ui,
    title: &'a str,
    choices: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        container(label(ui, title))
            .width(Length::Fill)
            .align_y(Alignment::Center),
        row(choices)
            .spacing(ui.space(Space::X1))
            .align_y(Alignment::Center),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .padding(Padding::from([ui.space(Space::X2), 0.0]))
    .into()
}

/// 一个互斥按钮（选中态用 `SecondaryContainer`，未选中用透明 + `OnSurfaceVariant`）。
pub(super) fn choice<'a>(
    ui: &'a Ui,
    text: &'a str,
    selected: bool,
    change: Change,
) -> Element<'a, Message> {
    let height = ui.metrics().control_height;
    let content_color = if selected {
        ui.color(Role::OnSecondaryContainer)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };

    button(
        container(frame::text_role(
            ui,
            TypeRole::LabelLarge,
            selected,
            text,
            content_color,
        ))
        .align_y(Alignment::Center),
    )
    .height(height)
    .padding(Padding::from([0.0, ui.space(Space::X3)]))
    .style(move |_theme: &IcedTheme, status| {
        let background = if selected {
            Some(Background::Color(ui.color(Role::SecondaryContainer)))
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            Some(Background::Color(ui.state_over(
                unlucky_design::color::StateLayer::Hover,
                Role::OnSurface,
                Role::Surface,
            )))
        } else {
            None
        };

        button::Style {
            background,
            text_color: content_color,
            border: Border {
                radius: ui.radius(ShapeScale::Full, height),
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        }
    })
    .on_press(Message::Appearance(change))
    .into()
}
