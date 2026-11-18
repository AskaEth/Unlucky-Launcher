//! 主题与 `Ui` 的重算口径（从 `state.rs` 搬出来守 400 行硬线；M119 拆，纯搬不改）。
//!
//! 三件事在这里：①`Ui` 什么时候重建（[`App::rebuild_ui`]）②主题档怎么定
//! （[`App::set_flavor`] / [`App::apply_config_theme`]）③读当前求值器（[`App::ui`]）。
//!
//! 截图通道的判据 [`App::has_screenshot`] 也放在这儿：上面三条里两条都要「截图不读用户配置」。

use crate::app::diag::diag;
use crate::app::message::Message;
use iced::Task;
use unlucky_design::color::{Scheme, SchemeFlavor};
use unlucky_presenter::prefs::ThemeMode;

use crate::app::components::motion;
use crate::theme::Ui;

use super::App;

/// 出图取证的**起手板**（`ULL_THEME_FROM=light|dark`，只在相位被钉住时生效）。
///
/// 为什么需要它：运行期换主题的中间相位，截图通道拍不到——窗口在第 3 帧就落盘，而换主题是
/// 用户点出来的。所以取证时让**启动那一次** `Ui` 重算从另一块板淡进来：`--screenshot-theme=dark`
/// 配 `ULL_THEME_FROM=light` + `ULL_MOTION_AT=<ms>`，第 3 帧拍到的就是两块板之间的中间值。
///
/// 产品路径**读不到**它：`pinned()` 只在 `ULL_MOTION_AT` 给了值时为真（`app/update/boot_env.rs`），
/// 而环境变量本身也只有取证脚本会设。同一条纪律见 `app/view.rs::scan_phase`（`ULL_INSTANCE_PHASE`）。
fn fade_fixture(families: crate::theme::Families) -> Option<Scheme> {
    if !motion::theme::pinned() {
        return None;
    }
    let raw = std::env::var("ULL_THEME_FROM").ok()?;
    let flavor = match raw.trim() {
        "light" => SchemeFlavor::Light,
        "dark" => SchemeFlavor::Dark,
        _ => return None,
    };
    Some(*Ui::new(flavor, families).scheme())
}

impl App {
    /// 本次进程是否走截图通道。
    pub(in crate::app) fn has_screenshot(&self) -> bool {
        self.options.screenshot.is_some()
    }

    /// 切主题：重建 [`Ui`]（= 重建 Style），**不动任何 UI 状态**。
    ///
    /// M93 起 `Ui` 有两个来源：配色来源取到了图（①②级）就用解析出的方案，否则用令牌 baseline。
    /// 所以这里改叫「重算 `Ui`」：先让外观状态记住新深浅（配色解析要它），再重建。
    pub(in crate::app) fn set_flavor(&mut self, flavor: SchemeFlavor) {
        if self.ui.flavor() == flavor && self.appearance.flavor() == flavor {
            return;
        }

        self.appearance.set_flavor(flavor);
        let previous = self.ui.name();
        self.rebuild_ui();
        diag!(
            "[theme] {} -> {} (Theme::name 变化触发缓存重算)",
            previous,
            self.ui.name()
        );
    }

    /// 按「强调色 / 配色来源是否接管」重建 [`Ui`]。
    ///
    /// **优先级（M250）：显式强调色 > D38 取色方案 > 出厂品牌板**。手填的色是直接意图，取色是从
    /// 图片派生的，所以强调色只在用户真的填了合法值时压过取色——两条路都只决定「颜色来自哪里」，
    /// `flavor`（明暗 / 跟随系统）仍由 `self.appearance.flavor()` 决定。
    ///
    /// 色卡档（③）刻意继续走 `Scheme::baseline`：`resolve_source` 的色卡档返回的是
    /// `DEFAULT_CARD`（天蓝卡）方案，与令牌 baseline 逐色不同，应用它会把**全部既有验收图的
    /// 像素**改掉。这条偏离登记在 `pages::appearance` 的偏离表 #4。
    pub(in crate::app) fn rebuild_ui(&mut self) {
        // 面板 α（D36）在这里注入 `Ui`：全仓所有表面色都从 `Ui::layer` 出去，所以这是**唯一**
        // 生效点（外壳 / 三页卡片 / 外观分组卡片 / 空态图标块自动同源，调用点没有机会各处自乘）。
        let panel_alpha = self.appearance.effective_panel_alpha();
        // 三选一规则只在 `theme::tokens` 里写一遍：出图 harness 也调它（两头各抄一份迟早
        // 出现「图上有强调色、产品里没有」，而那种偏差在图上看不出来）。
        let (ui, colors) = crate::theme::tokens(
            self.appearance.flavor(),
            self.appearance.accent.color(),
            self.appearance.derived_scheme().copied(),
            self.families,
        );
        match colors {
            crate::theme::Colors::Accent => {
                // M250：一行里同时给「强调色生效了」与「取色方案被谁压住」，两件事都能读出来。
                diag!(
                    "[appearance] accent={} → Ui::with_accent（只换 primary/secondary/tertiary；取色方案 palette={} 被压住）",
                    self.appearance.accent.log_token(),
                    self.appearance.palette_level()
                );
            }
            crate::theme::Colors::Scheme => {
                diag!(
                    "[appearance] palette={} → Ui::from_scheme（取色方案接管 49 个角色）",
                    self.appearance.palette_level()
                );
            }
            crate::theme::Colors::Board => {
                // 非法值：**保持出厂板**，但错行留在设置页（不静默），日志里也留一条。
                if let Some(error) = self.appearance.accent.error() {
                    diag!("[appearance] accent 非法（{error:?}）→ 出厂板，错行见设置页");
                }
            }
        }
        let next = ui.with_panel_alpha(panel_alpha);
        // M313：主题交叉淡入的**起跑点**（update 侧唯一一处，见 `components/motion/pump/theme.rs`）。
        //
        // 三件事在这里一次说清：
        // ①两块板**逐角色不同**才值得起跑：只改面板 α / 只换背景图成功的这一次重建，颜色一个都没变，
        //   起跑就是白烧 200ms 的帧（`Theme::observe` 自己也会把 `from == to` 判成不起跑）；
        // ②起手板 = **这一帧屏幕上那块**（`self.ui.scheme()`）：淡入从用户正看着的颜色出发，
        //   所以换主题那一拍看不到台阶；
        // ③启动期（`frames == 0`，一帧都还没画）**不淡入**：那一屏没有「旧颜色」可言，淡进去只会
        //   让开机第一眼是半块板。取证通道例外（`ULL_THEME_FROM` + `ULL_MOTION_AT`，见 [`fade_fixture`]）。
        let fixture = fade_fixture(self.families);
        let from = fixture.unwrap_or(*self.ui.scheme());
        let started =
            (self.frames > 0 || fixture.is_some()) && motion::theme::observe(from, *next.scheme());
        if started {
            // 起跑当拍必须置脏：`refresh_view` 只在置脏时跑，而**逐帧采样**就挂在它的尾巴上
            // （`sync_theme_fade`）。不置脏的话这条路径（`set_flavor` 直接调进来）要等到
            // 下一次别的状态变化才动第一帧——用户看到的是"主题没换"。
            self.view_dirty = true;
        }
        self.ui = next;
        // 正在淡入时**当拍**把中间值接回来：否则这条路径（外观改动触发的重建）会先画一帧
        // 「已经到终点」的树，再由下一次 `refresh_view` 拉回中间值——那就是一次闪切。
        self.sync_theme_fade();
    }

    /// 主题淡入的**逐帧采样点**（update 侧）：把这一帧的方案灌回 `self.ui`。
    ///
    /// 返回 `true` = 这一帧之后还要续（[`Self::refresh_view`] 末尾据此重新置 `view_dirty`；
    /// 落到位之后恒假 ⇒ 空闲不烧帧）。
    ///
    /// 组装方式与 [`Self::rebuild_ui`] **完全一致**（`Ui::from_scheme` + 面板 α）：末帧逐位等于
    /// `rebuild_ui` 的产物，所以淡入结束时不会留一个「差 1%」的颜色（`pump/theme.rs` 的模块文档
    /// 写了为什么记下的终点板要一直留着）。
    pub(in crate::app) fn sync_theme_fade(&mut self) -> bool {
        let Some(frame) = motion::theme::frame() else {
            return false;
        };
        let panel_alpha = self.appearance.effective_panel_alpha();
        self.ui = Ui::from_scheme(frame.scheme, self.families).with_panel_alpha(panel_alpha);
        frame.running
    }

    /// 按设置文件里的主题模式切一次外观；`--theme` 显式给了就不动。
    ///
    /// **截图通道直接跳过**：`--screenshot` 的主题唯一取值口是 `Options::capture_flavor`
    /// （D30 可复现），读本机 `launcher.json` 会让同一份代码在两台机器上出不同的图。
    ///
    /// `ThemeMode::Auto` 不在这里处理：那要看系统主题（`iced::system::theme()` 的结果在
    /// `Message::SystemTheme` 里落地）。
    pub(in crate::app) fn apply_config_theme(&mut self) {
        if self.has_screenshot() || self.options.theme.is_some() {
            return;
        }
        let mode = self.bridge.load().1;
        self.theme_mode = mode;
        // M229：设置页那一行画的是这个字段（唯一写者见 [`App::set_theme_mode`]）。
        self.settings.language_appearance.theme = mode;
        match mode {
            ThemeMode::Light => self.set_flavor(SchemeFlavor::Light),
            ThemeMode::Dark => self.set_flavor(SchemeFlavor::Dark),
            ThemeMode::Auto => {}
        }
    }

    /// 换主题档（M229：设置页「通用 → 主题」→ `pages::Message::Theme`）。
    ///
    /// 四件事：①改 `theme_mode`（唯一真相）②同步设置页那一行的档位 ③重建 `Ui`（色板换了，
    /// `Theme::name` 与 49 个角色一起变，iced 才会重算缓存）④落盘 `appearance.theme_mode`。
    /// `Auto` 自己没有颜色，要等系统回答——所以返回 `iced::system::theme()` 那个 Task，
    /// 由既有的 `Message::SystemTheme` 分支落地（它只在 `theme_mode == Auto` 时吃这条消息）。
    pub(in crate::app) fn set_theme_mode(&mut self, mode: ThemeMode) -> Task<Message> {
        self.settings.language_appearance.theme = mode;
        if mode == self.theme_mode {
            return Task::none();
        }
        self.theme_mode = mode;

        let prefs = self.appearance_prefs.clone();
        self.bridge.persist(&prefs, mode);
        match self.bridge.path() {
            Some(path) => diag!("[theme] mode={} 已写回 {}", mode.as_str(), path.display()),
            None => diag!(
                "[theme] mode={} 未落盘（数据根解析不出来，内存态运行）：重启会回到旧值",
                mode.as_str()
            ),
        }

        match mode {
            ThemeMode::Light => {
                self.set_flavor(SchemeFlavor::Light);
                self.after_theme_switch();
                Task::none()
            }
            ThemeMode::Dark => {
                self.set_flavor(SchemeFlavor::Dark);
                self.after_theme_switch();
                Task::none()
            }
            // 跟随系统：当前先按已有深浅画，紧接着由系统主题回答覆盖
            // （`set_flavor` 有「同档不重建」的保护，所以这里不必先猜一次）。
            ThemeMode::Auto => iced::system::theme().map(Message::SystemTheme),
        }
    }

    /// 换档之后重建视图模型 + 记一行取证日志（色板名就是证据：`ink` / `silver`）。
    fn after_theme_switch(&mut self) {
        self.view_dirty = true;
        self.refresh_view();
        diag!(
            "[theme] mode={} board={} Theme::name={}",
            self.theme_mode().as_str(),
            self.board_name(),
            self.ui.name()
        );
    }

    /// 当前主题档（设置页那一行的选中态来自 `settings.groups`，这里是取证与测试的读数口）。
    pub(in crate::app) fn theme_mode(&self) -> ThemeMode {
        self.theme_mode
    }

    /// 当前色板名（日志与测试用：`ink` / `silver`；取色方案接管时仍报它所属深浅的那块）。
    pub(in crate::app) fn board_name(&self) -> &'static str {
        self.ui.board().name()
    }

    /// 当前令牌求值器（测试与截图日志用）。
    pub(in crate::app) fn ui(&self) -> &Ui {
        &self.ui
    }
}

#[cfg(test)]
#[path = "theme/tests.rs"]
mod tests;
