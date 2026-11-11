//! 外观的**运行期状态**：配置值 → 生效值（钳位 / CPU 档降级）→ 要下发的字节与要画的格子。
//!
//! 这一层是「键 → 像素」的中间站，App 持有一份（`crate::app::state::App::appearance`）：
//!
//! ```text
//! launcher.json ──SettingsBridge::load/load_base──┐
//!                                                ▼
//!                                          State（本文件）
//!                                   ┌────────────┴────────────┐
//!                     整窗 α → 字节 │                         │ 面板 α / 背景图 → Element
//!                                   ▼                         ▼
//!                        app::window_alpha（FFI）      shell / 设置页（像素）
//! ```
//!
//! **纯的与不纯的**分界刻意写在这里：钳位链（[`super::alpha`]）、几何（[`super::background`]）、
//! 优先序（[`super::palette`]）都是纯函数；本文件只把它们的输入输出连起来，并把「读盘失败」
//! 变成一条**可读诊断**而不是一个 `unwrap`。

use unlucky_design::color::{Scheme, SchemeFlavor};
use unlucky_i18n::catalog::Key;
use unlucky_presenter::prefs::{
    AppearanceBase, AppearancePrefs, BgFill, PaletteSource, WindowCorner,
};

use crate::Backend;

use super::accent::Accent;
use super::alpha::{self, effective_glass, effective_panel_alpha, panel_is_forced_opaque};
use super::background::{self, Image, Note};
use super::palette::{self, Resolved, Source};

/// 取证开关的环境变量名（截图矩阵与真机日志用；生产默认全部不设）。
pub const ENV_WINDOW_ALPHA: &str = "ULL_APPEARANCE_WINDOW_ALPHA";
/// 面板 α。
pub const ENV_PANEL_ALPHA: &str = "ULL_APPEARANCE_PANEL_ALPHA";
/// 背景图路径。
pub const ENV_BG_IMAGE: &str = "ULL_APPEARANCE_BG_IMAGE";
/// 填充档（`cover` / `contain` / `tile`）。
pub const ENV_BG_FILL: &str = "ULL_APPEARANCE_BG_FILL";
/// 暗化强度。
pub const ENV_BG_DIM: &str = "ULL_APPEARANCE_BG_DIM";
/// 配色来源档（`background-image` / `wallpaper` / `preset`）。
pub const ENV_PALETTE: &str = "ULL_APPEARANCE_PALETTE";

/// 外观的运行期状态。
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    /// 用户请求的整窗 α（未钳位原值；下发前过 [`super::alpha`]）。
    pub window_opacity: f32,
    /// 用户请求的面板 α（CPU 档会被强制成 1.0，见 [`State::effective_panel_alpha`]）。
    pub panel_alpha: f32,
    /// 背景图总开关。
    pub bg_enabled: bool,
    /// 背景图路径（空 = 未设置）。
    pub bg_path: String,
    /// 填充档。
    pub bg_fill: BgFill,
    /// 暗化强度。
    pub bg_dim: f32,
    /// 配色来源档。
    pub palette_source: PaletteSource,
    /// 玻璃拟态开关（CPU 档强制关）。
    pub glass: bool,
    /// 自定义强调色（M250 ACCENTWIRE：输入原文 / 解析结果 / 生效方案 / 结构化错误都在里面）。
    pub accent: Accent,
    /// 窗口圆角策略（M116：`system` / `square`；真能不能圆由平台判，见 `platform::plan_corner`）。
    pub window_corner: WindowCorner,
    /// 渲染后端（决定 CPU 档的两条降级）。
    pub backend: Backend,
    /// 设置页当前分组（0 = General … 1 = Appearance；见 [`super::APPEARANCE_GROUP`]）。
    pub selected_group: usize,
    /// 已解码的背景图（`None` = 没设 / 读失败）。
    image: Option<Image>,
    /// 读盘/解码失败的**可读诊断**（按发生顺序追加；`Note` = key + 参数，句子在词表里）。
    diagnostics: Vec<Note>,
    /// 上一次的配色解析结果。
    resolved: Option<Resolved>,
    /// 当前深浅（配色解析的输入）。
    flavor: SchemeFlavor,
}

impl State {
    /// 从配置读一份（启动时调一次）。
    #[must_use]
    pub fn new(
        prefs: &AppearancePrefs,
        base: &AppearanceBase,
        backend: Backend,
        flavor: SchemeFlavor,
    ) -> Self {
        let mut state = Self {
            window_opacity: base.window_opacity,
            panel_alpha: prefs.panel_alpha,
            bg_enabled: prefs.bg_image_enabled,
            bg_path: prefs.bg_image.clone(),
            bg_fill: prefs.bg_fill,
            bg_dim: prefs.bg_dim,
            palette_source: base.palette_source,
            window_corner: base.window_corner,
            glass: prefs.glass,
            accent: Accent::new(&prefs.accent_color),
            backend,
            selected_group: super::APPEARANCE_GROUP,
            image: None,
            diagnostics: Vec::new(),
            resolved: None,
            flavor,
        };
        state.refresh_background();
        state.refresh_palette();
        state
    }

    /// 截图/真机取证用：把 `ULL_APPEARANCE_*` 灌进来（不设 = 不动）。为什么用环境变量而不是 CLI
    /// 参数：`crates/app-iced/src/cli.rs` 不在本 mission 的 scope。语义与既有取证开关（`ULL_TICK_TRACE`）
    /// 一致：**只影响这次进程**，不落盘。
    pub fn apply_env(&mut self) {
        if let Some(value) = env_f32(ENV_WINDOW_ALPHA) {
            self.window_opacity = value;
        }
        if let Some(value) = env_f32(ENV_PANEL_ALPHA) {
            self.panel_alpha = value;
        }
        if let Some(value) = std::env::var_os(ENV_BG_IMAGE) {
            let path = value.to_string_lossy().to_string();
            self.bg_enabled = !path.trim().is_empty();
            self.bg_path = path;
        }
        if let Some(value) = std::env::var_os(ENV_BG_FILL) {
            self.bg_fill = BgFill::from_key_value(&value.to_string_lossy());
        }
        if let Some(value) = env_f32(ENV_BG_DIM) {
            self.bg_dim = value;
        }
        if let Some(value) = std::env::var_os(ENV_PALETTE) {
            self.palette_source = PaletteSource::from_key_value(&value.to_string_lossy());
        }
        // 强调色的两档开关（`ULL_APPEARANCE_ACCENT` / `…_COMMIT`）收在 `accent` 自己的模块里：
        // 本文件贴 400 行硬线，而那两个开关与 `Accent` 的状态机是一件事（见 `Accent::apply_env`）。
        self.accent.apply_env();
        self.refresh_background();
        self.refresh_palette();
    }

    /// 面板 α 的**生效值**（CPU 档恒 1.0）。
    #[must_use]
    pub fn effective_panel_alpha(&self) -> f32 {
        effective_panel_alpha(self.backend, self.panel_alpha)
    }

    /// 面板 α 是否被后端强制（设置页据此禁用滑块并给说明）。
    #[must_use]
    pub fn panel_is_forced(&self) -> bool {
        panel_is_forced_opaque(self.backend)
    }

    /// 当前圆角策略（设置页显示选中项、日志与下发都用它）。
    #[must_use]
    pub fn window_corner(&self) -> WindowCorner {
        self.window_corner
    }

    /// 改圆角策略（消息落点见 `app::update::appearance`；这里只管状态，不碰窗口）。
    pub fn set_window_corner(&mut self, policy: WindowCorner) {
        self.window_corner = policy;
    }

    /// 玻璃是否生效（CPU 档恒关）。
    #[must_use]
    pub fn effective_glass(&self) -> bool {
        effective_glass(self.backend, self.glass)
    }

    /// 整窗 α 要下发的字节（`None` = 不调 FFI）。
    #[must_use]
    pub fn window_alpha_byte(&self) -> Option<u8> {
        alpha::window_alpha_byte(self.backend, self.window_opacity)
    }

    /// 背景图是否真的会画（开关开 + 路径非空 + 解码成功）。
    #[must_use]
    pub fn background_active(&self) -> bool {
        self.bg_enabled && self.image.is_some()
    }

    /// 已解码的背景图。
    #[must_use]
    pub fn image(&self) -> Option<&Image> {
        self.image.as_ref()
    }

    /// 取色得到的方案（`None` = 色卡档，继续用令牌 baseline）。
    ///
    /// 为什么色卡档不换 scheme：`resolve_source` 的色卡档走的是 `DEFAULT_CARD`（天蓝卡），
    /// 与令牌 baseline 逐色不同；直接应用会把**全部既有验收图的像素**改掉。色卡档的视觉基线
    /// 归令牌层（`Scheme::baseline`），这里只接管「取色真的成功」的那两档。
    #[must_use]
    pub fn derived_scheme(&self) -> Option<&Scheme> {
        let resolved = self.resolved.as_ref()?;
        if resolved.level() == unlucky_design::palette::Level::Preset {
            return None;
        }
        Some(&resolved.scheme)
    }

    /// 读盘/解码失败的诊断（设置页逐条显示；一条 = 一条 key + 它的参数）。
    #[must_use]
    pub fn diagnostics(&self) -> &[Note] {
        &self.diagnostics
    }

    /// 配色来源的提示 key（`None` = 一级成功）。
    #[must_use]
    pub fn palette_note(&self) -> Option<Key> {
        self.resolved.as_ref()?.note()
    }

    /// 生效的配色层级码（日志用）。
    #[must_use]
    pub fn palette_level(&self) -> &'static str {
        match &self.resolved {
            Some(resolved) => resolved.level().code(),
            None => self.palette_source.as_str(),
        }
    }

    /// 深浅切换后重算配色（`Ui` 换主题时调）。强调色不参与：它是用户给的绝对色值，与深浅无关。
    pub fn set_flavor(&mut self, flavor: SchemeFlavor) {
        if self.flavor == flavor {
            return;
        }
        self.flavor = flavor;
        self.refresh_palette();
    }

    /// 当前深浅。
    #[must_use]
    pub fn flavor(&self) -> SchemeFlavor {
        self.flavor
    }

    /// 应用一条改动（设置页 → 状态）。返回**是否需要立刻下发整窗 α**。
    pub fn apply(&mut self, change: super::Change) -> bool {
        use super::Change;
        // M250 p2-3 / P18：除「敲字」（`Accent`）以外都意味着这一格定下来了 ⇒ 提交；回车 `AccentCommit` 同路。
        if !matches!(change, Change::Accent(_)) {
            self.accent.commit();
        }
        match change {
            Change::WindowOpacity(alpha) => {
                self.window_opacity = alpha;
                true
            }
            Change::PanelAlpha(alpha) => {
                self.panel_alpha = alpha;
                false
            }
            Change::BgEnabled(enabled) => {
                self.bg_enabled = enabled;
                self.refresh_background();
                self.refresh_palette();
                false
            }
            Change::BgImage(path) => {
                self.bg_path = path;
                self.bg_enabled = !self.bg_path.trim().is_empty();
                self.refresh_background();
                self.refresh_palette();
                false
            }
            Change::BgFill(fill) => {
                self.bg_fill = fill;
                false
            }
            Change::BgDim(dim) => {
                self.bg_dim = dim;
                false
            }
            Change::PaletteSource(source) => {
                self.palette_source = source;
                self.refresh_palette();
                false
            }
            Change::Corner(policy) => {
                self.window_corner = policy;
                false
            }
            Change::Accent(text) => {
                self.accent.set(&text);
                false
            }
            Change::AccentCommit => false, // P18 回车：语义就是「提交」，上面那条规则已 `commit()`
            Change::SelectGroup(index) => {
                self.selected_group = index;
                false
            }
        }
    }

    /// 写回配置用的快照（`SettingsBridge::persist` 覆盖的那 9 个字段）。
    ///
    /// M250 起 `accent_color` 也在里面（以前恒给空串 ⇒ 设置页改的强调色落不了盘），给的是**上一次合法值**。
    #[must_use]
    pub fn current_values(&self) -> AppearancePrefs {
        AppearancePrefs {
            bg_image_enabled: self.bg_enabled,
            bg_image: self.bg_path.clone(),
            bg_fill: self.bg_fill,
            bg_dim: self.bg_dim,
            panel_alpha: self.panel_alpha,
            glass: self.glass,
            accent_color: self.accent.persist_value().to_owned(),
        }
    }

    /// 取证日志（**一行**，数值而不是观感）：`[appearance] window_alpha=… panel_alpha=… byte=…`。
    #[must_use]
    pub fn log_line(&self, context: &str) -> String {
        let byte = self
            .window_alpha_byte()
            .map_or_else(|| "none".to_owned(), |byte| byte.to_string());
        format!(
            "[appearance] {context} window_alpha={:.3} panel_alpha={:.3} (requested {:.3}) \
             byte={byte} corner={} backend={} bg={} fill={} dim={:.2} palette={} accent={} group={}",
            self.window_opacity,
            self.effective_panel_alpha(),
            self.panel_alpha,
            self.window_corner.as_str(),
            self.backend.name(),
            if self.background_active() {
                self.bg_path.as_str()
            } else {
                "none"
            },
            self.bg_fill.as_str(),
            self.bg_dim,
            self.palette_level(),
            self.accent.log_token(),
            self.selected_group
        )
    }

    /// 重读配置后同步（热切）。
    pub fn reload(&mut self, prefs: &AppearancePrefs, base: &AppearanceBase) {
        self.window_opacity = base.window_opacity;
        self.palette_source = base.palette_source;
        self.window_corner = base.window_corner;
        self.panel_alpha = prefs.panel_alpha;
        self.bg_enabled = prefs.bg_image_enabled;
        self.bg_fill = prefs.bg_fill;
        self.bg_dim = prefs.bg_dim;
        self.glass = prefs.glass;
        // M250：文件被外部改过（CLI / AI）时强调色也要热生效，错行同样重置。
        self.accent.reset(&prefs.accent_color);
        if self.bg_path != prefs.bg_image {
            self.bg_path = prefs.bg_image.clone();
        }
        self.refresh_background();
        self.refresh_palette();
    }

    // ------------------------------------------------------------ 内部

    fn refresh_background(&mut self) {
        self.image = None;
        self.diagnostics.clear();
        let trimmed = self.bg_path.trim();
        if !self.bg_enabled || trimmed.is_empty() {
            return;
        }
        match background::load(trimmed) {
            Ok(image) => self.image = Some(image),
            Err(error) => self.diagnostics.push(error.note()),
        }
    }

    fn refresh_palette(&mut self) {
        let background = match (&self.image, self.bg_enabled) {
            (Some(image), true) => Source::Pixels(image),
            (None, true) if !self.bg_path.trim().is_empty() => Source::Undecodable,
            _ => Source::Absent,
        };
        // 桌面壁纸探测（M73）尚未落地：这一级永远 `Absent`，回落链仍按 design 的规则走。
        self.resolved = Some(palette::resolve(
            self.palette_source,
            background,
            Source::Absent,
            self.flavor,
        ));
    }
}

fn env_f32(key: &str) -> Option<f32> {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite())
}
