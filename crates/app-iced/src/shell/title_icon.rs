//! 标题栏左侧的品牌标记（M379）：把 `assets/branding/ull-64.rgba` 缩到令牌尺寸画在标题左边。
//!
//! ## 为什么是裸 RGBA，而不是 PNG（与 M376 的窗口图标同一条路）
//!
//! `ull-64.rgba` 是 64×64 的**裸字节**（行优先、每像素 R,G,B,A、无文件头，16,384 B）。
//! 走 `iced_core` 的 `image::Handle::from_rgba(..)` 就不必给 `iced` 打开图像解码路径：像素在
//! 编译期就进了二进制（`include_bytes!`），运行时只是把字节交给渲染器建纹理。
//! `ull-256.png` 是 39 KB，**不许**为了一个 20px 的标记塞进运行时（D40 的口径见
//! `assets/branding/FORMAT.md`：`image` 类解码依赖只住在 `palette-extract`）。
//!
//! ## 尺寸从令牌取，不从壳层拍
//!
//! 边长与「标记 ↔ 标题」间距来自 [`Component::TopAppBar`] 的 [`ComponentMetrics`]
//! （`crates/design/src/component_map/metrics.rs` 的 `TOP_APP_BAR`），壳层一个数字都不写。
//! 那条常量的文档解释了为什么是 20（项目偏离：官方 `AppBarTokens.IconSize = 24`）。
//!
//! ## 为什么不吞拖动事件
//!
//! `iced_widget` 的 `image` 控件**没有** `on_event` 实现（`iced_widget-0.14.2/src/image.rs`），
//! 它对鼠标事件一律返回 `Ignored`，所以把它放进标题栏已有的 `mouse_area(..).on_press(DragWindow)`
//! 内层，事件照旧冒到拖动层——点标记同样能拖窗。`shell/tests.rs` 用源码结构守着这条。
//!
//! ## 句柄只建一次
//!
//! `Handle::from_rgba(..)` 每次调用都造一个新的 `Id::unique()`（`iced_core::image::Id`），而渲染
//! 器的纹理缓存以 `Id` 为键：**每帧新建句柄 = 每帧判定「新图」并重传 16 KB**。所以这里用
//! `OnceLock` 把句柄钉成 `'static`，视图每帧只是 `clone()`（句柄内部是 `Arc`/`Bytes`，不拷像素）。

use std::sync::OnceLock;

use iced::widget::{image, Image};
use iced::{ContentFit, Element, Length};

use unlucky_design::component_map::{Component, ComponentMetrics};

use crate::app::Message;

/// 源图边长（像素）：`assets/branding/ull-64.rgba` 是 64×64。
const SOURCE_SIDE: u32 = 64;

/// 源图像素（裸 RGBA，与窗口图标同一个文件；窗口图标另有一份 128 档，见 `app/boot/diagnostics.rs`）。
const SOURCE_PIXELS: &[u8] = include_bytes!("../../../../assets/branding/ull-64.rgba");

/// 资产被换成别的尺寸时**编不过**，而不是等运行时拿到一张错位的图。
const _: () = assert!(SOURCE_PIXELS.len() == (SOURCE_SIDE * SOURCE_SIDE * 4) as usize);

/// 顶栏微度量（`icon` = 标记边长、`gap` = 标记 ↔ 标题间距、`pad_start` = 栏内左内边距）。
///
/// 走 `Component::TopAppBar.metrics()` 而不是在壳层写数字：D30/G10 的口径，控件内部几何只有
/// `ComponentMetrics` 一个落点。
pub(super) fn metrics() -> ComponentMetrics {
    Component::TopAppBar
        .metrics()
        .expect("M379：TopAppBar 必须带 ComponentMetrics（见 component_map/metrics.rs）")
}

/// 源图 → 纹理句柄（**全进程一份**，理由见模块文档「句柄只建一次」）。
fn handle() -> &'static image::Handle {
    static HANDLE: OnceLock<image::Handle> = OnceLock::new();
    HANDLE
        .get_or_init(|| image::Handle::from_rgba(SOURCE_SIDE, SOURCE_SIDE, SOURCE_PIXELS.to_vec()))
}

/// 标记本身：边长取令牌，等比缩到那一档（64 → 20，`ContentFit::Contain` 保证不裁）。
pub(super) fn view<'a>() -> Element<'a, Message> {
    let side = metrics().icon;

    let logo: Image = image(handle())
        .width(Length::Fixed(side))
        .height(Length::Fixed(side))
        .content_fit(ContentFit::Contain);

    logo.into()
}
