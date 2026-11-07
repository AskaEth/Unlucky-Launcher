//! 截图 harness：把**一页**按给定窗口宽度 / 主题单独渲染并出图（`examples/pages_demo.rs` 的入口）。
//!
//! **刻意不接 `cli.rs`**：等 `cli.rs` 具备 `--screen` / `--window-width` 之后，这个 harness 应当
//! 删掉，改由正式启动参数出图。参数：`--screen=instances|download|settings|logs`、`--width`、
//! `--height`、`--theme` / `--screenshot-theme`（后者优先）、`--screenshot`、`--renderer`、
//! `--software-renderer`、`--scale-factor`、`--appearance=real|demo`、`--passthrough=on|off`。
//! 解析与默认值在 [`request`]（M100 拆出去：本文件当时正好 400 行、零余量）；没给 `--screenshot`
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
//!    `GWL_EXSTYLE` 读回来（[`caps`] → `platform::win32::transparency::click_through`）。
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
//! 改成 900×600，落盘的就是 900×600 那张）。现在闸门在 [`should_request_capture`]，日志里的
//! `落盘次数=` 必须恒为 1。
//!
//! **为什么不是给每个控件灌 forced**：设置页「外观」区的滑块 / 文本框 / 裸 `iced::button` 都不
//! 消费 forced，那样做要改十几个组件文件——为取证去动产品组件，代价与风险都不对，而且新产品
//! 组件随时会漏。
//!
//! ```text
//! cargo run -p unlucky-app-iced --example pages_demo -- --screen=instances --width=1180 \
//!   --height=760 --screenshot=out.png --screenshot-theme=light --software-renderer
//! ```
//!
//! 等 3 帧再截图与 `app.rs` 同一口径（M60 坑 #5）：第一帧可能是启动时的旧尺寸，字体首次上屏会抖动。

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use iced::{window, Element, Point, Subscription, Task};

use crate::pages::appearance;
use crate::renderer;
use crate::shot;
use crate::theme::Ui;

use super::{app_with_appearance, demo_model, Model};

mod app;
mod caps;
mod request;

pub use app::run;
pub use request::{Request, CAPTURE_POSITION};

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
    /// 不变）。密闭的**主证据**是 `[caps] passthrough_ext=`，见 [`caps`]。
    pointer: u32,
    /// 进程起点（`run()` 的开头）——`[perf]` 的首帧口径与产品路径一致：从进 `run()` 算。
    started: Instant,
    /// 第一帧到达的时刻（相对 `started`）。`None` = 一帧都没到过。
    first_frame: Option<std::time::Duration>,
    /// 出这张图的后端（`wgpu` / `tiny-skia`）。图要能**自证来源**——以前它证不了（M97）。
    backend: &'static str,
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

impl Harness {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Boot => window::latest().map(Message::WindowReady),
            Message::WindowReady(id) => {
                self.window = id;
                // 取证窗口按住**那一刻**就断开指针输入（M100 模块文档「取证密闭」）。
                // 交互模式走不到这里：`request.passthrough` 在没有 `--screenshot` 时恒为 false
                // （`Request::parse` 里显式给值会直接报错，不给值就是 false）。
                let Some(id) = id else {
                    return Task::none();
                };
                let mut tasks: Vec<Task<Message>> = Vec::new();
                if self.request.passthrough {
                    eprintln!("[harness] 指针穿透已开：本窗口不再接收鼠标事件（取证专用）");
                    tasks.push(window::enable_mouse_passthrough(id));
                }
                tasks.push(window::raw_id::<Message>(id).map(Message::RawId));
                tasks.push(window::position(id).map(Message::Placed));
                Task::batch(tasks)
            }
            Message::RawId(raw) => {
                self.hwnd = isize::try_from(raw).ok();
                Task::none()
            }
            Message::Placed(position) => {
                self.placed = position;
                match self.window {
                    Some(id) => window::size(id).map(Message::Sized),
                    None => Task::none(),
                }
            }
            Message::Sized(size) => {
                match self.placed {
                    Some(Point { x, y }) => eprintln!(
                        "[harness] 窗口实测矩形：({x:.0}, {y:.0}) {:.0}×{:.0}（window::position/size 的回值）",
                        size.width, size.height
                    ),
                    None => eprintln!("[harness] 窗口落点取不到（window::position 回 None）"),
                }
                Task::none()
            }
            Message::Frame => {
                if self.frames == 0 {
                    self.first_frame = Some(self.started.elapsed());
                }
                self.frames += 1;
                // 兜底（M100 r2 p3-b）：采样派发了却回不来时，不许挂死也不许 exit 0。
                // 这里用 `std::process::exit(1)` 而不是 `iced::exit()`：iced 0.14 没有「带退出码
                // 结束」的入口，`exit()` 恒 0 —— 而这条的全部意义就是让 CI 看到**红**而不是超时。
                if capture_stalled(self.frames, self.written.load(Ordering::Relaxed)) {
                    eprintln!(
                        "[harness] 采样超时：已过 CAPTURE_AT_FRAME+{CAPTURE_GIVE_UP_FRAMES} 帧\
                         （当前 {} 帧）仍未收到 Captured，放弃并红着退出",
                        self.frames
                    );
                    std::process::exit(1);
                }
                // **一次一采样**（M100 r1 P2-1）：以前这里是 `frames >= CAPTURE_AT_FRAME`，于是第 3 帧
                // 之后**每一帧**都发一次 `window::screenshot`，而 `Captured` 每次都覆盖同一文件才
                // `exit()`——已派发的采样不会因为 exit 被撤回 ⇒ **最后写入者说了算**。复审实测 12 次里
                // 7 次真的写了两次；把窗口在两次采样之间改成 900×600，落盘的就是 900×600 那张。
                // 现在 `dispatched` 一置位就不再请求（`window` 还没就绪时不置位，保留重试，不会挂死）。
                if !should_request_capture(
                    self.frames,
                    self.dispatched,
                    self.request.screenshot.is_some(),
                ) {
                    return Task::none();
                }
                match self.window {
                    Some(id) => {
                        self.dispatched = true;
                        window::screenshot(id).map(Message::Captured)
                    }
                    None => Task::none(),
                }
            }
            Message::Pointer => {
                self.pointer += 1;
                Task::none()
            }
            Message::Captured(screenshot) => {
                let Some(path) = self.request.screenshot.clone() else {
                    return Task::none();
                };
                // 只在真的写成功时计数（U1）：这个计数器（旧字段名叫 `writes`）若在 save **之前**
                // 自增，一次落盘失败也会被记成「有图了」，而 `run()` 的终态不变量正是拿它判「有没有证据」。
                match shot::save(&screenshot, &path) {
                    Ok(summary) => {
                        eprintln!("{}", summary.describe());
                        self.written.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(error) => eprintln!("[harness] 截图失败：{error}"),
                }
                eprintln!(
                    "[harness] slug={} backend={} 落盘次数={} 指针事件={}",
                    self.request.slug(),
                    self.backend,
                    self.written.load(Ordering::Relaxed),
                    self.pointer
                );
                // OS 侧密闭指标（M100 r1 P2-2）：`WS_EX_TRANSPARENT` 在不在。三态，`unknown` 不许
                // 写成 `off`。这一行是「窗口真的把命中测试关掉了」的主证据；`指针事件=` 只是辅助。
                eprintln!(
                    "[caps] passthrough_ext={}（GWL_EXSTYLE 的 WS_EX_TRANSPARENT；unknown = 没读到）",
                    caps::passthrough_label(self.hwnd)
                );
                // `[perf]` 两格都是实测：`first_frame` = 首帧事件的时间戳，`fps` = 帧数 / 时长
                // （**取证窗口**的吞吐，不是长跑稳态帧率——那个看产品路径的 `[tick]`）。
                renderer::acceptance::report_perf_scene(
                    &self.request.slug(),
                    self.frames,
                    self.first_frame,
                    self.started.elapsed(),
                );
                iced::exit()
            }
            Message::Page(message) => {
                eprintln!("[harness] 页面动作：{message:?}（骨架期不做业务）");
                Task::none()
            }
        }
    }

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

    fn theme(&self) -> iced::Theme {
        self.ui.theme()
    }

    fn view(&self) -> Element<'_, Message> {
        app_with_appearance(&self.ui, &self.model, self.appearance.as_ref()).map(Message::Page)
    }
}

#[cfg(test)]
#[path = "harness/tests.rs"]
mod tests;
