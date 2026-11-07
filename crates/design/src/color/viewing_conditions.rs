//! CAM16 的**观察条件**（viewing conditions）。
//!
//! HCT 的色相/彩度来自 CAM16，而 CAM16 是「在外界光环境下看起来如何」的模型——同一个 hex，
//! 换观察条件就是不同的 hue/chroma。M3 固定用 sRGB 默认条件：D65 白点、200 lux（适应亮度）、
//! 背景 `L* = 50`、surround = 2.0、**不做**光源折扣（自发光屏幕）。
//!
//! 照搬 `java/hct/ViewingConditions.java`（Apache-2.0，见 `NOTICE`）。

use super::space;

/// CAM16 观察条件里与颜色无关的那堆中间量（只为避免重复计算）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ViewingConditions {
    /// 白点响应（achromatic response to the white point）。
    pub(crate) aw: f64,
    /// 背景亮度因子。
    pub(crate) n: f64,
    /// 亮度指数。
    pub(crate) nbb: f64,
    /// 彩度指数。
    pub(crate) ncb: f64,
    /// 环绕度决定的对比度系数。
    pub(crate) c: f64,
    /// 环绕度。
    pub(crate) nc: f64,
    /// 三通道的色适应折扣系数。
    pub(crate) rgb_d: [f64; 3],
    /// 亮度适应因子。
    pub(crate) fl: f64,
    /// 基函数。
    pub(crate) z: f64,
}

// 与 XYZ 有关的常量矩阵在 CAM16 里也要用，从 space 引一次保持单一来源。
use super::cam16::XYZ_TO_CAM16_RGB;

impl ViewingConditions {
    /// 按官方参数构造（`white_point` 是 XYZ，`adapting_luminance` 由 lux 换算）。
    pub(crate) fn make(
        white_point: [f64; 3],
        adapting_luminance: f64,
        background_lstar: f64,
        surround: f64,
        discounting_illuminant: bool,
    ) -> Self {
        // 纯黑背景是非物理的，会让「任何颜色都看不见」这类无穷大出现，官方取下限 0.1。
        let background_lstar = background_lstar.max(0.1);
        let [r_w, g_w, b_w] = space::matrix_multiply(white_point, XYZ_TO_CAM16_RGB);
        let f = 0.8 + surround / 10.0;
        let c = if f >= 0.9 {
            space::lerp(0.59, 0.69, (f - 0.9) * 10.0)
        } else {
            space::lerp(0.525, 0.59, (f - 0.8) * 10.0)
        };
        let d = if discounting_illuminant {
            1.0
        } else {
            let raw = f * (1.0 - (1.0 / 3.6) * ((-adapting_luminance - 42.0) / 92.0).exp());
            space::clamp(0.0, 1.0, raw)
        };
        let nc = f;
        let rgb_d = [
            d * (100.0 / r_w) + 1.0 - d,
            d * (100.0 / g_w) + 1.0 - d,
            d * (100.0 / b_w) + 1.0 - d,
        ];
        let k = 1.0 / (5.0 * adapting_luminance + 1.0);
        let k4 = k * k * k * k;
        let k4_f = 1.0 - k4;
        let fl = k4 * adapting_luminance + 0.1 * k4_f * k4_f * (5.0 * adapting_luminance).cbrt();
        let n = space::y_from_lstar(background_lstar) / white_point[1];
        let z = 1.48 + n.sqrt();
        let nbb = 0.725 / n.powf(0.2);
        let ncb = nbb;
        let rgb_a_factors = [
            (fl * rgb_d[0] * r_w / 100.0).powf(0.42),
            (fl * rgb_d[1] * g_w / 100.0).powf(0.42),
            (fl * rgb_d[2] * b_w / 100.0).powf(0.42),
        ];
        let rgb_a = [
            400.0 * rgb_a_factors[0] / (rgb_a_factors[0] + 27.13),
            400.0 * rgb_a_factors[1] / (rgb_a_factors[1] + 27.13),
            400.0 * rgb_a_factors[2] / (rgb_a_factors[2] + 27.13),
        ];
        let aw = (2.0 * rgb_a[0] + rgb_a[1] + 0.05 * rgb_a[2]) * nbb;
        Self {
            aw,
            n,
            nbb,
            ncb,
            c,
            nc,
            rgb_d,
            fl,
            z,
        }
    }

    /// 指定背景 `L*` 的 sRGB 默认条件。
    pub(crate) fn default_with_background_lstar(background_lstar: f64) -> Self {
        Self::make(
            space::WHITE_POINT_D65,
            200.0 / std::f64::consts::PI * space::y_from_lstar(50.0) / 100.0,
            background_lstar,
            2.0,
            false,
        )
    }
}

/// 全仓共用的默认观察条件（sRGB 屏，背景 `L* = 50`）。
pub(crate) fn default_conditions() -> &'static ViewingConditions {
    static DEFAULT: std::sync::OnceLock<ViewingConditions> = std::sync::OnceLock::new();
    DEFAULT.get_or_init(|| ViewingConditions::default_with_background_lstar(50.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_conditions_match_the_official_constants() {
        // 官方 material-color-utilities 的 ViewingConditions.DEFAULT 解出来的值（Java 实测，
        // `hct.Count` 打印）。
        let vc = default_conditions();
        assert!(
            (vc.n - 0.184_186_518_512_444_16).abs() < 1.0e-12,
            "n = {}",
            vc.n
        );
        assert!(
            (vc.aw - 29.980_997_194_447_333).abs() < 1.0e-12,
            "aw = {}",
            vc.aw
        );
        assert!(
            (vc.nbb - 1.016_919_180_445_875_5).abs() < 1.0e-12,
            "nbb = {}",
            vc.nbb
        );
        assert!((vc.c - 0.69).abs() < 1.0e-12);
        assert!((vc.nc - 1.0).abs() < 1.0e-12);
        assert!(
            (vc.fl - 0.388_481_453_780_035_3).abs() < 1.0e-12,
            "fl = {}",
            vc.fl
        );
        assert!(
            (vc.z - 1.909_169_568_483_652).abs() < 1.0e-12,
            "z = {}",
            vc.z
        );
        assert!((vc.rgb_d[0] - 1.021_177_702_757_52).abs() < 1.0e-12);
        assert!((vc.rgb_d[1] - 0.986_307_729_428_012_4).abs() < 1.0e-12);
        assert!((vc.rgb_d[2] - 0.933_960_508_280_229_9).abs() < 1.0e-12);
    }

    #[test]
    fn white_point_has_zero_chroma_in_cam16() {
        // CAM16 的白点在默认条件下应当接近中性（这才是「色适应」的意义）。
        use super::super::cam16::Cam16;
        let white = Cam16::from_color(super::super::Color::WHITE);
        assert!(white.chroma < 5.0, "白点彩度 {}", white.chroma);
    }
}
