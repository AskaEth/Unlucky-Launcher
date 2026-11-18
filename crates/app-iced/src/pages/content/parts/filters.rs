//! 筛选条：来源 / 排序 / 游戏版本 / 加载器四组 chip，外加分组标签与换行容器。
//!
//! 从 `pages/content/parts.rs` 拆出（那个文件已贴 400 行硬上限）。
//!
//! 缺 key 的源（CurseForge）画成**禁用**而不是消失：筛选条要能被发现，也要能解释自己。
//!
//! ## 两档排法（M358）
//!
//! 同一批 chip、同一批消息，只是**怎么摆**分两档（判据在 `pages/content/head.rs`）：
//!
//! | 档 | 排法 | 高 |
//! |---|---|---|
//! | [`filters`]（常规） | 四组**各占一排**，纵排 | 4 × 32 + 3 × 8 = 152 |
//! | [`filter_strip`]（收敛） | 四组**并成一排**，横向滚动 | `control_height + Space::X2` = 48 |
//!
//! 为什么收敛档能省 104：**面积没变，行数变了**。四排的 152 里 24 是排间距，128 才是 chip——
//! 并成一排后 chip 仍是 32 高的一行，剩下的只有那一行的高度。

use iced::widget::{column, container, row, scrollable};
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

/// 四组 chip 的原始形态（只有 vec，怎么摆由调用方决定）。
///
/// 抽出来是为了让 [`filters`] 与 [`filter_strip`] **共用同一批控件**：两档里同一个 chip
/// 的标签、选中态、消息都一样，差别只在容器。自己再拼一遍就是两套筛选条（改一处忘一处）。
struct Groups<'a> {
    versions: Vec<Element<'a, PageMessage>>,
    loaders: Vec<Element<'a, PageMessage>>,
    source: Vec<Element<'a, PageMessage>>,
    sort: Vec<Element<'a, PageMessage>>,
}

fn groups<'a>(ui: &'a Ui, catalog: Catalog, model: &'a Content) -> Groups<'a> {
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

    Groups {
        versions,
        loaders,
        source,
        sort,
    }
}

/// 常规档：四组**各占一排**，纵排（高窗口的版面，M109 起的现状）。
///
/// 组内是一行 `row`（[`wrapped_row`]）：**不做自动换行**——屏幕太窄时 chip 会被裁掉，
/// 而不是折到第二行（`row` 没有 wrap 能力）。这是既定行为，别把这条注释读成「会自动换行」；
/// 宽度由列表列下限 424（[`super::layout::compact_min`]）兜住，正常窗口下不会撞到。
pub(crate) fn filters<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
) -> Element<'a, PageMessage> {
    let groups = groups(ui, catalog, model);

    let mut column = column![
        labelled(ui, catalog.text(Key::ContentFilterVersion), groups.versions),
        labelled(ui, catalog.text(Key::ContentFilterLoader), groups.loaders),
        wrapped_row(ui, groups.source),
        wrapped_row(ui, groups.sort),
    ]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill);

    if let Some(note) = source_notice(ui, catalog, model) {
        column = column.push(note);
    }
    column.into()
}

/// 收敛档（M358）：四组**并成一排**，整条横向滚动。
///
/// 只在「正文装不下『常规列头 + 一张整卡』」时用（判据在 `pages/content/head.rs`）。
/// 为什么是横向滚动而不是「只画前面几组」：`versions` 里可能是 20 个游戏版本，裁掉的那些
/// **永远点不到**（四排的现状就是如此——每排都按可用宽裁）。横向滚动让每一组都可达，
/// 代价只是要先横向拖一下；滚动条常显，这就是「还能往右」的可见承诺。
///
/// 高度取 `control_height + Space::X2`：与下载页 `stacked_chips` 的 chips 行**同一条式子**
/// （32 的 chip + 6 的滚动条 + 余量），不是这里新拍的一个数。
pub(crate) fn filter_strip<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
) -> Element<'a, PageMessage> {
    let groups = groups(ui, catalog, model);

    // 组与组之间留 `Space::X4`（比组内 `Space::X1` 宽一档）：来源与排序两组没有分组标签
    // （i18n 没有这两个 key，加 key 是另一个 crate 的事），间隔就是它们唯一的分界。
    let strip = row![
        labelled_shrink(ui, catalog.text(Key::ContentFilterVersion), groups.versions),
        labelled_shrink(ui, catalog.text(Key::ContentFilterLoader), groups.loaders),
        wrapped_row(ui, groups.source),
        wrapped_row(ui, groups.sort),
    ]
    .spacing(ui.space(Space::X4))
    .align_y(Alignment::Center);

    scrollable(strip)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new()
                .width(frame::SCROLLBAR_WIDTH)
                .scroller_width(frame::SCROLLBAR_WIDTH)
                .margin(frame::SCROLLBAR_MARGIN),
        ))
        .height(Length::Fixed(strip_height(ui)))
        .width(Length::Fill)
        .style(frame::scroll_style(ui))
        .into()
}

/// 收敛档筛选条的高度（`control_height` 32/40 那一档 + 一条 `Space::X2` 给滚动条）。
///
/// 与 [`filter_strip`] 里的 `.height(..)` **同一处取值**：列头预算（`head::chrome_of`）读的是
/// 这个函数，盒子和预算不会各算一套。
pub(crate) fn strip_height(ui: &Ui) -> f32 {
    ui.metrics().control_height + ui.space(Space::X2)
}

/// 「来源不可用」那一行（**唯一**定义：常规档排在四排之下，收敛档排在筛选条之下）。
///
/// 两档都要画：这条说的是「CurseForge 需要 API key，所以那两个 chip 是灰的」——
/// 收起到两档里任何一档看不见，用户就只会看到两个点不动的 chip。
pub(crate) fn source_notice<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &Content,
) -> Option<Element<'a, PageMessage>> {
    if model.curseforge {
        return None;
    }
    Some(frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        catalog.text(Key::ContentSourceUnavailable),
        ui.color(Role::OnSurfaceVariant),
    ))
}

/// 分组标签 + 一行 chip（标签左、chip 右，纵向居中对齐）。
pub(crate) fn labelled<'a>(
    ui: &'a Ui,
    label: &'a str,
    chips: Vec<Element<'a, PageMessage>>,
) -> Element<'a, PageMessage> {
    labelled_with(ui, label, chips, Length::Fill)
}

/// 同 [`labelled`]，但按内容宽排（横向滚动的筛选条里用：`Fill` 在无限宽的容器里没有意义）。
fn labelled_shrink<'a>(
    ui: &'a Ui,
    label: &'a str,
    chips: Vec<Element<'a, PageMessage>>,
) -> Element<'a, PageMessage> {
    labelled_with(ui, label, chips, Length::Shrink)
}

fn labelled_with<'a>(
    ui: &'a Ui,
    label: &'a str,
    chips: Vec<Element<'a, PageMessage>>,
    width: Length,
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
    .width(width)
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
