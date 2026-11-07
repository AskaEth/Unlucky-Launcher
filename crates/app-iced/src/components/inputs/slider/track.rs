//! Slider 的**纯几何**（无 iced 画布依赖）：比例 ⇄ 像素，`draw` 与单测共用同一条公式。
//!
//! 单独成文件的原因：`slider.rs` 已到 400 行红线，而这几条公式是「0% / 100% 竖条不越界」的
//! 唯一实现点——独立出来才好被人一眼审完。`draw` 与 `#[cfg(test)] mod tests` 都调这里。

use iced::Rectangle;

use super::super::geometry::HANDLE_WIDTH;

/// 比例比较的容差（浮点拖动会产生 1e-7 级抖动，别让它每帧发一条消息）。
pub(super) const EPSILON: f32 = 1e-4;

/// 量化到停靠点（连续值时原样返回）。
pub(super) fn quantize(fraction: f32, steps: Option<u32>) -> f32 {
    match steps {
        Some(steps) => {
            let steps = steps as f32;
            (fraction.clamp(0.0, 1.0) * steps).round() / steps
        }
        None => fraction.clamp(0.0, 1.0),
    }
}

/// 一条竖条的横坐标（**位移区间是 `handle/2 .. width - handle/2`**，所以 0%/100% 都不越界）。
fn handle_center(bounds: Rectangle, fraction: f32, handle: f32) -> f32 {
    let travel = (bounds.width - handle).max(0.0);
    bounds.x + handle / 2.0 + travel * fraction.clamp(0.0, 1.0)
}

/// 指针横坐标 → 比例（`handle_center` 的逆函数，同样不越界）。
pub(super) fn fraction_at(bounds: Rectangle, x: f32, handle: f32) -> f32 {
    let travel = (bounds.width - handle).max(1.0);
    ((x - bounds.x - handle / 2.0) / travel).clamp(0.0, 1.0)
}

/// 轨道 / 停靠点 / 竖条的纯几何（像素级断言用；`Widget::draw` 与测试共用同一条公式）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackGeometry {
    /// 激活段的左缘。
    pub active_left: f32,
    /// 激活段的宽。
    pub active_width: f32,
    /// 竖条左缘（`f = 0` 时应 = 轨道左缘）。
    pub handle_left: f32,
    /// 竖条右缘（`f = 1` 时应 = 轨道右缘）。
    pub handle_right: f32,
}

impl TrackGeometry {
    /// 按轨道矩形与比例求值（`f` 会被 clamp 到 0..=1）。
    pub fn of(track: Rectangle, fraction: f32) -> Self {
        let centre = handle_center(track, fraction, HANDLE_WIDTH);
        let handle_left = centre - HANDLE_WIDTH / 2.0;
        Self {
            active_left: track.x,
            active_width: centre - track.x,
            handle_left,
            handle_right: handle_left + HANDLE_WIDTH,
        }
    }

    /// 竖条中心（= 激活段与未激活段的分界）。
    pub fn handle_centre(&self) -> f32 {
        self.handle_left + HANDLE_WIDTH / 2.0
    }
}
