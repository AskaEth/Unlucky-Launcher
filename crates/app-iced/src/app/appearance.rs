//! App 侧的外观动作：**下发整窗 α / 写回配置 / 打取证日志**。
//!
//! 三件事各自只有一条路，且都从这里出去（`update` 只负责把它们按消息串起来）：
//!
//! | 动作 | 唯一落点 | 为什么 |
//! |---|---|---|
//! | 下发整窗 α | [`App::apply_window_alpha`] → `app::window_alpha::dispatch` | 判据在 `platform`、FFI 在 `platform/win32`，这里只做「取 α → 交给执行侧 → 记日志」 |
//! | 写回配置 | [`App::persist_appearance`]（8 个 UI 字段）+ `AppCommand::SetSetting`（另外**三个**只读键） | 每个键**只有一个写者**：`AppearancePrefs` 里没有 `window_opacity` / `palette_source` / `window_corner`，它们只能走 schema 指令 |
//! | 取证日志 | [`App::log_appearance`] | 一行里同时有请求值、生效值、下发字节与 FFI 结局 |
//!
//! ## 为什么「写回」要分两条路
//!
//! `SettingsBridge::persist` 收的是 `AppearancePrefs`（背景图四件 + 面板 α + 玻璃 + 强调色），
//! 它的实现是「先读文件当前值，再覆盖我们改的那几个字段」——所以用它写**它覆盖的键**最省事，
//! 也不会把用户的自定义强调色冲掉。而 `appearance.window_opacity` / `appearance.palette_source` /
//! `appearance.window_corner` 不在那个结构里（它们是「只读下层键」，见
//! `presenter::prefs::AppearanceBase` 的**四个**字段：整窗 α / 标题栏三态 / 配色来源 / 窗口圆角），
//! 写它们只能走 schema 白名单指令 `AppCommand::SetSetting`（真 host 用 `SettingStore::set` 落盘）。
//! 其中「标题栏三态」目前**没有 UI 写者**（D37 的接线未落地），所以走 `SetSetting` 的是**三个**键。

use unlucky_presenter::dto::AppCommand;

use crate::pages::appearance::Change;

use super::state::App;
use super::window_alpha::{self, Dispatch};

impl App {
    /// 把当前整窗 α 交给执行侧（补 `WS_EX_LAYERED` + `SetLayeredWindowAttributes`）并记一行日志。
    ///
    /// 没有句柄时**也打日志**（`hwnd=none`）：不能让「α 没生效」这件事只体现为「屏幕上没变化」。
    pub(super) fn apply_window_alpha(&self, context: &str) {
        let Some(hwnd) = self.hwnd else {
            self.log_appearance(context, None);
            return;
        };
        let alpha = crate::platform::effective_window_alpha(
            self.appearance.backend,
            f64::from(self.appearance.window_opacity),
        );
        let dispatch = window_alpha::dispatch(hwnd, alpha);
        self.log_appearance(context, Some(&dispatch));
    }

    /// 取证日志：`[appearance] <状态行> layered=… outcome=…`。
    ///
    /// 数值来自 [`crate::pages::appearance::State::log_line`]（请求值 / 生效值 / 下发字节），
    /// 尾巴来自执行侧。**这一行就是「透明度到底设成了多少」的证据**——RDP 上的观感不可信。
    pub(super) fn log_appearance(&self, context: &str, dispatch: Option<&Dispatch>) {
        let mut line = self.appearance.log_line(context);
        if let Some(dispatch) = dispatch {
            line.push(' ');
            line.push_str(&dispatch.describe());
        } else {
            line.push_str(" layered=skipped outcome=no-window");
        }
        eprintln!("{line}");
    }

    /// 设置页改了一项外观。
    pub(super) fn on_appearance(&mut self, change: Change) {
        let needs_window = self.appearance.apply(change.clone());

        // 配色来源 / 背景图可能变了 ⇒ `Ui` 可能要改用解析出的方案。
        self.rebuild_ui();
        self.view_dirty = true;
        self.refresh_view();

        match change.key() {
            // 三个只读下层键（整窗 α / 配色来源 / 窗口圆角）：走 schema 指令
            // （`SettingsBridge` 的结构里没有它们；标题栏三态还没有 UI 写者，不在此列）。
            Some(_)
                if matches!(
                    change,
                    Change::WindowOpacity(_) | Change::PaletteSource(_) | Change::Corner(_)
                ) =>
            {
                if let (Some(key), Some(value)) = (change.key(), change.value()) {
                    self.dispatch(AppCommand::SetSetting {
                        key: key.to_owned(),
                        value,
                    });
                }
            }
            // 其余键：走 bridge（它知道怎么保留强调色这类本 mission 不管的字段）。
            Some(_) => self.persist_appearance(),
            None => {}
        }

        if needs_window {
            self.apply_window_alpha("change");
        } else {
            self.log_appearance("change", None);
        }
        // M116：圆角档位变了要**重新判定 + 重新下发**（与整窗 α 是两条独立的管线）。
        self.apply_window_corner("change");
    }

    /// 重读配置后同步外观（热切：外部 CLI / AI 改完 1 秒内生效）。
    pub(super) fn reload_appearance(&mut self) {
        let (prefs, _theme) = self.bridge.load();
        let base = self.bridge.load_base();
        self.appearance.reload(&prefs, &base);
        self.appearance_prefs = prefs;
        self.rebuild_ui();
        self.view_dirty = true;
        self.apply_window_alpha("reload");
        // M116：外部（CLI / AI）改了 `appearance.window_corner` 也要热生效。
        self.apply_window_corner("reload");
    }

    /// 把外观写回 `launcher.json`（只改本 mission 管的字段，强调色/玻璃原样保留）。
    fn persist_appearance(&mut self) {
        let mut prefs = self.appearance_prefs.clone();
        prefs.bg_image_enabled = self.appearance.bg_enabled;
        prefs.bg_image = self.appearance.bg_path.clone();
        prefs.bg_fill = self.appearance.bg_fill;
        prefs.bg_dim = self.appearance.bg_dim;
        prefs.panel_alpha = self.appearance.panel_alpha;
        prefs.glass = self.appearance.glass;

        self.bridge.persist(&prefs, self.theme_mode);
        self.appearance_prefs = prefs;

        match self.bridge.path() {
            Some(path) => eprintln!("[settings] 外观已写回 {}", path.display()),
            None => eprintln!("[settings] 外观未落盘（数据根解析不出来，内存态运行）"),
        }
    }
}
