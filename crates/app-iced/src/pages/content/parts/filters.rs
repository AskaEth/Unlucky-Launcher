//! 筛选条：来源 / 排序 / 游戏版本 / 加载器四组 chip，外加分组标签与换行容器。
//!
//! 从 `pages/content/parts.rs` 拆出（那个文件已贴 400 行硬上限）。
//!
//! 缺 key 的源（CurseForge）画成**禁用**而不是消失：筛选条要能被发现，也要能解释自己。

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{loader_facet, ContentSort, ContentSource, LoaderKind};

use crate::app::components::selection::{chip, Chip};
use crate::theme::Ui;

use super::super::model::{loader_name, Content};
use super::super::Message as ContentMessage;
use super::{intent, sort_label};
use crate::pages::frame;
use crate::pages::Message as PageMessage;

/// 来源 + 排序 + 版本 + 加载器四组 chip（**每组一行**，组与组之间纵向排）。
///
/// 组内是一行 `row`（[`wrapped_row`]）：**不做自动换行**——屏幕太窄时 chip 会被裁掉，
/// 而不是折到第二行（`row` 没有 wrap 能力）。这是既定行为，别把这条注释读成「会自动换行」；
/// 宽度由列表列下限 424（[`super::layout::compact_min`]）兜住，正常窗口下不会撞到。
pub(crate) fn filters<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
) -> Element<'a, PageMessage> {
    let mut source = Vec::new();
    for item in ContentSource::ALL {
        let available = match item {
            ContentSource::Modrinth => true,
            ContentSource::Curseforge => model.curseforge,
        };
        let label = match item {
            ContentSource::Modrinth => catalog.text(Key::ContentSourceModrinth),
            ContentSource::Curseforge => catalog.text(Key::ContentSourceCurseforge),
        };
        let mut spec = Chip::filter(
            label,
            model.query.source == item,
            intent(ContentMessage::SetSource(item)),
        );
        if !available {
            // 缺 key 的源：**禁用并说明**，不是静默消失（否则用户找不到它）
            spec = spec.disabled();
        }
        source.push(chip(ui, spec));
    }

    let mut sort = Vec::new();
    for item in ContentSort::ALL {
        sort.push(chip(
            ui,
            Chip::filter(
                sort_label(item, catalog),
                model.query.sort == item,
                intent(ContentMessage::SetSort(item)),
            ),
        ));
    }

    let mut versions = vec![chip(
        ui,
        Chip::filter(
            catalog.text(Key::ContentFilterAny),
            model.query.version.is_none(),
            intent(ContentMessage::SetVersion(None)),
        ),
    )];
    for version in &model.versions {
        versions.push(chip(
            ui,
            Chip::filter(
                version.as_str(),
                model.query.version.as_deref() == Some(version.as_str()),
                intent(ContentMessage::SetVersion(Some(version.clone()))),
            ),
        ));
    }

    let mut loaders = vec![chip(
        ui,
        Chip::filter(
            catalog.text(Key::ContentFilterAny),
            model.query.loader.is_none(),
            intent(ContentMessage::SetLoader(None)),
        ),
    )];
    for loader in [
        LoaderKind::Fabric,
        LoaderKind::Forge,
        LoaderKind::NeoForge,
        LoaderKind::Quilt,
    ] {
        let usable = loader_facet(loader).is_some();
        let mut spec = Chip::filter(
            loader_name(loader),
            model.query.loader == Some(loader),
            intent(ContentMessage::SetLoader(Some(loader))),
        );
        if !usable {
            spec = spec.disabled();
        }
        loaders.push(chip(ui, spec));
    }

    let mut column = column![
        labelled(ui, catalog.text(Key::ContentFilterVersion), versions),
        labelled(ui, catalog.text(Key::ContentFilterLoader), loaders),
        wrapped_row(ui, source),
        wrapped_row(ui, sort),
    ]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill);

    if !model.curseforge {
        column = column.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::ContentSourceUnavailable),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    column.into()
}

/// 分组标签 + 一行 chip（标签左、chip 右，纵向居中对齐）。
pub(crate) fn labelled<'a>(
    ui: &'a Ui,
    label: &'a str,
    chips: Vec<Element<'a, PageMessage>>,
) -> Element<'a, PageMessage> {
    row![
        container(frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            label,
            ui.color(Role::OnSurfaceVariant),
        )),
        wrapped_row(ui, chips),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

/// `Vec<Element>` → 一行（顺序就是声明顺序）。
pub(crate) fn wrapped_row<'a>(
    ui: &'a Ui,
    chips: Vec<Element<'a, PageMessage>>,
) -> Element<'a, PageMessage> {
    row(chips)
        .spacing(ui.space(Space::X1))
        .align_y(Alignment::Center)
        .into()
}
