//! 快照 → 视图模型的重建口径（从 `state.rs` 搬出来守 400 行硬线；M119 拆，纯搬不改）。
//!
//! 全仓只有 [`App::refresh_view`] 这一个「presenter 快照 → 页面要画的东西」的转换点，
//! 由 [`App::view_dirty`](super::App) 门控（M90 r2/P2-1 起）：只有真的改了状态的事情才置位，
//! 30 Hz 空转不重建（重建要把 N 个实例的名字/版本串克隆一遍）。

use unlucky_presenter::toast::ToastKind;

use crate::pages;

use super::App;

impl App {
    /// 把 presenter 的快照转换成页面视图模型（**展示转换在 UI 侧做**）。
    ///
    /// 代价是「克隆 N 个实例的名字/版本串」，所以只在 [`App::view_dirty`] 置位时调用（P2-1）。
    pub(in crate::app) fn refresh_view(&mut self) {
        // **不变式先对齐再算宽度**（M104 / 复审 r1 P2-1）：`Route::Agent` ⟺ `RailState::Detached`。
        // 每一次路由变化都会走到这里，所以它是「档位 ↔ 路由」的唯一点；放在算宽度之前，
        // 是为了让模型拿到的 `content_width` 与页面头部读到的档位来自同一份状态。
        self.sync_agent_route();
        // M119：把「栏可见吗」写回 presenter（未读口径的输入，两个窄入口：`navigate` + `prefs_mut`）。
        // 必须在 `sync_agent_route` 之后：档位刚被它对齐过，这里读到的才是这一帧的真值。
        self.sync_agent_presenter();

        // M105：取消安装的提示语在**这里**弹（`refresh_view` 是事件 apply 之后必经的那一处）。
        // 文案由页面层求（`pages::cancelled_toast`），presenter 只发信号 ——
        // 否则会出现「选项区英文、取消弹窗中文」的同屏两语（复审 P2-2）。
        if self.state.install_cancelled != self.install_cancelled_seen {
            self.install_cancelled_seen = self.state.install_cancelled;
            // M106：提示语跟着**生效语言**走（M105 留的 `view::ui_lang()` 占位已删——它恒英文，
            // 中文界面会弹出英文提示，那正是 M105 r1 复审 P2-2 要堵的同屏两语）。
            let text = pages::cancelled_toast(self.lang());
            self.state.toasts.push(ToastKind::Info, text);
        }

        // 分类是**页面自己持有的过滤条件**（真 host 丢弃 `ListVersions` 的分类参数）：
        // 重建视图模型时带回去，否则每次事件都会把 chip 打回「全部」（复审 P2-1）。
        let category = self.view.download.category;
        // 草稿同理（M105）：它也是页面自己持有的输入，重建模型时带回去，
        // 否则每来一条事件（连安装进度都算）用户填的加载器就被抹回「原版」。
        let draft = self.download_draft.clone();
        // 右栏占掉的宽度先扣掉：面板展开时页面按更窄的宽度排版（「收缩而不是溢出」，M104）
        let width = self.agent.content_width(self.width);
        // M106：语言形参跟着 `LangState` 走（`draft` 那一位是 M105 的安装草稿）。
        self.view = super::super::view::model(
            &self.state,
            width,
            self.options.force_state,
            category,
            &draft,
            self.lang(),
            &self.content,
        );
        // 账户页（M114）：与 `view` 同一次门控重建（实现见 `view/account_page.rs`，
        // 皮肤解码走缓存，所以重复重建不重解图）。
        self.rebuild_account_page(width);
        self.view_dirty = false;
        self.refreshes += 1;
    }
}
