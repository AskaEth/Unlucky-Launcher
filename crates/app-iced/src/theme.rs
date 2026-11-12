//! # 令牌 → iced 类型（`Color` / `Font` / `Border` / `Radius` / `Theme`）。
//!
//! 本模块是**唯一**允许把 `unlucky-design` 的值翻成 iced 类型的地方（M61 §0）：组件层只调
//! [`Ui`]，绝不出现色值/字号/半径/间距字面量（`tests/no_hardcoded_tokens.rs` 扫 `src/**`）。
//!
//! ## 两套来源：品牌色板（出厂）与取色方案（D38）
//!
//! M229 起 [`Ui::new`] 走 **品牌色板**（[`BrandBoard`]：Ink = 深色主方案 / Silver = 浅色主方案），
//! `Scheme::baseline`（tone 派生）退回它本来的角色——D38 取色链的中性锚点。两条路径并列：
//!
//! - [`Ui::brand`]：出厂外观，色板自己决定深浅（[`BrandBoard::flavor`]）；
//! - [`Ui::with_accent`]：用户自定义强调色接管 primary/secondary/tertiary 三族（M250），
//!   中性表面与 error 保持基线；
//! - [`Ui::from_scheme`]：背景图 / 壁纸取色成功时接管 49 个角色（取色失败回落色板）；
//! - [`Ui::baseline`]：老的 tone 派生口径，只给对照与测试用（产品路径不再默认走它）。
//!
//! 「跟随系统」是**设置层的第三种态**，不是第三套板：系统浅色 → Silver、系统深色 → Ink
//! （[`BrandBoard::for_flavor`]）。色板始终只有两套。
//!
//! ## 为什么不用 `iced::theme::Base` 承载 49 个角色
//!
//! iced 的 `Palette` 只有 6 色，M3 有 49 个角色（M60 Q5）：真源是本模块的 [`Scheme`]，`Theme`
//! 只用来「换名字」（名字变了 iced 才重算缓存），控件外观由各自的 `Style` 注入。
//! `success`/`warning` 没有对应角色，按邻近语义映射（→ `Secondary` / `Tertiary`），不许读。

use iced::border::Radius;
use iced::theme::Palette;
use iced::{Border, Color, Font, Theme};

use unlucky_design::color::{BrandBoard, Role, Scheme, SchemeFlavor, StateLayer};
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::inkcat::metrics::{self as inkcat, SpecBox};
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::{LayoutMetrics, Space};
use unlucky_design::typography::{FontFamily, Platform, TypeRole, TypeStyle, Weight};

/// 全站描边宽度（rubric G7：1px）。令牌层的 `stroke::HAIRLINE` 是唯一定义点，这里只再导出。
pub use unlucky_design::stroke::HAIRLINE as STROKE_PX;

/// 已经解析到系统的字体族名（`&'static str` 才能塞进 `iced::Font`）。
///
/// 由 `fonts` 模块按令牌栈 + 系统字体库求值，本模块只消费。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Families {
    /// 正文族（`FontFamily::Text` 栈里第一个装机可用的）。
    pub text: &'static str,
    /// 展示族（`FontFamily::Display`）。
    pub display: &'static str,
    /// 等宽族（`FontFamily::Mono`，日志/版本号用）。
    pub mono: &'static str,
}

/// 令牌求值器：一个 `Ui` 对应一套主题（浅或深）。热切 = 重建 `Ui` 并整树重绘，
/// 控件里不要缓存颜色。
#[derive(Debug, Clone)]
pub struct Ui {
    scheme: Scheme,
    /// 当前色板（Ink / Silver）。`from_scheme` 时取方案深浅对应的那块，`name()` 与图例用它。
    board: BrandBoard,
    platform: Platform,
    metrics: LayoutMetrics,
    families: Families,
    /// 面板层不透明度（D36 / M93）：**唯一的 α 来源**，由 [`Ui::layer`] 统一施加。
    panel_alpha: f32,
}

impl Ui {
    /// 按平台（由 `cfg!` 决定）与**品牌色板**求值：深浅决定色板（浅 → Silver / 深 → Ink）。
    ///
    /// 这是产品出厂外观的唯一入口（M229）：`DesignTokens::brand(BrandBoard::for_flavor(flavor))`。
    pub fn new(flavor: SchemeFlavor, families: Families) -> Self {
        Self::with_platform(flavor, Platform::current(), families)
    }

    /// 显式指定平台（跨平台核对与测试用）。
    pub fn with_platform(flavor: SchemeFlavor, platform: Platform, families: Families) -> Self {
        Self::with_board(BrandBoard::for_flavor(flavor), platform, families)
    }

    /// 按**指定色板**求值（色板自己决定深浅，见 [`BrandBoard::flavor`]）。
    pub fn brand(board: BrandBoard, families: Families) -> Self {
        Self::with_board(board, Platform::current(), families)
    }

    /// 指定色板 + 平台（截图与测试用）。
    pub fn with_board(board: BrandBoard, platform: Platform, families: Families) -> Self {
        Self {
            scheme: board.scheme(),
            board,
            platform,
            metrics: LayoutMetrics::DESKTOP,
            families,
            panel_alpha: 1.0,
        }
    }

    /// tone 派生的 `Scheme::baseline` 口径（D38 取色链的中性锚点；产品路径不再默认走它）。
    ///
    /// 留成公开入口是为了对照与测试：`Ui::new(flavor)` 与它在**颜色上不同**，那是 M229 的换肤。
    pub fn baseline(flavor: SchemeFlavor, families: Families) -> Self {
        Self::from_scheme(Scheme::baseline(flavor), families)
    }

    /// 当前深浅。
    pub fn flavor(&self) -> SchemeFlavor {
        self.scheme.flavor()
    }

    /// 当前色板（Ink / Silver）。
    pub fn board(&self) -> BrandBoard {
        self.board
    }

    /// 用**已解析的方案**构造求值器（D38：背景图 / 壁纸取色 → 49 个角色换掉色板）。
    ///
    /// 色板字段取方案深浅对应的那块（`Light → Silver` / `Dark → Ink`）：取色方案只换颜色，
    /// 不改变「这一屏是深色还是浅色」，所以 `name()` 仍然说实话。
    pub fn from_scheme(scheme: Scheme, families: Families) -> Self {
        Self {
            board: BrandBoard::for_flavor(scheme.flavor()),
            scheme,
            platform: Platform::current(),
            metrics: LayoutMetrics::DESKTOP,
            families,
            panel_alpha: 1.0,
        }
    }

    /// 按**用户自定义强调色**求值（M250 ACCENTWIRE）：`DesignTokens::with_accent` 的 iced 侧入口。
    ///
    /// 语义来自 design 的 [`Scheme::from_accent`]（既定产品口径，不是缺口）：只换
    /// primary / secondary / tertiary 三族，**中性表面与 error 保持基线**——「换强调色不该让整个
    /// 窗口底色变脏」。所以拿它跟 [`Ui::new`] 比，Primary 类角色会不同，Surface 系列**不会**。
    pub fn with_accent(
        flavor: SchemeFlavor,
        accent: unlucky_design::color::Color,
        families: Families,
    ) -> Self {
        Self::from_scheme(accent_scheme(flavor, accent), families)
    }

    /// 当前平台（决定字体族与字重降级）。
    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// 骨架尺寸（标题栏/导航/行高…）。
    pub fn metrics(&self) -> LayoutMetrics {
        self.metrics
    }

    /// 已解析的字体族名。
    pub fn families(&self) -> Families {
        self.families
    }

    /// 间距档位（4px 栅格）。
    pub fn space(&self, space: Space) -> f32 {
        space.dp()
    }

    /// M3 角色 → iced 颜色。
    pub fn color(&self, role: Role) -> Color {
        to_iced(self.scheme.get(role))
    }

    /// 当前生效的 **49 角色方案**（品牌色板 / D38 取色方案，二选一的那一份）。
    ///
    /// 凡是要**在令牌侧合成颜色**的地方（禁用态 38% 叠底、对话框遮罩 scrim、状态层叠色）都必须
    /// 从这里取，**不许**再 `Scheme::baseline(self.flavor())` 重算：
    ///
    /// - 重算出来的永远是 tone 派生的旧色（天蓝中性），品牌板上的禁用滑块轨道、填充框、遮罩会
    ///   各画各的（M229 复审 F1 实测：Ink 下三个蓝系像素值，应为绿系去饱和、遮罩为中性
    ///   绿黑；具体数值与出处见 `DevDocs/artifacts/m229/README.md` §6）。
    /// - 取色方案（D38）成功时也必须同源，否则「取色接管 49 个角色」这件事在禁用态上破一个洞。
    pub fn scheme(&self) -> &Scheme {
        &self.scheme
    }

    /// 材质层（Canvas/Card/Overlay）→ 容器色；**只有令牌允许半透明的层才乘面板 α**（M93 唯一来源）。
    ///
    /// α 乘在这里而不是调用点：所有表面色都从这里出去（根容器/标题栏/导航轨/页头带/卡片），
    /// 一次同源、调用点无从「各自自乘」。Canvas 层恒不透明（r1 P2-2）：令牌
    /// [`MaterialLayer::allows_translucency`] 写死 `Canvas => false`（窗口底），而三个窗口键与
    /// 导航轨、页头带都取 Canvas——给它们乘 α 会让文字直接压在背景照片上（α=0.55 实测不可读）。
    pub fn layer(&self, layer: MaterialLayer) -> Color {
        let base = self.color(layer.surface_role());
        if layer.allows_translucency() {
            base.scale_alpha(self.panel_alpha)
        } else {
            base
        }
    }

    /// 当前面板 α（CPU 档由 `app` 侧强制成 1.0）。
    pub fn panel_alpha(&self) -> f32 {
        self.panel_alpha
    }

    /// 换一个面板 α（只影响 `layer()` 出的表面色，不动角色色 `color()`；默认 `1.0`）。
    #[must_use]
    pub fn with_panel_alpha(mut self, alpha: f32) -> Self {
        self.panel_alpha = if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self
    }

    /// 状态层叠在容器色上，返回**不透明**结果（D27：GPU/CPU 两档状态色逐像素一致；
    /// 只有材质/面板才允许两档不同）。
    pub fn state_over(&self, state: StateLayer, content: Role, container: Role) -> Color {
        to_iced(state.over(self.scheme.get(content), self.scheme.get(container)))
    }

    /// 圆角（`side` = 控件在该轴上的边长，`Full` 要按它折算）。
    pub fn radius(&self, shape: ShapeScale, side: f32) -> Radius {
        Radius::new(shape.radius_px(side))
    }

    /// **规格卡口径的卡片方框**（`ELEVATED CARD` / `CARD`，`inkcat::metrics::CARD`）。
    ///
    /// 与 `component_map` 的 `OutlinedCard = Medium(12)` 差 4：规格卡画的是 `rx=16`（Large），
    /// M228 的 `inkcat` 模块把这条差异登记为「换肤时取 `CARD`」，M229 就是那次换肤。表值不动
    /// （它还服务别的面），页面卡片统一从这里取。
    pub fn card_box(&self) -> SpecBox {
        inkcat::CARD
    }

    /// 卡片圆角档（= [`Self::card_box`] 的 `shape`，即 `ShapeScale::Large` = 16）。
    pub fn card_shape(&self) -> ShapeScale {
        self.card_box().shape
    }

    /// 固定档圆角。`ShapeScale::Full` 必须走 [`Ui::radius`]（它没有绝对值）。
    pub fn radius_dp(&self, shape: ShapeScale) -> Radius {
        debug_assert!(
            !matches!(shape, ShapeScale::Full),
            "Full 需要边长，请用 Ui::radius(shape, side)"
        );
        Radius::new(if matches!(shape, ShapeScale::Full) {
            0.0
        } else {
            shape.dp()
        })
    }

    /// 描边：`OutlineVariant` + [`STROKE_PX`]，圆角走刻度（固定档）。
    pub fn outlined(&self, shape: ShapeScale) -> Border {
        Border {
            color: self.color(Role::OutlineVariant),
            width: STROKE_PX,
            radius: self.radius_dp(shape),
        }
    }

    /// baseline 排版档。
    pub fn style(&self, role: TypeRole) -> TypeStyle {
        role.style()
    }

    /// Expressive 强调档（官方值：字重升到 500/700）。
    pub fn emphasized(&self, role: TypeRole) -> TypeStyle {
        role.emphasized()
    }

    /// 排版档 → iced 字体（族 + 字重）。
    pub fn font_of(&self, style: TypeStyle) -> Font {
        let family = match style.family {
            FontFamily::Display => self.families.display,
            FontFamily::Text => self.families.text,
            FontFamily::Mono => self.families.mono,
        };

        Font {
            family: iced::font::Family::Name(family),
            weight: weight_of(style.weight),
            ..Font::DEFAULT
        }
    }

    /// baseline 档的字体。
    pub fn font(&self, role: TypeRole) -> Font {
        self.font_of(self.style(role))
    }

    /// 强调档的字体。
    pub fn emphasized_font(&self, role: TypeRole) -> Font {
        self.font_of(self.emphasized(role))
    }

    /// iced 的 6 色 `Palette`（只给 devtools；M3 的 49 角色在 [`Ui::color`]）。
    pub fn palette(&self) -> Palette {
        Palette {
            background: self.color(Role::Surface),
            text: self.color(Role::OnSurface),
            primary: self.color(Role::Primary),
            success: self.color(Role::Secondary),
            warning: self.color(Role::Tertiary),
            danger: self.color(Role::Error),
        }
    }

    /// iced 主题对象：**名字带深浅**，切主题时 `Theme::name()` 变化会触发重算。
    pub fn theme(&self) -> Theme {
        Theme::custom(self.name(), self.palette())
    }

    /// `Theme::name()` 的取值（稳定字符串，改名要先在协作文档登记）。
    ///
    /// M229 起按**色板**取名（`ull-ink` / `ull-silver`）：换肤之后「深色」这个词不再等于
    /// 「M3 深色 tone 派生」，名字必须能区分「是哪块板」。D38 取色方案仍叫它所属深浅的那块
    /// （取色只换颜色，不换深浅）。
    pub fn name(&self) -> &'static str {
        match self.board {
            BrandBoard::Ink => "ull-ink",
            BrandBoard::Silver => "ull-silver",
        }
    }
}

/// 令牌色（8 bit sRGB）→ iced 的 sRGB 分量。
pub fn to_iced(color: unlucky_design::color::Color) -> Color {
    Color::from_rgba8(
        color.r,
        color.g,
        color.b,
        f32::from(color.a) / f32::from(u8::MAX),
    )
}

/// 强调色 → 49 角色方案：`DesignTokens::with_accent` 的**唯一**调用点（M250）。
///
/// 放在适配层而不是调用点，是因为「强调色只换三族、中性色保持基线」这条语义属于令牌层；
/// 页面侧只该拿到「一套方案」，不该各自决定换哪几个角色。
pub fn accent_scheme(flavor: SchemeFlavor, accent: unlucky_design::color::Color) -> Scheme {
    unlucky_design::DesignTokens::with_accent(flavor, accent).colors
}

/// 这一屏的颜色从哪来（M250）。日志与出图都靠它说清「是谁生效」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colors {
    /// 用户显式填的强调色（`appearance.accent_color`）。
    Accent,
    /// D38 取色方案（背景图 / 壁纸取色成功，`State::derived_scheme`）。
    Scheme,
    /// 出厂品牌色板（`DesignTokens::brand`）。
    Board,
}

/// 强调色 / 取色方案 / 出厂板 **三选一**：全仓**唯一**的优先级定义点（M250）。
///
/// 产品路径（`App::rebuild_ui`）与出图 harness（`pages::harness`）都调它。为什么必须有这一个
/// 函数：两头各抄一份两选一/三选一，迟早出现「出图上有强调色、产品里没有」或者反过来，而那种
/// 偏差在图上看不出来（图是对的，产品是错的）。
///
/// 优先级：**显式强调色 > D38 取色方案 > 出厂板**。手填的色是直接意图，取色是从图片派生的。
/// `flavor`（明暗 / 跟随系统）不在这里解析，由调用方按现有主题逻辑给。
pub fn tokens(
    flavor: SchemeFlavor,
    accent: Option<unlucky_design::color::Color>,
    scheme: Option<Scheme>,
    families: Families,
) -> (Ui, Colors) {
    if let Some(accent) = accent {
        (Ui::with_accent(flavor, accent, families), Colors::Accent)
    } else if let Some(scheme) = scheme {
        (Ui::from_scheme(scheme, families), Colors::Scheme)
    } else {
        (Ui::new(flavor, families), Colors::Board)
    }
}

/// 令牌字重 → iced 字重。
pub fn weight_of(weight: Weight) -> iced::font::Weight {
    match weight {
        Weight::Regular => iced::font::Weight::Normal,
        Weight::Medium => iced::font::Weight::Medium,
        Weight::SemiBold => iced::font::Weight::Semibold,
        Weight::Bold => iced::font::Weight::Bold,
    }
}

#[cfg(test)]
mod tests;
