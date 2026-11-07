//! 令牌 → iced 类型（`Color` / `Font` / `Border` / `Radius` / `Theme`）。
//!
//! 本模块是**唯一**允许把 `unlucky-design` 的值翻成 iced 类型的地方（M61 §0）：组件层只调
//! [`Ui`]，绝不出现色值/字号/半径/间距字面量（`tests/no_hardcoded_tokens.rs` 扫 `src/**`）。
//!
//! ## 为什么不用 `iced::theme::Base` 承载 49 个角色
//!
//! iced 的 `Palette` 只有 6 色，M3 有 49 个角色（M60 Q5）：真源是本模块的 [`Scheme`]，`Theme`
//! 只用来「换名字」（名字变了 iced 才重算缓存），控件外观由各自的 `Style` 注入。
//! `success`/`warning` 没有对应角色，按邻近语义映射（→ `Secondary` / `Tertiary`），不许读。

use iced::border::Radius;
use iced::theme::Palette;
use iced::{Border, Color, Font, Theme};

use unlucky_design::color::{Role, Scheme, SchemeFlavor, StateLayer};
use unlucky_design::elevation::MaterialLayer;
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
    platform: Platform,
    metrics: LayoutMetrics,
    families: Families,
    /// 面板层不透明度（D36 / M93）：**唯一的 α 来源**，由 [`Ui::layer`] 统一施加。
    panel_alpha: f32,
}

impl Ui {
    /// 按平台（由 `cfg!` 决定）与令牌基线方案求值。
    pub fn new(flavor: SchemeFlavor, families: Families) -> Self {
        Self::with_platform(flavor, Platform::current(), families)
    }

    /// 显式指定平台（跨平台核对与测试用）。
    pub fn with_platform(flavor: SchemeFlavor, platform: Platform, families: Families) -> Self {
        Self {
            scheme: Scheme::baseline(flavor),
            platform,
            metrics: LayoutMetrics::DESKTOP,
            families,
            panel_alpha: 1.0,
        }
    }

    /// 当前深浅。
    pub fn flavor(&self) -> SchemeFlavor {
        self.scheme.flavor()
    }

    /// 用**已解析的方案**构造求值器（D38：背景图 / 壁纸取色 → 49 个角色换掉 baseline）。
    ///
    /// 追加式入口：`new` / `with_platform` 的行为一字未动（默认仍是 [`Scheme::baseline`]）；
    /// 平台与骨架尺寸仍按当前环境求值——方案只带颜色。
    pub fn from_scheme(scheme: Scheme, families: Families) -> Self {
        Self {
            scheme,
            platform: Platform::current(),
            metrics: LayoutMetrics::DESKTOP,
            families,
            panel_alpha: 1.0,
        }
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
    pub fn name(&self) -> &'static str {
        match self.flavor() {
            SchemeFlavor::Light => "ull-m3-light",
            SchemeFlavor::Dark => "ull-m3-dark",
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
mod tests {
    use super::*;

    const FAMILIES: Families = Families {
        text: "Segoe UI Variable Text",
        display: "Segoe UI Variable Display",
        mono: "Cascadia Mono",
    };

    fn ui(flavor: SchemeFlavor) -> Ui {
        Ui::with_platform(flavor, Platform::Windows, FAMILIES)
    }

    #[test]
    fn theme_name_tracks_the_flavor() {
        assert_eq!(ui(SchemeFlavor::Light).name(), "ull-m3-light");
        assert_eq!(ui(SchemeFlavor::Dark).name(), "ull-m3-dark");
        assert_ne!(
            ui(SchemeFlavor::Light).theme().to_string(),
            ui(SchemeFlavor::Dark).theme().to_string()
        );
    }

    #[test]
    fn canvas_card_overlay_follow_surface_containers() {
        let ui = ui(SchemeFlavor::Light);
        assert_eq!(ui.layer(MaterialLayer::Canvas), ui.color(Role::Surface));
        assert_eq!(
            ui.layer(MaterialLayer::Card),
            ui.color(Role::SurfaceContainerLow)
        );
        assert_eq!(
            ui.layer(MaterialLayer::Overlay),
            ui.color(Role::SurfaceContainerHigh)
        );
    }

    #[test]
    fn radii_only_come_from_the_scale() {
        let ui = ui(SchemeFlavor::Light);
        // 卡片 Medium(12)：边长够大时就是 12
        assert_eq!(ui.radius_dp(ShapeScale::Medium).top_left, 12.0);
        assert_eq!(ui.radius_dp(ShapeScale::ExtraSmall).top_left, 4.0);
        // 药丸 Full：按边长一半折算
        assert_eq!(ui.radius(ShapeScale::Full, 32.0).top_left, 16.0);
        // 不许超过边长一半
        assert_eq!(ui.radius(ShapeScale::ExtraLarge, 20.0).top_left, 10.0);
    }

    #[test]
    fn outlines_are_one_pixel_and_use_outline_variant() {
        let ui = ui(SchemeFlavor::Light);
        let border = ui.outlined(ShapeScale::Medium);
        assert_eq!(border.width, STROKE_PX);
        assert_eq!(border.width, 1.0, "rubric G7：全站 1px");
        assert_eq!(border.color, ui.color(Role::OutlineVariant));
        assert_eq!(border.radius.top_left, 12.0);
    }

    #[test]
    fn emphasized_weights_are_the_official_ones() {
        let ui = ui(SchemeFlavor::Light);
        // title-small 的强调档是 700（官方 Emphasized）
        assert_eq!(
            ui.emphasized_font(TypeRole::TitleSmall).weight,
            iced::font::Weight::Bold
        );
        // baseline 正文是 400
        assert_eq!(
            ui.font(TypeRole::BodyMedium).weight,
            iced::font::Weight::Normal
        );
    }

    #[test]
    fn body_text_keeps_wcag_contrast_in_both_flavors() {
        for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
            let scheme = Scheme::baseline(flavor);
            assert!(
                scheme.contrast(Role::OnSurface, Role::Surface) >= 4.5,
                "{flavor:?} 的正文对比度不够"
            );
            assert!(
                scheme.contrast(Role::OnSecondaryContainer, Role::SecondaryContainer) >= 4.5,
                "{flavor:?} 的选中态药丸对比度不够"
            );
        }
    }

    #[test]
    fn every_role_maps_to_an_iced_color() {
        for role in Role::ALL {
            let color = ui(SchemeFlavor::Dark).color(role);
            assert!((0.0..=1.0).contains(&color.a), "{role:?} 的 alpha 越界");
        }
    }

    #[test]
    fn state_layers_are_opaque_so_cpu_and_gpu_match() {
        let ui = ui(SchemeFlavor::Light);
        let hovered = ui.state_over(StateLayer::Hover, Role::OnSurface, Role::Surface);
        assert_eq!(hovered.a, 1.0, "状态层必须给出不透明结果");
        assert_ne!(hovered, ui.color(Role::Surface));
    }

    /// D38：配色方案必须真的换色；默认构造逐字段等于 baseline（防回流的那一半）。
    /// 抓不住：取色好不好看、设置页是否真切过去（后者由 app 侧日志举证）。
    #[test]
    fn a_resolved_scheme_replaces_the_baseline_colours() {
        use unlucky_design::palette::{resolve_source, CardKey, ImageInput};

        // 4×4 的纯红图：种子彩度远高于门槛，必然走到「背景图取色」这一级。
        let pixels = [0xC0_u8, 0x20, 0x20, 0xFF].repeat(16);
        let input = ImageInput::pixels(&pixels, 4, 4);
        let (scheme, provenance) =
            resolve_source(Some(input), None, Some(CardKey::Sky), SchemeFlavor::Light);
        assert_eq!(
            provenance.level,
            unlucky_design::palette::Level::BackgroundImage,
            "纯红图必须取色成功（否则本测试的输入不合格）"
        );

        let tinted = Ui::from_scheme(scheme, FAMILIES);
        let baseline = ui(SchemeFlavor::Light);
        assert_ne!(
            tinted.color(Role::Primary),
            baseline.color(Role::Primary),
            "取色方案必须换掉 primary"
        );
        // 深浅仍跟着方案走（方案自带 flavor），不会把 Light 变成 Dark。
        assert_eq!(tinted.flavor(), SchemeFlavor::Light);
        assert_eq!(tinted.name(), baseline.name());

        // 默认构造不变：`with_platform` 与 baseline 逐块相等。
        let explicit = Ui::from_scheme(Scheme::baseline(SchemeFlavor::Light), FAMILIES);
        for role in Role::ALL {
            assert_eq!(
                explicit.color(role),
                baseline.color(role),
                "{role:?}：不传 scheme 时默认行为必须与改动前一致"
            );
        }
    }
}
