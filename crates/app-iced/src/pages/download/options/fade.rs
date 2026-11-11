//! 选项区滚动视口底部的**淡出遮罩**（M227 返工）。
//!
//! 单独一个文件只为守 400 行硬线（`components::gate::every_source_file_stays_under_the_line_limit`）。

use iced::widget::{column, container, space};
use iced::{Color, Element, Length};

use unlucky_design::elevation::MaterialLayer;

use crate::theme::Ui;

use super::super::frame;
use super::super::Message;

/// 窄档滚动视口底部的淡出高度（像素）。
///
/// 取值落在 8~12 这一段：视口在 660 处硬裁，版本菜单的容器上沿（639）露出来 21px 且**一个字形
/// 都没有**——D30 返工把它判成「空壳蓝条」（深色下容器 `(22,33,41)` vs 画布 `(9,21,28)`，
/// CR 1.99，像渲染错而不是「下面还有内容」）。12px 遮罩让最后 12px 单调渐变到画布色，
/// 留下 8px 圆角上沿做发现性提示。
pub(super) const VIEWPORT_FADE_PX: f32 = 12.0;

/// 滚动视口底部的**淡出遮罩**：把内容在最后 `height` 像素内渐变到**画布色**。
///
/// 为什么需要它：`scrollable` 会硬裁内容（渲染层的 `with_layer`），裁切位置落在什么上面是不确定的
/// ——窄档正落在版本菜单容器上沿，用户看到的是一条没有字形的色块，而不是「还能往下滚」。
///
/// 为什么是**画布色的 α 阶梯**而不是白/黑雾：遮罩的颜色必须等于它下方那块背景的颜色，否则
/// 浅色档会浮出一层灰幕、深色档会浮出白雾。`ui.layer(MaterialLayer::Canvas)` 就是页面底色
/// （含面板 α），于是浅色淡出到纸面、深色淡出到墨底，两档都不新增颜色；方向也对
/// （内容向背景淡出，不是叠一层高光）。
///
/// 为什么用 12 条 1px 实色块拼：iced 0.14 没有渐变原语。1px 步进的 α 斜坡在 100% DPI 下与
/// 连续渐变的差别在 1/255 量级——D30 的逐像素量测只会看到 α 单调、无硬边。
///
/// 形状上它必须**贴在视口内**（调用侧把它放在滚动区底部的 `stack!` 上）：视口下沿是 660，
/// 钉死区的实例名从 668 起，所以遮罩压不到任何控件（M227 返工的零回退证据就是逐像素
/// 差异只有 648–660 这 12 行）。
pub(super) fn viewport_fade<'a>(ui: &'a Ui, height: f32) -> Element<'a, Message> {
    let canvas = ui.layer(MaterialLayer::Canvas);
    let band_height = height / VIEWPORT_FADE_STEPS as f32;

    let mut bands = column![].width(Length::Fill);
    for index in 0..VIEWPORT_FADE_STEPS {
        let alpha = canvas.a * step_alpha(index);
        bands = bands.push(
            container(space::horizontal())
                .width(Length::Fill)
                .height(Length::Fixed(band_height))
                .style(move |_theme| frame::filled(Color { a: alpha, ..canvas })),
        );
    }
    bands.into()
}

/// 遮罩的条数：每条高 `VIEWPORT_FADE_PX / VIEWPORT_FADE_STEPS` 像素（现在是 1px）。
///
/// 单测直接对**这一对生产常量** + [`step_alpha`] 断言，所以改步数或改曲线都会当场变红
/// ——M227 r2 复审实测过「单测空转」的形态：旧测试在函数体里又抄了一遍 `12`，改生产常量它照样绿。
pub(super) const VIEWPORT_FADE_STEPS: usize = 12;

/// 第 `index` 条（0 = 最上面、`VIEWPORT_FADE_STEPS - 1` = 视口底边）的**不透明度系数**。
///
/// 顶端 ≈0（几乎透出内容）→ 底端 = 1（完全是画布色，与视口外的背景接上）；`t^1.6` 让前几条更淡，
/// 观感是「内容渐隐到背景」，不是一层均匀的纱。
pub(super) fn step_alpha(index: usize) -> f32 {
    // t：0 < t <= 1（`index + 1` 让顶端不落在 0 上，否则第一条完全看不见）。
    let t = (index + 1) as f32 / VIEWPORT_FADE_STEPS as f32;
    t.powf(1.6)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 遮罩的几何自检：**对生产常量断言**，不是对测试里抄的副本。
    ///
    /// 抓得住（r2 复审指出的空转形态：旧测试在函数体里又抄了一遍 `12`，改生产常量照样绿）：
    /// `VIEWPORT_FADE_STEPS` 改成 6（每档 2px ⇒ 见得到阶梯）、改成不能整除高度的数（半像素带）、
    /// `step_alpha` 写成常数（等于一层纯色幕）、把曲线拉直（`^1.0`，前几条不再更淡）、
    /// 或让末项小于 1（视口底边与画布之间留一道缝）。
    #[test]
    fn the_fade_bands_are_one_pixel_and_the_ramp_is_convex_to_opaque() {
        // ① 每条**恰好 1px**：这是设计口径（「12 条 1px 实色块」），也是「肉眼分不出阶梯」的前提。
        //    换成 6 条就是 2px/条、换成 5 条还会出半像素带 —— 两种都当场红。
        let band = VIEWPORT_FADE_PX / VIEWPORT_FADE_STEPS as f32;
        assert_eq!(
            VIEWPORT_FADE_STEPS as f32, VIEWPORT_FADE_PX,
            "步数必须等于高度像素数：{VIEWPORT_FADE_PX}px 要 1px 一条，现在 {VIEWPORT_FADE_STEPS} 条 ⇒ {band}px/条"
        );
        assert_eq!(band, 1.0, "每条恰好 1px");

        // ② α 严格递增、除末条外都不透明（末条必须是画布色本身，与视口外无缝接上）。
        let mut previous = 0.0_f32;
        let mut previous_delta = 0.0_f32;
        for index in 0..VIEWPORT_FADE_STEPS - 1 {
            let alpha = step_alpha(index);
            let delta = alpha - previous;
            assert!(
                delta > 0.0,
                "第 {index} 条的 α ({alpha}) 必须大于前一条 ({previous})：遮罩要单调变浓"
            );
            assert!(
                alpha < 1.0,
                "第 {index} 条就已不透明，后面 {VIEWPORT_FADE_STEPS} 条白给"
            );
            assert!(
                delta >= previous_delta,
                "第 {index} 条比前一条浓得更少（{delta} < {previous_delta}）：曲线要凸（t^1.6），否则顶端不再「几乎透出内容」"
            );
            previous = alpha;
            previous_delta = delta;
        }
        assert!(
            step_alpha(0) < 0.1,
            "最上面一条 α = {} 不够淡：遮罩顶端要几乎透出内容",
            step_alpha(0)
        );
        assert_eq!(
            step_alpha(VIEWPORT_FADE_STEPS - 1),
            1.0,
            "最末一条必须是画布色本身（否则与视口外的背景之间有一道缝）"
        );
    }
}
