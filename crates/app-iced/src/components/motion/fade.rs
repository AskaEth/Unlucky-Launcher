//! **颜色预合成**（M305）：把 α 在令牌层算掉，渲染器只收实色。
//!
//! ## 这条纪律是什么（D27）
//!
//! 「CPU 档零半透明」：整窗重绘里每多一层半透明，软件渲染就多一遍混合。仓内 α 的正统出口是
//! **令牌层预合成**（`c.with_alpha(α).over(底)`，见 `interaction::disabled` 与
//! `dialog/view.rs::backdrop_color_at`）：产物**恒不透明**。
//! `interaction/mod.rs` 的用例把这条钉成硬线（「为了淡出改用 `Color { a: 0.5 }`」直接红）。
//!
//! 本模块的工具是 [`mix`]——两个**不透明**色之间按进度插值，用于「进度条到达 100% 时那一下
//! 强调收束」（`feedback/progress.rs::linear_with`）。它不改几何、不改厚度，所以**不引起重排**。
//!
//! ## 把 α 交给渲染器的地方（口径写在这里，别处不许再有第二种说法）
//!
//! | 场景 | 做法 | 产物透明度 | 是不是动画 |
//! |---|---|---|---|
//! | 颜色插值（进度条收束、遮罩浓度、禁用层） | [`mix`] / `with_alpha(α).over(底)` | **恒 1.0** | 是 |
//! | **一整棵子树淡出**（列表行） | `motion/reveal.rs` 的收尾遮罩：与**画布同色**的 quad，α = 1 − 可见度，且**必须自成一层** | 动画期间 < 1，**静息帧严格不画** | 是（唯一一处**动画中间帧**的 α） |
//! | **滚动视口底部的淡出带**（M227） | `pages/download/options/fade.rs::viewport_fade`：12 条 1px 的**画布色**块，α 从 `(1/12)^1.6 ≈ 0.019` 递到 1.0，经 `frame::filled` 进 `container::Style` | 常驻 α∈(0,1] | **否**（静态渐变） |
//! | 面板层（Card / Overlay） | `theme.rs::layer` × `panel_alpha`（`MaterialLayer::allows_translucency`） | CPU 档被强制 **1.0**；GPU 档开背景图时可 < 1 | 否（静态） |
//!
//! 两处**静态** α 为什么不归 D27 管（也**不在**本文「`mix` 恒不透明」那条口径内）：
//! ①`viewport_fade` 是**常驻**的发现性提示——iced 0.14 没有渐变原语，12 条 1px 实色块是它的替代品，
//! 而且它的颜色恒等于它盖住的画布色（`ui.layer(Canvas)`），浅色/深色都不新增颜色；
//! ②`panel_alpha` 是「GPU 档开背景图」那条线自己的决定（`elevation.rs` 的 `allows_translucency`），
//! CPU 档由 `app` 侧强制 1.0，与动效无关。这一支属**另一条线**，本文不覆盖。
//!
//! 为什么子树淡出做不到「预合成」：预合成要求先知道被盖住的像素（`content.with_alpha(α).over(底)`），
//! 而「一行的内容」在 `Element` 里是黑盒；唯一可行的替代是把整份 `Scheme` 预合成后换一个 `Ui`
//! 传下去，但那要求淡化的 `Ui` 活得比 `Element<'a>` 久——视图函数里现造的 `Ui` 撑不到返回，
//! 泄漏成 `'static` 则是每帧每行泄漏一份 49 色方案（长会话无界增长）。
//!
//! ⚠️ **守卫盲区（r1 复审 P2-4 要求写清）**：`interaction/mod.rs` 那条「中间帧也必须不透明」的
//! 用例只扫**状态层取色**，**扫不到** `motion/reveal.rs::draw` 递出去的遮罩（那一处是渲染器的
//! `fill_quad`，不经过任何 `Style`）。所以「遮罩的 α 只在动画期间出现、静息帧一个 quad 都不画」
//! 这条口径改由 `pages/download/rows/motion_tests.rs` 的结构断言 + 出图基线盯着
//! （静息帧与改造前逐像素相同，见 `DevDocs/artifacts/m305/`）。

use unlucky_design::color::Color;

/// 两个**不透明**颜色之间按 `t` 的插值。
///
/// 用在「进度条收束」这类没有背景色可叠的地方：`to.with_alpha(t).over(from)` 得到的是
/// 「把 `to` 按 t 盖在 `from` 上」的**实色**，产物恒不透明（D27）；端点 `t = 0` / `1`
/// 逐位等于入参（不是「差不多相等」）。返回 iced 色值（组件层的取色口径）。
pub fn mix(from: Color, to: Color, t: f32) -> iced::Color {
    if t <= 0.0 {
        return crate::theme::to_iced(from);
    }
    if t >= 1.0 {
        return crate::theme::to_iced(to);
    }
    crate::theme::to_iced(to.with_alpha(t).over(from))
}
