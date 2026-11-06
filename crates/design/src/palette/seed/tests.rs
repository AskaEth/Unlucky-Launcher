//! `seed` 模块的单测：五类输入各自的处置 + **端到端反证**（不许退化成平均色）。
//!
//! 为什么端到端测试必须在这里（而不是只测 `quantize`/`score`）：那两条单测手工构造
//! `Cluster` 直接喂中间层，**绕过了 `analyze_rgba` 的接线**。把 `analyze_rgba` 换成
//! 「不透明像素求算术平均」时，中间层单测照样绿，只有这里会红。

use super::*;

/// 生成 `width × height` 的全 `color` 图。
fn solid(color: Color, width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..(width * height) {
        out.extend_from_slice(&[color.r, color.g, color.b, color.a]);
    }
    out
}

/// 生成「其余是 `base`，`accent_pixels` 个像素是 `accent`」的图（强调色排在末尾）。
///
/// 这是**反证「不许取平均色」**的输入构造器：强调色面积小到平均色会被底色吃掉，
/// 但它是人眼会记住、能当强调色用的那一小撮。
fn two_tone(width: u32, height: u32, base: Color, accent: Color, accent_pixels: usize) -> Vec<u8> {
    let total = (width * height) as usize;
    assert!(accent_pixels <= total);
    let mut out = Vec::with_capacity(total * 4);
    for index in 0..total {
        let color = if index < total - accent_pixels {
            base
        } else {
            accent
        };
        out.extend_from_slice(&[color.r, color.g, color.b, color.a]);
    }
    out
}

#[test]
fn end_to_end_picks_the_five_percent_accent() {
    // ⚠️ 端到端反证：这条必须走 `analyze_rgba` 的**完整管线**（量化 → 打分 → 彩度门槛），
    // 不许像 `quantize`/`score` 的单测那样手工构造 `Cluster` 直接喂中间层。
    // 如果 `analyze_rgba` 被改成「不透明像素求算术平均」，平均值会是米白（chroma ≈4）→
    // 返回 `LowChroma` 而不是种子，这条立刻红。
    let beige = Color::from_hex("#F5F1EA").expect("合法 hex");
    let coral = Color::from_hex("#FF6B5A").expect("合法 hex");
    let pixels = two_tone(40, 40, beige, coral, 80); // 95% 米白 + 5% 珊瑚

    let seed = analyze_rgba(&pixels, 40, 40).expect("5% 的珊瑚应当被选中当种子");
    assert_eq!(
        seed.color,
        coral,
        "种子必须是珊瑚（实际 {}）；米白占比 95% 但它不是「主色」",
        seed.color.to_hex()
    );
    assert!(seed.chroma > 40.0, "珊瑚彩度应当很高，实测 {}", seed.chroma);
    // 与「平均色」泾渭分明：原图的算术平均值是米白，色相/彩度都差一整个量级。
    assert!(!seed.is_muted());
}

#[test]
fn end_to_end_picks_the_three_percent_accent() {
    // 面积再降到 3%（48/1600）：`Score` 的 1% 人气门槛仍然放行，平均色更不可能靠巧合命中。
    let beige = Color::from_hex("#F5F1EA").expect("合法 hex");
    let orange = Color::from_hex("#FF8800").expect("合法 hex");
    let pixels = two_tone(40, 40, beige, orange, 48);

    let seed = analyze_rgba(&pixels, 40, 40).expect("3% 的橙应当被选中当种子");
    assert_eq!(seed.color, orange, "实际 {}", seed.color.to_hex());
    assert!(seed.chroma > 40.0, "彩度 {}", seed.chroma);
}

#[test]
fn solid_vivid_image_yields_seed() {
    let pixels = solid(Color::rgb(0x3C, 0x8D, 0xBC), 4, 4);
    let seed = analyze_rgba(&pixels, 4, 4).expect("天蓝纯色图有种子");
    assert_eq!(seed.color, Color::from_hex("#3C8DBC").expect("合法 hex"));
    assert!(seed.chroma > MIN_SEED_CHROMA);
    assert!((seed.share - 1.0).abs() < 1.0e-6);
    assert!(!seed.is_muted());
}

#[test]
fn gray_white_image_is_rejected_with_measured_chroma() {
    // D40 的反证：灰白壁纸硬取色会得到青色 #006874，这里必须拒绝并报出彩度。
    let pixels = solid(Color::rgb(0xF2, 0xF2, 0xF0), 8, 8);
    match analyze_rgba(&pixels, 8, 8) {
        Err(Failure::LowChroma { chroma, .. }) => {
            assert!(chroma < MIN_SEED_CHROMA, "彩度 {chroma} 不该过门槛");
        }
        other => panic!("灰白图必须被拒，实际 {other:?}"),
    }
    assert_eq!(seed_from_rgba(&pixels, 8, 8), None);
}

#[test]
fn neutral_gray_hash_rejects_instead_of_inventing_cyan() {
    // 与 M70 复审实测的坏味道对齐：#808080 的 CAM16 色相 209.5°，硬取色会得到青色。
    let pixels = solid(Color::rgb(0x80, 0x80, 0x80), 4, 4);
    assert_eq!(seed_from_rgba(&pixels, 4, 4), None);
}

#[test]
fn tiny_images_are_too_small() {
    let pixels = solid(Color::rgb(0xFF, 0x00, 0x00), 1, 3);
    assert_eq!(
        analyze_rgba(&pixels, 1, 3),
        Err(Failure::TooSmall {
            width: 1,
            height: 3
        })
    );
    assert_eq!(
        Failure::TooSmall {
            width: 1,
            height: 3
        }
        .code(),
        "too-small"
    );
}

#[test]
fn wrong_buffer_length_is_reported() {
    let pixels = solid(Color::rgb(0xFF, 0x00, 0x00), 2, 2);
    match analyze_rgba(&pixels[..12], 2, 2) {
        Err(Failure::BadDimensions {
            expected, actual, ..
        }) => {
            assert_eq!((expected, actual), (16, 12));
        }
        other => panic!("应当报维度错误，实际 {other:?}"),
    }
}

#[test]
fn fully_transparent_image_has_no_seed() {
    let pixels = solid(Color::rgba(0x3C, 0x8D, 0xBC, 0), 4, 4);
    assert_eq!(analyze_rgba(&pixels, 4, 4), Err(Failure::NoOpaquePixels));
}

#[test]
fn half_transparent_image_keeps_its_opaque_colour() {
    // 复审 p2-1 的反证：512×512、一半完全不透明砖红、一半全透明（RGB=0）交错。
    // 修复前「先块平均、再拿平均 alpha 比 128」：块内 255 与 0 平均被整数除法截成 127 < 128，
    // 于是全部 131072 个不透明像素被判成「没有不透明像素」→ Err(NoOpaquePixels)。
    // 修复后（先过滤再平均）必须取到砖红本身。
    let (width, height) = (512usize, 512usize);
    let mut pixels = Vec::with_capacity(width * height * 4);
    for index in 0..width * height {
        if index % 2 == 0 {
            pixels.extend_from_slice(&[0xC0, 0x39, 0x2B, 0xFF]);
        } else {
            pixels.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        }
    }
    let seed = analyze_rgba(&pixels, 512, 512).expect("一半不透明像素必须能取色");
    assert_eq!(seed.color, Color::rgb(0xC0, 0x39, 0x2B));
    assert!(seed.chroma > 40.0, "彩度 {}", seed.chroma);
}

#[test]
fn mostly_transparent_image_still_samples_the_visible_part() {
    // 90% 全透明 + 10% 不透明绿：可见部分够鲜艳就该取到它，而不是报「没有不透明像素」。
    let (width, height) = (64usize, 64usize);
    let mut pixels = Vec::with_capacity(width * height * 4);
    for index in 0..width * height {
        if index % 10 == 0 {
            pixels.extend_from_slice(&[0x2E, 0x7D, 0x4F, 0xFF]);
        } else {
            pixels.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        }
    }
    let seed = analyze_rgba(&pixels, 64, 64).expect("10% 可见像素必须能取色");
    assert_eq!(seed.color, Color::rgb(0x2E, 0x7D, 0x4F));
}

#[test]
fn large_image_is_downsampled_and_still_deterministic() {
    let (width, height) = (512_u32, 512_u32);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            // 细网格：等距抽样会走样，块平均能得到稳定的底色。
            let checker = (x + y) % 2 == 0;
            pixels.extend_from_slice(if checker {
                &[0xD2, 0x3B, 0x2A, 0xFF]
            } else {
                &[0x2A, 0x3B, 0xD2, 0xFF]
            });
        }
    }
    let first = analyze_rgba(&pixels, width, height).expect("大图有种子");
    let second = analyze_rgba(&pixels, width, height).expect("可复现");
    assert_eq!(first, second);
    let samples = sample_blocks(&pixels, width as usize, height as usize);
    assert!(
        samples.len() <= MAX_SAMPLES,
        "采样点 {} 超标",
        samples.len()
    );
    // 块平均把红蓝网格合成偏紫的中间色，而不是撞上纯红或纯蓝之一。
    assert!(first.chroma > MIN_SEED_CHROMA, "{first:?}");
}

#[test]
fn block_sampling_averages_instead_of_picking() {
    // 2×2 红蓝棋盘，块边长 1（图很小）→ 4 个采样点；只验证采样点数量与顺序稳定。
    let pixels = [
        0xFF, 0x00, 0x00, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00,
        0xFF,
    ];
    let samples = sample_blocks(&pixels, 2, 2);
    assert_eq!(samples.len(), 4);
    assert_eq!(samples[0], Color::rgb(0xFF, 0x00, 0x00));
    assert_eq!(samples[1], Color::rgb(0x00, 0x00, 0xFF));
}

#[test]
fn seed_scheme_meets_wcag_on_the_primary_pair() {
    let pixels = solid(Color::rgb(0xC0, 0x39, 0x2B), 4, 4);
    let seed = analyze_rgba(&pixels, 4, 4).expect("砖红有种子");
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let scheme = seed.scheme(flavor, 12.0);
        let ratio = scheme.contrast(crate::color::Role::OnPrimary, crate::color::Role::Primary);
        assert!(ratio >= 4.5, "{flavor:?} 只有 {ratio}");
    }
}

#[test]
fn chroma_just_below_the_gate_is_rejected() {
    // 把 D40 的口径（`MIN_SEED_CHROMA` = 8，「< ~8 视为无信息量」）钉成可执行的断言：
    // 图里的颜色实测 chroma ≈7.23 —— 落在 `Score` 的候选门槛（5）之上、D40 的种子门槛（8）之下。
    // 这类图必须**回落**，而不是「勉强取一个低彩度种子」。
    //
    // 变异反证：把 `MIN_SEED_CHROMA` 从 8.0 改成 6.0，这个种子会通过门槛 → 本条变红。
    let faint = Color::from_hex("#7E8489").expect("合法 hex");
    let chroma = crate::color::Hct::from_color(faint).chroma;
    assert!(
        (6.0..8.0).contains(&chroma),
        "这条测试依赖「彩度落在 6~8 之间」，实测 {chroma}"
    );

    let pixels = solid(faint, 16, 16);
    match analyze_rgba(&pixels, 16, 16) {
        Err(Failure::LowChroma {
            chroma: measured, ..
        }) => {
            assert!(
                (6.0..MIN_SEED_CHROMA).contains(&measured),
                "报回来的彩度 {measured} 应当仍在门槛之下"
            );
        }
        other => panic!("彩度 {chroma} 必须回落，实际 {other:?}"),
    }
}
