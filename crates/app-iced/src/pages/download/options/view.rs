//! 选项区的控件树（M105）。
//!
//! ## 版面（424 正文列里的四段）
//!
//! ```text
//! 1.21.9                                          [安装]
//! 安装选项
//! [原版][Forge][NeoForge]
//! [Fabric][Quilt]
//! 加载器版本            ← 只在选了加载器时出现
//! ┌ 0.16.5            ┐  ← 官方那份列表（menu + 滚动，最多看两行）
//! │ 0.16.4            │
//! └───────────────────┘
//! [实例名                ]
//! NeoForge 需要 Minecraft 1.20.1 或更新      ← 不兼容时（Error 色）
//! ```
//!
//! ## 为什么加载器分成两行
//!
//! 五家一起排在 424 宽里最宽的那组（`原版 + Forge + NeoForge + Fabric + Quilt`）实测会溢出，
//! 而 chip 的几何是组件自己的真源（`ChipGeometry`），页面不许压它 ⇒ 折成两行。
//! 顺序仍是任务书的口径（原版 / Forge / NeoForge / Fabric / Quilt），只是换行。
//!
//! ## 为什么「不兼容」不禁用 chip、而是禁用确认键
//!
//! 禁用的 chip 点不动，用户就**看不到原因**（那正是「选完才失败」的另一种形态）。
//! 现在的做法：五家永远都能点，选中不兼容的那家时下面出现一行具名原因，同时「安装」变成禁用态。
//! 于是「为什么不能装」和「怎么才能装」永远同屏。

use iced::widget::{column, container, row, scrollable};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::app::components::inputs::{menu, Menu, MenuItem};
use crate::app::components::inputs::{text_field, Field, FieldVariant};
use crate::app::components::selection::{chip, Chip};
use crate::theme::Ui;

use super::super::frame;
use super::super::Message;
use super::model::{Loader, LoaderVersionsView, OptionsView};

/// 五家加载器（一行放得下：实测五枚 chip 合计约 311 dp，正文列 424）。
const LOADERS: [Loader; 5] = [
    Loader::Vanilla,
    Loader::Forge,
    Loader::NeoForge,
    Loader::Fabric,
    Loader::Quilt,
];

/// 菜单项高度 = 48（`MenuGeometry` 的项高）×2 行 + 上下留白 8×2 = 112。
/// 这么算是为了避免「列表被截掉半行」——滚动区高度必须覆盖整行。
const VERSION_ROWS: f32 = 7.0;

/// 选项区（面板的下半块）。
pub(crate) fn section<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    options: &'a OptionsView,
) -> Element<'a, Message> {
    let head = row![
        container(frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            options.version.as_str(),
            ui.color(Role::OnSurface),
        ))
        .width(Length::Fill),
        confirm(ui, catalog, options),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center);

    let mut body = column![
        head,
        frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            catalog.text(Key::DownloadOptionsTitle),
            ui.color(Role::OnSurfaceVariant),
        ),
        loader_row(ui, catalog, options),
    ]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill);

    // 「加载器版本」这一段只在**真该有列表**时出现：原版没有、静态不兼容没有（原因在下面那行）、
    // 取不到也没有（同样在原因行）。留一个空标题只会让人以为界面缺了一块。
    if show_versions(options) {
        body = body.push(frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            catalog.text(Key::DownloadOptionsLoaderVersion),
            ui.color(Role::OnSurfaceVariant),
        ));
        body = body.push(version_list(ui, catalog, options));
    }

    body = body.push(text_field(
        ui,
        Field::new(options.instance_name.as_str())
            .variant(FieldVariant::Outlined)
            .label(catalog.text(Key::DownloadOptionsInstanceName))
            .placeholder(catalog.text(Key::DownloadOptionsInstanceNameHint)),
        Message::SetInstanceName,
    ));

    if let Some(problem) = &options.problem {
        body = body.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            problem.as_str(),
            ui.color(Role::Error),
        ));
    }

    body.into()
}

/// 加载器版本这一段要不要画（见 `section` 的注释）。
fn show_versions(options: &OptionsView) -> bool {
    if options.loader == Loader::Vanilla || options.problem.is_some() {
        return false;
    }
    match &options.versions {
        LoaderVersionsView::Loading => true,
        LoaderVersionsView::Ready(items) => !items.is_empty(),
        LoaderVersionsView::Failed(_) => false,
    }
}

/// 一行加载器 chip（Filter 档：选中 = `SecondaryContainer`）。
fn loader_row<'a>(ui: &'a Ui, catalog: Catalog, options: &'a OptionsView) -> Element<'a, Message> {
    let mut line = row![]
        .spacing(ui.space(Space::X1))
        .align_y(Alignment::Center);
    for loader in LOADERS.iter() {
        line = line.push(chip(
            ui,
            Chip::filter(
                loader.label(catalog),
                options.loader == *loader,
                Message::SetLoader(*loader),
            ),
        ));
    }
    line.into()
}

/// 官方那份加载器版本列表（`menu` + 滚动：Fabric 有上百条，不做下拉也不铺满屏）。
fn version_list<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    options: &'a OptionsView,
) -> Element<'a, Message> {
    match &options.versions {
        LoaderVersionsView::Loading => frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::DownloadOptionsLoaderVersionsLoading),
            ui.color(Role::OnSurfaceVariant),
        ),
        // 取不到 / 一条都没有：原因已经在上面那行 `problem` 里说清，这里不再重复。
        LoaderVersionsView::Failed(_) => frame::gap(0.0),
        LoaderVersionsView::Ready(items) if items.is_empty() => frame::gap(0.0),
        LoaderVersionsView::Ready(items) => {
            let mut list = Menu::new().width(Length::Fill);
            for item in items {
                let mut entry =
                    MenuItem::new(item.as_str(), Message::SetLoaderVersion(item.clone()));
                if *item == options.loader_version {
                    entry = entry.selected();
                }
                list = list.item(entry);
            }
            scrollable(menu(ui, list))
                .height(Length::Fixed(ui.space(Space::X4) * VERSION_ROWS))
                .style(frame::scroll_style(ui))
                .into()
        }
    }
}

/// 「安装」：不兼容 / 列表没到位 / 已装 都按下不去（禁用态保留几何，只清消息）。
fn confirm<'a>(ui: &'a Ui, catalog: Catalog, options: &'a OptionsView) -> Element<'a, Message> {
    if options.installed {
        return buttons::outlined(ui, Button::disabled(catalog.text(Key::DownloadInstalled)));
    }
    if !options.can_confirm {
        return buttons::outlined(ui, Button::disabled(catalog.text(Key::DownloadInstall)));
    }
    buttons::filled(
        ui,
        Button::new(
            catalog.text(Key::DownloadInstall),
            Message::InstallVersion(options.version.clone()),
        )
        .leading(IconKind::Download),
    )
}
