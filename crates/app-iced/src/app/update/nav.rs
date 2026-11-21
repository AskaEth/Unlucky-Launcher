//! 左侧导航轨折叠 / 展开的**唯一落点**（M300 `NAVCOLLAPSE`）。
//!
//! 为什么单独一个文件：`update.rs` 385 行贴着 400 行硬线（只剩十几行余量），而这条分支要做
//! 三件事（翻 prefs、起跑宽度轨道、落盘）。这正是 M94/M223 拆 `update/<名>.rs` 的同一种手法：
//! `update.rs` 只留一支 3 行的转发。
//!
//! ## 这条分支为什么不置 `view_dirty`
//!
//! 轨宽与标签可见性都在 **`view()` 里逐帧采样**（`app/view/rail.rs` 读
//! `motion::nav(open)` + `prefs.nav_collapsed`），页面模型（`pages::Model`）里**没有**这一项
//! ——所以折叠不需要重建模型，只需要**帧**。帧由动效帧泵自己续（`Pump::nav` 那条轨道到期即清），
//! 于是「折叠 → 动画 → 静息」整段结束后 `view_dirty` 仍是 `false`、`needs_frames()` 也回到
//! `false`，不会退化成「靠每帧重建模型来画动画」（M298 r1 的返工点）。
//!
//! ## 落盘
//!
//! `ui.nav.collapsed` 是 `ui.*` 一族里**第一条真落盘**的偏好键（M300 打通：core schema 登记 →
//! `SettingsBridge::persist_nav_collapsed` 写 → 启动 `construct.rs` 读回）。写失败**不打断**
//! 折叠这个动作：配置写不进去不该让界面卡住，失败原因进 `launcher.log`。

use iced::Task;

use crate::app::components::motion;
use crate::app::diag::diag;

use super::*;

impl App {
    /// 收起 / 展开左侧导航轨（`Message::ToggleNav` 的落点）。
    pub(super) fn on_toggle_nav(&mut self) -> Task<Message> {
        // ① 起跑前先把泵的目标对到**真实档位**（幂等，且只在没有轨道在跑时做）：
        //    - 泵的目标出厂是「展开」，而用户可能上次就是折叠着退出的（`ui.nav.collapsed` 落盘），
        //      不对表的话这次展开会走 `observe_nav` 的「目标没变」早退 ⇒ 硬跳（T37 的形态）；
        //    - 有轨道在跑时**不许**对表：那会把正在跑的过渡当场掐掉（半路反向就变成跳变）。
        if !motion::nav_running() {
            motion::snap_nav(!self.state.prefs.nav_collapsed);
        }
        self.state.toggle_nav();
        let collapsed = self.state.prefs.nav_collapsed;

        // ① 起跑宽度过渡。**必须在 update 侧**：iced 的订阅表在 `update()` 里求值、早于 `view()`，
        // 在 `view` 里插轨道当拍订阅不上（`components/motion` 的模块文档有源码行号）。
        motion::observe_nav(!collapsed);
        // ② 落盘（`ui.nav.collapsed`）：不落盘就是「重启回到展开」的假折叠。
        let saved = self.bridge.persist_nav_collapsed(collapsed);
        diag!(
            "[nav] collapsed={collapsed} persist={}",
            if saved { "saved" } else { "skipped" }
        );

        // ③ 不置 `view_dirty`，理由见模块文档「这条分支为什么不置」。
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    //! `Message::ToggleNav` 的接线判据（M300）。
    //!
    //! 三条都和「任务单里的验收句」一一对应：①点击后**宽度真的变**（不是只改 state）
    //! ②动画结束后**不烧帧**（`needs_frames()` / `view_dirty` 都回 false）③落盘**真读盘**
    //! （换一个桥对象/重造 App 读回来）。
    //!
    //! 时间轴一律用 `motion::advance_by(FRAME_MS)` 连拍（T37），不等真实时钟（T38：两条腿读
    //! 同一个 `Pump::now_ms`，所以这里推多少就是多少）。

    use super::*;
    use crate::app::components::motion::{self, FRAME_MS};
    use crate::cli::Options;
    use crate::theme::Families;
    use std::path::PathBuf;
    use unlucky_design::color::SchemeFlavor;
    use unlucky_design::motion::MotionScale;
    use unlucky_presenter::mock::MockBackend;
    use unlucky_presenter::settings_bridge::SettingsBridge;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    /// 一份临时 `launcher.json`（父目录建好，文件删掉 = 出厂态）。
    fn temp_config(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ull-nav-{label}-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("config")).expect("临时目录");
        let path = dir.join("config/launcher.json");
        let _ = std::fs::remove_file(&path);
        path
    }

    fn app_with(bridge: SettingsBridge, scale: MotionScale) -> App {
        motion::reset();
        motion::configure_for_test(scale, None);
        let mut app = App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(MockBackend::new()),
            bridge,
        );
        // 有窗口才谈得上动效（`sync_motion` 在没有窗口时直接返回）。
        app.window = Some(iced::window::Id::unique());
        // 先建一次模型：构造期 `hydrate_*` 会置 `view_dirty`（它们改了状态但还没人重建），
        // 不清掉的话「折叠不置脏」这条断言测的是启动那一下。
        app.refresh_view();
        assert!(!app.view_dirty, "前提：构造 + 一次重建之后 dirty 已清");
        // 三样会**独立**点亮 `App::needs_frames()` 的东西清掉，否则「动画跑完就退订」这条
        // 断言测的是它们、不是动效（首屏 Java 扫描在构造这一拍就会亮）。
        app.watch_until = None;
        app.settings.java.scanning = None;
        app
    }

    /// 推一拍时间轴并采样这一帧的轨宽。
    fn step(app: &App) -> f32 {
        motion::advance_by(FRAME_MS);
        app.nav_rail_width()
    }

    /// **点击 → 宽度真的变 → 落盘真的写进文件 → 再点回去 → 空闲。**
    ///
    /// 抓得住：①只改 `prefs` 不画（宽度采样停在 88）；②过度写成「一帧到底」（中间帧列表里没有
    /// 严格递减的中间值）；③没落盘（重新构造的桥读回 false）；④动画之后还在要帧（最后两条断言）。
    /// 抓不住：真机上 250/200ms 的手感与中间帧的像素（像素那半由 `DevDocs/artifacts/m300/` 两张图 +
    /// 本文件的数值列表共同举证）。
    #[test]
    fn toggling_narrows_the_rail_for_real_and_survives_a_restart() {
        let path = temp_config("toggle");
        let mut app = app_with(SettingsBridge::with_path(&path), MotionScale::STANDARD);
        assert_eq!(app.nav_rail_width(), 88.0, "前提：出厂展开");

        let _ = app.update(Message::ToggleNav);
        assert!(app.state.prefs.nav_collapsed, "点击必须翻掉 prefs");
        assert!(
            app.watch_until.is_none(),
            "折叠不该打开观察窗（帧是**动效**要来的）"
        );
        assert!(
            app.needs_frames(),
            "同一拍的订阅表必须看得见左轨轨道（iced 先算订阅表后建视图）"
        );

        // 连拍（两次 advance 之间没有任何别的调用 = 「状态已切换但没有事件」那一段）
        let mut widths = vec![app.nav_rail_width()];
        let mut guard = 0;
        while motion::needs_frames() {
            let width = step(&app);
            // `--nocapture` 时这行就是报告里那张「中间帧」数值表（像素图拍不到中间帧：
            // 截图通道是 `MotionScale::NONE`，只有端值）。
            println!("[nav] frame {guard}: width = {width:.2}");
            widths.push(width);
            guard += 1;
            assert!(guard < 200, "左轨轨道没跑完");
        }
        assert_eq!(widths.first().copied(), Some(88.0), "起跑那一帧还在展开档");
        assert_eq!(widths.last().copied(), Some(64.0), "终值必须**严格**是 64");
        assert!(
            widths.len() >= 5,
            "必须有中间帧（实测 {} 帧：{:?}）",
            widths.len(),
            widths
        );
        for pair in widths.windows(2) {
            assert!(pair[1] <= pair[0], "折叠必须单调不增：{:?}", pair);
        }
        assert!(
            widths.windows(2).any(|pair| pair[1] < pair[0]),
            "折叠必须真的改宽度，而不是只改 state"
        );

        // 落盘：**换一个桥对象**读同一份文件（= 重启后的读法）
        assert!(
            SettingsBridge::with_path(&path).load_nav_collapsed(),
            "折叠必须落盘，否则重启就回到展开"
        );
        assert!(!app.view_dirty, "折叠不该把页面模型置脏");
        assert!(!app.needs_frames(), "动画跑完必须退回 1 Hz 心跳");

        // 再点回来：反向同一条判据（只断言端点与空闲）
        let _ = app.update(Message::ToggleNav);
        assert!(!app.state.prefs.nav_collapsed);
        let mut guard = 0;
        while motion::needs_frames() {
            step(&app);
            guard += 1;
            assert!(guard < 200, "左轨轨道没跑完（展开）");
        }
        assert_eq!(app.nav_rail_width(), 88.0);
        assert!(!app.view_dirty, "展开也不该置脏");
        assert!(
            !SettingsBridge::with_path(&path).load_nav_collapsed(),
            "展开也要落盘"
        );

        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }

    /// **重启读回折叠档，且第一次展开仍然有动画。**
    ///
    /// 这条守两件事：①`construct.rs` 真的在启动时读了 `ui.nav.collapsed`（不读的话第一段红）；
    /// ②`sync_motion` 的 `snap_nav` 对表真的存在（没有它，`observe_nav(true)` 会走「目标没变」
    /// 早退 ⇒ 展开是硬跳，第二段红）。T37 说得很直白：数值对不等于产品路径会调到它。
    #[test]
    fn a_restart_boots_collapsed_and_the_first_expand_still_animates() {
        let path = temp_config("restart");
        let mut writer = SettingsBridge::with_path(&path);
        assert!(
            writer.persist_nav_collapsed(true),
            "先造一份「上次折叠着退出」"
        );

        // 「重启」：新 App + 新桥，读同一份文件
        let mut app = app_with(SettingsBridge::with_path(&path), MotionScale::STANDARD);
        assert!(
            app.state.prefs.nav_collapsed,
            "启动必须读回折叠档（`construct.rs` 的接线）"
        );
        assert_eq!(app.nav_rail_width(), 64.0, "首帧就该是折叠档");
        assert!(!motion::needs_frames(), "静息折叠态不该要帧");

        // 第一次展开：必须有中间帧（不是硬跳）
        let _ = app.update(Message::ToggleNav);
        let first = step(&app);
        assert!(
            first > 64.0 && first < 88.0,
            "启动折叠之后第一次展开必须走过渡，实测第一帧 {first}"
        );
        let mut guard = 0;
        while motion::needs_frames() {
            step(&app);
            guard += 1;
            assert!(guard < 200);
        }
        assert_eq!(app.nav_rail_width(), 88.0);

        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }

    /// **关掉动效（`MotionScale::NONE`）：一步到位，且一帧都不订阅。**
    ///
    /// 抓得住：把 `observe_nav` 的 `is_off()` 分支删掉（插一条 0 时长轨道 ⇒ 进度还是终值，但
    /// `needs_frames()` 会有一拍为真 ⇒ 第一条红）。这也顺带守住「空闲不烧帧」：截图通道/CPU 档下
    /// 折叠不该把帧循环拉起来。
    #[test]
    fn with_motion_off_the_rail_jumps_and_asks_for_no_frames() {
        let mut app = app_with(SettingsBridge::memory_only(), MotionScale::NONE);
        let _ = app.update(Message::ToggleNav);
        assert_eq!(app.nav_rail_width(), 64.0, "NONE 下必须**逐位**落在终值");
        assert!(!motion::needs_frames(), "NONE 下一帧都不许要");
        assert!(!app.view_dirty);
    }
}
