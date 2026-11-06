//! 图标令牌：`IconKind`（一次冻结，只允许在末尾追加）+ 24 网格几何数据。
//!
//! ```no_run
//! use unlucky_design::icons::{IconKind, IconSize};
//!
//! let icon = IconKind::Settings.icon();
//! let w = icon.stroke_width(IconSize::Nav.px());
//! assert!((w - 1.8).abs() < 1e-6);
//! ```
//!
//! ## 几何口径（三线统一，M54 定稿）
//!
//! - **网格 24×24**（[`VIEWBOX`]），内容留 2~3 单位边距；
//! - **视觉描边 1.5px**（[`VISUAL_STROKE_PX`]）：不管渲染多大，线宽看起来都是 1.5 逻辑像素；
//! - **圆头端点 + 圆角连接**（`stroke-linecap: round` / `stroke-linejoin: round`），
//!   适配层设这两个属性，数据里不表达；
//! - **实心图形才用 fill**（播放/停止/三点），其余一律描边，保证与文字并排时重量一致；
//! - 不用字体字符当图标（`+` `→` 一律禁止）。
//!
//! ## ⚠️ 描边语义差异：Slint 与 SVG **不是一回事**
//!
//! - Slint 的 `Path.stroke-width` 是**绝对像素**：只乘 DPI 缩放，**不乘 viewBox**。
//!   所以老代码写 `stroke-width: 1.5px` 意味着「渲染出来就是 1.5px」，viewBox 缩放对它无影响
//!   （实证：femtovg 后端 `PathItemRenderer::render` 里的
//!   `paint.set_line_width(stroke_width * scale_factor)`——只有 DPI 缩放因子，没有 viewBox 项）。
//! - SVG / iced canvas 的 `stroke-width` 是**用户单位**（viewBox 单位）：渲染时会被
//!   `render_px / VIEWBOX` 缩放。同样的 `1.5` 在 20px 渲染下只有 1.25px 视觉宽度。
//!
//! 因此 iced 侧必须**按 SVG 语义重算**：要保持 1.5px 视觉线宽，viewBox 里的 stroke-width 得取
//! `1.5 × 24 / render_px`（24px → 1.5、20px → 1.8、18px → 2.0、16px → 2.25）。[`Icon::stroke_width`]
//! 就是这个换算的纯函数——**不要在适配层手抄 1.5 这个数**，那是 Slint 口径。

mod glyphs;
mod kinds;

pub use kinds::IconKind;

/// 图标网格边长（viewBox 宽高）。
pub const VIEWBOX: f32 = 24.0;

/// 视觉描边宽度（逻辑像素，与渲染尺寸无关）。
pub const VISUAL_STROKE_PX: f32 = 1.5;

/// 描边语义换算：给定渲染尺寸与目标视觉线宽，返回 **viewBox 单位**的 stroke-width。
///
/// `render_px <= 0` 时返回 `visual_px`（退化情形不产生除零/无穷大）。
pub fn stroke_width_for(render_px: f32, visual_px: f32) -> f32 {
    if render_px <= 0.0 {
        return visual_px;
    }
    visual_px * VIEWBOX / render_px
}

/// 反向换算：viewBox 单位的 stroke-width 在实际渲染尺寸下的视觉线宽。
pub fn visual_stroke_px(render_px: f32, stroke_width: f32) -> f32 {
    stroke_width * render_px / VIEWBOX
}

/// 图标的绘制方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconPaint {
    /// 只描边（默认，绝大多数图标）。
    Stroke,
    /// 填充（播放/停止/三点这类实心图形）。
    Fill,
}

/// 图标的标准渲染尺寸（页面只从这几档里取，不许自定义零散值）。
///
/// ## 每一档服务于哪个规格（出处全部是 A 级：`material3-android-1.4.0.aar` 的字节码）
///
/// 判据顺序固定为：**先看官方组件实际消费哪个值，再看 token 类里写了什么**。两者冲突时以
/// 「有消费者的那个」为准——1.4.0 里 `ButtonSmallTokens.IconSize = 20` 就是**零消费者**的死令牌
/// （`javap` 全 AAR 搜 `getIconSize` 调用，没有任何类读它；`ButtonDefaults` 只借了同一
/// token 类的 `IconLabelSpace` / `ContainerHeight` / `ContainerShapeRound`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconSize {
    /// 12px：行内小图标（徽标内）。
    Inline,
    /// 16px：**控件内微字形**——Switch 滑块内的勾/图标。
    ///
    /// A 级：`SwitchTokens.SelectedIconSize = 16.0d` / `UnselectedIconSize = 16.0d`。
    /// 别拿它当按钮图标用：按钮是 18（见 [`IconSize::Button`]），两者在官方是两个 token。
    SwitchThumb,
    /// 18px：**按钮内前置/后置图标**，也用于 Chip 图标。
    ///
    /// A 级：`ButtonDefaults.<clinit>` 里 `bipush 18` → `IconSize = 18.dp`（12b 1.6 引的
    /// `ButtonDefaults.IconSize=18` 就是这一行）；`AssistChipTokens.IconSize = 18.0d`（12b 13.4）。
    /// 死令牌说明：`BaselineButtonTokens.IconSize` / `ButtonSmallTokens.IconSize` 都写 20，
    /// 但 AAR 里没有任何类调用它们的 `getIconSize`（`FilledTonalButtonTokens` 反而是 18）。
    Button,
    /// 20px：**目录档，当前无消费**。
    ///
    /// ⚠️ 别按名字拿它当导航/列表图标：官方导航轨与列表行的图标是 **24**
    /// （`AppBarTokens.IconSize = 24.0d`、12b 5.5 / 4.4），实现侧也一直是 24（[`IconSize::Toolbar`]）。
    /// 标题栏 caption 按钮的 20 不是这一档，见 [`TITLEBAR_CAPTION_ICON_PX`]（那是项目偏离）。
    Nav,
    /// 24px：卡片头、工具栏、IconButton、标题栏 caption（官方口径）。
    ///
    /// A 级：`SmallIconButtonTokens.IconSize = 24.0d`（12b 2.2 / 16.5）、`AppBarTokens.IconSize = 24.0d`。
    Toolbar,
    /// 32px：空态图标块。
    Empty,
}

impl IconSize {
    /// 像素值。
    pub const fn px(self) -> f32 {
        match self {
            IconSize::Inline => 12.0,
            IconSize::SwitchThumb => 16.0,
            IconSize::Button => 18.0,
            IconSize::Nav => 20.0,
            IconSize::Toolbar => 24.0,
            IconSize::Empty => 32.0,
        }
    }

    /// 全部档位，由小到大。
    pub const ALL: [IconSize; 6] = [
        IconSize::Inline,
        IconSize::SwitchThumb,
        IconSize::Button,
        IconSize::Nav,
        IconSize::Toolbar,
        IconSize::Empty,
    ];
}

/// 标题栏 caption 按钮（最小化 / 最大化 / 关闭）的图标边长——**用户点名的项目偏离**。
///
/// 官方 `SmallIconButtonTokens.IconSize = 24.0d`（AAR 字节码，12b 16.5 同值），我们的 24
/// **本来就是对的**，所以这条不是「对齐规范」，是业主主观调整：用户原话「右上角三个金刚键的
/// 图标显示稍大了些，能不能在**保持其容器尺寸不变**的前提下把图标改小些」。
///
/// 约束：**容器不动**（46×48 仍来自 `LayoutMetrics::titlebar_button_width` /
/// `titlebar_height`），只缩图标；且这条**只适用于 caption 三键**，不许扩散到导航/卡片/表单的
/// 24px 图标（那些是 12b 2.2 / 4.4 / 5.5 / 9.6 / 16.5 的判据）。
pub const TITLEBAR_CAPTION_ICON_PX: f32 = 20.0;

/// 一个图标的几何数据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Icon {
    /// 图标种类。
    pub kind: IconKind,
    /// SVG path 数据（viewBox `0 0 24 24`，可能含多个子路径）。
    pub path: &'static str,
    /// 描边还是填充。
    pub paint: IconPaint,
}

impl Icon {
    /// 表项构造器（只在 [`glyphs`] 的数据表里用）。
    pub(super) const fn data(kind: IconKind, path: &'static str, paint: IconPaint) -> Self {
        Self { kind, path, paint }
    }

    /// 该图标在 `render_px` 下的 **viewBox 单位** stroke-width（[`IconPaint::Fill`] 返回 0）。
    pub fn stroke_width(self, render_px: f32) -> f32 {
        match self.paint {
            IconPaint::Fill => 0.0,
            IconPaint::Stroke => stroke_width_for(render_px, VISUAL_STROKE_PX),
        }
    }

    /// 是否实心填充。
    pub fn is_filled(self) -> bool {
        self.paint == IconPaint::Fill
    }

    /// 路径里是否含曲线命令（适配层可选：纯直线路径走不走曲线细分）。
    pub fn has_curves(self) -> bool {
        self.path
            .bytes()
            .any(|b| matches!(b, b'C' | b'Q' | b'A' | b'c' | b'q' | b'a'))
    }

    /// 子路径个数（`M` 命令计数）：适配层可以据此一次性分配 `lyon` 的缓冲区。
    pub fn subpath_count(self) -> usize {
        self.path.matches('M').count() + self.path.matches('m').count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_use_only_svg_commands() {
        const ALLOWED: &str = "MmLlHhVvAaCcQqZz0123456789 .,-";
        for kind in IconKind::ALL {
            let path = kind.icon().path;
            assert!(path.starts_with('M'), "{} 的路径必须以 M 开头", kind.name());
            for byte in path.bytes() {
                assert!(
                    byte.is_ascii(),
                    "{} 的路径含非 ASCII（不许用字体字符/中文）",
                    kind.name()
                );
            }
            for ch in path.chars() {
                assert!(
                    ALLOWED.contains(ch),
                    "{} 的路径含非法字符 {ch:?}",
                    kind.name()
                );
            }
            // 不许出现科学计数法（能解析但可读性差、易手误）。
            assert!(
                !path.contains('e') && !path.contains('E'),
                "{} 用了科学计数法",
                kind.name()
            );
            assert!(
                path.matches(' ').count() >= 2,
                "{} 的坐标分隔异常",
                kind.name()
            );
        }
    }

    #[test]
    fn coordinates_stay_inside_the_grid() {
        for kind in IconKind::ALL {
            for token in kind.icon().path.split([' ', ',']) {
                if let Ok(value) = token.parse::<f32>() {
                    assert!(
                        (-1.0..=VIEWBOX + 1.0).contains(&value),
                        "{} 出现 {value}：超出 24 网格（弧半径/控制点也受这条约束）",
                        kind.name()
                    );
                }
            }
        }
    }

    #[test]
    fn subpaths_are_counted() {
        assert_eq!(IconKind::Plus.icon().subpath_count(), 2);
        assert_eq!(IconKind::Instances.icon().subpath_count(), 4);
        assert_eq!(IconKind::Play.icon().subpath_count(), 1);
    }

    #[test]
    fn filled_icons_have_no_stroke() {
        for kind in IconKind::ALL {
            let icon = kind.icon();
            if icon.is_filled() {
                assert_eq!(icon.stroke_width(20.0), 0.0, "{}", kind.name());
            } else {
                assert!(icon.stroke_width(20.0) > 1.0, "{}", kind.name());
            }
        }
    }

    #[test]
    fn stroke_width_follows_svg_semantics() {
        // SVG 语义：viewBox 单位 × (render/24) = 视觉像素。
        assert!((stroke_width_for(20.0, VISUAL_STROKE_PX) - 1.8).abs() < 1.0e-6);
        assert!((stroke_width_for(16.0, VISUAL_STROKE_PX) - 2.25).abs() < 1.0e-6);
        assert!((stroke_width_for(24.0, VISUAL_STROKE_PX) - 1.5).abs() < 1.0e-6);
        assert!((stroke_width_for(48.0, VISUAL_STROKE_PX) - 0.75).abs() < 1.0e-6);
        // 退化输入不炸、不产生无穷大（Slint 口径的 1.5 绝对值这里不能照抄）。
        assert_eq!(stroke_width_for(0.0, VISUAL_STROKE_PX), VISUAL_STROKE_PX);
        assert_eq!(stroke_width_for(-4.0, VISUAL_STROKE_PX), VISUAL_STROKE_PX);
        // 往返一致。
        for size in IconSize::ALL {
            let w = stroke_width_for(size.px(), VISUAL_STROKE_PX);
            assert!((visual_stroke_px(size.px(), w) - VISUAL_STROKE_PX).abs() < 1.0e-6);
        }
    }

    #[test]
    fn icon_sizes_are_ordered() {
        let px: Vec<f32> = IconSize::ALL.iter().map(|s| s.px()).collect();
        for pair in px.windows(2) {
            assert!(pair[0] < pair[1]);
        }
    }

    #[test]
    fn curve_flags_are_reported() {
        assert!(IconKind::Settings.icon().has_curves());
        assert!(!IconKind::Plus.icon().has_curves());
        assert!(!IconKind::ChevronDown.icon().has_curves());
    }

    /// 每一档的像素值都被**钉在官方出处**上：改值必须同时改这条测试，改的人会看到出处。
    ///
    /// 这不是「断言一个常量等于它自己」——它断言的是「我们读到的官方值」，
    /// 所以下一个人按 `research/12b` 或按某个 token 类把 18 改回 16/20 时，这条会红并打印出处。
    #[test]
    fn icon_sizes_are_pinned_to_their_official_sources() {
        // A 级：ButtonDefaults.<clinit> 的 `bipush 18` → IconSize = 18.dp
        //       （`AssistChipTokens.IconSize = 18.0d` 同值）。
        assert_eq!(
            IconSize::Button.px(),
            18.0,
            "按钮/Chip 图标 18：AAR ButtonDefaults.IconSize（12b 1.6 同值）。\
             ButtonSmallTokens.IconSize=20 是零消费者的死令牌，不要按它改回去"
        );
        // A 级：SwitchTokens.SelectedIconSize / UnselectedIconSize = 16.0d
        assert_eq!(
            IconSize::SwitchThumb.px(),
            16.0,
            "Switch 滑块图标 16：AAR SwitchTokens.Selected/UnselectedIconSize"
        );
        // A 级：SmallIconButtonTokens.IconSize / AppBarTokens.IconSize = 24.0d
        assert_eq!(
            IconSize::Toolbar.px(),
            24.0,
            "IconButton/标题栏/卡片头 24：AAR SmallIconButtonTokens / AppBarTokens"
        );
        // 目录档（当前无消费）：不许被当成 nav 图标用（nav 是 24）。
        assert_eq!(IconSize::Nav.px(), 20.0, "目录档 20（无消费）");
        assert_eq!(IconSize::Inline.px(), 12.0);
        assert_eq!(IconSize::Empty.px(), 32.0);
    }

    /// 标题栏 caption 的 20 是**项目偏离**，不是规范值；这条把「偏离的是哪两个数」钉住。
    #[test]
    fn titlebar_caption_icon_is_a_registered_deviation() {
        assert_eq!(
            TITLEBAR_CAPTION_ICON_PX, 20.0,
            "用户点名的偏离：官方 SmallIconButtonTokens.IconSize = 24，caption 三键缩到 20"
        );
        // 绑定成局部变量再过断言：直接比两个常量会被 clippy 判成「常量断言」（assertions_on_constants）。
        let caption = TITLEBAR_CAPTION_ICON_PX;
        let official = IconSize::Toolbar.px();
        assert!(
            caption < official,
            "偏离方向是「更小」：{caption} 应 < 官方 {official}；若哪天反了，那就不是用户要的那件事了"
        );
        // 20px 渲染时视觉线宽仍是 1.5（SVG 语义），这条防「缩图标顺手把线宽也带细」。
        let stroke = stroke_width_for(caption, VISUAL_STROKE_PX);
        assert!(
            (stroke - 1.8).abs() < 1.0e-6,
            "20px 渲染应取 1.8 个 viewBox 单位"
        );
        assert!((visual_stroke_px(caption, stroke) - VISUAL_STROKE_PX).abs() < 1.0e-6);
    }
}
