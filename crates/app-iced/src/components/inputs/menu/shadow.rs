//! 菜单「要不要画 L2 阴影」（M238）。
//!
//! 从 `menu.rs` 拆出来只为守 400 行硬线（`components::gate::every_source_file_stays_under_the_line_limit`）：
//! 那边加完「内联列表口径」正好 400 行、零余量。搬移的是**新增**的两小块，没有改动既有逻辑。

use iced::{Color, Shadow, Vector};

use unlucky_design::color::Role;
use unlucky_design::elevation::ShadowSpec;

use crate::theme::Ui;

use super::Menu;

impl<'a, M> Menu<'a, M> {
    /// **内联列表口径**：不画 L2 阴影（容器色 / 圆角 / 项几何照旧）。
    ///
    /// 用在「菜单被放进**会被裁切的容器**（滚动视口）」的场合——下载页的加载器版本列表就是
    /// 这一种：它是面板里的一串版本，不是浮在页面上的弹出层。
    ///
    /// 为什么必须在这里让步而不是「让它裁掉」：`iced_tiny_skia` 0.14 画 quad 阴影时用
    /// `pixmap.draw_pixmap(.., mask: None)`（`iced_tiny_skia/src/engine.rs:136-143`），**绕过**
    /// 了 `clip_mask` ⇒ 阴影照画到视口之外，压暗下面钉死区的实例名（M238 实测：浅色画面布
    /// `(249,249,250)` 被压成 `(231,231,232)`，−7.2%）。`iced_wgpu` 那条路走整层的 scissor
    /// （`iced_wgpu/src/quad.rs:96-101`）⇒ 同一棵树在 GPU / CPU 两个后端上**画出来的不一样**。
    /// 不画阴影让两边一致，也把「浮层」语义收回到真正浮起来的地方。
    ///
    /// 层级观感：`Component::Menu.spec().elevation` 仍是 **L2**（令牌表不动），真实浮层
    /// （gallery / 将来的下拉）照旧带阴影；只有「内联列表」这一种用法放弃阴影，靠
    /// `SurfaceContainer` 底色 + 4 圆角与画布区分（L1 卡片 → L2 菜单这条阶梯不被削平）。
    pub fn flat(mut self) -> Self {
        self.shadow = false;
        self
    }
}

/// 面板阴影：`elevated` 为假（[`Menu::flat`]）时给 `Shadow::default()`——**不是**把 α 调小，
/// 而是让 tiny-skia 的 `shadow.color.a > 0.0` 一判就直接跳过整块阴影绘制。
///
/// 单独抽出来是为了让「内联列表不画阴影」这条可断言：`Element` 没有几何/样式内省，
/// 单测只能钉这个纯函数（见 `menu/tests.rs`）。
pub(super) fn panel_shadow(ui: &Ui, spec: ShadowSpec, elevated: bool) -> Shadow {
    if !elevated {
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
