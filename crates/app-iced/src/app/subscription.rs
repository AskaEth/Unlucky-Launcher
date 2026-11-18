//! 订阅：**按需**帧节拍（presenter 的 TICK）+ 窗口尺寸事件。
//!
//! ## 为什么要「按需」
//!
//! iced_winit 在处理完任何一条消息后都会 `request_redraw()`，而 `window::frames()` 监听的是
//! `RedrawRequested` —— 于是「订阅帧 → 收 Frame → update → 再触发重绘」是一条**自持循环**：
//! 只要订阅着，窗口就永远在重绘（M90 r1 就是这样，CPU 空转）。
//!
//! 现在只在**真的需要 33 ms 节拍**的时候订阅（[`App::needs_frames`]）：
//!
//! | 条件 | 为什么需要 33 ms 节拍 |
//! |---|---|
//! | 截图通道 | 第 3 帧要落盘 |
//! | **有动画在跑**（M192） | 动效时间轴要逐帧采样（帧泵自己按 16 ms 结算，见 `components::motion`） |
//! | 有 `Busy` / `Running` 实例 | 启动进度、日志尾随都靠 `backend.tick` 推进 |
//! | 有未过期的 toast | toast 按毫秒倒计时 |
//! | 刚派发过指令（3 s 观察窗） | 真 host 的异步事件要靠节拍抽出来 |
//!
//! 上面五条**共享同一个布尔**：订阅表里 `window::frames()` 只出现一处（写在同一个 `if` 里），
//! 所以「动画」与「观察窗 / Busy」同时成立时也只有一条订阅——不双订阅靠的是结构，不是判重。
//!
//! 空闲态**一条帧消息都不来** ⇒ 窗口不重绘、`refresh_view` 不跑（配合 `view_dirty`）。
//!
//! ## 空闲态的时钟从哪来（M90 r3 / P2-2）
//!
//! r2 把空闲的帧订阅掐掉之后，`config_poll_due` 就再也没被调用过 —— 外部 CLI / AI 工具改
//! `launcher.json` 后主题**永不生效**（不是「等下次交互」，是「等下一次 dispatch」）。这与
//! D4「AI 能读改 ULL 配置」直接冲突。
//!
//! 所以空闲态换成一个 **1 Hz 的自续心跳**（[`crate::app::message::Message::Heartbeat`]，
//! 由 `Task::perform(sleep(1s))` 驱动，见 `update::heartbeat`）：每秒醒一次、推进一拍
//! presenter（内部按 33 ms 折算 30 拍，于是 `config_poll_due` 仍是「约 1 秒比对一次 mtime」）。
//!
//! 为什么不用 `window::frames()` + 1 s 闸门来实现这个低频时钟：帧订阅**本身就是**自持重绘循环
//! （iced_winit 处理完任何消息都 `request_redraw()`），闸门只能省掉 `advance()` 里的活，省不掉
//! 整窗重绘——实测软件渲染约 **500 帧/秒**。心跳是「每秒一帧」，差三个数量级。
//!
//! ## 已知代价（都登记，不藏）
//!
//! 1. **忙碌态仍是自持帧循环**：`Running`/`Busy` 一天不结束，就一天按帧刷（软件渲染实测约
//!    970 帧/秒，refreshes 不增长 = 纯像素空转）。功能上需要 tick 来发现游戏退出，但「整段
//!    运行期近千帧/秒重绘」是真实开销；要收口得让 `Running` 走更慢的时钟（follow-up）。
//! 2. 心跳用 `thread::sleep` 占线程池一个线程 1 秒；1 Hz 下可接受，但不是优雅解法（正解仍是
//!    `iced::time::every` + tokio/smol，Cargo.toml 在 scope 外，已报 finding）。

use std::time::Instant;

use iced::{window, Subscription};

use super::components::motion;
use super::message::Message;
use super::state::App;

impl App {
    /// 本次进程的订阅表。
    pub(super) fn subscription(&self) -> Subscription<Message> {
        let frames = if self.needs_frames() {
            window::frames().map(|_| Message::Frame)
        } else {
            Subscription::none()
        };

        // 断点 600 / 840 要跟着真窗口宽度走；截图模式的窗口尺寸由参数钉死，不吃 resize。
        let events = if self.has_screenshot() {
            Subscription::none()
        } else {
            window::events().map(|(_, event)| Message::Window(event))
        };

        // 键盘（M104）：`Esc` 收栗 / `Tab` 走焦点环 / `Enter` 激活焦点键。
        //
        // 为什么订阅整条键盘流而不是自己挑键：`iced::keyboard::listen()` 是唯一的全窗键盘入口，
        // 挑键与「什么状态下该做什么」的解释权放在 `app/update/agent.rs`（一眼看完），
        // 订阅表只负责搬运。截图通道不开它——D30 要可复现，键盘事件一个都不该进图。
        let keys = if self.has_screenshot() {
            Subscription::none()
        } else {
            iced::keyboard::listen()
                .map(|event| Message::Agent(crate::shell::agent::Message::Key(event)))
        };

        Subscription::batch([frames, events, keys])
    }

    /// 是否订阅帧事件（= 截图模式要帧数闸门）。
    pub(super) fn wants_frames(&self) -> bool {
        self.has_screenshot()
    }

    /// 现在要不要**时钟**（见模块文档的表）。
    pub(super) fn needs_frames(&self) -> bool {
        self.has_screenshot()
            // M192：只有动画在跑时才多这一条（帧泵清空轨道后立刻为假 ⇒ 退回 1 Hz 心跳）。
            || motion::needs_frames()
            || !self.state.toasts.is_empty()
            || !self.state.running().is_empty()
            // M175：Java 扫描进行中（进度 3/7 要动起来，否则用户只看到「正在扫描」卡着）。
            || self.settings.java.scanning.is_some()
            || self
                .watch_until
                .is_some_and(|deadline| Instant::now() < deadline)
    }
}
#[cfg(test)]
mod tests {
    //! 帧订阅的**有界**守卫（M192）：动画期间订阅、结束即退订、关掉动效一帧都不订阅。
    //!
    //! ## 这一组按**运行时顺序**写（M192 r1 复审 P1-1 的教训）
    //!
    //! 第一版这里是「先 `motion::enter(...)` 再问 `needs_frames()`」——而 iced 的真实顺序是
    //! 「`update()`（订阅表在这里求值）→ `view()`」，于是守卫全绿、生产路径上却根本播不出来。
    //! 现在全部改走**真消息**，并断言 `watch_until` 仍是 `None`（排除「观察窗顺手开了帧」的假绿）。
    //! 时间轴用 `motion::advance_by` 摆（不等真实时钟）；动效全局态是 thread-local，天然隔离。

    use super::*;
    use crate::app::components::motion::FRAME_MS;
    use crate::cli::Options;
    use crate::pages;
    use crate::theme::Families;
    use unlucky_design::color::SchemeFlavor;
    use unlucky_design::motion::MotionScale;
    use unlucky_presenter::mock::MockBackend;
    use unlucky_presenter::router::Route;
    use unlucky_presenter::settings_bridge::SettingsBridge;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    /// 不碰磁盘、后端是 mock 的 App（与 `app/tests.rs` 的 `headless` 同构）。
    ///
    /// 定档走 `motion::configure_for_test`（P2-5）：产品入口读宿主机「减少动效」开关，用例在关了
    /// 系统动画的机器上会红；真探针的接线另有专门用例守。
    fn headless() -> App {
        App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(MockBackend::new()),
            SettingsBridge::memory_only(),
        )
    }

    /// 走到「真·空闲」：没有观察窗、没有忙碌实例、没有动画（首屏那一条 Boot 已跑完）。
    ///
    /// `Boot` 自己就会起跑一次首屏动效（`on_boot` → `sync_motion`），所以必须先把它推完，
    /// 否则后面的 `assert!(!needs_frames())` 测的是「动画还在跑」而不是「空闲」。
    fn idle_app() -> App {
        motion::reset();
        motion::configure_for_test(MotionScale::STANDARD, None);
        let mut app = headless();
        // 有窗口才谈得上动效（`sync_motion` 在没有窗口时直接返回，见它的文档）
        app.window = Some(iced::window::Id::unique());
        let _ = app.update(Message::Boot);
        app.watch_until = None;
        run_until_idle();
        assert!(!motion::needs_frames(), "前提：首屏动效已经跑完");
        app
    }

    /// 把时间轴推到全部轨道结束（每步 16 ms，上限 20 s）。
    fn run_until_idle() {
        for _ in 0..1250 {
            if !motion::needs_frames() {
                return;
            }
            motion::advance_by(FRAME_MS);
        }
    }

    /// **按运行时顺序**：空闲态一条消息进来（换页）⇒ **同一拍**订阅表就要有帧；跑完立刻退订。
    ///
    /// 变异证明：把起跑搬回 `view`（删掉 `App::update` 里的 `sync_motion`）⇒ 第一步的
    /// `assert!(app.needs_frames())` 红；把 `Pump::advance` 的「到期即删除」删掉 ⇒ 最后一步红。
    #[test]
    fn a_state_change_opens_the_frame_subscription_in_the_same_tick() {
        let mut app = idle_app();
        assert!(!app.needs_frames(), "前提：空闲交互态本来就不订阅帧");

        // 用户点了一次导航轨：**一条**消息，之后不再补任何消息
        let _ = app.update(Message::Page(pages::Message::Navigate(Route::Download)));

        assert!(
            app.watch_until.is_none(),
            "换页不该打开观察窗（这里要证明帧是**动效**要来的，不是派发顺手开的）"
        );
        assert!(motion::needs_frames(), "换页必须在同一拍起跑");
        assert!(
            app.needs_frames(),
            "同一拍的订阅表必须能看见轨道（iced 先算订阅表、后建视图 —— 复审 P1-1）"
        );

        run_until_idle();
        assert!(!motion::needs_frames(), "动画跑完帧泵必须清空");
        assert!(
            !app.needs_frames(),
            "动画结束后必须退回 1 Hz 心跳（占着帧订阅 = 空闲烧帧）"
        );
        motion::reset();
    }

    /// 列表 ↔ 详情同样走「一条消息 ⇒ 当拍有帧」，且 `Detail` 槽真的是那条。
    #[test]
    fn opening_a_detail_also_opens_the_frames_in_the_same_tick() {
        let mut app = idle_app();
        // 出厂快照会自动选中第一条 ⇒ 先回列表，让「点进详情」真的是一次详情变化
        let _ = app.update(Message::Page(pages::Message::Navigate(Route::Instances)));
        run_until_idle();
        assert!(!app.needs_frames());
        let _ = app.update(Message::Page(pages::Message::Navigate(Route::Detail(
            "survival".to_string(),
        ))));
        assert!(app.needs_frames(), "点进详情必须当拍订阅帧");
        run_until_idle();
        assert!(!app.needs_frames());
        motion::reset();
    }

    /// 账户页 / AI 独立页**不归 pages 画**：切过去不起跑（否则白跑 300 ms 帧）；切回来照旧跑。
    #[test]
    fn pages_that_render_elsewhere_do_not_start_a_track() {
        let mut app = idle_app();
        let _ = app.update(Message::Page(pages::Message::Navigate(Route::Accounts)));
        assert!(
            !motion::needs_frames(),
            "账户页没有 pages 树可动，不许白开帧"
        );

        let _ = app.update(Message::Page(pages::Message::Navigate(Route::Instances)));
        assert!(motion::needs_frames(), "从账户页回到某一屏仍然是换页，要跑");
        run_until_idle();
        motion::reset();
    }

    /// **没有窗口就不起跑**：`window::frames()` 在没窗口时一条消息都不会有，起跑只是白占一档；
    /// 首屏那一条也因此要等窗口到了才算「第一眼」。
    #[test]
    fn without_a_window_nothing_animates() {
        motion::reset();
        motion::configure_for_test(MotionScale::STANDARD, None);
        let mut app = headless();
        assert!(app.window.is_none(), "前提：还没拿到窗口句柄");
        let _ = app.update(Message::Boot);
        app.watch_until = None;
        assert!(!motion::needs_frames(), "没有窗口 ⇒ 不许起跑");

        // 窗口来了：首屏入场这时才开始（不是被启动期那棵空树吃掉）
        let _ = app.update(Message::WindowReady(Some(iced::window::Id::unique())));
        assert!(app.needs_frames(), "窗口到了之后「首屏」才起跑");
        run_until_idle();
        motion::reset();
    }

    /// **`MotionScale::NONE` 真的零订阅**：不是「跑一遍 0 时长动画」。
    /// 变异证明：把 `Pump::play_page` 的 `is_off()` 分支删掉 ⇒ 帧采样次数非 0，本用例红。
    #[test]
    fn no_scale_requests_no_frames_at_all() {
        motion::reset();
        let mut app = headless();
        app.window = Some(iced::window::Id::unique());
        let _ = app.update(Message::Boot);
        app.watch_until = None;
        motion::configure_for_test(MotionScale::NONE, None);

        // 三种状态变化：换页 / 换详情 / 再换回来
        let _ = app.update(Message::Page(pages::Message::Navigate(Route::Download)));
        assert!(!app.needs_frames(), "关掉动效后换页一帧都不该订阅");
        let _ = app.update(Message::Page(pages::Message::Navigate(Route::Detail(
            "survival".to_string(),
        ))));
        assert!(!app.needs_frames());
        run_until_idle();

        assert!(!motion::needs_frames());
        assert_eq!(motion::samples(), 0, "NONE 档的帧采样次数必须是 0");
        motion::reset();
    }

    /// 帧订阅在**产品接线**里只出现一处：五条理由共享同一个布尔（不会「动画与 Busy 各订一条」）。
    /// 口径是**全仓 `src/**`**，只看代码行；唯一允许的第二个文件是 `pages/harness.rs`
    /// （`pages_demo` 这条独立 example 自己的 app，不进产品路径）。
    #[test]
    fn there_is_exactly_one_frame_subscription() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        collect_rust_files(&src, &mut files);
        assert!(
            files.len() >= 40,
            "只扫到 {} 个文件：扫描逻辑坏了",
            files.len()
        );

        let mut hits = Vec::new();
        for path in files {
            let text = std::fs::read_to_string(&path).expect("读源文件");
            let product = text.split("#[cfg(test)]").next().unwrap_or(&text);
            let code = product
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join(
                    "
",
                );
            let count = code.matches("window::frames()").count();
            if count > 0 {
                let name = path
                    .strip_prefix(&src)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/");
                hits.push((name, count));
            }
        }

        hits.sort();
        let expected = [
            ("app/subscription.rs".to_string(), 1_usize),
            ("pages/harness.rs".to_string(), 1_usize),
        ];
        assert_eq!(
            hits, expected,
            "`window::frames()` 只许出现在 `app/subscription.rs`（产品接线）\
             与 `pages/harness.rs`（独立 example 的自有 app），且各一处"
        );
    }

    /// 递归收集 `dir/**/*.rs`。
    fn collect_rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("读目录") {
            let path = entry.expect("目录项").path();
            if path.is_dir() {
                collect_rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    /// 「**页面拿不到时长**」的源码守卫（M256 取代 M192 的 `Slot` 三条断言）。
    /// 口径：`pages/**` 的产品代码不许出现时长的任何入口（`DurationToken` / `page_spec` /
    /// `modal_spec` / `FRAME_MS` / `advance(`）：页面只能说「我要换页了」，用哪一档由
    /// `components/motion/transition.rs` 那张表决定（D31）。
    ///
    /// **抓得住**：页面里写 `DurationToken::Medium2` / `motion::FRAME_MS` / 自己推时间轴（变异：
    /// 在 `pages/download.rs` 顶上加 `use unlucky_design::motion::DurationToken;` ⇒ 红）。
    /// **抓不住**：①某档数字写错（那是 `components/motion/tests` 的登记用例）；②绕过符号名的
    /// 路径（转一手、通配导入）——只按符号名扫，不做类型分析。
    #[test]
    fn pages_cannot_reach_for_a_duration() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("pages");
        let mut files = Vec::new();
        collect_rust_files(&dir, &mut files);
        assert!(
            files.len() >= 20,
            "只扫到 {} 个页面文件：扫描逻辑坏了",
            files.len()
        );

        const FORBIDDEN: [&str; 5] = [
            "DurationToken",
            "page_spec",
            "modal_spec",
            "FRAME_MS",
            "advance(",
        ];
        let mut hits = Vec::new();
        for path in files {
            let text = std::fs::read_to_string(&path).expect("读源文件");
            // 只看产品代码（测试与文档注释不算：`#[cfg(test)]` 之后整段跳过）。
            let product = text.split("#[cfg(test)]").next().unwrap_or(&text);
            let code = product
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"));
            for symbol in FORBIDDEN {
                if code.clone().any(|line| line.contains(symbol)) {
                    let name = path
                        .strip_prefix(&dir)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace(std::path::MAIN_SEPARATOR, "/");
                    hits.push((name, symbol));
                }
            }
        }
        assert!(
            hits.is_empty(),
            "页面层不许直接碰时长/时间轴（用哪一档是 `transition.rs` 的表决定的）：{hits:?}"
        );
    }
}
