//! # 内容库页（M109 `CONTENTDL`）：整合包 / Mod / 资源包 / 光影
//!
//! 用户令：「下载页只负责下载整合包和Mod/光影」。于是版本管理页（M107/M108 改名）只做版本，
//! **这一页**做内容：四条内容类型（页内一级侧栏）+ 关键词/来源/版本/加载器/排序筛选 +
//! 结果列表与详情面板 + **一键装入指定实例**。
//!
//! ## 入口（B1 裁定：不新增导航项）
//!
//! `Route::Content` 是新顶层路由，但 `Route::nav()` 返回 `NavSection::Download`：导航轨仍是
//! 五项，页头右上是「版本管理 | 内容库」一级切换（[`parts::section_switcher`]）。
//! 为什么不是第六个图标：轨宽 88 塞六项会挤，且用户要的是「同一入口下两块内容」。
//!
//! ## 分页 vs 无限滚动：**选分页**
//!
//! 三条理由，都是这一页的实际约束：
//!
//! 1. **快照只有一个槽**（`AppState::content.results`）：无限滚动要求「滚到底就自动续」，而
//!    自动续在没有虚拟化列表的 iced 里会一次挂上几百个控件；
//! 2. **翻页要有明确出口**：站点限流 / 失败重试都发生在「下一批」上，用户得能看见自己正在
//!    等第 3 页，而不是画面莫名一卡；
//! 3. **可截图复现**：分页的每一次状态都是一份确定的快照，D30 的矩阵能钉住「还有更多 / 没有更多了」。
//!
//! 所以尾部是一个「加载更多」按钮 + 一句「没有更多了」（[`parts::more`]），追加逻辑在
//! presenter 的 `AppState::apply_content`（同条件换页 ⇒ 追加；换条件 ⇒ 整批替换；迟到 ⇒ 丢掉）。
//!
//! ## 四态（都必须能画出来）
//!
//! | 态 | 判据 | 画什么 |
//! |---|---|---|
//! | 加载 | `pending` 或还没有结果 | 一句「正在加载」+ 说明 + 重试 |
//! | 空 | 拉到了、零命中 | 真 `EmptyState` + 重试 CTA |
//! | 失败 | `failed` 有原因 | `Error` 图标 + 原因原文 + 重试（实心） |
//! | 有数据 | 有命中项 | 计数头 + 列表 +（还有更多 ⇒ 加载更多 / 否则 ⇒ 没有更多了） |
//!
//! ## 不在这里做的事
//!
//! - **不发 HTTP**：检索与装入都在 presenter 的 `host/addon`（任务书第 4 条）。
//! - **不做乐观回显**：点「装入」不改本地状态，进度只由 `InstallProgress` 事件回流。
//! - **不自己画安装面板**：复用 `pages::download::install_panel`（同一个函数、同一套文案）。
//!
//! ## 偏离登记
//!
//! | # | 偏离 | 为什么 | 何时收口 |
//! |---|---|---|---|
//! | 1 | 四种内容类型借用现有字形（`Folder` / `Plus` / `Restore` / `Expand`，见 `parts::kind_icon`） | 设计令牌的 28 枚图标里没有这四类内容，且**不许**借用导航那五枚（同屏撞形）、不许用填充字形（门禁） | 图标集补「整合包 / Mod / 资源包 / 光影」时 |
//! | 2 | 本页自带一份栏几何（`content/layout.rs`） | M107 的 `pages/layout/rails.rs` 未并入本分支的 base | M107 合并后改为复用、删掉本文件 |
//! | 3 | 整合包的「装入」是禁用态 | core 没有远程整合包的安装管线 | 导入管线落地后接同一入口 |
//! | 5 | CurseForge 的「装入」同样是禁用态（`Blocker::SourceNotWired`） | 检索通了，**文件端点**还没接（CF 的文件列表要带 key） | 接上 CF 文件端点后删掉这条原因 |
//! | 4 | 安装进度只有阶段级（无字节条） | presenter 依赖表里没有 tokio/futures，拿不到 `ProgressEmitter` 的接收端 | presenter 补依赖后换成字节级 |

mod demo;
mod layout;
mod message;
mod model;
// M113: import/export —— 导入/导出区的界面（新代码全在 `packui*`）
mod packui;
mod parts;
mod view;

#[cfg(test)]
mod tests;

pub use message::Message;
pub use model::{loader_name, Blocker, Content, ContentQueryView, Hit, PageState, Status, Target};
// M113: import/export
pub use packui::{request_export, Pack, PackMessage, PackPageState};

use iced::Element;

use unlucky_i18n::Lang;

pub use unlucky_presenter::dto::ContentKind;

use crate::pages::layout::FrameMetrics;
use crate::pages::Message as PageMessage;
use crate::theme::Ui;

/// 整页入口（`pages/mod.rs` 的 `render` 调它；页面层不认识 presenter 的 DTO）。
pub fn view<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    model: &'a Content,
    lang: Lang,
) -> Element<'a, PageMessage> {
    view::view(ui, metrics, model, lang)
}

/// 演示数据（`pages_demo` 出图与单测用；产品路径走 `Content::from_snapshot`）。
pub fn demo(kind: ContentKind, lang: Lang) -> Content {
    demo::demo(kind, lang)
}

/// 页头那条一级切换（版本管理页也要用同一个函数：两页共用一个入口才不会各画一套）。
pub(crate) fn section_switcher<'a>(
    ui: &'a Ui,
    catalog: unlucky_i18n::Catalog,
) -> Element<'a, PageMessage> {
    parts::section_switcher(ui, catalog, true)
}
