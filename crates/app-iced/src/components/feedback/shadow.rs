//! 浮层阴影：把 [`ShadowSpec`] 翻成 iced 的 [`Shadow`]（M430）。
//!
//! ## 为什么不在组件里直接写三个数
//!
//! blur / offset_y / α 的唯一真源是 `Elevation::shadow()`（M428 照 `11-m3-glass-visual-spec.md`
//! §5 填成真值）。组件层只做一件事：取**自己那一档**，再翻成 iced 类型。否则同一份阴影会在
//! Dialog 与 Snackbar 里各抄一遍，将来改档位必然只改一半。
//!
//! ## 与 `inputs::menu::shadow::panel_shadow` 的关系：同构，不是重复定义
//!
//! 那边是 `pub(super)`，同 crate 不同模块，取不到；而且它的 `elevated: bool` 参数服务的是菜单
//! 独有的 `Menu::flat`（内联列表不画阴影，因为 tiny-skia 的阴影会画出裁切视口之外，M238）。
//! feedback 的两个浮层没有「内联用法」，所以这里只保留「翻类型」这一半，**不**复制那个开关。
//!
//! ⚠️ `Menu::flat` 只能写成代码片段，**不要**做成 intra-doc 链接：`menu` 在同一条路径上既是
//! 模块（`inputs::menu`）又是函数（`inputs::menu::menu`），链接到 `Menu::flat` 会被解析成
//! 「`menu` is a function, not a module … cannot have associated items」，中间段加 `mod@` 也不
//! 解歧。M430 r1 P1-1 的实测：写成链接时
//! `RUSTDOCFLAGS=-D rustdoc::broken_intra_doc_links cargo doc -p unlucky-app-iced --no-deps
//! --document-private-items` 直接 EXIT=101（`doc-links.yml` 的原命令）。
//!
//! ## 为什么 L0 走 [`ShadowSpec::is_visible`] 而不是比较数值
//!
//! 不画时返回 [`Shadow::default()`]：`color.a == 0` 让两个渲染后端（tiny-skia / wgpu）都直接
//! 跳过整块阴影绘制——这与 `panel_shadow` 的 `flat` 落点是同一个判据，不是「画一层透明的」。

use iced::{Color, Shadow, Vector};

use unlucky_design::color::Role;
use unlucky_design::elevation::ShadowSpec;

use crate::theme::Ui;

/// [`ShadowSpec`] → iced 阴影（**单层**：`11b` R4/R5 要求每个元素只有一层）。
///
/// 色取 [`Role::Shadow`] 的 RGB，α 换成令牌给的那一档（`ShadowSpec.alpha`）——
/// `Role::Shadow` 自带的 `SHADOW_ALPHA` 是「没有阴影表时」的通用值，这里以规格表为准。
pub(super) fn overlay_shadow(ui: &Ui, spec: ShadowSpec) -> Shadow {
    if !spec.is_visible() {
        return Shadow::default();
    }
    Shadow {
        color: Color {
            a: spec.alpha,
            ..ui.color(Role::Shadow)
        },
        offset: Vector::new(0.0, spec.offset_y_px),
        blur_radius: spec.blur_px,
    }
}
