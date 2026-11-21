//! 截图 harness：把**一页**按给定窗口宽度 / 主题单独渲染并出图（`examples/pages_demo.rs` 的入口）。
//!
//! **刻意不接 `cli.rs`**：等 `cli.rs` 具备 `--screen` / `--window-width` 之后，这个 harness 应当
//! 删掉，改由正式启动参数出图。参数：`--screen=instances|download|settings|logs`、`--width`、
//! `--height`、`--theme` / `--screenshot-theme`（后者优先）、`--screenshot`、`--renderer`、
//! `--software-renderer`、`--scale-factor`、`--appearance=real|demo`、`--passthrough=on|off`、
//! `--force-state=rest|hover|pressed`（M285：把控件钉到那一态出图，`rest` / 不给时截图通道默认就是它）。
//! 解析与默认值在 [`Request::parse`]（M100 拆出去：本文件当时正好 400 行、零余量）；没给 `--screenshot`
//! 就只是开窗看。
//!
//! ## 取证密闭（M100）：取证图不许跟着系统指针变
//!
//! M99 复审实测过：同一个 exe 连跑 8 次，第 6 次给出**第三个 hash**，差异区是某个按钮被画成 Hovered
//! ——根因是 harness 开真窗口，`button::Status` 由**系统光标位置**决定。
//!
//! 于是 `--screenshot` 有一条**通道级**钉死：开窗那一刻调 `window::enable_mouse_passthrough`
//! （winit 的 `set_cursor_hittest(false)`）⇒ 窗口从此收不到指针事件 ⇒ 全树控件恒为 `Active`。
//!
//! ⚠️ **限定**：上句的因果链在**有指针输入的会话**里才被实测过。本机（RDP 断开）连
//! `SetCursorPos` 都失败，所以「`指针事件=0`」在本环境里**既可能是穿透生效，也可能是根本没输入**。
//! 要让 `指针事件=N` 变成密闭证据，必须同一 argv 再跑一次 `--passthrough=off` 做对照：
//! 那一跑该给出 `N>0`（复审注入后实测 12）**并且** hash 与开穿透时不同。没有这个对照，
//! `指针事件=0` 就退回成「没观测到输入」，而不能读成「输入被挡住了」。
//!
//! ### 证据怎么读（M100 r1 复审订正，优先级从高到低）
//!
//! 1. **`[caps] passthrough_ext=on|off|unknown`** ← 主证据。winit 0.30 在 Windows 把
//!    `set_cursor_hittest(false)` 落成 `WS_EX_TRANSPARENT | WS_EX_LAYERED`，这一位可以从
//!    `GWL_EXSTYLE` 读回来（`caps` → `platform::win32::transparency::click_through`）。
//!    复审探针实测：`--screenshot` 默认 176/214 次采样命中、`--passthrough=off` **0/320**、
//!    产品 bin **0/535**。三态里的 `unknown` 是「没读到」，**不许**读成 `off`。
//! 2. `指针事件=N`：**辅助**指标。它证明「鼠标消息到没到」，**不**证明「像素会不会变」
//!    （复审实测：注入 12 个事件而像素不变）。别拿它当密闭证据。
//! 3. ~~hash 连跑几次都一样~~：**不是证据**。同 argv 加 `--passthrough=off` 出来的 hash 与
//!    开穿透时完全相同 ⇒ 它只能说明「那几帧恰好静止」。
//!
//! ### 同一族的另一个坑：一次一采样（P2-1）
//!
//! 以前第 3 帧之后**每一帧**都请求一次采样、每次都覆盖同一文件才 `exit()`，而已派发的采样不会
//! 因为 exit 撤回 ⇒ **最后写入者说了算**（复审实测 12 次里 7 次写了两次；把窗口在两次采样之间
//! 改成 900×600，落盘的就是 900×600 那张）。现在闸门在 `should_request_capture`，日志里的
//! `落盘次数=` 必须恒为 1。
//!
//! **forced 能到哪儿（M285 订正）**：`--force-state` 只改**画出来的**态，不改状态机。它经
//! `pages::Model.forced` → `settings::view(.., forced)` → `Group.body` 递到各控件。
//!
//! **已经消费的**：外观区 11 处 `choice`、语言 3 枚 / 主题 3 枚 chip、日志 2 颗 `action_button`
//! ——这几类（`choice` / chip / `action_button`）一律走 `settings::resolve_state` 这个唯一 resolve 点；
//! 另有 `components::buttons::Button` 的 7 处 `.state(..)` 走 `settings::forced_button`
//! （update 3 / ai 2 / java 2）。
//!
//! **仍然不消费的（残留快照——**别**把它们当成「已接」）**：
//!
//! 1. **Downloads 组整组**：`settings/download.rs` 的 `body` 形参就是 `_forced`（接住不用），组里的
//!    `switch` / `chip` / 两颗 `Button::new` 都不看 forced。实测 `--screen=download` 加
//!    `--force-state=rest|hover|pressed` **三张图 md5 全等**（`e283c585fc06a0dd08efaf5596c9aa6f`）；
//! 2. 外观区的**滑块 / 文本框 / 9 颗预设色块**（`ACCENT_PRESETS: [&str; 9]`；`accent::swatch` 是
//!    本地助手，没有 forced 形参，描边闭包只看 `button::Status`）；
//! 3. `components::selection` 的 `chip` / `switch` 站点里**还没搭线的那几处**（已知：update 的自动
//!    检查 filter chip、ai 的 provider chip / reveal chip / 开关、download 的 switch 与 chip、java 的
//!    `prefs_view` 两对上下移 chip 与 `download_view` 的三处 chip、以及 **`java/view.rs` 运行时的
//!    「设为默认」chip**——`action_chip` 在非默认行返回可点的 `Chip::filter`，`runtime_row` 没有
//!    forced 形参。这一串会漂，接线时请更新）。**同源 chip 里 `general/language.rs` 与
//!    `general/theme.rs` 那 6 枚是已接的**（见上一段），别算进这里；
//! 4. java 的字段 / 预设按钮（`java/fields.rs`、`java/prefs_view.rs`）——`java/download_view.rs` 里
//!    一个 `Button` 都没有，它的下载控件是 chip，已在第 3 条；
//! 5. **其它组的文本框**：`ai/view.rs` 的 base_url / model / api_key 三只 `Field`（java 的两样在第 4
//!    条、download 的整组在第 1 条）。`Field` 本来就有 `.state(..)`，这里是没搭线。
//!
//! 这份清单是**残留快照，不是契约**：接上任何一处就把它从清单里划掉，别让它变成「文档说没接所以
//! 不用接」；也别把它当穷举——判据只有一条：**这个控件有没有拿到 `forced`**。
//!
//! ⚠️ **别把「没搭线」说成「组件库缺能力」**：`Slider::state` / `Field::state` / `Chip::state` /
//! `Switch::state` 全都存在且**在被消费**（它们本来就是 gallery 状态矩阵用的「强制视觉态」）。
//! 上面那些站点只是没把 `forced` 递进去；真正缺 forced 入口的只有本地助手 `accent::swatch`。
//!
//! 本段之前写的是「裸 `iced::button` 都不消费 forced」——**那句正是 M282/M284 放弃像素验收的理由**，
//! 已经不成立（残余清单之外的那几类现在出得来像素证据，见 M285 的 `DevDocs/artifacts/forcedwire/`）。
//! 但也**别**反过来把「裸 `button(...)`」当成「已消费」的同义词：9 颗预设色块本身就是裸
//! `button(...)` 且不消费。
//!
//! ```text
//! cargo run -p unlucky-app-iced --example pages_demo -- --screen=instances --width=1180 \
//!   --height=760 --screenshot=out.png --screenshot-theme=light --software-renderer
//! ```
//!
//! ## 交互后态：`--after=<action>`（M361 r3）
//!
//! 这个通道以前只出**首帧**——能证明「静态版面长什么样」，证明不了「点了之后用户看到什么」。
//! `--after` 让它在采样**之前**派**一次**页面层动作，于是滚动位置这类运行期状态第一次有了像素证据：
//!
//! ```text
//! pages_demo --screen=content --width=880 --height=900 --screenshot=out.png \
//!   --screenshot-theme=dark --software-renderer --after=content-select
//! ```
//!
//! | 项 | 说明 |
//! |---|---|
//! | 时机 | **第 3 帧派动作 → 第 4 帧请求采样 → 第 5 帧落盘**（无 `--after` 时是第 3 帧请求、第 4 帧落盘；`[perf] frames=` 实测） |
//! | 词表 | **只有** `content-select` = `pages::content::snap_to_detail()`（与 `pages/content/message.rs::Select` 派的是同一个函数） |
//! | 页 | **只对 `--screen=content` 有效**；别的页**报错**（打过去是静默空转：日志说派了，图与首帧逐字节相同） |
//! | 没有 `--screenshot` | **报错**，不静默忽略：交互模式没有帧节拍，「开机后」这个时刻不存在 |
//! | 派几次 | 一次（`after_fired`）。多派几次就不叫「一次交互之后」了 |
//!
//! 为什么排成这个时机（与 `CAPTURE_AT_FRAME` 的关系、隔一帧的作用）见 [`request::After`] 的文档。
//!
//! ⚠️ **它不是事件回放框架，别扩成那样**：不支持点击坐标 / 键盘 / 多步序列 / 消息队列。它只回答
//! 一类问题——「某个**运行期状态**（滚动位置、焦点）变化之后，这一屏画出来是什么样」。要加第二项，
//! 先问「这个动作能不能用一个**页面层函数**表达」；不能（需要真 `App` / presenter）就别塞这里。
//!
//! 它也**不**等于跑了一遍真 `App`：harness 没有 `App`，`Select` 的状态副作用（`request_detail` /
//! `view_dirty`）在这里不发生。「`Select` 真的派了这个动作」由 `app/update/content/tests.rs` 的
//! Task 计数用例钉住——图证明「派了之后长这样」，用例证明「点一下会派」。
//!
//! 等 3 帧再截图与 `app.rs` 同一口径（M60 坑 #5）：第一帧可能是启动时的旧尺寸，字体首次上屏会抖动。

use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::Instant;

use iced::{window, Point, Subscription};

use crate::pages::appearance;
use crate::theme::Ui;

use super::instances;
use super::{demo_model, Model};

mod app;
mod caps;
mod request;
// M416: 状态机主体（`Harness` 的 `update` / `theme` / `view`）——贴线拆分；`subscription` 仍在
// 本文件（见它上面的注释：`app/subscription.rs` 那条守门按**文件**数 `window::frames()`）
mod state;

pub use app::run;
pub use request::{After, Request, CAPTURE_POSITION};

/// 截图落盘的帧号（见模块文档）。
const CAPTURE_AT_FRAME: u32 = 3;

/// 这一帧要不要请求采样？
///
/// **M100 r1 P2-1 的核心判据**：`dispatched` 一旦置位就永不再次请求。以前这里是
/// `frames >= CAPTURE_AT_FRAME`，于是第 3 帧之后**每一帧**都发一次 `window::screenshot`，
/// 而 `Captured` 每次都覆盖同一文件才 `exit()`——已派发的采样不会因为 exit 被撤回，
/// 落盘内容变成调度结果（复审实测 12 次里 7 次写了两次）。
///
/// 单独拎成自由函数是为了让它可被断言（`Harness::update` 要 iced 运行时才能跑）。
fn should_request_capture(frames: u32, dispatched: bool, capture_mode: bool) -> bool {
    capture_mode && !dispatched && frames >= CAPTURE_AT_FRAME
}

/// 采样「派发了却回不来」的宽限帧数（M100 r2 p3-b）。
///
/// 为什么需要它：一次性闸门（上面那条）把「每帧重试」换成了「只试一次」，于是若
/// `window::screenshot` 的 Task 回不来，harness 既不再采样也不 `exit()` ⇒ CI 上表现成**超时**，
/// 而不是红灯。超时会被当成基础设施抖动重跑过去，红才会被人看。
///
/// ⚠️ **这条兜底只覆盖「还有帧」的情形**（M100 r3 P2-1 订正，我原来在这里写反了）：
/// `window::frames()` 只在 `Event::Window(RedrawRequested)` 时产消息，**窗口消失 ⇒ 不再有帧
/// ⇒ 这里的判据根本求值不到**。那一路由 `run()` 里的**终态不变量**兜住（跑完没图 ⇒ 红着退出）；
/// 而后端报错是另一回事：`run()` 直接返回 `Err`，本来就是非 0。
///
/// 30 帧**不换算成秒**（M100 r4 P3-1）：帧间隔在不同机器/负载下量级差异极大——复审实测 2~3ms/帧，
/// 而本机 `[perf]` 日志里是 `frames=4 elapsed=1133ms` / `frames=5 elapsed=1605ms`（≈200~300ms/帧）。
/// 所以这里只承诺「若干帧的宽限」，不写时间预算；要调就按「正常路径是第 3 帧派发、第 4~5 帧落盘」
/// 这个结构关系调，别按秒。
const CAPTURE_GIVE_UP_FRAMES: u32 = 30;

/// 采样是不是**卡住了**：过了宽限帧数，一次都没落盘。
///
/// 不区分「一直没派发」（窗口始终没就绪）与「派发了没回来」——两者对使用者的表现都是
/// 「没有图」，都该红着退出而不是挂死。
fn capture_stalled(frames: u32, written: u32) -> bool {
    written == 0 && frames > CAPTURE_AT_FRAME + CAPTURE_GIVE_UP_FRAMES
}

struct Harness {
    request: Request,
    ui: Ui,
    /// 视图模型（演示数据；harness 不接 presenter，只出**单页**图）
    model: Model,
    /// 外观运行期状态（`--appearance=real`）；`None` = 设置页画演示行。
    appearance: Option<appearance::State>,
    /// 实例页「启动参数」区的表单（M157）：harness 没有 `App`，自己持有演示表单。
    form: instances::LaunchForm,
    window: Option<window::Id>,
    /// 窗口原生句柄（`window::raw_id` 的回值）。OS 侧密闭指标要在**真句柄**上读，
    /// 拿不到就留 `None` ⇒ 日志记 `unknown`（不是 `off`）。
    hwnd: Option<isize>,
    /// 窗口落点（`window::position` 的**返回值**，不是请求值）。
    placed: Option<Point>,
    frames: u32,
    /// 采样**已派发**（M100 r1 P2-1）：置位后不再请求第二张，避免「最后写入者说了算」。
    dispatched: bool,
    /// **成功落盘**的次数（共享给 [`run`]，终态不变量读它）。
    ///
    /// 只在 `shot::save` 返回 `Ok` 之后自增：这是「真的写出去了」，不是「我们打算写」（U1）。
    /// 正常必须恰好 1；打进日志让复审能数。
    written: Arc<AtomicU32>,
    /// 本张图收到过几个鼠标事件（`Message::Pointer` 的计数）。
    ///
    /// ⚠️ 它只证明「鼠标消息到没到」，**不**证明「像素会不会变」（复审实测：注入 12 个事件而像素
    /// 不变）。密闭的**主证据**是 `[caps] passthrough_ext=`，见 `caps`。
    pointer: u32,
    /// 进程起点（`run()` 的开头）——`[perf]` 的首帧口径与产品路径一致：从进 `run()` 算。
    started: Instant,
    /// 第一帧到达的时刻（相对 `started`）。`None` = 一帧都没到过。
    first_frame: Option<std::time::Duration>,
    /// 出这张图的后端（`wgpu` / `tiny-skia`）。图要能**自证来源**——以前它证不了（M97）。
    backend: &'static str,
    /// `--after` 已经派过了吗（M361 r3）。**只派一次**：多派几次就不再是「一次交互之后」。
    after_fired: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Boot,
    WindowReady(Option<window::Id>),
    /// `window::raw_id` 的回值（原生句柄，OS 侧密闭指标要它）。
    RawId(u64),
    /// `window::position` 的回值。
    Placed(Option<Point>),
    /// `window::size` 的回值。
    Sized(iced::Size),
    Frame,
    Captured(window::Screenshot),
    /// 收到一个指针事件（只计数，不画；见 [`Harness::subscription`]）。
    Pointer,
    /// 页面里的动作（骨架期只打印，接线后由 app 层处理）。
    Page(super::Message),
}

/// 帧节拍 + **指针事件计数**两样都订阅的那一个方法。
///
/// M416：状态机其余三个方法（`update` / `theme` / `view`）搬去了 [`state`]，这一个**留在本文件**：
/// `app/subscription.rs::there_is_exactly_one_frame_subscription` 按**文件**数 `window::frames()`
/// 的出现次数（期望 `pages/harness.rs` 恰好 1 处），搬走会让那条守门红——那是测试断言，不是本
/// mission 能改的面。函数体逐字未动。
impl Harness {
    fn subscription(&self) -> Subscription<Message> {
        if self.request.screenshot.is_none() {
            return Subscription::none();
        }
        // 帧节拍 + **指针事件计数**两样都订阅。后者是 M100 加的：以前「取证密闭」靠
        // 「同一场景连跑几次 hash 一样」证明，而进程压根收不到鼠标时那条恒真（U5 的同族假绿）。
        Subscription::batch([
            window::frames().map(|_| Message::Frame),
            iced::event::listen_with(|event, _status, _window| match event {
                iced::Event::Mouse(_) => Some(Message::Pointer),
                _ => None,
            }),
        ])
    }
}

#[cfg(test)]
#[path = "harness/tests.rs"]
mod tests;
