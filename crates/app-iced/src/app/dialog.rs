//! 模态对话框（M187 接通 `DialogQueue`）+ 启动失败卡片。
//!
//! ## 为什么先补这一层
//!
//! 接线审计（M177）的结论很硬：`DialogQueue` / `DialogRequest` 在 app-iced 里**零引用**，
//! 所有弹窗都画不出来。于是「像 PCL2 / HMCL 那样，启动失败时弹一个带崩溃说明的框」这件事
//! 在此之前根本无处落脚。本模块是最小可用的一层：
//!
//! ```text
//! presenter AppState::dialogs（唯一队列）
//!         │ current()
//!         ▼
//! dialog::current(lang, request, card)  ← 纯视图模型（测试打这里）
//!         │
//!         ▼
//! dialog::view::layer(ui, base, modal)  ← 模态遮罩 + 卡片 + 主/次按钮
//! ```
//!
//! 交互：主按钮 / 次按钮 / 点遮罩 / `Esc` 都关当前条（`DialogQueue::close_current`）。
//!
//! M298 把**视图模型**那一段（`Modal` / `CardView` / `current` 与四个纯函数）搬到 [`model`]，
//! `pub use` 保路径：本文件只留「卡片事实 + 消息枚举 + 与 `App` 的接线」。
//!
//! ## 崩溃卡片额外带什么
//!
//! `DialogRequest`（presenter 的 DTO）只有标题/正文/按钮，而这次用户要的是**证据**：
//! 退出码、一句人话原因、**文件路径**、可展开的崩溃说明、以及三个按钮（导出 / 打开文件夹 /
//! 复制路径）。这些东西不进 `DialogRequest`——它是 presenter 的公共 DTO，塞进 UI 专用字段
//! 会把「谁拥有证据」搅浑。做法是：队列管**弹窗**，[`CrashCard`] 管**这次失败的事实**，
//! 两者按 `DialogId` 绑定，见 [`App::open_failure_dialog`]。
//!
//! 事实来源是 `unlucky_core::launch::crash_report`（启动线程收尾时登记的现场）。这里只读它，
//! 不重算：**界面不读盘、不猜原因**。

use unlucky_core::launch::crash_report::CrashReport;
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_presenter::dialogs::{DialogId, DialogKind, DialogSource};

use crate::app::state::App;

pub(super) mod export;
/// 「下载 Java {major}」那条出口的状态机 + 判据（M334 r1 从 `dialog.rs` 拆出）。
mod guard;
mod model;
/// M331 出图夹具（`ULL_JAVA_GUARD_SHOT`）+ M349 的建议框夹具（`ULL_LEGACY_ADVISORY_SHOT`），
/// 都只在 `--screenshot` 下生效。
pub(super) mod shot;
#[cfg(test)]
mod tests;
pub(super) mod view;

#[cfg(test)]
pub(super) use model::path_rows;
pub(super) use model::{current, DialogRequestView, Modal};

// 路径保持不变（`crate::app::dialog::GuardInstall`）：`update/dialog.rs` 与用例都这么写。
use guard::classify_install;
pub use guard::{GuardInstall, GuardInstallFailure};

/// Java 不符的**启动前**卡片（M331）：一次拦下的**事实**（不含文案）。
///
/// 与 [`CrashCard`] 同一套做法（队列管弹窗、卡片管这一次的事实、按 [`DialogId`] 绑定），
/// 但两者说的是**互斥的两件事**：
///
/// | 卡片 | 意思 | 框里的出口 |
/// |---|---|---|
/// | [`CrashCard`] | 进程**起过**，然后崩了 | 导出诊断包 / 关掉 |
/// | [`Self`] | 进程**一次都没起**（Java 与版本要求不符，或老版本原版配了更高的 Java——M349 的 `Advisory` 那一支） | 去设置改 Java / 仍要启动 / 下载 Java {major}（M334） |
///
/// 混成一张卡就会在「还没启动」的时候给人看崩溃证据（那里一个字都没有）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaGuardCard {
    /// 拦下的事实（实例 / 版本 / 方向 / 三个版本号）
    pub view: unlucky_presenter::dto::LaunchBlockedView,
    /// 与哪一条队列请求绑定（认框不认人：id 对不上就不许动这一条）
    pub dialog: DialogId,
    /// 「下载 Java {major}」这条出口走到哪一步（M334）
    pub install: GuardInstall,
    /// 已经**消化过**的那一份 Java 页快照——判「这是新一代吗」用（M334 r1 P1）。
    ///
    /// 为什么必须有这一格：「再点一次下载」发生在 `Failed` 之后，而那一拍 `state.java` 里
    /// 躺着的仍然是**上一次失败留下的**旧快照（`busy = false`、没有那个大版本、链上写着上次
    /// 的失败）。没有这道闸，`Requested` 在同一拍就被旧快照重新判回 `Failed`——用户点了没
    /// 反应，而后台真的又起了一条安装（复审 r1 P1 实测）。
    ///
    /// 语义就是「代次」：快照**变了**才算新一代，与上一份逐字段相同的一份一律丢弃
    /// （`fold_guard_install` 那句比较就是全部实现）。没在 `presenter::state` 里加计数器，
    /// 是因为那要到 `CoreEvent::JavaChanged` 的落地处（`state.rs`）去动，而那格不在本条 scope。
    pub consumed: Option<Box<unlucky_presenter::dto::JavaPageView>>,
}

/// 崩溃卡片：一次失败的**事实**（不含任何结论）。
#[derive(Debug, Clone)]
pub struct CrashCard {
    /// 现场。
    pub report: CrashReport,
    /// 「详细崩溃说明」是否展开。
    pub expanded: bool,
    /// 与哪一条队列请求绑定（关掉它时把卡片一起清掉）。
    pub dialog: DialogId,
    /// 上一次动作的结论（导出到哪 / 打不开文件夹的原因）。
    ///
    /// 为什么写在卡片里而不是只推 Toast：app-iced 的 Toast 通道**至今没有任何渲染点**
    /// （`state.toasts` 只被推入与过期，`view` 里一次都没读）。导出这种「必须看见落到哪」的
    /// 结果不能挂在一条不显示的通道上。
    pub outcome: Option<String>,
}

/// 弹窗消息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// 主按钮。
    Primary,
    /// 次按钮。
    Secondary,
    /// 点遮罩 / Esc。
    Dismiss,
    /// 展开 / 收起详细说明。
    Toggle,
    /// 打开日志文件夹。
    OpenFolder,
    /// 复制全部路径。
    CopyPaths,
    /// 复制第 `index` 条路径（`Modal::paths` 的下标）。
    CopyPath(usize),
    /// 第三个出口（M334）：下载该版本要求的 Java / 装好之后重新启动。
    ///
    /// 只有「Java 不符」那个框有它（`view` 也只在 `modal.tertiary` 有值时才画这枚按钮）；
    /// 别的框根本不需要多一枚按钮。
    Tertiary,
    /// 「去这条实例的启动参数」（M349 r3 / 复审 r2 P2-1）。
    ///
    /// 只出现在**钉过实例 Java** 的建议框里：那时装机出口注定被影子掉（钉死那份优先），
    /// 唯一能改对的地方是实例自己的「启动参数 → Java 路径」，而**全局**设置页对它无效
    /// （实例层优先级更高）。所以这一枚必须指向实例详情，不是 `Route::Settings`。
    InstanceLaunchArgs,
}

// --------------------------------------------------------------- 与 App 的接线

impl App {
    /// 队列当前条 → 视图模型（`view()` 与测试共用同一个入口）。
    ///
    /// `&mut self` 只为**一处**副作用：[`Self::fold_guard_install`]（把最新一份 Java 页快照
    /// 折叠进「下载 Java」那条出口）。它是幂等的：没有框、没点过、或者已经是终态就一行不动。
    /// 放在这里是因为「快照 → 视图模型」全仓只有这一个转换点（`refresh_view` 是唯一调用方），
    /// 而事件本身不经过弹窗模块——`CoreEvent::JavaChanged` 说的是 Java 页的形状，不是「这个框
    /// 该说什么」。
    pub(super) fn dialog_modal(&mut self) -> Option<Modal> {
        self.fold_guard_install();
        let request = self.state.dialogs.current()?;
        let view = DialogRequestView::of(request);
        let card = self
            .dialog_card
            .as_ref()
            .filter(|card| card.dialog == request.id);
        Some(current(self.lang(), &view, card, self.guard_for_current()))
    }

    /// 把最新一份 Java 页快照折叠进「下载 Java」那条出口（幂等，见 [`GuardInstall`]）。
    ///
    /// **只认新一代快照**：与 [`JavaGuardCard::consumed`] 逐字段相同的那一份直接丢弃（那是
    /// 上一次留下的旧快照，把它当成本次的结果就会「点了没反应」——复审 r1 P1）。
    ///
    /// 判据顺序（`state.java` 就是 presenter 那条 `CoreEvent::JavaChanged` 的落地）：
    /// ① `busy` ⇒ 正在下载；② 托管目录/链上已经有这个大版本 ⇒ 装好了；③ 链上那一跳告诉
    /// 我们是哪种失败（见 [`GuardInstallFailure`]）。都不命中 ⇒ 留在原状态等下一份。
    fn fold_guard_install(&mut self) {
        let Some(card) = self.java_guard.as_mut() else {
            return;
        };
        if !matches!(
            card.install,
            GuardInstall::Requested | GuardInstall::Running
        ) {
            return;
        }
        let page = &self.state.java;
        if card.consumed.as_deref() == Some(page) {
            // 旧代次：这一份早就看过了（多半是上一次失败的现场），丢掉。
            return;
        }
        // 装机出口只在 `install_major` 有值时才存在（M349 r3）：钉过实例 Java 的建议框没有它。
        let Some(major) = card.view.install_major else {
            return;
        };
        let Some(install) = classify_install(page, major) else {
            // 空链又不 busy：还没有任何结论，别急着写状态。
            return;
        };
        card.consumed = Some(Box::new(page.clone()));
        card.install = install;
        self.view_dirty = true;
    }

    /// 记下「用户点了下载」这一步（`Requested`；失败之后再点也是回到这里）。
    ///
    /// 同时把**当下那一份**快照记成「已消化」：它就是上一次的现场（失败原句也在里面），
    /// 这一次点下去之后不许它回来冒充结果（复审 r1 P1）。
    pub(super) fn mark_guard_install_requested(&mut self) {
        if let Some(card) = self.java_guard.as_mut() {
            card.install = GuardInstall::Requested;
            card.consumed = Some(Box::new(self.state.java.clone()));
            self.view_dirty = true;
        }
    }

    /// 当前队列条若绑着「Java 不符」的卡片就返回它（**认框不认人**：id 必须对上）。
    ///
    /// 返回 `None` = 这一条是普通弹窗 / 崩溃卡片。按钮据此分流，别的框行为一字不动。
    pub(super) fn guard_for_current(&self) -> Option<&JavaGuardCard> {
        let current = self.state.dialogs.current()?.id;
        self.java_guard
            .as_ref()
            .filter(|card| card.dialog == current)
    }

    /// 弹出「Java 与目标版本不符 · 还没启动」的框（M331，**启动前**，不是崩溃后那个）。
    ///
    /// 内容全部按当前语言现求（presenter 只给结构，见 `dto::LaunchBlockedView`）。
    /// 已经有一条同样的框在显示 ⇒ 不再压第二条：同一条事件被重投时队列不该堆起来。
    ///
    /// M349：`direction == Advisory(_)`（老版本原版 + 更高版本 Java）走的是**同一个框**——
    /// 标题与正文换一套（[`model::guard_title`] / [`model::guard_body`]），三枚出口一样。
    pub(super) fn open_java_guard_dialog(
        &mut self,
        blocked: &unlucky_presenter::dto::LaunchBlockedView,
    ) {
        if self
            .guard_for_current()
            .is_some_and(|card| card.view == *blocked)
        {
            return;
        }
        let catalog = self.lang().catalog();
        // M349：`Advisory(_)` 那一支的标题与正文是另一套（它不是「不符」），标题也从
        // `model::guard_title` 取——与 `current()` 画出来的那一屏同源，不在这里手抄一遍。
        let title = model::guard_title(&catalog, blocked).to_string();
        let body = model::guard_body(&catalog, blocked).join(" ");
        let id = self.state.dialogs.push(
            DialogSource::UserAction,
            title,
            body,
            DialogKind::Confirm {
                primary: catalog.text(Key::JavaGuardOpenSettings).to_string(),
                secondary: catalog.text(Key::JavaGuardLaunchAnyway).to_string(),
            },
        );
        self.java_guard = Some(JavaGuardCard {
            view: blocked.clone(),
            dialog: id,
            install: GuardInstall::Idle,
            consumed: None,
        });
        self.view_dirty = true;
    }

    /// 收到一次启动收尾：失败就弹框（成功不打扰）。
    ///
    /// 只在**真的失败**时弹：`LogOutcome::Ok` 且没有阶段失败 ⇒ 什么都不做，与既有 Toast 口径一致。
    pub(super) fn open_failure_dialog(
        &mut self,
        finished: &unlucky_presenter::dto::LaunchFinished,
    ) {
        let failed = finished.outcome != unlucky_presenter::dto::LogOutcome::Ok
            || finished.failure.is_some();
        if !failed {
            return;
        }
        let Some(report) = unlucky_core::launch::crash_report::failing_launch() else {
            return;
        };
        if !finished.session.is_empty() && report.session != finished.session {
            // 登记的是另一场（并发启动）：不拿它去解释这一场。
            return;
        }
        let catalog = self.lang().catalog();
        let crashed = report.started();
        let title = catalog.text(if crashed {
            Key::CrashDialogTitleCrashed
        } else {
            Key::CrashDialogTitleFailed
        });
        let summary = self.dialog_summary(&report);
        let at = self.lang();
        let id = self.state.dialogs.push(
            DialogSource::UserAction,
            title,
            summary,
            DialogKind::Confirm {
                primary: Catalog::of(at).text(Key::CrashDialogExport).to_string(),
                secondary: Catalog::of(at).text(Key::CrashDialogClose).to_string(),
            },
        );
        self.dialog_card = Some(CrashCard {
            report,
            // 默认展开：正文已经说「下面是捕获的最后若干行」，收着就成了空口承诺。
            expanded: true,
            dialog: id,
            outcome: None,
        });
        self.view_dirty = true;
    }

    /// 队列标题下的那一行摘要（弹窗正文由 [`current`] 按卡片重算，这里给它一份独立可读的短句）。
    fn dialog_summary(&self, report: &CrashReport) -> String {
        let catalog = self.lang().catalog();
        match report.exit_code {
            Some(code) => catalog.render(Key::CrashDialogExitCode, &[("code", &code.to_string())]),
            None => catalog.text(Key::CrashDialogTitleFailed).to_string(),
        }
    }

    /// 关掉当前弹窗（主/次按钮、遮罩、`Esc` 共用）。
    pub(super) fn close_dialog(&mut self) {
        let closed = self.state.dialogs.current().map(|request| request.id);
        self.state.dialogs.close_current();
        if self
            .dialog_card
            .as_ref()
            .is_some_and(|card| closed == Some(card.dialog))
        {
            self.dialog_card = None;
        }
        // M331：Java 不符的卡片与它那条队列请求同生共死（漏清的后果是下一轮点启动时
        // 按钮把「仍要启动」认到一条已经关掉的框上）。
        if self
            .java_guard
            .as_ref()
            .is_some_and(|card| closed == Some(card.dialog))
        {
            self.java_guard = None;
        }
        self.view_dirty = true;
    }

    /// 展开 / 收起详细说明。
    pub(super) fn toggle_dialog_detail(&mut self) {
        if let Some(card) = &mut self.dialog_card {
            card.expanded = !card.expanded;
            self.view_dirty = true;
        }
    }
}
