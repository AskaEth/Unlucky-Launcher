//! CAM16 正向换算：sRGB 色值 → CAM16 的 **hue**（色相角）与 **chroma**（彩度）。
//!
//! HCT = **H**ue / **C**hroma from CAM16 + **T**one from `L*`。这里只实现 M3 用到的方向
//! （色值 → hue/chroma）；反向（hue/chroma/tone → 色值）在 [`super::hct_solver`] 里按官方
//! 求解器内联实现——两边如果各写一份，数值会对不上。
//!
//! 照搬 `java/hct/Cam16.java`（Apache-2.0，见 `NOTICE`），逐行对应，包括 `Math.signum` 的
//! 「零返回 0」这种 Java 语义差别（见 [`space::signum`]）。

use super::space::{self, signum};
use super::viewing_conditions::{default_conditions, ViewingConditions};
use super::Color;

/// XYZ → CAM16 的「锥体响应」矩阵。
pub(crate) const XYZ_TO_CAM16_RGB: [[f64; 3]; 3] = [
    [0.401_288, 0.650_173, -0.051_461],
    [-0.250_268, 1.204_414, 0.045_854],
    [-0.002_079, 0.048_952, 0.953_127],
];

/// CAM16 里 M3 真正用到的两个坐标（`J` 明度不参与 HCT，HCT 的 tone 走 `L*`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Cam16 {
    /// 色相角（度，`0..360`）。
    pub(crate) hue: f64,
    /// 彩度（无上限；sRGB 立方体内最大值约 130）。
    pub(crate) chroma: f64,
}

impl Cam16 {
    /// 从色值取 CAM16 色相/彩度（默认 sRGB 观察条件）。
    pub(crate) fn from_color(color: Color) -> Self {
        Self::from_xyz(space::xyz_from_color(color), default_conditions())
    }

    /// 从 XYZ 取 CAM16 色相/彩度。
    pub(crate) fn from_xyz(xyz: [f64; 3], vc: &ViewingConditions) -> Self {
        let [r_t, g_t, b_t] = space::matrix_multiply(xyz, XYZ_TO_CAM16_RGB);

        // 折扣光源（色适应）
        let r_d = vc.rgb_d[0] * r_t;
        let g_d = vc.rgb_d[1] * g_t;
        let b_d = vc.rgb_d[2] * b_t;

        // 非线性压缩（视觉响应）
        let r_af = (vc.fl * r_d.abs() / 100.0).powf(0.42);
        let g_af = (vc.fl * g_d.abs() / 100.0).powf(0.42);
        let b_af = (vc.fl * b_d.abs() / 100.0).powf(0.42);
        let r_a = signum(r_d) * 400.0 * r_af / (r_af + 27.13);
        let g_a = signum(g_d) * 400.0 * g_af / (g_af + 27.13);
        let b_a = signum(b_d) * 400.0 * b_af / (b_af + 27.13);

        // 红绿 / 黄蓝对立通道
        let a = (11.0 * r_a - 12.0 * g_a + b_a) / 11.0;
        let b = (r_a + g_a - 2.0 * b_a) / 9.0;

        // 辅助量
        let u = (20.0 * r_a + 20.0 * g_a + 21.0 * b_a) / 20.0;
        let p2 = (40.0 * r_a + 20.0 * g_a + b_a) / 20.0;

        let hue = space::sanitize_degrees(b.atan2(a).to_degrees());
        let ac = p2 * vc.nbb;
        let j = 100.0 * (ac / vc.aw).powf(vc.c * vc.z);

        // 彩度：eHue 的 20.14° 断点是官方原样（把橙红段挪到 0 附近做插值）
        let hue_prime = if hue < 20.14 { hue + 360.0 } else { hue };
        let e_hue = 0.25 * ((hue_prime.to_radians() + 2.0).cos() + 3.8);
        let p1 = 50_000.0 / 13.0 * e_hue * vc.nc * vc.ncb;
        let t = p1 * a.hypot(b) / (u + 0.305);
        let alpha = (1.64 - 0.29_f64.powf(vc.n)).powf(0.73) * t.powf(0.9);
        let chroma = alpha * (j / 100.0).sqrt();

        Self { hue, chroma }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primaries_have_the_official_hue_angles() {
        // 官方 material-color-utilities 对纯红/绿/蓝解出的 CAM16 色相（Java 实测）。
        let cases = [
            (Color::rgb(255, 0, 0), 27.408_225, 113.357_887),
            (Color::rgb(0, 255, 0), 142.139_894, 108.410_061),
            (Color::rgb(0, 0, 255), 282.788_18, 87.230_694),
        ];
        for (color, hue, chroma) in cases {
            let cam = Cam16::from_color(color);
            assert!(
                (cam.hue - hue).abs() < 1.0e-4,
                "{} 的 hue 是 {}，官方 {hue}",
                color.to_hex(),
                cam.hue
            );
            assert!(
                (cam.chroma - chroma).abs() < 1.0e-4,
                "{} 的 chroma 是 {}，官方 {chroma}",
                color.to_hex(),
                cam.chroma
            );
        }
    }

    #[test]
    fn gray_has_near_zero_chroma() {
        for value in [16_u8, 64, 128, 200] {
            let cam = Cam16::from_color(Color::rgb(value, value, value));
            assert!(cam.chroma < 2.5, "灰 {value} 的彩度 {}", cam.chroma);
        }
    }

    #[test]
    fn black_is_finite() {
        let cam = Cam16::from_color(Color::BLACK);
        assert!(cam.hue.is_finite() && cam.chroma.is_finite());
        assert_eq!(cam.chroma, 0.0);
    }
}
