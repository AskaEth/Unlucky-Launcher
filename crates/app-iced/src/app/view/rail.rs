//! 导航轨底部**账户头像位**的状态换算（M114）+ 轨自身的折叠档（M300）+ 轨宽的**当帧采样**（M301）。
//!
//! 为什么单独一个文件：`app/view.rs` 贴着 400 行硬线，而这块逻辑与页面树无关——
//! 它只回答「轨底那个头像位现在该显示成未登录，还是某个账户」，以及「这一帧轨多宽」。
//!
//! ## 顺序契约（M301，给下一个改这里的人）
//!
//! [`App::rail_account`] 是轨宽的**唯一写入口**：它每次 `App::view()` 被调用**恰好一次**
//! （`app/view.rs` 的 `let account = self.rail_account();`），且**排在页面树之前**。所以
//! 页面几何（`pages::layout::rail::rail_width`）读到的值与这里递给 `shell::NavRail` 的
//! **是同一个数、同一拍**——不存在「轨折叠了、页面还按 88 扣宽」的二段状态。
//! 这条顺序是代码顺序，不是约定：谁把这次写挪到页面树之后再调用，`pages/tests/rail_width.rs`
//! 的接线判据会红。

use crate::app::components::motion;
use crate::shell;

use super::super::state::App;

/// 导航轨收成「图标条」的窗口高度（M114）。
///
/// 判据不是「项数」而是**真实可用高度**：四项 + 底部头像位在 48 标题栏之上还要放得下
/// 4 × 52 + 4 × 4 + 52 ≈ 300；留出一屏正文的最小高度（424 行宽 + 页头 + 边距），
/// 低于 720 就把标签收掉，只留图标与头像位（头像位**永远可见**，任务要求）。
const RAIL_COMPACT_HEIGHT: f32 = 720.0;

impl App {
    /// 导航轨底部头像位的状态（M114）：未登录 / 已登录两态要一眼能分。
    ///
    /// M300：这里同时是轨的折叠档与过渡进度的**唯一构造点**——`shell::NavRail`（不在 scope 内）
    /// 只负责把它转交给 `nav::rail`，几何全部来自 [`shell::RailAccount::nav_width`]。
    ///
    /// M301：**同时**是页面层「这一帧扣多宽」的写入口（同一个 `nav_width`，见模块文档的顺序契约）。
    pub(super) fn rail_account(&self) -> shell::RailAccount {
        let compact = self.height < RAIL_COMPACT_HEIGHT;
        let (collapsed, progress) = self.nav_state();
        let account = match self.state.active_account() {
            Some(account) => {
                shell::RailAccount::signed_in(account.monogram(), account.name.clone())
                    .compact(compact)
                    .nav(collapsed, progress)
            }
            // M106：未登录标签跟着**生效语言**走（早期钉死 `Lang::En` 的写法已删）。
            None => shell::RailAccount::signed_out(
                self.lang()
                    .catalog()
                    .text(unlucky_i18n::Key::AccountRailSignedOut),
            )
            .compact(compact)
            .nav(collapsed, progress),
        };
        // M301：页面几何要扣的那一段（折叠 64 / 展开 88 / 动画中间帧插值）。**先写后读**——
        // 页面树紧跟着在同一次 `view()` 里建，所以两侧永远同一帧同一个数。
        crate::pages::layout::note_rail_width(account.nav_width(self.ui().metrics()));
        account
    }

    /// 轨的折叠档 + **这一帧**的过渡进度（0 = 折叠 64，1 = 展开 88）。
    ///
    /// 进度是**采样**来的，不是自己算的：`motion::nav(open)` 在「帧泵的目标与 `open` 不符」时
    /// 直接给终值，于是截图 harness / 单测直接摆状态也不会把界面卡在中间那一帧。
    fn nav_state(&self) -> (bool, f32) {
        let collapsed = self.state.prefs.nav_collapsed;
        (collapsed, motion::nav(!collapsed))
    }

    /// 这一帧导航轨的宽度（展开 88 ⇄ 折叠 64，中间帧按动效进度插值）。
    ///
    /// **给用例用**：它读的就是 `view()` 递给 `nav::rail` 的那个值（同一个 `RailAccount`），
    /// 所以「点击后宽度真的变了」这条断言不会与视图各算一套而假绿。
    #[cfg(test)]
    pub(in crate::app) fn nav_rail_width(&self) -> f32 {
        self.rail_account().nav_width(self.ui().metrics())
    }
}

#[cfg(test)]
mod tests {
    //! **M301 的接线判据**：外壳这一帧采样 ⇒ 页面这一帧扣宽（同一个数、同一拍，含动效中间帧）。
    //!
    //! 几何面（折叠 −64 / 展开 −88 / 差 24 / 门檻 / 唯一写入口）在 `pages/tests/rail_width.rs`；
    //! 这里只守「外壳真的每帧都写、写了就立刻生效、不会粘住上一帧」。

    use unlucky_design::color::SchemeFlavor;
    use unlucky_design::motion::MotionScale;
    use unlucky_presenter::mock::MockBackend;
    use unlucky_presenter::settings_bridge::SettingsBridge;

    use crate::app::components::motion::{self, FRAME_MS};
    use crate::cli::Options;
    use crate::pages::layout::{self, WidthClass};
    use crate::theme::Families;

    use super::*;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    /// 出厂态 App（桥指向一个不存在的临时文件 = 出厂偏好）。
    fn app() -> App {
        motion::reset();
        motion::configure_for_test(MotionScale::STANDARD, None);
        let path =
            std::env::temp_dir().join(format!("ull-m301-{}-{}.json", std::process::id(), line!()));
        let mut app = App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(MockBackend::new()),
            SettingsBridge::with_path(path),
        );
        app.window = Some(iced::window::Id::unique());
        app
    }

    /// 验收窗宽（与 `pages/tests.rs` 同一个对照尺寸）。
    const WINDOW: f32 = 1180.0;

    /// 页面这一帧扣的宽度必须就是外壳这一帧采到的轨宽。
    fn assert_page_follows(app: &App) -> f32 {
        let _ = app.rail_account();
        let rail = layout::rail_width();
        assert_eq!(
            layout::available_width(WINDOW),
            WINDOW - rail - 2.0 * WidthClass::Expanded.gutter(),
            "页面扣的宽度必须等于外壳这一帧的轨宽（{rail}）"
        );
        rail
    }

    /// **折叠 → 采样 → 展开 → 再采样**：第二次必须回到 88（不粘滞）。
    ///
    /// **抓得住**：①写入口只在折叠方向生效（展开时还停在 64）；②写进去的值被页面忽略；
    /// ③复位成常量（折叠也按 88 扣宽——这时折叠那条断言直接红）。
    /// **抓不住**：真画出来的像素（见 `DevDocs/artifacts/m301/`）。
    #[test]
    fn the_shell_sample_follows_the_collapse_state_and_reverts() {
        let mut app = app();
        let collapsed = layout::rail_width_collapsed();
        let expanded = layout::rail_width_expanded();

        assert_eq!(
            assert_page_follows(&app),
            expanded,
            "出厂展开 ⇒ 页面按 88 扣宽"
        );

        // 折叠：泵的目标与 prefs 不符 ⇒ `motion::nav(open)` 给终值 0.0 ⇒ 当帧就是 64（不靠动画）。
        app.state.prefs.nav_collapsed = true;
        assert_eq!(
            assert_page_follows(&app),
            collapsed,
            "折叠 ⇒ 当帧就按 64 扣宽"
        );

        // 展开回去：第二次采样必须回到 88（删掉写入 / 只在折叠时写 ⇒ 这里绿不了）。
        app.state.prefs.nav_collapsed = false;
        assert_eq!(
            assert_page_follows(&app),
            expanded,
            "第二帧必须回到 88，不许粘住 64"
        );
    }

    /// **动效期间页面跟着插值**（任务判据④）：中间帧既不等于 88 也不等于 64，且首尾是端值。
    ///
    /// **抓得住**：页面只在端值上跳变（中间帧列表里没有严格递减的中间值）、或是拿端值当宽度画
    /// （那会让 `available_width` 与轨宽在这一帧不等）；**抓不住**：25 帧的观感（那要连拍出图）。
    #[test]
    fn the_page_width_interpolates_during_the_collapse_motion() {
        let mut app = app();
        let collapsed = layout::rail_width_collapsed();
        let expanded = layout::rail_width_expanded();
        motion::snap_nav(true);
        app.state.prefs.nav_collapsed = true;
        assert!(motion::observe_nav(false), "折叠必须起跑一条宽度轨道");

        let mut widths = vec![assert_page_follows(&app)];
        let mut guard = 0;
        while motion::needs_frames() {
            motion::advance_by(FRAME_MS);
            widths.push(assert_page_follows(&app));
            guard += 1;
            assert!(guard < 200, "左轨轨道没跑完");
        }

        assert_eq!(
            widths.first().copied(),
            Some(expanded),
            "起跑那一帧还在展开档"
        );
        assert_eq!(
            widths.last().copied(),
            Some(collapsed),
            "终值必须是折叠档，逐位"
        );
        assert!(
            widths
                .iter()
                .any(|width| *width > collapsed && *width < expanded),
            "必须有中间帧（页面跟插值走，不是端值跳变）：{widths:?}"
        );
        for pair in widths.windows(2) {
            assert!(pair[1] <= pair[0], "折叠必须单调不增：{pair:?}");
        }
    }
}
