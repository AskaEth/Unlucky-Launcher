//! 内容库页的零件：筛选条 / 结果列表 / 详情面板 / 四态（加载 / 空 / 失败 / 无更多）/ 一级切换。
//!
//! 全是既有真组件（`chip` / `list_item` / `buttons` / `empty_state`），页面不自己画状态层。
//!
//! ## 消息口径
//!
//! 本文件的每个控件都直接发 [`crate::pages::Message`]：内容库自己的意图包一层
//! `PageMessage::Content(..)`（[`intent`]），而「取消安装 / 打开日志」用的是**下载页就有的**
//! 那两个变体——因为对用户来说它们是同一件事（同一个 `AppCommand::CancelInstall`）。

use iced::widget::{column, container, row, scrollable};
use iced::{Alignment, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::ContentSort;
use unlucky_presenter::router::Route;

use crate::app::components::buttons::{self, Button};
use crate::app::components::navigation::list_item::{list_item, ListItem};
use crate::app::components::selection::{chip, Chip};
use crate::theme::Ui;

use super::layout::Rails;
use super::model::{Content, Hit};

mod detail;
mod filters;
mod retry;

pub(crate) use detail::detail;
pub(crate) use filters::{filters, labelled};
pub(crate) use retry::maybe as retry_panel;

use super::Message as ContentMessage;
use crate::pages::frame;
use crate::pages::Message as PageMessage;

/// 内容库意图 → 页面消息（唯一的一层包装）。
pub(super) fn intent(message: ContentMessage) -> PageMessage {
    PageMessage::Content(message)
}

/// 结果行与行之间的间距（与下载页同值）。
const ROW_GAP: Space = Space::X1;

/// 一级切换：版本管理 | 内容库（M109 按 B1 裁定：共用同一条导航轨项，页内切换）。
pub(super) fn section_switcher<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    on_content: bool,
) -> Element<'a, PageMessage> {
    let versions = Chip::filter(
        catalog.text(Key::ContentSectionVersions),
        !on_content,
        PageMessage::Navigate(Route::Download),
    );
    let content = Chip::filter(
        catalog.text(Key::ContentSectionContent),
        on_content,
        PageMessage::Navigate(Route::Content),
    );
    row![chip(ui, versions), chip(ui, content)]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center)
        .into()
}

/// 关键词搜索条（输入只改草稿；回车/按钮才发检索指令）。
pub(super) fn search<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
) -> Element<'a, PageMessage> {
    // 输入只改草稿，**提交**才发检索（每敲一个字就打一次网络既没必要也会被限流）。
    // 搜索条自身没有提交回调，所以旁边给一个显式的「搜索」按钮——这也是键盘外的唯一出口。
    let bar = crate::app::components::inputs::search_bar(
        ui,
        crate::app::components::inputs::SearchBar::new(model.draft.as_str())
            .placeholder(catalog.text(Key::ContentSearchPlaceholder)),
        move |text| intent(ContentMessage::QueryChanged(text)),
    );
    let submit = crate::app::components::buttons::text(
        ui,
        crate::app::components::buttons::Button::new(
            catalog.text(Key::ContentSearch),
            intent(ContentMessage::Search),
        )
        .leading(IconKind::Search),
    );
    row![bar, submit]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

/// 结果列表。
///
/// 两档（M161，判据同 [`super::layout::BodyMode`]）：
///
/// * `mode.list_scrolls()`：列表**自己滚**（筛选条常驻，高窗口的常态）；
/// * 否则：行按自然高度铺开，滚动交给外层主列——矮窗口里一行 `ListItem` 不许被压成半行。
pub(super) fn list<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
    rails: Rails,
    mode: super::layout::BodyMode,
) -> Element<'a, PageMessage> {
    let rows = column(
        model
            .hits
            .iter()
            .map(|hit| hit_row(ui, model, hit))
            .collect::<Vec<_>>(),
    )
    .spacing(ui.space(ROW_GAP))
    .padding(Padding::from([0.0, ui.space(Space::X2)]))
    .width(Length::Fill);

    let _ = catalog;
    let inner: Element<'a, PageMessage> = if mode.list_scrolls() {
        container(
            scrollable(rows)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new()
                        .width(frame::SCROLLBAR_WIDTH)
                        .scroller_width(frame::SCROLLBAR_WIDTH)
                        .margin(frame::SCROLLBAR_MARGIN),
                ))
                .height(Length::Fill)
                .style(frame::scroll_style(ui)),
        )
        .height(Length::Fill)
        .into()
    } else {
        // 自然高度（不给 `Fill`）：外层主列的那个滚动条负责滚。
        rows.into()
    };
    container(inner)
        .width(Length::Fixed(rails.list))
        .height(if mode.list_scrolls() {
            Length::Fill
        } else {
            Length::Shrink
        })
        .into()
}

fn hit_row<'a>(ui: &'a Ui, model: &Content, hit: &'a Hit) -> Element<'a, PageMessage> {
    let mut item = ListItem::new(
        hit.title.as_str(),
        intent(ContentMessage::Select(hit.id.clone())),
    )
    .overline(hit.supporting.as_str())
    .supporting(hit.summary.as_str())
    .leading(kind_icon(model.kind));
    if model.selected.as_deref() == Some(hit.id.as_str()) {
        item = item.selected();
    }
    list_item(ui, item)
}

/// 计数头 + 翻页尾（分页而非无限滚动：见 `pages/content.rs` 的模块文档）。
pub(super) fn count_head<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &Content,
) -> Element<'a, PageMessage> {
    let count = if model.total > 0 {
        model.total
    } else {
        u32::try_from(model.hits.len()).unwrap_or(u32::MAX)
    };
    frame::text_role_owned(
        ui,
        TypeRole::LabelMedium,
        false,
        catalog.render(
            Key::ContentCount,
            &[("count", &super::model::thousands(u64::from(count)))],
        ),
        ui.color(Role::OnSurfaceVariant),
    )
}

/// 列表尾部：还有更多 ⇒「加载更多」；没有了 ⇒ 一句「没有更多了」。
pub(super) fn more<'a>(ui: &'a Ui, catalog: Catalog, model: &Content) -> Element<'a, PageMessage> {
    if model.has_more {
        return buttons::text(
            ui,
            Button::new(
                catalog.text(Key::ContentLoadMore),
                intent(ContentMessage::LoadMore),
            )
            .leading(IconKind::ChevronDown),
        );
    }
    frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        catalog.text(Key::ContentNoMore),
        ui.color(Role::OnSurfaceVariant),
    )
}

/// 内容类型 → 图标（**借用**现有字形：图标集里还没有「整合包/Mod/资源包/光影」四枚专用图标，
/// 已在报告里登记为偏离）。
pub(super) const fn kind_icon(kind: unlucky_presenter::dto::ContentKind) -> IconKind {
    // 两条硬约束（都来自门禁与复审）：
    // ① `Play`/`Stop`/`More` 是**填充**字形，pages 里禁用（`tests/no_hardcoded_tokens`）；
    // ② **不许与全局导航撞形**：`Instances`/`Download`/`Logs`/`Settings`/`Ai` 五枚在同一个屏上
    //    就在左边那条轨里，页内四类再借用它们会让「同形不同义」（复审 P2-3）。
    //
    // 剩下的描边字形里挑语义最近的：
    // `Folder`（包=文件夹）/ `Plus`（加装内容=加号）/ `Restore`（双层方框≈贴图分层）/ `Expand`（四散光线）。
    match kind {
        unlucky_presenter::dto::ContentKind::Modpack => IconKind::Folder,
        unlucky_presenter::dto::ContentKind::Mod => IconKind::Plus,
        unlucky_presenter::dto::ContentKind::Resourcepack => IconKind::Restore,
        unlucky_presenter::dto::ContentKind::Shader => IconKind::Expand,
    }
}

pub(super) fn sort_label(sort: ContentSort, catalog: Catalog) -> &'static str {
    match sort {
        ContentSort::Relevance => catalog.text(Key::ContentSortRelevance),
        ContentSort::Downloads => catalog.text(Key::ContentSortDownloads),
        ContentSort::Newest => catalog.text(Key::ContentSortNewest),
        ContentSort::Updated => catalog.text(Key::ContentSortUpdated),
    }
}

mod status;

pub(crate) use status::status_body;
