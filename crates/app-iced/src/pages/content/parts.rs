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
use crate::app::components::divider;
use crate::app::components::navigation::list_item::{list_item, ListItem};
use crate::app::components::selection::{chip, Chip};
use crate::theme::Ui;

use super::layout::Rails;
use super::model::{Content, Hit};

mod detail;
mod filters;
/// M358：主列列头的两档（四排筛选 / 一排横向滚动）与它的几何。
pub(crate) mod head;
mod retry;

pub(crate) use detail::detail;
pub(crate) use filters::{filter_strip, filters, labelled, source_notice, strip_height};
pub(crate) use retry::maybe as retry_panel;

use super::Message as ContentMessage;
use crate::app::components::navigation::geometry::ListItemGeometry;

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

/// 结果行的**整行节距**（M357 / P2-2）：每张结果卡按 `ListItem` 自己的几何量出来的高，
/// 加上行距 [`ROW_GAP`]。外层滚动区据此按整行取高（`frame::whole_rows_by`），
/// 于是 880×560 的裁切线落在卡片之间，而不是切在第 2 张卡的首行字中间。
///
/// 为什么不能像版本管理页那样用一条常数节距：结果卡的高度随 `ListItem` 的行数档
/// （`overline` + 标题 + 副标题 = 三行档 76，只有标题时是两行档 60）变。
pub(super) fn row_pitches<'a>(ui: &'a Ui, model: &'a Content) -> impl Iterator<Item = f32> + 'a {
    let geometry = ListItemGeometry::of(ui);
    let gap = ui.space(ROW_GAP);

    model.hits.iter().flat_map(move |hit| {
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
        [geometry.height(item.lines()), gap]
    })
}

/// 结果列表。
///
/// 两档（M161，判据同 [`super::layout::BodyMode`]）：
///
/// * `mode.list_scrolls()`：列表**自己滚**（筛选条常驻，高窗口的常态）；
/// * 否则：行按自然高度铺开，滚动交给外层主列——矮窗口里一行 `ListItem` 不许被压成半行。
///
/// 三样「非结果」的内容都可能落进**同一个**滚动区（判据与算式在 [`super::stack`]）：
///
/// * `lead` / `tail`：接在结果行**之前** / **之后**的块（M374 的导入/导出区，位置由
///   [`super::layout::pack_before_results`] 定）；
/// * `detail`：详情尾块（M361）。它**必须是最后一块** —— 点一条结果之后 `snap_to(END)` 直接
///   落到它身上（[`snap_to_detail`]），中间再塞东西就滚歪了；`RESULTS_SCROLL_ID` 也只挂在这一路
///   （没有详情时挂上去会让 `Wide` / `Narrow` 的 `Select` 莫名其妙地滚列表）。
///
/// 三样都空时这条函数返回的就是 `rows` 本身（与 M358 逐像素一致，不套一层空 column）。
pub(super) fn list<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
    rails: Rails,
    mode: super::layout::BodyMode,
    lead: Vec<Element<'a, PageMessage>>,
    tail: Vec<Element<'a, PageMessage>>,
    detail: Option<Element<'a, PageMessage>>,
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
        let mut blocks: Vec<Element<'a, PageMessage>> = lead;
        blocks.push(rows.into());
        blocks.extend(tail);
        let has_detail = detail.is_some();
        blocks.extend(detail);
        let content: Element<'a, PageMessage> = if blocks.len() == 1 {
            // 只有结果行：不套 column（M358 那一路逐像素不变）
            blocks.remove(0)
        } else {
            column(blocks)
                .spacing(ui.space(Space::X2))
                .width(Length::Fill)
                .into()
        };
        // M361 r2（P2-2）：**只有**带详情尾块的滚动区挂 Id —— 「点一条结果 ⇒ 滚到详情」就靠它
        // （[`snap_to_detail`]）。其余档位没有这个 Id，iced 的 `snap_to` 对没有匹配 Id 的树是
        // 空操作，所以调用侧不必再判档位。
        let mut view = scrollable(content)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new()
                    .width(frame::SCROLLBAR_WIDTH)
                    .scroller_width(frame::SCROLLBAR_WIDTH)
                    .margin(frame::SCROLLBAR_MARGIN),
            ))
            .height(Length::Fill)
            .style(frame::scroll_style(ui));
        if has_detail {
            view = view.id(RESULTS_SCROLL_ID);
        }
        container(view).height(Length::Fill).into()
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

/// 列表滚动区的 Id（M361 r2）：只在**带详情尾块**时挂上（见 [`list`] 的 `Some(tail)` 分支）。
pub(crate) const RESULTS_SCROLL_ID: &str = "content-results";

/// 点一条结果 ⇒ 把列表滚到末尾，让详情块与「装入」按钮进入视野（M361 r2，复审 P2-2）。
///
/// ## 为什么是无条件派一个 `snap_to(END)`
///
/// 详情是列表滚动内容的**最后一块**，所以「滚到末尾」= 「详情在视野内」：
/// `snap_to(END)` 之后视口落在内容末尾 ⇒ 视野 = 内容最后 `viewport` px。
///
/// * 视口 ≥ 尾块高（分隔线 1 + `X2` + 详情）⇒ **整块详情**都在（880×900：391 ≥ 338）；
/// * 否则 ⇒ 至少末尾 `viewport` px 在，而「装入」按钮是详情块的**最后一项**（40 高，
///   视口下限 131 远大于它）⇒ 按钮一定在视野内（944×720：211，看到的是详情的下半截）。
///
/// 不判档位是刻意的：Id 只挂在带尾块的那个滚动区上，`Wide`/`Narrow`（独立详情列）与
/// `ScrollWhole`（整列滚动区）里没有这个 Id，iced 的 `snap_to` 对**没有匹配 Id** 的树是空操作
/// ⇒ 那两档的滚动手感一个像素都不变，update 侧也就不必把档位几何再算一遍（两份真相必然漂）。
pub(crate) fn snap_to_detail() -> iced::Task<PageMessage> {
    use iced::advanced::widget::operation::scrollable::{snap_to, RelativeOffset};
    use iced::widget::Id;

    iced::advanced::widget::operate(snap_to(
        Id::new(RESULTS_SCROLL_ID),
        RelativeOffset::END.into(),
    ))
}

/// 没有独立详情列时，接在结果行**之后**的详情尾块（M361）：分隔线 + 详情。
///
/// 它与结果行同处一个滚动区（[`list`] 的 `tail`），所以详情不再和列表抢主列的高度：
/// 修前两者挤在同一个 column 里，详情（高随数据变、没有上界）能把 `Fill` 的列表压成 0
/// （880×900 实测 45px、944×720 实测 0）。算式与代价见 [`super::stack`]。
pub(super) fn detail_tail<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
) -> Element<'a, PageMessage> {
    column![divider::full(ui), detail(ui, catalog, model)]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill)
        .into()
}

/// 导入/导出区在列表滚动区里的那一块（M374）：分隔线 + 那块区本身。
///
/// 分隔线**永远在块的前面**：那块区在滚动区里有两个位置（结果之前 / 结果之后），共用一个形状
/// 才不会与详情尾块自带的那条分隔线撞成双线。
///
/// **右边留 `SCROLLBAR_WIDTH + SCROLLBAR_MARGIN` 的空白**（M374 r2 的 P2-2）：iced 的纵向滚动条是
/// **盖在内容上**的，不预留这 8px，进度行右对齐的标签就会被滚动条压住最后一个字
/// （`after/content-880x900-modpack-importing.png` 修前那版切在 x≈755）。这块区在主列里时本来就
/// 占满 `rails.list`，进滚动区之后少 8px 是它的真实可用宽 —— 与其让文字钻到滚动条底下，不如
/// 明确把那 8px 让出去。
pub(super) fn pack_block<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
) -> Element<'a, PageMessage> {
    let bar = frame::SCROLLBAR_WIDTH + frame::SCROLLBAR_MARGIN;
    column![
        divider::full(ui),
        container(super::packui::section(ui, catalog, &model.pack)).padding(Padding {
            top: 0.0,
            right: bar,
            bottom: 0.0,
            left: 0.0,
        }),
    ]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill)
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
