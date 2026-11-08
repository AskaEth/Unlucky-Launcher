//! 解码适配层的单测：真 PNG / 真 JPEG 字节进，种子或**具体错误**出。
//!
//! 这里刻意不 mock 解码器：本 crate 存在的意义就是「真解码」，mock 掉它等于不测。
//! 测试用内存里的字节（`encode_*` 现造），不依赖任何外部素材文件。

use std::io::Cursor;

use super::*;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

fn solid_image(width: u32, height: u32, rgb: [u8; 3]) -> RgbaImage {
    RgbaImage::from_pixel(width, height, Rgba([rgb[0], rgb[1], rgb[2], 0xFF]))
}

fn encode_png(image: &RgbaImage) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("PNG 编码应当成功");
    bytes.into_inner()
}

fn encode_jpeg(image: &RgbaImage, quality: u8) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality);
    encoder
        .encode_image(&DynamicImage::ImageRgba8(image.clone()))
        .expect("JPEG 编码应当成功");
    bytes.into_inner()
}

#[test]
fn solid_png_keeps_the_exact_seed_colour() {
    let png = encode_png(&solid_image(32, 32, [0xC0, 0x39, 0x2B]));
    let seed = seed_from_bytes(&png).expect("砖红纯色图应当有种子");
    // 32×32 不触发缩放；design 侧对纯色图做桶内原值平均 → 逐通道相等。
    assert_eq!(seed.color.r, 0xC0);
    assert_eq!(seed.color.g, 0x39);
    assert_eq!(seed.color.b, 0x2B);
    assert!(seed.chroma > 8.0);
}

#[test]
fn jpeg_decoding_is_wired_up() {
    // 只验证「jpeg feature 真的接上了」：JPEG 会动色，只要求色相别跑偏太多。
    let jpeg = encode_jpeg(&solid_image(48, 48, [0xC0, 0x39, 0x2B]), 92);
    let seed = seed_from_bytes(&jpeg).expect("JPEG 应当能解码并取色");
    assert!(seed.chroma > 8.0, "彩度 {}", seed.chroma);
    assert!(
        (seed.hue - 30.0).abs() < 25.0,
        "砖红的 CAM16 色相应当在 30° 附近，实际 {}",
        seed.hue
    );
}

#[test]
fn extension_is_ignored_and_format_is_sniffed() {
    // 用户把 .jpg 改成 .png 的情况很常见：按魔数判断，不按扩展名。
    let jpeg = encode_jpeg(&solid_image(16, 16, [0x2E, 0x7D, 0x4F]), 95);
    let decoded = decode_bytes(&jpeg).expect("魔数嗅探应当认得 JPEG");
    assert_eq!((decoded.width(), decoded.height()), (16, 16));
}

#[test]
fn bmp_is_supported() {
    // M73 的 Windows 壁纸探测不筛扩展名，`.bmp` 壁纸会真的出现在取色路径上。
    // BMP 编码不是无损于色的（24 bit 存 RGB，这里是 32 bit Rgba），所以逐通道比对即可。
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(solid_image(32, 32, [0xB2, 0x3A, 0x48]))
        .write_to(&mut bytes, ImageFormat::Bmp)
        .expect("BMP 编码应当成功");
    let seed = seed_from_bytes(&bytes.into_inner()).expect("BMP 应当能解码并取色");
    assert_eq!(
        seed.color,
        unlucky_design::color::Color::rgb(0xB2, 0x3A, 0x48)
    );
    assert!(seed.chroma > 8.0);
}

#[test]
fn garbage_bytes_fail_and_map_to_undecodable() {
    let loaded = decode_bytes(b"this is definitely not an image");
    assert!(
        matches!(
            loaded,
            Err(ExtractError::Decode(_)) | Err(ExtractError::Io(_))
        ),
        "非图片字节应当解码失败"
    );
    assert_eq!(input(&loaded), ImageInput::Undecodable);
}

#[test]
fn truncated_png_fails_instead_of_returning_half_an_image() {
    let png = encode_png(&solid_image(64, 64, [0x3C, 0x8D, 0xBC]));
    let truncated = &png[..png.len() / 2];
    let loaded = decode_bytes(truncated);
    assert!(loaded.is_err(), "截断的 PNG 不许当成有效图");
    assert_eq!(input(&loaded), ImageInput::Undecodable);
}

#[test]
fn missing_file_is_an_io_error() {
    let loaded = load("Z:/definitely/not/here/background.png");
    match loaded {
        Err(ExtractError::Io(_)) => {}
        other => panic!("应当报 IO 错误，实际 {other:?}"),
    }
}

#[test]
fn one_pixel_image_is_reported_as_too_small() {
    let png = encode_png(&solid_image(1, 1, [0xC0, 0x39, 0x2B]));
    match seed_from_bytes(&png) {
        Err(ExtractError::Palette(palette::Failure::TooSmall { width, height })) => {
            assert_eq!((width, height), (1, 1));
        }
        other => panic!("1×1 图应当取色失败，实际 {other:?}"),
    }
}

#[test]
fn gray_png_falls_back_instead_of_inventing_a_colour() {
    // D40 的反证：灰白图**不许**变成青色。这里必须报低彩度，而不是返回一个种子。
    let png = encode_png(&solid_image(24, 24, [0xEE, 0xEE, 0xEC]));
    match seed_from_bytes(&png) {
        Err(ExtractError::Palette(palette::Failure::LowChroma { chroma, .. })) => {
            assert!(chroma < 8.0, "彩度 {chroma}");
        }
        other => panic!("灰白图应当报低彩度，实际 {other:?}"),
    }
}

#[test]
fn large_image_is_scaled_to_the_max_edge() {
    let png = encode_png(&solid_image(1024, 512, [0xB2, 0x6A, 0x00]));
    let decoded = decode_bytes(&png).expect("大图应当解码成功");
    assert_eq!(decoded.width(), MAX_EDGE);
    assert_eq!(decoded.height(), MAX_EDGE / 2);
    assert_eq!(
        decoded.pixels().len(),
        (MAX_EDGE * MAX_EDGE / 2 * 4) as usize
    );
    // 缩放后仍是那个橙色：取色的结论不该被「缩了多少」改变。
    let seed = seed_from_rgba8(&decoded).expect("琥珀橙有种子");
    assert!(seed.chroma > 8.0);
}

#[test]
fn absurd_dimensions_are_rejected_before_allocating() {
    // 头里写着 20000 宽的 PNG：超过 MAX_SOURCE_EDGE，必须在解码前拒掉。
    let png = encode_png(&solid_image(20_000, 1, [0x3C, 0x8D, 0xBC]));
    match decode_bytes(&png) {
        Err(ExtractError::Decode(_)) => {}
        other => panic!("超限尺寸应当被拒，实际 {other:?}"),
    }
}

#[test]
fn max_edge_stays_within_the_design_sample_budget() {
    // 结构性防线（复审 r2 要求）：`unlucky_design` 只在「像素数 ≤ MAX_SAMPLES」时才不做块平均。
    // 当前 256² = 65536 = MAX_SAMPLES，**余量为零**：谁把 MAX_EDGE 调到 512，本适配层交出去的
    // 缓冲就会超过 design 的采样预算，design 会**在适配层已经缩过之后再叠一层块平均**
    // （色彩被二次抹平），而这条退化没有任何测试会报错——两个常数此前是各写各的。
    //
    // 注意别把这条读成「≤ 预算 ⇒ 种子一定是原图真实颜色」：本适配层的 `thumbnail` 本身就会平均
    // （400×400 红蓝棋盘在 MAX_EDGE=256 下取到的 `#704070` 也不在原图上）。这条只保证
    // 「design 侧不再叠第二层平均」，逐像素语义见 `Seed::color` 的文档。
    let budget = unlucky_design::palette::MAX_SAMPLES;
    let edge = MAX_EDGE as usize;
    assert!(
        edge * edge <= budget,
        "MAX_EDGE={MAX_EDGE} 的平方（{}）超过 design 的采样预算 {budget}：         调大它之前先读 `unlucky_design::palette::Seed::color` 的文档——超过预算后 design 会在本适配层缩放之上再叠一层块平均",
        edge * edge
    );
}

#[test]
fn input_borrows_without_copying() {
    let loaded = decode_bytes(&encode_png(&solid_image(4, 4, [0x67, 0x50, 0xA4])));
    match input(&loaded) {
        ImageInput::Pixels(source) => {
            assert_eq!((source.width, source.height), (4, 4));
            // 零拷贝：视图指向的就是 `Rgba8` 自己的缓冲。
            assert_eq!(source.pixels, loaded.as_ref().expect("解码成功").pixels());
        }
        ImageInput::Undecodable => panic!("刚解码成功却报了 undecodable"),
    }
}
