//! 截图 harness 的**窗口状态机**：`Harness` 的 `update` / `theme` / `view`。
//!
//! ⚠️ `subscription` **不在**本文件：它留在 `../harness.rs`。M450（#25）之前那里有一条**按文件名**
//! 数 `window::frames()` 的守门（期望 `pages/harness.rs` 恰好一处），搬进来会红；现在判据改成
//! 「落在哪个模块」，`pages/harness/**` 与 `pages/harness.rs` 同属 harness，所以搬进来不再红——
//! 只是还没搬（`../harness.rs` 那个 `impl Harness` 上面的注释说的是改判据之前的理由）。
//!
//! M416：从 `../harness.rs` 整块搬出（那边贴着 400 行红线），**只搬不改**——函数体逐字不动，
//! 可见性只有一处变化：三个方法（`update` / `theme` / `view`）从 `fn` 变成 `pub(super) fn`
//! （`app.rs` 本来就把它们当函数指针跨模块取用，此前能取到是因为它是 `harness` 的子模块）。
//!
//! `Harness` 的**字段一个没动**，还留在 `super` 里：子模块读祖先模块定义的私有字段是合法的，
//! 所以这次搬移不需要给任何字段加可见性。`CAPTURE_AT_FRAME` / `should_request_capture` /
//! `CAPTURE_GIVE_UP_FRAMES` / `capture_stalled` 这组采样闸门也留在 `super`（`tests.rs` 直接读它们）。

use std::sync::atomic::Ordering;

use iced::{window, Element, Point, Task};

use crate::pages::app_with_appearance;
use crate::renderer;
use crate::shot;

use super::{
    app, caps, capture_stalled, should_request_capture, Harness, Message, CAPTURE_GIVE_UP_FRAMES,
};

impl Harness {
    pub(super) fn update(&mut self, message: Message) -> Task<Message> {
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
                        // M361 r3：`--after` 在**采样之前**派（第 1 帧派、第 3 帧拍）。`snap_to` 落的是
                        // 相对偏移，下一帧的 layout 就把它解算成绝对位置 ⇒ 派完当帧生效，不必等动画。
                        if let (Some(action), false) = (self.request.after, self.after_fired) {
                            self.after_fired = true;
                            eprintln!(
                                "[harness] --after={}：第 {} 帧派发（交互后态取证）",
                                action.name(),
                                self.frames
                            );
                            return app::after_task(action);
                        }
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
    pub(super) fn theme(&self) -> iced::Theme {
        self.ui.theme()
    }

    pub(super) fn view(&self) -> Element<'_, Message> {
        app_with_appearance(&self.ui, &self.model, self.appearance.as_ref(), &self.form)
            .map(Message::Page)
    }
}
