//! 应用状态：**只放字段与构造**，不放消息分支（那是 [`super::update`] 的事）。
//!
//! ## presenter 三件（`crates/presenter/src/lib.rs` crate 文档 §4）
//!
//! | 字段 | 谁给 | 干什么 |
//! |---|---|---|
//! | `backend: Box<dyn Backend>` | [`build_backend`](unlucky_presenter::backend::build_backend) | 派发 [`AppCommand`](unlucky_presenter::dto::AppCommand)、收 [`AppEvent`](unlucky_presenter::dto::AppEvent) |
//! | `bridge: SettingsBridge` | [`SettingsBridge::open`](unlucky_presenter::settings_bridge::SettingsBridge::open) | 读一次外观/主题，之后每 N 拍问「文件变了吗」 |
//! | `state: AppState` | [`AppState::new`](unlucky_presenter::state::AppState::new) | **唯一**的状态写入口（`apply` / `navigate`） |
//!
//! `view: pages::Model` 是**展示转换的缓存**：presenter 快照 → 页面要画的东西。
//! 它在**状态真的变了**时重建（[`App::view_dirty`] 门控，M90 r2/P2-1 起），不是每拍重建——
//! 重建要把 N 个实例的名字/版本串克隆一遍，30 Hz 空转没有意义。缓存成字段是为了让
//! `view(&self)` 能只借 `&self`：iced 的 `view` 签名不允许返回一棵借用局部变量的树。
//!
//! ## 本文件只管「有哪些字段」
//!
//! M119 起按功能拆出去（`state.rs` 当时正好 400 行、零余量）：
//!
//! | 子文件 | 内容 |
//! |---|---|
//! | [`construct`] | 两个构造器 + 字段初始化表 |
//! | [`refresh`] | [`App::refresh_view`]（快照 → 视图模型的唯一转换点） |
//! | [`theme`] | `Ui` 重算 / 主题档 / [`App::has_screenshot`] |
//! | [`boot_probe`] | 启动探针（`ULL_*` 取证开关与首帧日志） |

use std::time::Instant;

use iced::window;

use unlucky_presenter::dto::Backend;
use unlucky_presenter::prefs::ThemeMode;
use unlucky_presenter::settings_bridge::SettingsBridge;
use unlucky_presenter::state::AppState;

use crate::cli::Options;
use crate::pages;
use crate::theme::{Families, Ui};
use crate::ui::lang::LangState;

/// presenter 的 TICK 节拍（33 ms ≈ 30 fps）——**忙碌态**的节拍。
///
/// 与 Slint 版的 `TICK_MS` 同值：`backend.tick(..)` 的时间推进、toast 过期、
/// `theme::config_poll_due` 的计数都按这个节拍走，两个 UI 版的时间语义因此一致。
pub(super) const TICK_MS: u64 = 33;

/// [`TICK_MS`] 的 `Duration` 形态。
pub(super) const TICK: std::time::Duration = std::time::Duration::from_millis(TICK_MS);

/// **空闲态**的低频心跳（1 Hz，M90 r3 / P2-2）。
///
/// 空闲时不能订阅 `window::frames()`：那条订阅会自持重绘循环（实测软件渲染约 500 帧/秒），
/// 而空闲时唯一需要的时钟是「偶发一次」——外部 CLI / AI 工具改 `launcher.json` 后的热切
/// （presenter 的 `theme::config_poll_due` 语义是 30 × 33 ms ≈ 1 秒一拍）。
///
/// 实现用的是 `Task::perform(async { sleep(1s) })` 的自续链：[`App::heartbeat`]。它每秒只唤醒
/// 一次、只触发**一帧**重绘，而 `frames()` 是每秒几百帧。为什么不用 `iced::time::every`：
/// 它挂在 tokio/smol 后端下，本 crate 的执行器是 thread-pool（Cargo.toml 不在 scope）。
///
/// **M192 的第三条时钟不在这里**：动效（`components::motion`）也只在「有轨道」时借用帧订阅，
/// 最后一个轨道到期就清空 ⇒ 下一拍 [`App::needs_frames`] 自动退回这条 1 Hz 心跳。
pub(super) const IDLE_TICK: std::time::Duration = std::time::Duration::from_secs(1);

/// 派发指令后保持帧订阅的观察窗（见 [`App::watch_until`]）。
///
/// 3 秒够真 host 把「命令已受理」的第一条事件交回来；之后由「有 Busy/Running 实例」这个条件续订。
pub(super) const WATCH_WINDOW: std::time::Duration = std::time::Duration::from_secs(3);

mod boot_probe;
// M224: 出图取证用的 MODS 夹具（`ULL_MODS_FIXTURE`；产品路径零影响）
mod construct;
mod mods_shot;
mod refresh;
// M235: 出图取证用的 SAVES 夹具（`ULL_SAVES_FIXTURE=1`）
mod saves_shot;
mod theme;

/// iced 侧的 presenter 接线层（M119）：物理目录 `src/presenter/`，逻辑名字在 `crate::app::state` 下。
///
/// 为什么用 `#[path]` 而不是在 `lib.rs` 加一行 `mod presenter;`：本 mission 的 scope 不含
/// `lib.rs` / `app/mod.rs`，而 `shell::agent` 挂 `pages/agent/` 是同一个先例。没有第二份实现。
#[path = "../presenter/mod.rs"]
pub(super) mod presenter;

pub(crate) use boot_probe::{log_first_frame, STARTED};

/// 应用状态：UI 状态 + presenter 三件 + 展示转换缓存。
pub struct App {
    /// 启动参数（截图/主题/画面开关）。
    pub(super) options: Options,
    /// 字体族（切主题时重建 [`Ui`] 要用）。
    pub(super) families: Families,
    /// 令牌求值器（浅/深两套）。
    pub(super) ui: Ui,
    /// 后端通道（真 host 或 mock）。
    pub(super) backend: Box<dyn Backend>,
    /// 设置持久化桥。
    pub(super) bridge: SettingsBridge,
    /// presenter 的全局状态（唯一写入口是 `apply`）。
    pub(super) state: AppState,
    /// 已推进的拍数（`theme::config_poll_due` 的输入）。单位是 **33 ms 一拍**，不是调用次数。
    pub(super) ticks: u64,
    /// 下一次「比对配置文件 mtime」的 tick 号（见 `update::advance` 的注释）。
    pub(super) config_due_at: u64,
    /// 设置文件里的主题模式（`auto` / `light` / `dark`）。
    ///
    /// 用来定优先级：`--theme` > 配置文件 > 系统主题。没有它，`Message::SystemTheme` 会无条件
    /// 盖掉配置文件的显式主题（M90 r3 修）。
    pub(super) theme_mode: ThemeMode,
    /// 界面语言（M106）：用户档位 + 探测结论 + 目录选择。见 [`crate::ui::lang`]。
    ///
    /// 为什么不在 `Ui` 里：`theme.rs` 是令牌求值器（颜色/字号），而语言是**运行期状态**——
    /// 随设置变、要落盘、要重建视图模型（切主题只重建 Style）。
    pub(super) lang: LangState,
    /// 上一次推进节拍的时刻（节拍由帧事件驱动，按真实流逝时间累加）。
    pub(super) last_tick: Instant,
    /// 窗口宽度（布局档位的判据；交互模式跟着 resize 事件走）。
    pub(super) width: f32,
    /// 展示转换缓存（presenter 快照 → 页面视图模型）。
    pub(super) view: pages::Model,
    /// 视图模型是否过期（P2-1）：只有 `apply` / `navigate` / resize / 主题这类**真的改了状态**
    /// 的事情才置位，`advance()` 每拍只在置位时重建模型（否则 30 Hz × N 实例的白克隆）。
    pub(super) view_dirty: bool,
    /// 「刚派发过指令，等后端回事件」的观察窗（P2-1）。
    ///
    /// 空闲态**不订阅帧事件**（只有 1 Hz 的 `heartbeat`），所以派发之后必须有这段短窗口让
    /// 33 ms 节拍跑起来，否则真 host 的异步事件永远抽不出来。
    ///
    /// 空闲态不订阅帧事件（不点亮窗口重绘），但派发之后必须有一段短窗口保持节拍，
    /// 否则真 host 的异步事件永远抽不出来（`LaunchInstance` 之后状态还是 Idle ⇒ 帧订阅永不开启）。
    pub(super) watch_until: Option<Instant>,
    /// 窗口句柄（`window::latest()` 之后才有）。
    pub(super) window: Option<window::Id>,
    /// 已画的帧数（截图闸门用）。
    pub(super) frames: u32,
    /// 视图模型重建次数（P2-1 的可观测计数：空闲态 100 拍里必须是 0 增长）。
    pub(super) refreshes: u64,
    /// 首屏快照到位后是否已经自动选中过第一行（list-detail 的桌面惯例，见 `update::auto_select`）。
    pub(super) auto_selected: bool,
    /// 上一次看到的**中文字体下载状态**（M168）。
    ///
    /// 下载在后台线程里推进，状态是一份进程级快照（`fonts::install::status`）。这里存一份是为了
    /// 「变了才重建视图模型」：设置页那一行因此自己会从「正在获取」翻成「已就绪 / 失败」，
    /// 而空闲态不会为了它每拍白重建一次。
    pub(super) font_seen: crate::fonts::install::CjkInstall,
    /// 窗口高度（背景图平铺网格的视口判据；宽度在 `width`）。
    pub(super) height: f32,
    /// 外观运行期状态（M93）：面板 α / 背景图 / 配色来源 / 诊断。
    pub(super) appearance: pages::appearance::State,
    /// 设置文件里那 8 个「UI 写入」外观字段的最近快照（写回时只改我们真改了的字段，
    /// 没动过的字段原样保留；M250 起强调色也在「真改了」的那一列里）。
    pub(super) appearance_prefs: unlucky_presenter::prefs::AppearancePrefs,
    /// 窗口句柄（`Message::WindowHandle` 之后才有；非 Windows 平台恒 `None`）。
    pub(super) hwnd: Option<isize>,
    /// 设置页各组自己的运行期状态（M101 r2 的**状态漏斗**）。
    ///
    /// 所有者在 `pages/settings/state.rs` 的 `Groups`：加/改某一组的字段只改那一组自己的文件，
    /// 本字段与 `app/view.rs` 都不用动（`settings::States` 只看 `&Groups`，不看任何一组的字段）。
    /// 放在字段列表**末尾**：后续 mission 若也要往这里加字段，冲突面最小。
    pub(super) settings: pages::settings::Groups,
    /// AI 助手栏的 UI 状态（M104）：右栗三档 + 展开宽度 + 草稿/焦点。
    ///
    /// 挂在 `App` 上而不是全局：这棵树在 `view(&self)` 里只读、在 `update(&mut self)` 里改，
    /// 状态跟着 App 走才能 per-app、可测（tower 明确否掉了 `OnceLock<Mutex<..>>`）。
    ///
    /// **只留 UI 态**：会话内容 / 未读 / busy 在 `agent_host` 的 rail 上（M119 起）。
    pub(super) agent: crate::shell::agent::AgentUi,
    /// AI 助手的 presenter 句柄（M119）：四个契约 + 事件队列 + 具名错误。
    ///
    /// 为什么单独一份 `AppState` 而不是把 `state` 交给它：`Presenter` 的可写状态后门
    /// （`state_mut`）只在 presenter crate 的 `#[cfg(test)]` 里存在，而 `app/subscription.rs`
    /// 等处仍要直读 `App.state`。所以口径是——**`App.state` 管除 rail 外的一切，这个句柄管 rail**；
    /// `AppEvent::Agent` 只喂这里（`App.state.ai` 从此不再被读，也不再被写）。
    pub(super) agent_host: presenter::AgentHost,
    /// 下载页的安装草稿（M105：选中版本 + 加载器 + 加载器版本 + 实例名）。
    ///
    /// 为什么住在 `App` 而不是页面模型：`refresh_view` 每次事件都把 `self.view` 整棵重建，
    /// 页面自己存不住「用户正在填但还没确认」的东西；这与分类筛选（`view.download.category`）
    /// 是同一个理由、同一个做法。
    pub(super) download_draft: pages::InstallDraft,
    /// 已经提示过的取消次数（M105）：`AppState::install_cancelled` 是「发生过几次取消」，
    /// 这里是「其中几次已经弹过提示」。两者不等 ⇒ 弹一次 i18n 提示。
    pub(super) install_cancelled_seen: u64,
    /// 已经提示过的启动收尾次数（M161）：`AppState::launch_ends` 是「收尾过几场」，
    /// 这里是「其中几场已经弹过 Toast」。两者不等 ⇒ 弹一次（与 `install_cancelled_seen` 同构）。
    pub(super) launch_ends_seen: u64,
    /// 已经提示过的启停失败（M224）：`(实例, 文件, 大类)`。同一条失败重复到达不重弹。
    ///
    /// 为什么不用计数器（与上面两条不同）：失败是**并发可交错**的（A 的文件失败、B 的列表成功、
    /// A 又失败），计数差说明不了"哪一条没提示过"，三元组才是幂等键。
    pub(super) mods_failed_seen: Option<(String, String, unlucky_presenter::dto::ModFailureKind)>,
    /// 内容库页的页面内选择（M109）：内容类型 / 选中项目 / 目标实例 / 关键词草稿。
    ///
    /// 为什么在 `App` 而不是 presenter：这四样是「用户在这一屏上点了什么」，重建快照不该冲掉；
    /// 而检索条件与结果（会跨页存活、要被事件写）在 `AppState::content`。放进 `view` 模型也
    /// 不行——`refresh_view` 每次都用快照重建模型，选择会被冲回默认值。
    pub(super) content: pages::content::PageState,
    /// 账户页的**表单状态**（三条轨道的输入串；密码只活在这一次请求里）。
    pub(super) account_form: pages::account::Form,
    /// 账户页视图模型（与 `view` 同一条「状态变了才重建」的门控）。
    pub(super) account: pages::account::Page,
    /// 实例页「启动参数」区的表单（M157）：五项 + 脏标记 + 绑定的实例 id。
    ///
    /// 住在 `App` 的理由与 `download_draft` 完全一致：`refresh_view` 每次都整棵重建
    /// `pages::Model`，用户正在输入的内容存不到视图模型里。重播种点在
    /// [`App::refresh_view`]（`LaunchForm::sync`，换实例必重播种、同实例的脏草稿不覆盖）。
    pub(super) launch_form: pages::LaunchForm,
    /// 实例页「导入已有版本」向导的表单态（M223）：选中版本 / 展示名 / 目标档 / 在等回程。
    ///
    /// 住在 `App` 的理由与 `launch_form` 一致：向导跨帧（扫描 → 选 → 预检 → 确认），
    /// 而候选/预检/失败/完成这些**事件真相**在 `AppState.adopt`（presenter），不在这里。
    pub(super) adopt_form: super::adopt::Form,
    /// 启动失败弹窗的**事实**（M187）：与 `state.dialogs` 里的当前条按 id 绑定。
    ///
    /// 为什么事实不塞进 `DialogRequest`：那是 presenter 的公共 DTO，弹窗与「这次失败的现场」
    /// 是两件事——队列管弹窗的排队与按钮，卡片管证据（路径 / 崩溃摘要 / 捕获输出）。
    pub(super) dialog_card: Option<super::dialog::CrashCard>,
    /// 弹窗的**视图模型**（在 [`App::refresh_view`] 里重建；`view()` 只借它）。
    ///
    /// 为什么不像页面那样现算：`Element` 借字符串，现算的模型是局部变量，`view()` 一返回就
    /// 悬空（编译期就过不去）。与 `view` / `account` 两张缓存同一条理由、同一个门控。
    pub(super) dialog_model: Option<super::dialog::Modal>,
    /// 皮肤/头像解码缓存（M114：同一张皮肤只解一次）。
    pub(super) avatars: pages::account::avatar::Cache,
    /// 上一次**真下发过**的圆角判定指纹（M116 r1 复审 P2-4：判定没变就不重下、不重记日志）。
    ///
    /// `None` = 还没下发过（首屏 `boot` 那一次）。指纹类型见
    /// [`crate::platform::win32::corner::CornerApplied`]。
    pub(super) corner_applied: Option<crate::platform::win32::corner::CornerApplied>,
}

impl App {
    /// iced 侧的**设置组快照**（M201）：presenter 的 `CoreEvent::SettingsChanged` 回流进来的那一份。
    ///
    /// 真值只有一份——presenter 的 `AppState::settings_groups`（`apply_core` 是它唯一的写入口，
    /// 由 host 的 `refresh_all` / `SetSetting` 分支发出来）。这里**不另存一个字段**：那会变成第二份
    /// 真相，两份在 `pump` 之间必然有一步不一致（而「界面上是旧值、盘上是新值」正是本 mission 要
    /// 消灭的形状）。要画的那一份是 [`pages::Model::settings`]（`refresh_view` 里从同一个字段克隆）。
    ///
    /// `#[cfg(test)]`：产品路径不需要这个入口（页面拿的是 `Model.settings`，同一份数据的投影），
    /// 断言需要它——「App 认的值变了吗」必须能直接从 App 上问，不能去问页面模型（那是下游）。
    #[cfg(test)]
    pub(crate) fn settings_snapshot(&self) -> &[unlucky_presenter::dto::SettingGroupView] {
        &self.state.settings_groups
    }
}
