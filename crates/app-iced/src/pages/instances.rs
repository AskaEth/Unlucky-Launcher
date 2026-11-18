//! 实例页：**list-detail** 的唯一用例，也是 M90 唯一打通到「真组件 + 真数据 + 真命令」的页。
//!
//! ## 版式（由 [`super::layout`] 决议，页面不自己算宽度）
//!
//! | 版面 | 结构 | 退化行为 |
//! |---|---|---|
//! | `ListDetail`（窗口 ≥840） | 左列 336 固定、**独立滚动**；右列详情，散文 ≤424 | — |
//! | `SingleColumn`（<840） | 列表与详情**整页切换**：本页只渲染详情列（块贴左吃满页宽，散文 ≤424） | 顶部给「Back to instances」，列表退成上一屏 |
//!
//! 为什么窄窗不是「上下堆」：官方 compact 下 list-detail 的口径是一次只显示一个 pane（列表 → 详情，
//! 整页切换）。上下堆会把详情挤到折叠线以下，等于详情不存在。
//!
//! ## M90：占位控件 → 真组件（逐项对照）
//!
//! | 位置 | M84（占位） | M90（真组件） |
//! |---|---|---|
//! | 列表行 | 手搓 `container` + `text_role`（行高 56） | `navigation::list_item`（一/两/三行 44/60/76，五态状态层） |
//! | 页头动作 | `frame::text_action`（纯 `text` + 手搓 hover） | `buttons::text`（`Button` 五式 + 状态层） |
//! | 「Play」 | `frame::text_action` | `buttons::filled`（**全屏唯一实心 accent**，rubric K2） |
//! | 分隔线 | `frame::divider` | `divider::full` |
//! | 数据 | `DEMO` 常量 | [`super::model::Instances`]（真页面由 presenter 快照转，演示由 `DEMO` 转） |
//!
//! 判据没有因此放松：列宽仍由 [`super::layout`] 给（见 `pages/tests.rs` 的 424 断言）。
//!
//! ## 行高为什么从 56 变成 60（M90 登记）
//!
//! M84 的行高是「`row_height` 44 + `Space::X3` 12 = 56」的手工值；真 `ListItem` 的两行档是
//! **60**（令牌 `row_height` 44 + 每多一行 16，A1 是 72）。差异来自组件自己的几何真源，
//! 页面不许覆盖——这正是「接真组件」的应有结果。列表仍高于可视区，独立滚动条照样可见。
//!
//! ## 行距归零 + 底部内缩（M102，视觉复审 ②）
//!
//! 视觉复审在 1180×760 的截图里抓到「末行 Redstone Lab 被卡片底边切掉半行（副标题消失）」。
//! 根因是**两道页级附加量**：`Space::X1` 的行距让行节奏变成 64，与可视高度除不尽；
//! 列表滚动区贴到卡片的 1px 圆角描边上，切点正好落在描边线上。修法两件：
//!
//! | 改什么 | 值 | 结果 |
//! |---|---|---|
//! | 页级行距 | 4 → **0**（真源 [`ROW_SPACING`] = `None`） | 行节奏回到组件的 60 |
//! | 滚动区底部内缩 | 0 → **`Space::X2` 8**（真源 [`LIST_BOTTOM_INSET`]） | 切点落在卡片的内边距带里，不压描边 |
//!
//! 于是可视区 = 580 − 列头 32 − 内缩 8 = **540 = 9 × 60**，正好 9 整行：末行完整可见，
//! 第 10 行整体落在裁切线之外。两个数都**只有一个真源**，页面与护栏都从它取
//! （`instances/tests.rs` 引 [`LIST_BOTTOM_INSET`].`dp()` 与 [`ROW_SPACING`] 算 pitch，
//! 另有一条源码结构断言盯着 `list_column()` 不许再叠字面量间距）。
//! 列表内容 12×60 = 720 仍高过可视区，独立滚动条照样可见。

mod data;
mod demo;
mod detail;
mod launch_args;
// M313: 实例列表的行序与增删动效（复用 M305 的唯一入口，不另写一套）
mod list;
// M224: 详情页的 MODS 区（本地 mod 列表 + 启停开关）
mod mods;
// M235: 详情页的 SAVES 区（存档 / 截图的只读列表 + 打开文件夹）
mod saves;
mod state;
mod text;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use data::{DEMO, NOTE, SELECTED};
pub(crate) use demo::{demo, demo_form};
pub use mods::{from_state as mods_from_state, Mods};
pub use saves::{from_state as saves_from_state, Saves};

/// 存档区失败的一句话（app 层就地画红字用；取词入口仍是 [`Texts`]）。
///
/// 本版**不弹 Toast**：只读列表的失败离用户的眼睛很近（就在卡片里），再弹一条是刷屏。
pub fn saves_failure_line(
    failure: &unlucky_presenter::dto::SavesFailureView,
    catalog: unlucky_i18n::Catalog,
) -> String {
    saves::failure_line(failure, Texts::of(catalog))
}

/// 删除世界的确认框文案：`(标题, 正文, 主按钮, 次按钮)`。
///
/// 模态框在 app 层建（`state.dialogs.push`），但**取词入口仍是本页的 [`Texts`]**——与
/// [`saves_failure_line`] 同一条口径：页面文案只有一个入口，测试扫的也是那一份源码。
pub fn saves_delete_prompt(
    catalog: unlucky_i18n::Catalog,
    dir: &str,
) -> (String, String, String, String) {
    let texts = Texts::of(catalog);
    (
        texts.saves_delete_title().to_string(),
        texts.saves_delete_body(dir),
        texts.saves_delete_confirm().to_string(),
        texts.saves_cancel().to_string(),
    )
}

/// 启停失败/读目录失败的一句话（app 层弹 Toast 用；取词入口仍是 [`Texts`]）。
pub fn mods_failure_line(
    failure: &unlucky_presenter::dto::ModFailureView,
    catalog: unlucky_i18n::Catalog,
) -> String {
    mods::failure_line(failure, Texts::of(catalog))
}

/// 状态行的**腔调**（M314 / A6）：这一档的运行状态用哪个颜色角色。`None` = 这一档不画状态行。
///
/// 为什么在页面层：颜色是**视觉**决定，页面与它的测试都读这一处；update 侧
/// （`app/state/refresh.rs::sync_launch_tone`）只把它报给帧泵，页面
/// （`instances/detail.rs`）只按泵给的进度把两档合成——三处读的是同一张表。
///
/// 三档的角色口径：
///
/// | 档 | 角色 | 为什么 |
/// |---|---|---|
/// | `Busy`（启动中） | `OnSurfaceVariant` | 进行中的读数比正文弱一档：它在动，但不是结论 |
/// | `Running`（起来了） | `OnSurface` | 稳定事实，正文色 |
/// | `Failed`（失败） | `Error` | **红句**。全仓的失败色只有这一个角色（mods / saves / 内容库同款） |
///
/// `Idle` 给 `None`：那一档**没有**状态行（与 `app/view/instances.rs::runtime_status` 同一判据，
/// 它只对 `Idle` 返回 `None`）。出现 / 消失是版面变化，不走交叉淡入（见 `pump/launch.rs`）。
pub fn launch_tone(runtime: &unlucky_presenter::dto::InstanceRuntime) -> Option<Role> {
    use unlucky_presenter::dto::InstanceRuntime;
    match runtime {
        InstanceRuntime::Idle => None,
        InstanceRuntime::Busy { .. } => Some(Role::OnSurfaceVariant),
        InstanceRuntime::Running { .. } => Some(Role::OnSurface),
        InstanceRuntime::Failed { .. } => Some(Role::Error),
    }
}

/// 自动发现的实例按下「启动」时那句提示（**这一次**用的是哪一边的运行目录）。
///
/// 取值口径只有一处：[`Texts::discovered_run`]，判据是快照里的 `isolation`
/// （`shared` = 共用游戏根；其余 = 它自己的版本目录）。界面层不自己判盘。
pub fn discovered_run_toast(catalog: unlucky_i18n::Catalog, isolation: &str) -> String {
    Texts::of(catalog).discovered_run(isolation).to_string()
}
pub use state::{LaunchForm, Message as InstanceMessage};
pub(super) use text::Texts;

use iced::widget::{column, container, row, scrollable};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::app::components::buttons::{self, Button};
use crate::app::components::interaction::Interaction;
use crate::app::components::navigation::list_item::ListItem;
use crate::theme::Ui;

use super::frame::{self, Header};
use super::layout::{FrameMetrics, PageLayout};
use super::model::{Instances, Model, Row, ScanPhase};
use super::{Action, Message};

/// 列表行距：**`None` = 这一层不加间距**（页面与护栏**共用这一个真源**）。
///
/// M102 的 ② 就是页级附加量：`ListItem` 自己的 60 高已含上下 8px 内边距，页面再叠一道
/// `Space::X1`(4) 就把行节奏变成 64、与可视高度除不尽（1180×760 下末行被切半行）。
/// 所以行距不写成字面量、也不散落在 `list_column` 里，而是这个常量——
/// 页面 `match` 它，`instances/tests.rs` 也从它算 pitch（改这里，测试立刻红）。
pub(super) const ROW_SPACING: Option<Space> = None;

/// 列表滚动区的**底部内缩**：让裁切线落在卡片的内边距带里，而不是压在 1px 描边上。
///
/// 与 [`ROW_SPACING`] 是一对（行距管「切在哪」、内缩管「切在什么底色上」），
/// 两个数都由 `instances/tests.rs` 的几何断言直接引用。
pub(super) const LIST_BOTTOM_INSET: Space = Space::X2;

/// 整页（`block.detail = None` 时详情区显示空态）。
///
/// 文案全部走 [`Texts`]（`model.lang` 的语言）：这个函数是实例页唯一的取词入口，
/// 页面里不留任何可见英文字面量（守门见 `instances/tests/i18n.rs`）。
pub fn view<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    model: &'a Model,
    form: &'a LaunchForm,
) -> Element<'a, Message> {
    let block = &model.instances;
    let forced = model.forced;
    let texts = Texts::of(model.lang.catalog());
    let header = Header::new(texts.title())
        .subtitle(texts.subtitle())
        .action(buttons::text(
            ui,
            forced_state(
                Button::new(texts.new_instance(), Message::Action(Action::NewInstance)),
                forced,
            ),
        ))
        .action(buttons::text(
            ui,
            forced_state(Button::new(texts.import(), adopt_action()), forced),
        ));

    let body = match metrics.layout {
        PageLayout::ListDetail => frame::panes(
            metrics,
            list_column(
                ui,
                frame::body_height(ui, metrics, model.window_height, true),
                block,
                forced,
                texts,
            ),
            match &block.detail {
                Some(detail) => detail::detail_column(
                    ui,
                    metrics,
                    detail,
                    form,
                    &model.mods,
                    &model.saves,
                    forced,
                    texts,
                ),
                None => detail::detail_side(ui, block, texts),
            },
        ),
        _ => match &block.detail {
            Some(detail) => detail::narrow_detail(
                ui,
                metrics,
                detail,
                form,
                &model.mods,
                &model.saves,
                forced,
                texts,
            ),
            None => frame::body(metrics, detail::detail_side(ui, block, texts)),
        },
    };

    frame::page(ui, metrics, header, body)
}

/// 「导入已有版本」向导的入口意图——**页面里唯一产地**。
///
/// 两个入口共用它：页头的「导入」与空态的主行动（一个实例都没有时用户只看得到后一个）。
/// 落点是 `app/update/page_messages.rs` 的 `Action::Import` 臂（→ `AdoptMessage::Open`），
/// 所以中间任何一段被拆掉，向导就再也打不开——`instances/tests/empty_entry.rs` 专门钉这条链。
pub(crate) fn adopt_action() -> Message {
    Message::Action(Action::Import)
}

// --------------------------------------------------------------- 列表列

fn list_column<'a>(
    ui: &'a Ui,
    body_height: f32,
    block: &'a Instances,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    let mut head_row = row![container(frame::text_role(
        ui,
        TypeRole::LabelMedium,
        false,
        texts.all_title(),
        ui.color(Role::OnSurfaceVariant),
    ))
    .width(Length::Fill)
    .align_y(Alignment::Center)]
    .width(Length::Fill)
    .align_y(Alignment::Center);
    // M302：**未就绪时不许写 0**（复审 r1 P2-1：计数 0 与「还没有实例」是同一句假话的两半）。
    // 藏起来而不是写「—」：破折号读起来像「零个」，而这一刻根本没有数可给。
    if block.scan == ScanPhase::Ready {
        head_row = head_row.push(frame::text_role_owned(
            ui,
            TypeRole::LabelMedium,
            false,
            block.rows.len().to_string(),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    let head = container(head_row).width(Length::Fill).padding(Padding {
        top: ui.space(Space::X3),
        right: ui.space(Space::X3),
        bottom: ui.space(Space::X1),
        left: ui.space(Space::X3),
    });

    let mut rows = column(vec![list::column(ui, block, forced)])
        .padding(Padding::from([0.0, ui.space(Space::X2)]))
        .width(Length::Fill);

    // 行距只从 `ROW_SPACING` 来（当前是 None = 不加间距）。别在这里写字面量 `.spacing(…)`：
    // 那正是视觉复审 ② 的那道页级附加量（`pages/tests.rs` 有一条源码结构断言盯着这段）。
    if let Some(spacing) = ROW_SPACING {
        rows = rows.spacing(ui.space(spacing));
    }

    // **独立滚动**：只有列表列这一条 scrollable，详情列不跟着滚。
    // M357（P2-2）：高度按**整行**取（`list::viewport_height`，抄版本管理页的做法）；修前是
    // `Fill`，880×560 的裁切线落在第 5 行中间。代价：不足一行的余量留在卡片底部。
    let viewport = list::viewport_height(ui, block, forced, body_height);
    let scroller = scrollable(rows)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(frame::SCROLLBAR_WIDTH)
                .scroller_width(frame::SCROLLBAR_WIDTH)
                .margin(frame::SCROLLBAR_MARGIN),
        ))
        .height(if viewport > 0.0 {
            Length::Fixed(viewport)
        } else {
            Length::Fill
        })
        .style(frame::scroll_style(ui));

    // 底部内缩：裁切线落在卡片的内边距带里，而不是压在圆角描边上（M102 / 视觉复审 ②）。
    //
    // M237：这一层画的是 `MaterialLayer::Card`（= `frame::BASE` 的同一个层），所以卡里的
    // 列表行把底传 `frame::BASE`（见 `list_row`）——不另立常量，避免同一层出现两个「真源」。
    container(column![head, scroller].height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::default().bottom(ui.space(LIST_BOTTOM_INSET)))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.layer(MaterialLayer::Card))),
            border: Border {
                radius: ui.radius_dp(ShapeScale::Large),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

pub(super) fn is_selected(block: &Instances, entry: &Row) -> bool {
    block
        .detail
        .as_ref()
        .is_some_and(|detail| detail.id == entry.id)
}

/// 一行的 `ListItem` **描述**（不外框）：行高要从它自己的 `lines()` 数出来，所以先建描述、
/// 由 `list` 模块量高再画（`list::column` 里那一句 `geometry.height(item.lines())`）。
///
/// 点一行 = **压一层路由栈**（`Route::Detail(id)`），页面自己不留「选中项」副本——于是
/// 「当前选中」与「返回栈」永远是同一份真相（`presenter::router`）。
///
/// **M313**：这一段的实现从本文件的 `list_row` 搬过来（只改返回值：`Element` → `ListItem`），
/// 所以「卡片内的控件必须把 `frame::BASE` 传下去」这条接线**仍然在本文件**（见
/// `pages/tests/base_wiring.rs` 的计数）。
pub(super) fn row_item<'a>(
    entry: &'a Row,
    selected: bool,
    forced: Option<Interaction>,
) -> ListItem<'a, Message> {
    let mut item = ListItem::new(
        entry.name.as_str(),
        Message::Navigate(unlucky_presenter::router::Route::Detail(entry.id.clone())),
    )
    .supporting(entry.supporting.as_str())
    .leading(IconKind::Instances)
    .trailing(IconKind::ChevronRight);

    if let Some(status) = &entry.status {
        item = item.overline(status.as_str());
    }
    if selected {
        item = item.selected();
    }
    if let Some(state) = forced {
        item = item.state(state);
    }
    // M237 复审 P2-1：列表卡画的是 `MaterialLayer::Card`（本文件 `list_column` 的 style），
    // 行的 hover / 按下状态层按 `base.role()` 合成（`components/navigation/list_item.rs::palette`），
    // 不给底就落回 `Base::CANVAS` —— 卡片里的行抬起时会叠在画布色上，与本轮修的禁用态同一类错。
    item.base(frame::BASE)
}

/// `--force-state` 的落点：只改**画出来的**状态，不动任何状态机（cli `ForcedState` 的同一条纪律）。
pub(super) fn forced_state<'a, M>(
    button: Button<'a, M>,
    forced: Option<Interaction>,
) -> Button<'a, M> {
    match forced {
        Some(state) => button.state(state),
        None => button,
    }
}
