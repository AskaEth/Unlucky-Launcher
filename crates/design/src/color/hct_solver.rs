//! HCT 求解器：给定 `(hue, chroma, tone)` 求 sRGB 色值——**这就是 M3 的 gamut 映射**。
//!
//! M3 不做「越界就硬裁」：它先假设色域足够，用 Newton 迭代解出线性 RGB；只有当解落进 sRGB
//! 立方体之外时，才退到「在色调平面（`Y` 恒定）上沿色相二分」，把彩度压到该色相能取到的最大值，
//! **色相与 tone 都保持不变**。所以深色/浅色端的颜色是「变灰」而不是「变色」。
//!
//! 照搬 `java/hct/HctSolver.java`（Apache-2.0，见 `NOTICE`）：两段（Newton + 二分）、
//! 迭代次数、`100.01` 这类边界容差都一致。关键平面表见 [`super::critical_planes`]。

use super::space::{self, signum};
use super::viewing_conditions::default_conditions;
use super::Color;

/// 线性 RGB → 「折扣光源」标度的矩阵（求解器内联版，与 [`super::cam16`] 等价但省一次除法）。
const SCALED_DISCOUNT_FROM_LINRGB: [[f64; 3]; 3] = [
    [
        0.001_200_833_568_784_504,
        0.002_389_694_492_170_889,
        0.000_279_574_288_586_112_4,
    ],
    [
        0.000_589_108_665_137_599_9,
        0.002_978_550_257_343_875_8,
        0.000_327_066_610_400_839_8,
    ],
    [
        0.000_101_466_924_916_405_72,
        0.000_536_421_435_918_669_4,
        0.003_297_940_177_071_207_6,
    ],
];

/// 「折扣光源」标度 → 线性 RGB（[`SCALED_DISCOUNT_FROM_LINRGB`] 的逆）。
const LINRGB_FROM_SCALED_DISCOUNT: [[f64; 3]; 3] = [
    [
        1_373.219_870_959_423_1,
        -1_100.425_119_075_482_1,
        -7.278_681_089_101_213,
    ],
    [
        -271.815_969_077_903,
        559.658_046_594_073_3,
        -32.460_474_827_911_94,
    ],
    [
        1.962_289_959_966_566_6,
        -57.173_814_538_844_006,
        308.723_319_781_238_5,
    ],
];

/// 线性 RGB 的 `Y` 权重（Rec.709 亮度系数）。
const Y_FROM_LINRGB: [f64; 3] = [0.2126, 0.7152, 0.0722];

/// `(hue, chroma, tone)` → 色值。彩度取不到时压到该色相/色调下的最大值（色相与 tone 不变）。
pub(crate) fn solve_to_color(hue_degrees: f64, chroma: f64, lstar: f64) -> Color {
    // 端点与极低彩度直接走灰：官方用同一断点，保证 tone 0/100 是纯黑/纯白。
    if chroma < 0.0001 || lstar < 0.0001 || lstar > 99.9999 {
        return space::color_from_lstar(lstar);
    }
    let hue_radians = space::sanitize_degrees(hue_degrees).to_radians();
    let y = space::y_from_lstar(lstar);
    if let Some(exact) = find_result_by_j(hue_radians, chroma, y) {
        return exact;
    }
    space::color_from_linrgb(bisect_to_limit(y, hue_radians))
}

/// 牛顿迭代直接解（色域内能解出来时，这条路径是精确解）。
fn find_result_by_j(hue_radians: f64, chroma: f64, y: f64) -> Option<Color> {
    let vc = default_conditions();
    let mut j = y.sqrt() * 11.0;
    let t_inner_coeff = 1.0 / (1.64 - 0.29_f64.powf(vc.n)).powf(0.73);
    let e_hue = 0.25 * ((hue_radians + 2.0).cos() + 3.8);
    let p1 = e_hue * (50_000.0 / 13.0) * vc.nc * vc.ncb;
    let h_sin = hue_radians.sin();
    let h_cos = hue_radians.cos();
    for round in 0..5 {
        let j_normalized = j / 100.0;
        let alpha = if chroma == 0.0 || j == 0.0 {
            0.0
        } else {
            chroma / j_normalized.sqrt()
        };
        let t = (alpha * t_inner_coeff).powf(1.0 / 0.9);
        let ac = vc.aw * j_normalized.powf(1.0 / vc.c / vc.z);
        let p2 = ac / vc.nbb;
        let gamma = 23.0 * (p2 + 0.305) * t / (23.0 * p1 + 11.0 * t * h_cos + 108.0 * t * h_sin);
        let a = gamma * h_cos;
        let b = gamma * h_sin;
        let r_a = (460.0 * p2 + 451.0 * a + 288.0 * b) / 1403.0;
        let g_a = (460.0 * p2 - 891.0 * a - 261.0 * b) / 1403.0;
        let b_a = (460.0 * p2 - 220.0 * a - 6300.0 * b) / 1403.0;
        let linrgb = space::matrix_multiply(
            [
                inverse_chromatic_adaptation(r_a),
                inverse_chromatic_adaptation(g_a),
                inverse_chromatic_adaptation(b_a),
            ],
            LINRGB_FROM_SCALED_DISCOUNT,
        );
        if linrgb[0] < 0.0 || linrgb[1] < 0.0 || linrgb[2] < 0.0 {
            return None;
        }
        let f_nj = Y_FROM_LINRGB[0] * linrgb[0]
            + Y_FROM_LINRGB[1] * linrgb[1]
            + Y_FROM_LINRGB[2] * linrgb[2];
        if f_nj <= 0.0 {
            return None;
        }
        if round == 4 || (f_nj - y).abs() < 0.002 {
            if linrgb[0] > 100.01 || linrgb[1] > 100.01 || linrgb[2] > 100.01 {
                return None;
            }
            return Some(space::color_from_linrgb(linrgb));
        }
        // Newton：用 2 * fn(j) / j 近似 fn'(j)
        j -= (f_nj - y) * j / (2.0 * f_nj);
    }
    None
}

/// 色域外时的兜底：在 `Y` 恒定的平面上按色相二分，取边界上的颜色。
fn bisect_to_limit(y: f64, target_hue: f64) -> [f64; 3] {
    let (mut left, mut right) = bisect_to_segment(y, target_hue);
    let mut left_hue = hue_of(left);
    for axis in 0..3 {
        if left[axis] == right[axis] {
            continue;
        }
        let (mut l_plane, mut r_plane) = if left[axis] < right[axis] {
            (
                critical_plane_below(space::true_delinearized(left[axis])),
                critical_plane_above(space::true_delinearized(right[axis])),
            )
        } else {
            (
                critical_plane_above(space::true_delinearized(left[axis])),
                critical_plane_below(space::true_delinearized(right[axis])),
            )
        };
        for _ in 0..8 {
            if (r_plane - l_plane).abs() <= 1 {
                break;
            }
            let m_plane = ((f64::from(l_plane) + f64::from(r_plane)) / 2.0).floor() as i32;
            let coordinate = super::critical_planes::CRITICAL_PLANES[m_plane as usize];
            let mid = set_coordinate(left, coordinate, right, axis);
            let mid_hue = hue_of(mid);
            if are_in_cyclic_order(left_hue, target_hue, mid_hue) {
                right = mid;
                r_plane = m_plane;
            } else {
                left = mid;
                left_hue = mid_hue;
                l_plane = m_plane;
            }
        }
    }
    midpoint(left, right)
}

/// 找到包含目标色相的边界线段（sRGB 立方体与 `Y` 平面的交是多边形）。
fn bisect_to_segment(y: f64, target_hue: f64) -> ([f64; 3], [f64; 3]) {
    let mut left = [-1.0_f64; 3];
    let mut right = left;
    let mut left_hue = 0.0;
    let mut initialized = false;
    let mut uncut = true;
    for n in 0..12 {
        let Some(mid) = nth_vertex(y, n) else {
            continue;
        };
        let mid_hue = hue_of(mid);
        if !initialized {
            initialized = true;
            left = mid;
            right = mid;
            left_hue = mid_hue;
            continue;
        }
        if uncut || are_in_cyclic_order(left_hue, mid_hue, hue_of(right)) {
            uncut = false;
            if are_in_cyclic_order(left_hue, target_hue, mid_hue) {
                right = mid;
            } else {
                left = mid;
                left_hue = mid_hue;
            }
        }
    }
    (left, right)
}

/// `Y` 平面与立方体的第 `n` 个顶点（不在立方体内返回 `None`）。
fn nth_vertex(y: f64, n: usize) -> Option<[f64; 3]> {
    let [k_r, k_g, k_b] = Y_FROM_LINRGB;
    let coord_a = if n % 4 <= 1 { 0.0 } else { 100.0 };
    let coord_b = if n.is_multiple_of(2) { 0.0 } else { 100.0 };
    let vertex = if n < 4 {
        let (g, b) = (coord_a, coord_b);
        [((y - g * k_g - b * k_b) / k_r), g, b]
    } else if n < 8 {
        let (b, r) = (coord_a, coord_b);
        [r, ((y - r * k_r - b * k_b) / k_g), b]
    } else {
        let (r, g) = (coord_a, coord_b);
        [r, g, ((y - r * k_r - g * k_g) / k_b)]
    };
    if vertex.iter().all(|c| (0.0..=100.0).contains(c)) {
        Some(vertex)
    } else {
        None
    }
}

/// 线性 RGB 的色相（弧度，未归一）。
fn hue_of(linrgb: [f64; 3]) -> f64 {
    let scaled = space::matrix_multiply(linrgb, SCALED_DISCOUNT_FROM_LINRGB);
    let r_a = chromatic_adaptation(scaled[0]);
    let g_a = chromatic_adaptation(scaled[1]);
    let b_a = chromatic_adaptation(scaled[2]);
    let a = (11.0 * r_a - 12.0 * g_a + b_a) / 11.0;
    let b = (r_a + g_a - 2.0 * b_a) / 9.0;
    b.atan2(a)
}

/// 角度上 `b` 是否落在 `a → c` 的前进区间内。
fn are_in_cyclic_order(a: f64, b: f64, c: f64) -> bool {
    space::sanitize_radians(b - a) < space::sanitize_radians(c - a)
}

/// 把线段 `source → target` 在 `axis` 上截到 `coordinate`。
fn set_coordinate(source: [f64; 3], coordinate: f64, target: [f64; 3], axis: usize) -> [f64; 3] {
    let t = (coordinate - source[axis]) / (target[axis] - source[axis]);
    [
        source[0] + (target[0] - source[0]) * t,
        source[1] + (target[1] - source[1]) * t,
        source[2] + (target[2] - source[2]) * t,
    ]
}

/// 线段中点。
fn midpoint(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        (a[0] + b[0]) / 2.0,
        (a[1] + b[1]) / 2.0,
        (a[2] + b[2]) / 2.0,
    ]
}

/// 非线性压缩的正向（与 CAM16 里那段同公式）。
fn chromatic_adaptation(component: f64) -> f64 {
    let af = component.abs().powf(0.42);
    signum(component) * 400.0 * af / (af + 27.13)
}

/// 非线性压缩的逆。
fn inverse_chromatic_adaptation(adapted: f64) -> f64 {
    let adapted_abs = adapted.abs();
    let base = (27.13 * adapted_abs / (400.0 - adapted_abs)).max(0.0);
    signum(adapted) * base.powf(1.0 / 0.42)
}

/// 关键平面索引：向下取（官方 `(int) Math.floor(x - 0.5)`）。
fn critical_plane_below(x: f64) -> i32 {
    (x - 0.5).floor() as i32
}

/// 关键平面索引：向上取（官方 `(int) Math.ceil(x - 0.5)`）。
fn critical_plane_above(x: f64) -> i32 {
    (x - 0.5).ceil() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_gamut_requests_are_met_exactly() {
        // 官方 Java 实测：HCT(95, 36, 40) → #705D0D。
        assert_eq!(solve_to_color(95.0, 36.0, 40.0).to_hex(), "#705D0D");
        assert_eq!(solve_to_color(25.0, 84.0, 40.0).to_hex(), "#BA1A1A");
        assert_eq!(solve_to_color(95.0, 36.0, 90.0).to_hex(), "#FCE186");
    }

    #[test]
    fn out_of_gamut_requests_keep_hue_and_tone() {
        // 请求 chroma 200（sRGB 装不下）：官方只在彩度上退让，色相与 tone 必须保住。
        let color = solve_to_color(200.0, 200.0, 70.0);
        assert_eq!(color.to_hex(), "#00BEC3");
        let hct = super::super::Hct::from_color(color);
        assert!((hct.tone - 70.0).abs() < 1.0, "tone 掉了：{}", hct.tone);
        assert!(
            (hct.hue - 200.0).abs() < 1.0,
            "色相偏了：{}（应 ≈200）",
            hct.hue
        );
        assert!(hct.chroma < 200.0);
    }

    #[test]
    fn neutral_requests_return_exact_grays() {
        for lstar in [0.0_f64, 4.0, 10.0, 50.0, 87.0, 98.0, 100.0] {
            let color = solve_to_color(120.0, 0.0, lstar);
            assert!(color.r == color.g && color.g == color.b, "{:?}", color);
        }
        assert_eq!(solve_to_color(0.0, 0.0, 0.0), Color::BLACK);
        assert_eq!(solve_to_color(0.0, 0.0, 100.0), Color::WHITE);
    }
}
