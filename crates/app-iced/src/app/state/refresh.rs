//! 快照 → 视图模型的重建口径（从 `state.rs` 搬出来守 400 行硬线；M119 拆，纯搬不改）。
//!
//! 全仓只有 [`App::refresh_view`] 这一个「presenter 快照 → 页面要画的东西」的转换点，
//! 由 [`App::view_dirty`](super::App) 门控（M90 r2/P2-1 起）：只有真的改了状态的事情才置位，
//! 30 Hz 空转不重建（重建要把 N 个实例的名字/版本串克隆一遍）。

use unlucky_presenter::dto::{AppCommand, ModsCommand, SavesCommand};

use super::{mods_shot, saves_shot};
use unlucky_presenter::toast::ToastKind;

use crate::app::view;
use crate::pages;

use unlucky_presenter::router::Route;

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

        // M161：启动收尾的提示语也在这里弹（presenter 只发结构：结局 + 退出码 + 失败阶段）。
        // 此前 presenter 直接推一条自己拼好的中文 Toast，英文界面会弹中文。
        if self.state.launch_ends != self.launch_ends_seen {
            self.launch_ends_seen = self.state.launch_ends;
            if let Some(finished) = self.state.last_launch_end.as_deref() {
                let text = view::instances::launch_toast(finished, self.lang());
                let kind = if finished.outcome == unlucky_presenter::dto::LogOutcome::Ok
                    && finished.failure.is_none()
                {
                    ToastKind::Success
                } else {
                    ToastKind::Error
                };
                self.state.toasts.push(kind, text);
                // M187：失败要**弹窗**（Toast 会自己消失，崩溃现场不能只靠它）。
                let ended = finished.clone();
                self.open_failure_dialog(&ended);
            }
        }

        // M224：详情页 ⇄ MODS 区的绑定与取数（换实例就重新列一次 mods/）。
        // **必须在建模型之前**：它会把快照写进 presenter 状态，而下一行 `model()` 就读那份状态
        // （放在 `model()` 之后等于这一帧画的是旧快照——M224 出图实测：注入的夹具要等下一帧）。
        self.sync_mods();
        // M235：详情页 ⇄ SAVES 区的绑定与取数（换实例就重新列一次存档）。同样是**建模型之前**。
        self.sync_saves();
        // M224：启停失败的提示语（presenter 只发结构，文案在这里按 i18n 现求）。
        self.note_mods_failure();
        // M247（复审 r1 P2-1）：被确认的世界已经从快照里消失 ⇒ 收起那个确认框。
        // 不这样的话主按钮就是一次静默空操作（点了没删、也不说一句）。
        self.settle_saves_confirm();

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
            self.height,
            self.options.force_state,
            category,
            &draft,
            self.lang(),
            &self.content,
        );
        // M157：启动参数表单的重播种（换实例必重播种、同实例的脏草稿不覆盖）。
        self.sync_launch_form();
        // 账户页（M114）：与 `view` 同一次门控重建（实现见 `view/account_page.rs`，
        // 皮肤解码走缓存，所以重复重建不重解图）。
        self.rebuild_account_page(width);
        // M187：弹窗的视图模型也在这一处重建——`view()` 只借 `&self`，放不进局部变量
        // （Element 借字符串，局部一死树就悬空）。全仓「快照 → 视图模型」只在这里发生。
        self.dialog_model = self.dialog_modal();
        self.view_dirty = false;
        self.refreshes += 1;
    }

    /// 收掉「被确认的世界已经不在快照里」的那个删除确认框（M247，复审 r1 P2-1）。
    ///
    /// 判据由 presenter 给（`SavesState::take_closed_confirm`：那个世界从快照里消失了），
    /// **框的 id 也由它带着**——所以只收我们自己那个框，崩溃卡片/进度框一概不碰。
    ///
    /// 为什么必须收：目标已经作废，主按钮就成了一次**静默空操作**（用户点了「删除世界」，
    /// 框关了、什么也没发生、也没一句解释）。少删是安全的，但"看起来坏了"不是可接受的结果。
    fn settle_saves_confirm(&mut self) {
        let Some(dialog) = self.state.saves.take_closed_confirm() else {
            return;
        };
        if self
            .state
            .dialogs
            .current()
            .is_some_and(|current| current.id == dialog)
        {
            self.close_dialog();
        }
    }

    /// 把详情页当前实例的启动参数快照灌进表单（唯一的重播种点，见 `pages/instances/state.rs`）。
    ///
    /// 不在详情页（或实例已不在快照里）时解绑：下次进详情页会重新播种，
    /// 免得「A 的草稿被当成 B 的初值」。
    fn sync_launch_form(&mut self) {
        let Route::Detail(id) = self.state.route() else {
            self.launch_form.bound = None;
            return;
        };
        match self
            .state
            .instances
            .iter()
            .find(|instance| &instance.id == id)
        {
            Some(instance) => {
                let view = instance.launch.clone();
                self.launch_form.sync(&view);
            }
            None => self.launch_form.bound = None,
        }
    }

    /// 详情页 ⇄ MODS 区的绑定与取数。
    ///
    /// 判据只有一条：**绑定变了**（进详情页 / 换实例 / 退出详情页）。
    /// - 绑定函数自己会清掉旧快照（A 的列表不许画在 B 的详情页上，见 presenter 的 `ModsState::bind`）；
    /// - 绑定之后手上没有快照 ⇒ 派一条 `AppCommand::Mods(List)` 去后台列目录。
    ///
    /// 为什么放在 `refresh_view` 里：它是**唯一**的「路由变了」漏斗（点行、返回、首屏自动选中都走它）。
    /// 派发会再进一次 `refresh_view`（`dispatch` → `pump` → 重建），但那时 `bound` 已经相等，
    /// 这一支直接返回，不会重复发请求、也不会无限递归。
    fn sync_mods(&mut self) {
        let wanted = match self.state.route() {
            Route::Detail(id) => Some(id.clone()),
            _ => None,
        };
        if self.state.mods.bound == wanted {
            return;
        }
        self.state.bind_mods(wanted.as_deref());
        let Some(id) = wanted else {
            return;
        };
        // 出图取证（`ULL_MODS_FIXTURE=1`）：后台往返赶不上第 3 帧出图，注入一份固定快照。
        // 产品路径读不到这个变量 ⇒ 下面这行什么都不做（见 `mods_shot` 的模块文档）。
        if mods_shot::enabled() {
            self.state.mods.changed(mods_shot::view(&id));
            return;
        }
        // **不走 `App::dispatch`**：那条路会为「等第一条事件」把帧订阅打开 3 秒（`watch_until`），
        // 而这里只是"进详情页顺手后台列一次目录"——结果在下一拍心跳（1 Hz）落地就够了。
        // 走观察窗会把 M192 的「空闲不烧帧」守卫打红（`subscription` 的三条用例立刻红：实测过）。
        self.backend
            .dispatch(AppCommand::Mods(Box::new(ModsCommand::List(id))));
    }

    /// 详情页 ⇄ SAVES 区的绑定与取数（M235）。
    ///
    /// 判据与 `sync_mods` **一字不差**：绑定变了（进详情页 / 换实例 / 退出详情页）才动；
    /// 绑定之后手上没有快照 ⇒ 派一条 `AppCommand::Saves(List)` 去后台列目录。
    ///
    /// 不弹 Toast：存档区的失败就地画在卡片里（带「重试」），再弹一条是同一件事说两遍。
    fn sync_saves(&mut self) {
        let wanted = match self.state.route() {
            Route::Detail(id) => Some(id.clone()),
            _ => None,
        };
        if self.state.saves.bound == wanted {
            return;
        }
        self.state.bind_saves(wanted.as_deref());
        let Some(id) = wanted else {
            return;
        };
        // 出图取证（`ULL_SAVES_FIXTURE=1`）：后台往返赶不上第 3 帧出图，注入一份固定快照。
        // 产品路径读不到这个变量 ⇒ 下面这行什么都不做（见 `saves_shot` 的模块文档）。
        if saves_shot::enabled() {
            saves_shot::seed(self, &id);
            return;
        }
        // **不走 `App::dispatch`**：那条路会为「等第一条事件」把帧订阅打开 3 秒，
        // 这里只是"进详情页顺手后台列一次目录"，结果在下一拍心跳落地就够了（M224 同款理由：
        // 走观察窗会把 M192 的「空闲不烧帧」守卫打红）。
        self.backend
            .dispatch(AppCommand::Saves(Box::new(SavesCommand::List(id))));
    }

    /// 启停失败的提示语：每条**新**失败弹一次 Toast。
    ///
    /// 判据是 `(实例, 文件, 大类)` 三元组：同一个文件连点两次、每次都失败在同一个大类上时，
    /// **只弹一次**——第二次是同一件事的重复，再弹就是刷屏（用户已经看见了那句红字）。
    /// 换了文件、换了实例、或失败大类变了（例如"不覆盖冲突"变成"被占用"），三元组就变了，
    /// 于是照样会再弹一次：提示语跟的是"新事实"，不是"点击次数"。
    fn note_mods_failure(&mut self) {
        let Some(failure) = self.state.mods.failed.as_ref() else {
            self.mods_failed_seen = None;
            return;
        };
        let key = (failure.instance.clone(), failure.file.clone(), failure.kind);
        if self.mods_failed_seen.as_ref() == Some(&key) {
            return;
        }
        let text = pages::mods_failure_line(failure, self.lang().catalog());
        self.state.toasts.push(ToastKind::Error, text);
        self.mods_failed_seen = Some(key);
    }
}
