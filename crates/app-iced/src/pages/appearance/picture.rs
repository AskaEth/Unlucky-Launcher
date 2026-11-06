//! 背景层的**像素**：真图直出（`Cover` / `Contain` / `Tile`）+ 暗化叠层。
//!
//! 与 [`super::section`] 拆开只为单文件 ≤400 行的纪律；对外入口仍是
//! `pages::appearance::layer`（本文件 [`layer`] 的再导出）。
//!
//! ## 三种填充
//!
//! `Cover`/`Contain` 交给渲染器的 `ContentFit`（等比缩放 + 裁切/留白由它做）；`Tile` 用
//! [`background::tiles`] 算行列后铺 N 个图块（单块尺寸 = 原尺寸 × `tiles.scale`）。
//!
//! 暗化不是逐像素改色，而是**盖一层「版面底色 @ α=dim」**：图不透明时它与「向底色插值」
//! 数学等价（[`background::dim_alpha`] 有推导），而普通 alpha 混合 CPU/GPU 两档都支持。

use iced::widget::image;
use iced::widget::{column, container, image as image_widget, space};
use iced::{ContentFit, Element, Length};

use unlucky_design::color::Role;
use unlucky_presenter::prefs::BgFill;

use crate::pages::{frame, Message};
use crate::theme::Ui;

use super::background;
use super::state::State;

/// 背景层（铺满整窗、垫在最下层）。`None` = 没有可用的背景图（调用方就不叠这一层）。
///
/// 真图直出（M93 r2）：`Cover` / `Contain` 交给 iced 的 `ContentFit`，`Tile` 用
/// [`background::tiles`] 算网格铺 N 个图块。暗化 = 一层「版面底色 @ α=dim」盖在图上
/// （见 [`background::dim_alpha`] 的等价性说明）。
#[must_use]
pub fn layer(ui: &Ui, state: &State, viewport: (f32, f32)) -> Option<Element<'static, Message>> {
    let picture = state.image()?;
    if !state.bg_enabled {
        return None;
    }

    let handle = image_handle(state.bg_path.trim());
    let art: Element<'static, Message> = match state.bg_fill {
        BgFill::Cover => fitted(handle, ContentFit::Cover),
        BgFill::Contain => fitted(handle, ContentFit::Contain),
        BgFill::Tile => tiled(handle, picture.source, viewport),
    };

    // 暗化：底色 @ α=dim 盖在图上（等价于逐像素向底色插值，见 `background::dim_alpha`）。
    let scrim_color = ui
        .color(Role::Surface)
        .scale_alpha(background::dim_alpha(state.bg_dim));
    let scrim: Element<'static, Message> = container(space::horizontal())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme| frame::filled(scrim_color))
        .into();

    Some(
        iced::widget::stack![art, scrim]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

/// 文件字节 → iced 的图片句柄（渲染器自己解复用，不再读盘）。
fn image_handle(path: &str) -> image::Handle {
    image::Handle::from_path(path)
}

/// `Cover` / `Contain`：交给渲染器的等比缩放档。
fn fitted(handle: image::Handle, fit: ContentFit) -> Element<'static, Message> {
    image_widget(handle)
        .content_fit(fit)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// `Tile`：按 [`background::tiles`] 的网格平铺。
///
/// 三个细节都是为了「不留缝」（复审 r1 P2-1）：
/// 1. 单块边长 = `源尺寸 × tiles.scale`——超预算时图块**等比例放大**（不是少铺），
///    所以 `cols × 边长 >= 视口宽`、`rows × 边长 >= 视口高` 由 [`background::tiles`] 保证；
/// 2. `ContentFit::Fill`：`None` 会按**图片原生尺寸**画，比格子小就留空——那道缝正是它造成的；
///    这里格子尺寸恰好等于「等比放大后的尺寸」，`Fill` 就是等比缩放本身（不变形）；
/// 3. 行高显式写死（`Fixed`），列整体 `Fill`：不靠子控件撑，布局结果与算式一致。
fn tiled(
    handle: image::Handle,
    source: (u32, u32),
    viewport: (f32, f32),
) -> Element<'static, Message> {
    let tiles = background::tiles(source, viewport);
    let (width, height) = tiles.tile_size(source);

    let rows = (0..tiles.rows)
        .map(|_| {
            let cells = (0..tiles.cols)
                .map(|_| -> Element<'static, Message> {
                    image_widget(handle.clone())
                        .content_fit(ContentFit::Fill)
                        .width(Length::Fixed(width))
                        .height(Length::Fixed(height))
                        .into()
                })
                .collect::<Vec<_>>();
            iced::widget::row(cells)
                .width(Length::Fill)
                .height(Length::Fixed(height))
                .into()
        })
        .collect::<Vec<_>>();

    column(rows).width(Length::Fill).height(Length::Fill).into()
}
