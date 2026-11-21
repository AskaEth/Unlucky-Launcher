//! 色彩空间底层换算：sRGB ↔ 线性 RGB ↔ CIE XYZ ↔ `L*`（tone 轴）。
//!
//! 只放**纯数学**，不认识任何语义色角色。数值口径逐函数照搬 Google
//! `material-color-utilities` 的 `java/utils/ColorUtils.java`（Apache-2.0，见 `NOTICE`）：
//! 矩阵、阈值、取整方式都一致——差一位小数，HCT 求解器就会与官方差一个通道。
//!
//! 内部一律用 `f64`（官方实现是 Java `double`）。公开 API 仍以 `u8` 通道与 `f32` 标量对外，
//! 因为令牌消费方（绘制层）不需要比 8 bit 更高的精度。

use super::Color;

/// sRGB → XYZ（D65）。官方用的是 IEC 6-1 的舍入值，不是更精确的 sRGB 矩阵。
const SRGB_TO_XYZ: [[f64; 3]; 3] = [
    [0.412_338_95, 0.357_620_64, 0.180_510_42],
    [0.2126, 0.7152, 0.0722],
    [0.019_321_41, 0.119_163_82, 0.950_344_78],
];

/// D65 白点（XYZ，Y 归一化到 100）。
pub(crate) const WHITE_POINT_D65: [f64; 3] = [95.047, 100.0, 108.883];

/// 行向量 × 3×3 矩阵。
pub(crate) fn matrix_multiply(row: [f64; 3], matrix: [[f64; 3]; 3]) -> [f64; 3] {
    let mul = |m: [f64; 3]| row[0] * m[0] + row[1] * m[1] + row[2] * m[2];
    [mul(matrix[0]), mul(matrix[1]), mul(matrix[2])]
}

/// sRGB 编码值（8 bit 通道）→ 线性光强（0..=100，官方标度）。
pub(crate) fn linearized(component: u8) -> f64 {
    let normalized = f64::from(component) / 255.0;
    if normalized <= 0.040_449_936 {
        normalized / 12.92 * 100.0
    } else {
        ((normalized + 0.055) / 1.055).powf(2.4) * 100.0
    }
}

/// 线性光强（0..=100）→ sRGB 编码值，四舍五入并夹取到 0..=255。
pub(crate) fn delinearized(component: f64) -> u8 {
    let value = true_delinearized(component).round().clamp(0.0, 255.0);
    value as u8
}

/// 同 [`delinearized`]，但不取整也不夹取（求解器二分时需要连续值）。
pub(crate) fn true_delinearized(component: f64) -> f64 {
    let normalized = component / 100.0;
    let encoded = if normalized <= 0.003_130_8 {
        normalized * 12.92
    } else {
        1.055 * normalized.powf(1.0 / 2.4) - 0.055
    };
    encoded * 255.0
}

/// 线性 RGB（0..=100）→ 不透明颜色。
pub(crate) fn color_from_linrgb(linrgb: [f64; 3]) -> Color {
    Color::rgb(
        delinearized(linrgb[0]),
        delinearized(linrgb[1]),
        delinearized(linrgb[2]),
    )
}

/// 色值 → XYZ（Y 在 0..=100 标度）。
pub(crate) fn xyz_from_color(color: Color) -> [f64; 3] {
    let linear = [
        linearized(color.r),
        linearized(color.g),
        linearized(color.b),
    ];
    matrix_multiply(linear, SRGB_TO_XYZ)
}

/// `L*` 的比较函数（CIE 分段线性/立方根）。
fn lab_f(t: f64) -> f64 {
    const EPSILON: f64 = 216.0 / 24389.0;
    const KAPPA: f64 = 24389.0 / 27.0;
    if t > EPSILON {
        t.powf(1.0 / 3.0)
    } else {
        (KAPPA * t + 16.0) / 116.0
    }
}

/// [`lab_f`] 的逆。
fn lab_invf(ft: f64) -> f64 {
    const EPSILON: f64 = 216.0 / 24389.0;
    const KAPPA: f64 = 24389.0 / 27.0;
    let cubed = ft * ft * ft;
    if cubed > EPSILON {
        cubed
    } else {
        (116.0 * ft - 16.0) / KAPPA
    }
}

/// `L*` → 相对亮度 Y（0..=100）。
pub(crate) fn y_from_lstar(lstar: f64) -> f64 {
    100.0 * lab_invf((lstar + 16.0) / 116.0)
}

/// 相对亮度 Y（0..=100）→ `L*`。
pub(crate) fn lstar_from_y(y: f64) -> f64 {
    lab_f(y / 100.0) * 116.0 - 16.0
}

/// 色值 → `L*`（即 M3 的 **tone**）。
pub(crate) fn lstar_from_color(color: Color) -> f64 {
    lstar_from_y(xyz_from_color(color)[1])
}

/// 色值 → CIELAB（`[L*, a*, b*]`）。
///
/// `L*` 与 [`lstar_from_color`] 逐值相同（同一个 `lab_f`）；`a*`/`b*` 走 D65 白点归一。
/// 取色量化（`crate::palette::quantize`）用它在感知空间里算簇间距离——sRGB 欧氏距离会把
/// 暗部差异放大、亮部差异压扁，分出来的簇不符合「看起来是一族颜色」。
pub(crate) fn lab_from_color(color: Color) -> [f64; 3] {
    let xyz = xyz_from_color(color);
    let f = |component: f64, white: f64| lab_f(component / white);
    let fx = f(xyz[0], WHITE_POINT_D65[0]);
    let fy = f(xyz[1], WHITE_POINT_D65[1]);
    let fz = f(xyz[2], WHITE_POINT_D65[2]);
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// `L*` → 中性灰色值（`L*` 与彩度无关，纯灰）。
pub(crate) fn color_from_lstar(lstar: f64) -> Color {
    let component = delinearized(y_from_lstar(lstar));
    Color::rgb(component, component, component)
}

/// Java `Math.signum` 的口径：负数 −1、零 0、正数 1（**Rust 的 `f64::signum(0.0)` 是 1**，
/// 直接换会改变中性色的求解结果）。
pub(crate) fn signum(value: f64) -> f64 {
    if value < 0.0 {
        -1.0
    } else if value == 0.0 {
        0.0
    } else {
        1.0
    }
}

/// 角度归一到 `0.0..360.0`。
pub(crate) fn sanitize_degrees(degrees: f64) -> f64 {
    let wrapped = degrees % 360.0;
    if wrapped < 0.0 {
        wrapped + 360.0
    } else {
        wrapped
    }
}

/// 弧度归一到 `0.0..2π`（官方 `sanitizeRadians`，多加了 8π 保证非负）。
pub(crate) fn sanitize_radians(angle: f64) -> f64 {
    (angle + std::f64::consts::PI * 8.0) % (std::f64::consts::PI * 2.0)
}

/// 线性插值（官方 `lerp`）。
pub(crate) fn lerp(start: f64, stop: f64, amount: f64) -> f64 {
    (1.0 - amount) * start + amount * stop
}

/// 夹取到 `min..=max`。
pub(crate) fn clamp(min: f64, max: f64, input: f64) -> f64 {
    input.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lstar_and_luminance_are_inverse() {
        for lstar in [0.0_f64, 8.0, 20.0, 50.0, 87.0, 98.0, 100.0] {
            let back = lstar_from_y(y_from_lstar(lstar));
            assert!((back - lstar).abs() < 1.0e-9, "L* {lstar} 往返成 {back}");
        }
    }

    #[test]
    fn white_and_black_anchor_the_scale() {
        assert_eq!(delinearized(100.0), 255);
        assert_eq!(delinearized(0.0), 0);
        assert_eq!(color_from_lstar(100.0), Color::WHITE);
        assert_eq!(color_from_lstar(0.0), Color::BLACK);
        assert!((lstar_from_color(Color::WHITE) - 100.0).abs() < 1.0e-6);
        assert!(lstar_from_color(Color::BLACK).abs() < 1.0e-6);
    }

    #[test]
    fn white_point_matches_d65() {
        // 用官方那组舍入矩阵，纯白必须落在 D65 白点上（容差 1e-3 对应矩阵末位舍入）。
        let xyz = xyz_from_color(Color::WHITE);
        for (got, want) in xyz.iter().zip(WHITE_POINT_D65) {
            assert!(
                (got - want).abs() < 1.0e-3,
                "{xyz:?} vs D65 {WHITE_POINT_D65:?}"
            );
        }
        assert!(xyz_from_color(Color::BLACK).iter().all(|v| *v == 0.0));
    }

    #[test]
    fn signum_matches_java_semantics() {
        assert_eq!(signum(-1.5), -1.0);
        assert_eq!(signum(0.0), 0.0);
        assert_eq!(signum(-0.0), 0.0);
        assert_eq!(signum(2.5), 1.0);
    }

    #[test]
    fn degrees_sanitize_wraps_negatives() {
        assert!((sanitize_degrees(-30.0) - 330.0).abs() < 1.0e-9);
        assert!(sanitize_degrees(360.0).abs() < 1.0e-9);
    }
}
