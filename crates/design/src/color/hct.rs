//! **HCT** 色彩空间：色相/彩度取自 CAM16，亮度取自 CIELAB 的 `L*`（即 M3 的 tone）。
//!
//! 为什么不是 OKLab/HSL：`L*` 与人眼感知线性的**对比度**直接挂钩（`Y` 是它的单调函数），
//! 所以「tone 差 40 ⇒ 对比度 ≥ 3:1」「tone 差 50 ⇒ ≥ 4.5:1」这类性质在 HCT 里是恒等式，
//! 换任何种子色都成立。CAM16 的色相/彩度则保证「同一条调色板只有一种色相」在感知上也是真的。
//!
//! 语义与官方 `java/hct/Hct.java` 一致：`Hct` 的真身是那个 **8 bit 色值**，`hue/chroma/tone`
//! 是它的测量值。所以 [`Hct::from_components`] 解出来的 `tone` 可能比请求值差一点点
//! （例如请求 tone 40、实测 40.08），官方也是这样——请求值只是求解目标。
//!
//! 来源：`material-color-utilities`（Apache-2.0），见仓库根 `NOTICE`。

use super::space;
use super::{cam16::Cam16, hct_solver, Color};

/// 一个色值的 HCT 坐标。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hct {
    /// CAM16 色相角（度，`0..360`）。
    pub hue: f64,
    /// CAM16 彩度（无上限；sRGB 立方体内约 0..=130）。
    pub chroma: f64,
    /// CIELAB `L*`（0 = 黑 / 100 = 白）——M3 里叫 **tone**。
    pub tone: f64,
    /// 这个坐标对应的 8 bit 色值（HCT 的真身）。
    color: Color,
}

impl Hct {
    /// 量一个色值的 HCT 坐标。
    pub fn from_color(color: Color) -> Self {
        let cam = Cam16::from_color(color);
        Self {
            hue: cam.hue,
            chroma: cam.chroma,
            tone: space::lstar_from_color(color),
            color,
        }
    }

    /// 解 HCT 方程：给定色相/彩度/tone，求最接近的 sRGB 色值。
    ///
    /// 彩度装不下时**只压彩度**（色相与 tone 保住），这就是 M3 的 gamut 映射；
    /// 返回值的 `tone` 是实测值，可能与 `tone` 参数相差千分之几。
    pub fn from_components(hue: f64, chroma: f64, tone: f64) -> Self {
        Self::from_color(hct_solver::solve_to_color(hue, chroma, tone))
    }

    /// 该坐标对应的色值。
    pub fn to_color(self) -> Color {
        self.color
    }

    /// 色相是否落在「黄」段（官方 `TonalPalette` 的 tone 99 特例用它）。
    pub fn is_yellow(hue: f64) -> bool {
        (105.0..125.0).contains(&hue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 官方 Java 实测的种子色 HCT 坐标（`hct.Count` 打印，APCA/CAM16 D65 默认条件）。
    const SEED_HCT: [(&str, f64, f64, f64); 6] = [
        ("#6750A4", 298.980_997, 47.856_526, 40.083_244),
        ("#FF0000", 27.408_225, 113.357_887, 53.232_882),
        ("#00FF00", 142.139_894, 108.410_061, 87.737_033),
        ("#0000FF", 282.788_18, 87.230_694, 32.302_587),
        ("#6E7C8A", 245.036_867, 14.675_86, 51.342_101),
        ("#D000FF", 324.929_26, 99.544_477, 52.542_037),
    ];

    #[test]
    fn seed_coordinates_match_official() {
        for (hex, hue, chroma, tone) in SEED_HCT {
            let hct = Hct::from_color(Color::from_hex(hex).expect("合法 hex"));
            assert!((hct.hue - hue).abs() < 1.0e-4, "{hex} hue {}", hct.hue);
            assert!(
                (hct.chroma - chroma).abs() < 1.0e-4,
                "{hex} chroma {}",
                hct.chroma
            );
            assert!((hct.tone - tone).abs() < 1.0e-4, "{hex} tone {}", hct.tone);
        }
    }

    #[test]
    fn solver_reproduces_official_hexes() {
        // 官方 Java 实测（`hct.Probe`）：同一组 (h, c, t) 必须解出同一个 hex。
        let cases = [
            (95.0, 36.0, 40.0, "#705D0D"),
            (298.98, 36.0, 90.0, "#E9DDFF"),
            (25.0, 84.0, 80.0, "#FFB4AB"),
            (245.0, 16.0, 80.0, "#B8C8D9"),
            (245.0, 42.0, 30.0, "#004B72"),
            (10.0, 120.0, 5.0, "#2C000A"),
            (245.0, 4.0, 30.0, "#45474A"),
        ];
        for (hue, chroma, tone, hex) in cases {
            assert_eq!(
                hct_solver::solve_to_color(hue, chroma, tone).to_hex(),
                hex,
                "HCT({hue}, {chroma}, {tone}) 与官方不一致"
            );
        }
    }

    #[test]
    fn solver_keeps_hue_and_tone_when_chroma_is_available() {
        for (hue, chroma, tone) in [
            (95.0, 36.0, 40.0),
            (298.98, 36.0, 90.0),
            (245.0, 42.0, 30.0),
            (25.0, 84.0, 80.0),
            (245.0, 16.0, 80.0),
        ] {
            let solved = Hct::from_components(hue, chroma, tone);
            assert_eq!(
                solved.to_color(),
                hct_solver::solve_to_color(hue, chroma, tone)
            );
            assert!(
                (solved.tone - tone).abs() < 1.0,
                "tone {} vs {tone}",
                solved.tone
            );
            let delta = (solved.hue - hue)
                .abs()
                .min(360.0 - (solved.hue - hue).abs());
            assert!(delta < 1.0, "色相 {} vs {hue}", solved.hue);
            assert!(
                solved.chroma <= chroma + 1.0,
                "彩度反被拉高：{}",
                solved.chroma
            );
        }
    }

    #[test]
    fn low_chroma_hue_is_ill_conditioned_even_officially() {
        // 彩度 8 在 tone 30 上，8 bit 量化会让 CAM16 色相跳 6°——官方同样如此
        // （Java 实测 HCT(245,8,30) → #42474E / hue 251.5965）。这条钉的是「别去修它」。
        let solved = Hct::from_components(245.0, 8.0, 30.0);
        assert_eq!(solved.to_color().to_hex(), "#42474E");
        assert!(
            (solved.hue - 251.596_498).abs() < 1.0e-4,
            "hue {}",
            solved.hue
        );
    }

    #[test]
    fn yellow_band_matches_official_definition() {
        assert!(Hct::is_yellow(105.0));
        assert!(Hct::is_yellow(124.9));
        assert!(!Hct::is_yellow(125.0));
        assert!(!Hct::is_yellow(104.9));
    }
}
