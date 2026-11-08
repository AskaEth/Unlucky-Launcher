//! **版本管理页**（原下载页）：三栏版面 —— 版本类型 / 主版本 / 列表 + 安装面板。
//!
//! 用户令（信息架构调整）把这一页从「下载」改称「版本管理」：下载页只负责整合包与 Mod/光影，
//! 而这一页管的是**游戏版本与加载器安装**。本文件是那次改版后的页面装配点。
//! **标识符不改**（用户令 2026-11-04 的「只改显示层」边界）：`Route::Download` /
//! `Screen::Download` / 文件名 / 类型名一律保留，全仓重命名另开 finding 后置——所以本文件里
//! 「下载页」三个字到处都是，那是标识符的叫法，不是界面上出现的词。
//! **M102 版面订正**：旧版把整页块压到 424 并水平居中（1180 宽下左缘 422、页头标题在 112，
//! 左右各空 ~300px，视觉复审判为「内容缩在中间一条窄栏」）。现在页面块走 [`frame::body`]
//! ——左缘与页头标题同一条线（= 页边距 24），宽 = 块宽 `frame::body_width`（1180 下 1044，
//! 极宽窗封顶 **1440**）。424 只约束**成段散文**（本页没有），三栏的每一栏都吃块宽。
//!
//! ## 版面：三栏，宽度由布局系统裁决
//!
//! ```text
//! ┌─────────┬─────────┬──────────────────────────────┐
//! │ 版本类型 │ 主版本  │ 搜索条 · 命中数 · 计数头      │
//! │ 140/64  │ 140/64  │ 版本行（单行四项）            │
//! │         │         │ …                            │
//! │         │         │ 安装面板（M105 的选项区在这里）│
//! └─────────┴─────────┴──────────────────────────────┘
//! ```
//!
//! 三个数字**没有一个是在本文件里拍的**：它们来自
//! [`VersionRails`](super::layout::VersionRails)（宽档裁决 + 数值表在 `pages/layout/rails.rs`）。
//! 页面只做一件事：把裁决结果交给 [`rails`] 画出来。
//!
//! ## 原顶部 4 个 tab 的去处（不是新造分类）
//!
//! 四个 chip（全部 / 正式版 / 预览版 / 远古版）**语义一字不改**，只是从顶部横排搬到第一栏竖排；
//! 窄到三栏不成立时（可用宽 < 600）它们**原样回到顶部**（[`rails::stacked_chips`]）——同一条
//! [`Category`] 谓词、同一条 `AppCommand::ListVersions`。
//!
//! ## 意图 → 消息 → 指令（页面只发意图，不做乐观回显）
//!
//! | 用户动作 | 页面消息 | app 层落点 |
//! |---|---|---|
//! | 点分类 tab | [`Message::SetCategory`] | `AppCommand::ListVersions(category)`（`All` 不派：画现有快照） |
//! | 点一行 | [`Message::SelectVersion`] | **只改视图模型**（选中不是业务命令） |
//! | 点「安装」 | [`Message::InstallVersion`] | `AppCommand::CreateInstance(草稿)`（M105：带加载器与实例名） |
//! | 点「取消安装」 | [`Message::CancelInstall`] | `AppCommand::CancelInstall` |
//! | 点「刷新」/「打开日志」 | `Message::Action(RefreshSources)` / `Message::Navigate(Logs)` | `ListVersions` / `AppState::navigate` |
//!
//! ## M105：安装选项（加载器维度）
//!
//! 选中版本**不再直接开工**。面板下半块变成「安装选项」区：五家加载器（原版 / Forge /
//! NeoForge / Fabric / Quilt）的 chip、该加载器的官方版本列表、实例名，点「安装」才派
//! `CreateInstance`。选项的决策与画法在 [`options`]（子模块），本文件只负责把模型交给它。
//! 加载器版本列表来自 core 的 `LoaderSource::loader_versions`（经
//! `AppCommand::ListLoaderVersions` 回流到快照）；取不到时页面给具名原因并**禁掉确认**——
//! 不许「选完才失败」。
//!
//! 进度条只读 `model.install`：它由 presenter 的 `InstallProgress` 事件经 `apply` 灌进来。
//! 于是「点了没变」只有一种解释——后端还没回事件，这正是要的（P3-2 的纪律）。
//!
//! ## M107：两级侧栏从哪来（第一级 = 版本类型，第二级 = 主版本桶）
//!
//! 第一级 = 版本类型（4 项，固定）；第二级 = 主版本桶（由快照里的版本 id 按 [`group`] 的规则归纳，
//! 远古版这类「没有主版本概念」的只有「全部版本」一项）。
//!
//! ## 搜索（三层口径）
//!
//! | 情形 | 画面 |
//! |---|---|
//! | 有命中 | 列表照常；侧栏**位置不动**；搜索条右侧给「N 个匹配」（跨类型读数） |
//! | 一个都不命中 | [`states::no_match`]：没有匹配 + 换短片段 + 清空 |
//! | 有命中但不在当前类型 | [`states::elsewhere`]：N 个在别的类型 + 「看全部类型」 |
//! | 搜索中点第二级桶 | **退出搜索 + 按那个桶过滤**（点桶 = 「我就是要看这个桶」，见 `on_download`） |
//!
//! 查询词写在 presenter 快照的 `download_query` 里（页面无状态，见 [`model::Download::from_snapshot`]），
//! 所以「搜索 → 任何事件回来之后仍然在搜」这条不需要页面持有任何字段。
//!
//! ## 状态矩阵（加载 / 空 / 有数据 / 安装中 / 失败）
//!
//! 与 M94 时一致，唯一的改动是**空态与搜索态都留在三栏骨架里**：侧栏与搜索框照旧在，
//! 只有列表区换成占位内容——白屏会让人以为列表坏了。
//!
//! ## 两个纯视图态的字段（快照里没有，重建会丢）
//!
//! `selected_major`（第二级侧栏选中）与 `type_rail_folded`（类型栏手动折叠）**不在 presenter 快照里**
//! （`AppState` 没有这两个字段，加字段要动 `pages/mod.rs` 的偏离表与 presenter，见 M107 报告）。
//! 所以它们与既有的 `selected` 同一档处理：`on_download` 直接改视图模型，**不重建**；
//! 唯一会重建的路径是搜索（`rebuild_after_query`），那里把两个字段在重建前后各搬一次。
//! 其余事件（安装进度、版本列表刷新）回来时这两个字段会回到默认值——与 `selected` 的既有缺口同源，
//! 长期要靠 presenter 加字段，已登记在报告里。
//!
//! ## 版面判据（M102 口径：页面块 vs 散文列）
//!
//! 页面仍是 `PaneKit::Single`（页内自己切三栏）。整页内容走 [`frame::body`]：块左缘 =
//! 页头标题左缘 = `rail 88 + gutter 24 = 112`，块宽 = [`frame::body_width`]（pane 宽封顶
//! **1440**）——视觉复审 ①（内容缩在中间一条窄栏、与页头错位）因此收口。
//! **三栏的列宽从「块宽」算**（[`VersionRails::of_metrics`] 读 `frame::body_width`，不是
//! `metrics.available`）：否则 1576+ 的超宽窗上三列之和会超出 1440 的块、右侧被裁。
//! 424（[`super::layout::measure_cap_px`]）在本页只对**成段散文**有效——这一页没有散文。

mod demo;
mod group;
mod install;
mod model;
mod options;
mod rails;
mod rows;
mod states;

#[cfg(test)]
mod tests;

pub use demo::demo;
pub use model::{cancelled_toast, Category, Download};
pub use options::{InstallDraft, Loader};

// M109：内容库页复用**同一块**安装面板（同一个函数、同一套文案与几何），不另写一套。
//
// 两个薄包装为什么必须在**这个文件**里：①`install::panel` 是 `pub(super)`（只在 `pages::download`
// 可见），再导出到 crate 会撞 `E0364`；②面板的入参是 `&Download`，而 `InstallView` 是页面私有
// 类型（M105 起不再从本模块导出），所以「快照 → 面板形状」的转换也留在这里——内容库只拿到
// `Option<Download>`，它不需要也不该认识 `InstallView`。
pub(crate) fn install_panel<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Download,
) -> Element<'a, Message> {
    install::panel(ui, catalog, model)
}

/// 快照 → 面板要的 `Download` 形状（**只填 `install`**）。
///
/// 「装完 / 已取消」这类没有动作可给的终态返回 `None`（与下载页同一个判断
/// `InstallView::finished`）：留着只会让面板停在一个点不动的状态上。
pub(crate) fn install_panel_model(
    progress: &unlucky_presenter::dto::InstallProgressView,
    catalog: Catalog,
) -> Option<Download> {
    let view = model::InstallView::of(progress, catalog);
    if view.finished() {
        return None;
    }
    Some(Download {
        install: Some(view),
        ..Download::loading()
    })
}

use iced::widget::column;
use iced::{Element, Length};

use unlucky_design::spacing::Space;

use unlucky_i18n::{Catalog, Key, Lang};

use crate::app::components::buttons::{self, Button};
use crate::app::components::divider;
use crate::theme::Ui;

use super::content;
use super::frame::{self, Header};
use super::layout::{FrameMetrics, VersionRails};
use super::{Action, Message};

/// 整页（**唯一**的真数据入口：`model` 由 `app/view.rs` 从 presenter 快照建）。
///
/// `lang` 是 i18n 的入参，调用点传的是 `pages::Model.lang`（M106 起 = `App` 的生效语言，
/// 见 `ui::lang::LangState`）；页面自己不读全局、不猜语言。
pub fn view<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    model: &'a Download,
    lang: Lang,
) -> Element<'a, Message> {
    let catalog = lang.catalog();

    let header = Header::new(catalog.text(Key::DownloadTitle))
        .subtitle(catalog.text(Key::DownloadSubtitle))
        // M109：一级切换「版本管理 | 内容库」。两页共用同一条导航轨项（`NavSection::Download`），
        // 所以这个切换是进内容库的唯一产品入口——少了它，内容库只能靠 `ULL_START_ROUTE` 到达。
        .action(content::section_switcher(ui, catalog))
        .action(buttons::text(
            ui,
            Button::new(
                catalog.text(Key::DownloadRefresh),
                Message::Action(Action::RefreshSources),
            ),
        ));

    frame::page(ui, metrics, header, body(ui, metrics, catalog, model))
}

/// 正文：加载态仍是居中占位；其余状态都长在三栏骨架上。
fn body<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    catalog: Catalog,
    model: &'a Download,
) -> Element<'a, Message> {
    if model.is_loading() {
        // 还没拉到版本列表：三栏没有内容可填，给占位（安装面板照旧带着，见 M94 复审 p3）
        let content: Element<'a, Message> = if model.install.is_some() {
            column![
                states::loading(ui, catalog),
                divider::full(ui),
                install::panel(ui, catalog, model)
            ]
            .spacing(ui.space(Space::X3))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            states::loading(ui, catalog)
        };
        return frame::body(metrics, content);
    }

    // 宽度裁决：档位来自布局系统，「用户手动折叠了类型栏」这一条覆盖叠在上面。
    let rails = {
        let rails = VersionRails::of_metrics(metrics);
        if model.type_rail_folded {
            rails.with_type_folded()
        } else {
            rails
        }
    };

    let content: Element<'a, Message> = if rails.tier.is_three_column() {
        rails::three_column(ui, catalog, model, rails)
    } else {
        // 三栏不成立（可用宽 < 600）：类型与主版本原样退回顶部 chips，列表吃掉整条可用宽。
        // 高度分法跟着档位走：chips 档多花两行 chip，面板必须收回自己的一半（M125 缺陷 1）。
        column![
            rails::stacked_chips(ui, catalog, model),
            divider::full(ui),
            rows::content(
                ui,
                catalog,
                model,
                rows::TailShare::of(rails.tier, &install::panel_state(model)),
            ),
        ]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };

    // M102：整页块走 `frame::body`（贴左、块宽封顶 1440），与页头标题同一条左缘。
    // 三栏都在这个块里切，所以 `VersionRails` 读的是块宽而不是 `metrics.available`。
    frame::body(metrics, content)
}
