//! `background` 的单测：解码（png + jpeg）、失败分类、暗化钳位、平铺网格。
//!
//! 每条测试的「抓得住 / 抓不住」写在注释里——这是本 mission 的验收纪律：能断言的不许只靠肉眼。

use super::{dim_alpha, load, small_tile_note, tiles, LoadError, BG_DIM_MAX, MAX_TILES};
use std::path::{Path, PathBuf};
use unlucky_presenter::prefs::BgFill;

fn temp_file(label: &str, name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("ull-bg-{label}-{}-{}", std::process::id(), line!()));
    std::fs::create_dir_all(&dir).expect("临时目录");
    dir.join(name)
}

fn cleanup(path: &Path) {
    if let Some(dir) = path.parent() {
        std::fs::remove_dir_all(dir).ok();
    }
}

#[test]
fn a_real_png_decodes_with_its_original_size_and_raw_bytes() {
    // 抓得住：真磁盘 PNG 能读、原图尺寸按图头、缩略图不超预算、原始字节留着给渲染器。
    // 抓不住：渲染器用这些字节画出来的颜色（那要看截图）。
    let path = temp_file("png", "bg.png");
    let source: image::RgbImage = image::RgbImage::from_pixel(300, 20, image::Rgb([12, 200, 90]));
    source.save(&path).expect("写 png");

    let image = load(path.to_str().unwrap()).expect("真 PNG 必须能读");
    assert_eq!(image.source, (300, 20));
    assert!(
        image.width <= super::MAX_EDGE && image.height <= super::MAX_EDGE,
        "缩略图 {}×{} 超预算",
        image.width,
        image.height
    );
    assert_eq!(image.rgba.len(), (image.width * image.height * 4) as usize);
    assert_eq!(image.rgba.len(), (image.width * image.height * 4) as usize);
    cleanup(&path);
}

/// M93 r2 的硬要求：`.jpg` 必须能加载（用户挑的背景图/壁纸多半是 jpeg）。
///
/// 抓得住：`image` 的 jpeg 编码器真的开了（没开时 `load` 会返回 `Undecodable`）。
/// 抓不住：渲染器那边（`iced/image`）解码 jpeg 的能力——那条由截图矩阵里那张 jpg 出图举证。
#[test]
fn a_real_jpeg_decodes_too() {
    let path = temp_file("jpeg", "bg.jpg");
    let source =
        image::RgbImage::from_fn(64, 32, |x, y| image::Rgb([x as u8 * 4, y as u8 * 8, 90]));
    source
        .save_with_format(&path, image::ImageFormat::Jpeg)
        .expect("写 jpeg");

    let image = load(path.to_str().unwrap()).expect("jpeg 必须能读（M93 r2）");
    assert_eq!(image.source, (64, 32));
    assert_eq!(
        image.rgba.len(),
        (image.width * image.height * 4) as usize,
        "像素解出来了"
    );
    cleanup(&path);
}

#[test]
fn a_missing_or_broken_file_reports_a_readable_reason() {
    // 抓得住：①路径不存在 → Io；②文件存在但不是图片 → Undecodable；③空路径 → Io。
    // 都必须是 Err（不许静默回默认色），且 describe() 是人话。
    // 抓不住：webp/tiff 这类没开编码器的格式具体报什么字符串（只钉分类，不钉解码库原文）。
    let missing = temp_file("missing", "bg.png");
    let error = load(missing.to_str().unwrap()).expect_err("不存在的路径必须报错");
    assert!(matches!(error, LoadError::Io(_)), "实际 {error:?}");
    assert!(error
        .describe()
        .starts_with("Background image could not be read"));

    let broken = temp_file("broken", "bg.png");
    std::fs::write(&broken, b"this is not an image").expect("写坏文件");
    let error = load(broken.to_str().unwrap()).expect_err("非图片必须报错");
    assert!(matches!(error, LoadError::Undecodable(_)), "实际 {error:?}");
    cleanup(&broken);

    let empty = load("   ").expect_err("空路径必须报错");
    assert!(matches!(empty, LoadError::Io(_)));
}

#[test]
fn dim_is_clamped_to_the_supported_range() {
    // 抓得住：暗化系数夹在 0..=0.95（下限允许 0 = 不暗化，上限防「背景看不见」），NaN 取 0。
    // 抓不住：0.35 这个默认值在真实壁纸上的观感（视觉判断）。
    assert_eq!(dim_alpha(0.0), 0.0);
    assert_eq!(dim_alpha(0.35), 0.35);
    assert_eq!(dim_alpha(-1.0), 0.0);
    assert_eq!(dim_alpha(f32::NAN), 0.0);
    assert_eq!(dim_alpha(9.0), BG_DIM_MAX);
    assert_eq!(BG_DIM_MAX, 0.95, "与 core 同源（presenter 重导出）");
}

/// **P2-1 的守门**：任何尺寸/视口组合下，平铺网格铺出来的覆盖宽高都必须 ≥ 视口（不许留缝），
/// 同时块数不超预算（不留缝的代价只能是「放大图块」，不能是「少铺」）。
///
/// 抓得住：谁把 `tiles()` 改回「超预算就减列」——120×120 @ 1180×760 那条会立刻红
/// （r1 实测右缘裸白 100px）。
/// 抓不住：渲染器把单块图铺进格子时的实际像素对齐（有没有半像素缝由截图看）。
#[test]
fn tile_grid_always_covers_the_viewport_within_budget() {
    let cases = [
        ((120, 120), (1180.0, 760.0)),
        ((300, 200), (1180.0, 760.0)),
        ((16, 16), (1920.0, 1080.0)),
        ((8, 8), (3840.0, 2160.0)),
        ((1600, 900), (1180.0, 760.0)),
        ((1, 1), (1180.0, 760.0)),
    ];
    for (source, viewport) in cases {
        let grid = tiles(source, viewport);
        let (covered_w, covered_h) = grid.covered(source);
        assert!(
            covered_w >= viewport.0 - 0.01 && covered_h >= viewport.1 - 0.01,
            "{source:?} @ {viewport:?} 覆盖 {covered_w}×{covered_h} 小于视口（会留缝）：{grid:?}"
        );
        assert!(
            grid.count() <= MAX_TILES,
            "{source:?} @ {viewport:?} 块数 {} 超预算",
            grid.count()
        );
        assert!(grid.scale >= 1.0, "图块只许放大，不许缩小：{grid:?}");
    }

    // 图比视口大 ⇒ 1×1 原尺寸裁切（scale 必须还是 1.0，否则等于缩小了原图）。
    let big = tiles((1600, 900), (1180.0, 760.0));
    assert_eq!((big.cols, big.rows, big.scale), (1, 1, 1.0));

    // 小图按原尺寸铺够（不放大时）。
    let small = tiles((300, 200), (1180.0, 760.0));
    assert_eq!((small.cols, small.rows, small.scale), (4, 4, 1.0));
}

/// 编译期钉住：填充档解析走 core 的词表（`cover`/`contain`/`tile`）。
#[test]
fn fill_keys_are_the_schema_vocabulary() {
    // 抓得住：有人改了落盘词表却没改这里（截图矩阵的 env 也按这三个词传）。
    assert_eq!(BgFill::from_key_value("cover"), BgFill::Cover);
    assert_eq!(BgFill::from_key_value("contain"), BgFill::Contain);
    assert_eq!(BgFill::from_key_value("tile"), BgFill::Tile);
}

/// **复审 r1 P2-4**：解压炸弹的第二道闸——图头就超上限时要走 `TooLarge`，而且这条分支
/// 以前**没有任何测试**（三态诊断里唯一没被钉住的一态）。
///
/// 抓得住：`header_size` 的尺寸判据（构造一张 9000×4 的真 PNG，只花 ~100 KB 就能造出来）。
/// 抓不住：内存分配真的被 `image::Limits` 挡住（那要真喂一张巨图跑到 OOM，本机不做）。
#[test]
fn an_over_sized_image_reports_too_large_instead_of_trying_to_decode() {
    let path = temp_file("huge", "huge.png");
    let source = image::RgbImage::new(9_000, 4);
    source.save(&path).expect("写大尺寸图");

    let error = load(path.to_str().unwrap()).expect_err("超过 MAX_SOURCE_EDGE 必须报错");
    assert!(
        matches!(
            error,
            LoadError::TooLarge {
                width: 9_000,
                height: 4
            }
        ),
        "实际 {error:?}"
    );
    assert!(
        error.describe().contains("too large"),
        "提示要人话：{}",
        error.describe()
    );
    // 上限不许再放宽（r1 的 16384 等于允许 1 GB 的中间缓冲）：常量级约束放在测试里钉。
    const _: () = assert!(super::MAX_SOURCE_EDGE <= 8_192);
    cleanup(&path);
}

/// **复审 r1 P2-1 的诊断面**：小图平铺会被放大（图块步长放大，见 `tiles`），必须说出来。
///
/// 抓得住：短边 < `SMALL_TILE_EDGE` 的图在 tile 档下一定有提示；cover/contain 档不提示；
/// 大图不提示。
/// 抓不住：提示文案在真实界面上的位置（由截图看）。
#[test]
fn small_images_get_a_tile_diagnostic_instead_of_silently_degrading() {
    let small = super::Image::from_rgba((32, 32), 4, 4, vec![0; 64]);
    let note = small_tile_note(&small, BgFill::Tile).expect("小图 tile 必须有提示");
    assert!(note.contains("32×32"), "{note}");
    assert!(note.contains("scaled up"), "{note}");

    // 非 tile 档不提示（cover/contain 会等比铺满，不存在「小图被放大铺」这件事）。
    assert!(small_tile_note(&small, BgFill::Cover).is_none());
    assert!(small_tile_note(&small, BgFill::Contain).is_none());

    // 大图不提示。
    let big = super::Image::from_rgba((1200, 800), 4, 4, vec![0; 64]);
    assert!(small_tile_note(&big, BgFill::Tile).is_none());
}
