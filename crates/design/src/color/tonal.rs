//! tone 调色板：**固定 HCT 色相 + 固定 HCT 彩度**，沿 tone 轴取色。
//!
//! 这就是 M3 的 `TonalPalette`：一条调色板只回答一个问题——「我的第 N 号 tone 是什么色」。
//! 因为 HCT 的 tone 轴是 `L*`，同一条调色板上「tone 差 40 ⇒ 对比度 ≥ 3:1」天然成立；
//! 又因为 hue 固定，整条调色板在感知上是同一个色相（这正是 OKLCh 近似做不到的那一点）。
//!
//! 语义对齐 `java/palettes/TonalPalette.java`（Apache-2.0，见 `NOTICE`）。

use super::{Color, Hct};

/// 一条 tone 调色板（HCT 色相 + 彩度上限）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TonalPalette {
    hue_deg: f64,
    chroma: f64,
}

impl TonalPalette {
    /// 直接给 HCT 色相（度）与 HCT 彩度建调色板（彩度 0 = 中性灰）。
    pub fn new(hue_deg: f64, chroma: f64) -> Self {
        Self {
            hue_deg: hue_deg.rem_euclid(360.0),
            chroma: chroma.max(0.0),
        }
    }

    /// 从种子色取 HCT 色相与彩度（等价官方 `TonalPalette.fromInt`）。
    pub fn from_color(seed: Color) -> Self {
        let hct = Hct::from_color(seed);
        Self::new(hct.hue, hct.chroma)
    }

    /// 固定色相，换彩度（M3 用它把种子的彩度压到 secondary/neutral 档）。
    pub fn with_chroma(self, chroma: f64) -> Self {
        Self::new(self.hue_deg, chroma)
    }

    /// 旋转色相（derived hue，例如 tertiary = 种子色相 +60°）。
    pub fn rotated(self, deg: f64) -> Self {
        Self::new(self.hue_deg + deg, self.chroma)
    }

    /// 该调色板的 HCT 色相（度）。
    pub fn hue_deg(self) -> f64 {
        self.hue_deg
    }

    /// 该调色板的 HCT 彩度。
    pub fn chroma(self) -> f64 {
        self.chroma
    }

    /// 取某个 tone 的颜色（tone 超出 0..=100 时自然夹到纯黑/纯白）。
    ///
    /// 彩度装不下时按官方的 gamut 映射**只压彩度**，色相与 tone 不变。
    pub fn tone(self, tone: f64) -> Color {
        self.hct(tone).to_color()
    }

    /// 取某个 tone 的 HCT 坐标（`TonalPalette.hct` / 官方 `getHct`）。
    ///
    /// 黄相（105°..125°）的 tone 99 是官方的特例：那里 `L*` 与 sRGB 平面不平滑，
    /// 直接横向取自 tone 98 与 tone 100 的中点，避免出一格色带。
    pub fn hct(self, tone: f64) -> Hct {
        if tone == 99.0 && Hct::is_yellow(self.hue_deg) {
            let low = self.hct(98.0).to_color();
            let high = self.hct(100.0).to_color();
            return Hct::from_color(average(low, high));
        }
        Hct::from_components(self.hue_deg, self.chroma, tone)
    }
}

/// 两色的通道中点（官方 `averageArgb`，四舍五入）。
fn average(a: Color, b: Color) -> Color {
    let mid = |x: u8, y: u8| {
        let sum = u16::from(x) + u16::from(y);
        u8::try_from(sum.div_ceil(2)).unwrap_or(u8::MAX)
    };
    Color::rgb(mid(a.r, b.r), mid(a.g, b.g), mid(a.b, b.b))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: Color = Color::rgb(0x67, 0x50, 0xA4);

    #[test]
    fn tone_is_hit_within_quantisation_error() {
        let palette = TonalPalette::from_color(SEED).with_chroma(36.0);
        for tone in [
            4.0_f64, 6.0, 10.0, 12.0, 17.0, 20.0, 30.0, 40.0, 50.0, 80.0, 87.0, 90.0, 92.0, 94.0,
            95.0, 96.0, 98.0,
        ] {
            let got = Hct::from_color(palette.tone(tone)).tone;
            assert!(
                (got - tone).abs() <= 1.0,
                "tone {tone} 的 L* 是 {got}，偏差超过 1（8 bit 量化上限）"
            );
        }
    }

    #[test]
    fn extremes_are_pure_black_and_white() {
        let palette = TonalPalette::from_color(SEED);
        assert_eq!(palette.tone(0.0), Color::rgb(0, 0, 0));
        assert_eq!(palette.tone(100.0), Color::rgb(255, 255, 255));
        assert_eq!(palette.tone(-5.0), Color::rgb(0, 0, 0));
        assert_eq!(palette.tone(120.0), Color::rgb(255, 255, 255));
    }

    #[test]
    fn hue_is_preserved_across_the_whole_ramp() {
        let palette = TonalPalette::new(298.980_997, 36.0);
        for tone in [10.0_f64, 30.0, 50.0, 70.0, 90.0] {
            let hct = palette.hct(tone);
            let delta = (hct.hue - palette.hue_deg()).abs();
            let delta = delta.min(360.0 - delta);
            assert!(delta < 1.0, "tone {tone} 色相偏移 {delta}°");
        }
    }

    #[test]
    fn chroma_zero_is_neutral_gray() {
        let neutral = TonalPalette::new(120.0, 0.0);
        for tone in [6.0_f64, 30.0, 50.0, 90.0, 98.0] {
            let color = neutral.tone(tone);
            assert!(color.r == color.g && color.g == color.b, "{color:?}");
        }
    }

    #[test]
    fn yellow_tone_99_uses_the_official_average() {
        let palette = TonalPalette::new(110.0, 40.0);
        let expected = average(palette.tone(98.0), palette.tone(100.0));
        assert_eq!(palette.tone(99.0), expected);
        // 非黄相不触发特例：tone 99 走正常求解。
        let normal = TonalPalette::new(300.0, 40.0);
        assert_eq!(normal.tone(99.0), normal.hct(99.0).to_color());
    }

    #[test]
    fn requested_chroma_is_respected_when_the_gamut_allows() {
        // tone 50 是最「宽」的色调面，彩度 36 一定装得下。
        let palette = TonalPalette::new(298.980_997, 36.0);
        let hct = palette.hct(50.0);
        assert!((hct.chroma - 36.0).abs() < 0.5, "彩度 {}", hct.chroma);
    }
}
