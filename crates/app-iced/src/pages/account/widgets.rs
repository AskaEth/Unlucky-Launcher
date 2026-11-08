//! 本页小件：头像块（首字母块 / 真图）、章节标题、轨道徽标。
//!
//! ## 令牌纪律
//!
//! 本文件**零**色值 / 字号 / 半径字面量：颜色一律 [`Ui::color`]，尺寸走 `Space` /
//! `LayoutMetrics` / `ui.style(..)`。首字母块的「色号」也只是**挑哪一个角色**，
//! 不是一个具体颜色——深浅两套主题各自成立。

use iced::widget::text::LineHeight;
use iced::widget::{container, image, row, text};
use iced::{Alignment, Background, Border, ContentFit, Element, Length, Pixels};

use unlucky_design::color::Role;
use unlucky_design::component_map::Component;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::AccountNoticeCode;

use crate::pages::frame;
use crate::pages::frame::ICON_PX;
use crate::theme::Ui;

use crate::app::components::buttons::{self, Button};

use crate::pages::Message as PageMessage;
use crate::pages::Message;

use super::model::{Hint, Notice, Row};
use super::Message as Account;

/// 头像块的可选尺寸（列表 40 / 当前账户 56）。
pub const AVATAR_LIST: f32 = 40.0;
/// 当前账户卡片上的头像边长。
pub const AVATAR_ACTIVE: f32 = 56.0;

/// 色号 → (底色, 字色) 角色对。
///
/// 只有「挑角色」这一步是硬的：六个色号对应六个容器角色，深浅主题各有一套值，
/// 所以同一个账户在两种主题下都成立，也不会撞出低对比。
fn tone(ui: &Ui, tone: u8) -> (iced::Color, iced::Color) {
    match tone % 6 {
        0 => (
            ui.color(Role::PrimaryContainer),
            ui.color(Role::OnPrimaryContainer),
        ),
        1 => (
            ui.color(Role::SecondaryContainer),
            ui.color(Role::OnSecondaryContainer),
        ),
        2 => (
            ui.color(Role::TertiaryContainer),
            ui.color(Role::OnTertiaryContainer),
        ),
        3 => (
            ui.color(Role::SurfaceContainerHighest),
            ui.color(Role::OnSurface),
        ),
        4 => (ui.color(Role::PrimaryFixed), ui.color(Role::OnPrimaryFixed)),
        _ => (
            ui.color(Role::SurfaceVariant),
            ui.color(Role::OnSurfaceVariant),
        ),
    }
}

/// 提示条的语义档（决定图标块与图标字形用哪一对角色）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// 失败（`ErrorContainer` / `OnErrorContainer` 图标块 + `Error` 图形）
    Error,
    /// 成功/信息（`SecondaryContainer` / `OnSecondaryContainer` 图标块 + `Check` 图形）
    Info,
}

/// **M3 内联提示条**（账户页横幅与登录失败面板共用）。
///
/// ## 为什么不是通栏实心色块（tower r2 意见）
///
/// r1 的失败面板把 `errorContainer` 铺满整条内容宽——角色本身是对的（复审按像素采样反证过：
/// 暗色取到容器角色 30 档、浅色取到 90 档，与设计层逐值一致），但**面积**让它像一条通栏告警带。
/// 现在改成 M3 的 alert 形态：面板底走 `SurfaceContainerHigh` + 1dp 描边，
/// `ErrorContainer` / `OnErrorContainer` 只作左侧**图标块**的强调（成功档换成 Secondary 容器），
/// 标题用 `OnSurface`、正文与 code 行用 `OnSurfaceVariant`。语义仍由角色表达，视觉不再是红条。
pub fn alert<'a>(
    ui: &'a Ui,
    tone: Tone,
    title: &'a str,
    body: String,
    code: Option<String>,
    actions: Vec<Element<'a, PageMessage>>,
) -> Element<'a, PageMessage> {
    let (chip_bg, chip_fg, glyph, glyph_color) = match tone {
        Tone::Error => (
            ui.color(Role::ErrorContainer),
            ui.color(Role::OnErrorContainer),
            IconKind::Error,
            ui.color(Role::Error),
        ),
        Tone::Info => (
            ui.color(Role::SecondaryContainer),
            ui.color(Role::OnSecondaryContainer),
            IconKind::Check,
            ui.color(Role::OnSecondaryContainer),
        ),
    };
    let _ = chip_fg;

    let chip = container(frame::glyph_box(glyph, glyph_color, ICON_PX))
        .width(Length::Fixed(ui.space(Space::X8)))
        .height(Length::Fixed(ui.space(Space::X8)))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(chip_bg)),
            border: Border {
                radius: ui.radius(ShapeScale::Full, ui.space(Space::X8)),
                ..Border::default()
            },
            ..container::Style::default()
        });

    // 标题与 `Code: …` 同**一行**：`Code` 只是取证用的稳定 id，不值得占一整行高度
    // （1180×760 下多一行就把「All accounts」第三行挤到折线下）。
    let mut title_row = iced::widget::Row::new()
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center)
        .push(frame::text_role(
            ui,
            TypeRole::TitleSmall,
            true,
            title,
            ui.color(Role::OnSurface),
        ));
    if let Some(code) = code {
        title_row = title_row.push(frame::text_role_owned(
            ui,
            TypeRole::LabelSmall,
            false,
            code,
            ui.color(Role::OnSurfaceVariant),
        ));
    }

    let mut lines = iced::widget::Column::new().spacing(ui.space(Space::X1));
    lines = lines.push(title_row);
    lines = lines.push(frame::text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        body,
        ui.color(Role::OnSurfaceVariant),
    ));

    container(
        row![
            chip,
            container(lines).width(Length::Fill),
            row(actions)
                .spacing(ui.space(Space::X2))
                .align_y(Alignment::Center),
        ]
        .spacing(ui.space(Space::X3))
        .align_y(Alignment::Center)
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding(ui.metrics().card_padding)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ui.layer(MaterialLayer::Card))),
        border: ui.outlined(Component::OutlinedCard.spec().shape),
        ..container::Style::default()
    })
    .into()
}

/// 账户页横幅：添加/删除/切换的结果（**页面上真实可见的控件**，不是 toast）。
///
/// 为什么必须有它：app-iced 不渲染 toast（复审 r1 P2-2），离线名字不合法/重名/落盘失败时
/// 用户原本什么都看不到。文案按 `AccountNoticeCode` 查 i18n，后端原文不进 UI。
pub fn banner<'a>(ui: &'a Ui, notice: &'a Notice, catalog: Catalog) -> Element<'a, PageMessage> {
    let tone = if notice.failure {
        Tone::Error
    } else {
        Tone::Info
    };
    let title = if notice.failure {
        catalog.text(Key::AccountNoticeTitleFailed)
    } else {
        catalog.text(Key::AccountNoticeTitleDone)
    };
    let message = notice_text(notice, catalog);
    let dismiss = buttons::text(
        ui,
        Button::new(
            catalog.text(Key::AccountNoticeDismiss),
            Account::DismissNotice.into_page(),
        ),
    );
    // 只有失败才显示稳定 code（成功态列一串 `added` 是噪音）
    let code = notice
        .failure
        .then(|| catalog.render(Key::AccountFailureCode, &[("code", notice.code.code())]));
    alert(ui, tone, title, message, code, vec![dismiss])
}

/// 横幅文案（码 → i18n；主题名拼进带占位符的几句）。
fn notice_text(notice: &Notice, catalog: Catalog) -> String {
    let name = notice.subject.as_str();
    match notice.code {
        AccountNoticeCode::Added => catalog.render(Key::AccountNoticeAdded, &[("name", name)]),
        AccountNoticeCode::Removed => catalog.render(Key::AccountNoticeRemoved, &[("name", name)]),
        AccountNoticeCode::Switched => {
            catalog.render(Key::AccountNoticeSwitched, &[("name", name)])
        }
        AccountNoticeCode::OfflineNameInvalid => catalog
            .text(Key::AccountNoticeOfflineNameInvalid)
            .to_string(),
        AccountNoticeCode::AccountExists => {
            catalog.text(Key::AccountNoticeAccountExists).to_string()
        }
        AccountNoticeCode::StoreFailed => catalog.text(Key::AccountNoticeStoreFailed).to_string(),
        AccountNoticeCode::NotFound => catalog.text(Key::AccountNoticeNotFound).to_string(),
        AccountNoticeCode::Other => catalog.text(Key::AccountNoticeOther).to_string(),
    }
}

/// 一次性引导（重新登录时告诉用户下一步）。
pub fn hint<'a>(
    ui: &'a Ui,
    hint: Hint,
    page: &'a super::Page,
    catalog: Catalog,
) -> Element<'a, PageMessage> {
    let message = match hint {
        Hint::AuthlibReauth => catalog.render(
            Key::AccountHintAuthlibReauth,
            &[("server", page.form.server.as_str())],
        ),
        Hint::OfflineNoRelogin => catalog.text(Key::AccountHintOfflineNoRelogin).to_string(),
    };
    container(frame::text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        message,
        ui.color(Role::OnSurfaceVariant),
    ))
    .width(Length::Fill)
    .into()
}

/// 头像块：有真图就画图（圆角裁切），没有就画**首字母块**。
///
/// 三条铁律：① 图解码失败一定回到首字母块（不白块）；② 首字母块的字来自 `AccountView::monogram`，
/// 永不空串；③ 两者尺寸完全一致，切换时不跳版。
pub fn avatar<'a>(ui: &'a Ui, row: &Row, size: f32) -> Element<'a, Message> {
    if let Some(handle) = &row.head {
        return container(
            image(handle.clone())
                .width(Length::Fixed(size))
                .height(Length::Fixed(size))
                .content_fit(ContentFit::Cover),
        )
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_theme| container::Style {
            border: Border {
                radius: ui.radius(ShapeScale::Full, size),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into();
    }
    monogram(ui, &row.monogram, row.tone, size)
}

/// 首字母块（没有皮肤时的头像位）。
pub fn monogram<'a>(ui: &'a Ui, letter: &str, tone_index: u8, size: f32) -> Element<'a, Message> {
    let (background, foreground) = tone(ui, tone_index);
    let style = ui.emphasized(TypeRole::TitleMedium);
    container(
        text(letter.to_string())
            .size(style.size_px)
            .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
            .font(ui.font_of(style))
            .color(foreground),
    )
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            radius: ui.radius(ShapeScale::Full, size),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// 章节标题（`TitleMedium` + 可选副文案）。
pub fn section<'a>(ui: &'a Ui, title: &'a str, support: Option<&'a str>) -> Element<'a, Message> {
    let mut column = iced::widget::column![frame::text_role(
        ui,
        TypeRole::TitleMedium,
        true,
        title,
        ui.color(Role::OnSurface),
    )]
    .spacing(ui.space(Space::X1));
    if let Some(support) = support {
        column = column.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            support,
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    column.into()
}

/// 轨道徽标（小药丸：`LabelMedium` + `SecondaryContainer`）。
pub fn badge<'a>(ui: &'a Ui, label: &'a str, active: bool) -> Element<'a, Message> {
    let (background, foreground) = if active {
        (
            ui.color(Role::SecondaryContainer),
            ui.color(Role::OnSecondaryContainer),
        )
    } else {
        (
            ui.color(Role::SurfaceContainerHighest),
            ui.color(Role::OnSurfaceVariant),
        )
    };
    let style = ui.style(TypeRole::LabelMedium);
    container(
        text(label.to_string())
            .size(style.size_px)
            .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
            .font(ui.font_of(style))
            .color(foreground),
    )
    .padding(iced::Padding::from([0.0, ui.space(Space::X2)]))
    .height(ui.metrics().control_height - ui.space(Space::X2) * 2.0)
    .align_y(Alignment::Center)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            radius: ui.radius(ShapeScale::Full, ui.metrics().control_height),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// 一条「标签 + 值」的元信息行（列表行副文案之外的结构化信息）。
pub fn meta_line<'a>(ui: &'a Ui, label: &'a str, value: &'a str) -> Element<'a, Message> {
    row![
        frame::text_role(
            ui,
            TypeRole::LabelSmall,
            false,
            label,
            ui.color(Role::OnSurfaceVariant),
        ),
        frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            value,
            ui.color(Role::OnSurface)
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center)
    .into()
}
