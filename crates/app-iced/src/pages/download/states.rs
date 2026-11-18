//! 占位状态（加载 / 空）：**同一套占位规则——内容在可用区居中**（M94 r1 / P2-1）。
//!
//! | 状态 | 条件（都能从快照推出来） | 画什么 |
//! |---|---|---|
//! | 加载 | `model.is_loading()`：快照里还没有版本列表 | 标题 + 一句说明（**没有任何进度形状**） |
//! | 空 | `model.is_empty()`：列表拉到了，但过滤后一行不剩 | 真 [`empty_state`]（图标块 96 + 24 标题 + 说明 + 刷新 CTA） |
//!
//! ## 为什么是「都居中」（P2-1）
//!
//! `components::feedback::empty_state` 的既有口径（`12b §15.4`）就是「内容在可用区居中」；
//! 加载态原先贴顶，等于同一个页面里两套占位规则，两态互切时内容会纵向跳一次。所以本轮把加载态
//! 也放进同一条规则里。**有数据时列表仍然贴顶**——那是有内容的状态，不属于占位规则。
//!
//! ## 为什么加载态没有进度条（P2-3）
//!
//! 页面**没有帧订阅**（`app/subscription.rs` 的 `needs_frames` 归 app 层），所以
//! `Progress::indeterminate(phase)` 拿到的是一个**静止的**相位——用户看到的是「17% 卡住」。
//! 与其留一个「看起来有进度但不动」的控件，不如给一句明确的「正在加载」文字；等 app 层真有了
//! 帧订阅再来带动画（那时这里换回 indeterminate 才有意义）。
//!
//! ## 文案口径（P2-2）
//!
//! 只说页面真能做到的动作：说明里那句「点右上角的刷新」对应页头那个真的 Refresh 按钮
//! （`Message::Action(Action::RefreshSources)`）；没有写「稍后会自动重试」这种 UI 做不到的承诺。

use iced::widget::{column, container};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::app::components::feedback::{empty_state, EmptyState};
use crate::theme::Ui;

use super::super::frame;
use super::super::{Action, Message};
use super::model::Category;

/// 空态图标块边长（12b §15.1：**96×96**，与 `components::feedback::EmptyState` 内部同一个项目值；
/// 那里是 `frame::pieces::EMPTY_ICON_BLOCK`（`pub(super)`，本模块取不到，只能同值声明一次）。
const EMPTY_ICON_BLOCK: f32 = 96.0;

/// 两个占位状态共用的外壳：水平 + 垂直都居中（见模块文档的两条理由）。
fn centered<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

/// 加载态：一句「正在加载」+ 一句这个页面真能做的事（点刷新）。
pub(super) fn loading<'a>(ui: &'a Ui, catalog: Catalog) -> Element<'a, Message> {
    centered(
        column![
            frame::text_role_centered(
                ui,
                TypeRole::TitleMedium,
                true,
                catalog.text(Key::DownloadLoading),
                ui.color(Role::OnSurface),
            ),
            frame::text_role_centered(
                ui,
                TypeRole::BodySmall,
                false,
                catalog.text(Key::DownloadLoadingHint),
                ui.color(Role::OnSurfaceVariant),
            ),
        ]
        .spacing(ui.space(Space::X3))
        .width(Length::Fill)
        .align_x(Alignment::Center),
    )
}

/// 空态（真 `EmptyState`：CTA 就是标准 Filled 按钮，走按钮家族）。
pub(super) fn empty<'a>(ui: &'a Ui, catalog: Catalog) -> Element<'a, Message> {
    centered(empty_state(
        ui,
        EmptyState::new(
            IconKind::Download,
            catalog.text(Key::DownloadEmptyTitle),
            catalog.text(Key::DownloadEmptyBody),
        )
        .action(
            catalog.text(Key::DownloadRefresh),
            Message::Action(Action::RefreshSources),
        ),
    ))
}

/// 搜索**一个都不命中**（跨类型也没有）：说「没有匹配」+ 一句「换短一点的片段」+ 清空入口。
///
/// 与 [`empty`] 分开的理由：那个是「这个分类里没有版本」，这个是「这个词没有任何版本」——
/// 两种情况该给的下一步动作不同（前者换分类/刷新，后者改搜索词）。
pub(super) fn no_match<'a>(ui: &'a Ui, catalog: Catalog) -> Element<'a, Message> {
    centered(empty_state(
        ui,
        EmptyState::new(
            IconKind::Search,
            catalog.text(Key::VersionSearchEmptyTitle),
            catalog.text(Key::VersionSearchEmptyBody),
        )
        .action(
            catalog.text(Key::VersionSearchClear),
            Message::ClearVersionQuery,
        ),
    ))
}

/// **有命中但不在当前分类**：说清「N 个在别的类型里」+ 一个「看全部类型」的出口。
///
/// 不给出口的话，用户只能自己猜「是不是要切到快照 tab」——这正是本条的验收口径。
///
/// 标题**不再复用命中数那条 key**（复审 P2-4）：原来同一个数在同屏报三遍
/// （右上角「1 matches」+ 标题「1 matches」+ 正文「1 versions match」），而且单数时英文是错的。
/// 现在标题是一条固定文案，正文的单复数由 [`super::rows::elsewhere_text`] 选 key。
pub(super) fn elsewhere<'a>(ui: &'a Ui, catalog: Catalog, hits: usize) -> Element<'a, Message> {
    let title = catalog.text(Key::VersionSearchElsewhereTitle).to_owned();
    let body = super::rows::elsewhere_text(catalog, hits);

    // 手搭而不是用 `EmptyState`：它的标题/正文借 `&'a str`，而这里的句子是 `render()` 出来的
    // `String`（带命中数），借不到 `'a`。字形、字级、按钮全走同一套令牌，视觉上仍是空态。
    centered(
        column![
            frame::glyph_box(
                IconKind::Search,
                ui.color(Role::OnSurfaceVariant),
                EMPTY_ICON_BLOCK
            ),
            frame::text_role_owned(
                ui,
                TypeRole::HeadlineSmall,
                false,
                title,
                ui.color(Role::OnSurface),
            ),
            frame::text_role_owned(
                ui,
                TypeRole::BodySmall,
                false,
                body,
                ui.color(Role::OnSurfaceVariant),
            ),
            buttons::filled(
                ui,
                Button::new(
                    catalog.text(Key::VersionSearchElsewhereAction),
                    Message::SetCategory(Category::All),
                ),
            ),
        ]
        .spacing(ui.space(Space::X3))
        .width(Length::Fill)
        .align_x(Alignment::Center),
    )
}
