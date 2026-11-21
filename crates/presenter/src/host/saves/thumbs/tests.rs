//! `host::saves::thumbs` 的用例：**数解码器调用次数**，不靠"看起来一样"。
//!
//! 每一条都按 D54 的判据写，并且都带一句「抓得住什么 / 抓不住什么」。T27 要求的两条变异都有对应用例：
//!
//! | 变异 | 必红的用例 |
//! |---|---|
//! | 缓存键去掉 mtime（或改成只按路径） | [`a_changed_file_is_decoded_again`]（同一路径、同一大小、只改 mtime ⇒ 必须重解）与 [`a_changed_size_is_decoded_again`] |
//! | 单张 8 MiB 上限改成无上限 | [`a_file_over_the_size_cap_never_reaches_the_decoder`]（解码器调用次数必须为 0） |
//!
//! 每个用例开头都调 [`clear_cache`]：缓存是进程级静态，用例之间不该互相借光（M251 复审 r1）。
//!
//! 抓不住：缩略图**画质**（盒式采样的观感只能靠出图看）；缓存淘汰策略的观感（`MAX_CACHE_ENTRIES`
//! 只有一条「不会无限长」的结构性断言，没有逐条淘汰用例）。

use std::path::{Path, PathBuf};

use super::*;

/// 一张真的 8×6 PNG（143 字节，RGBA，逐像素渐变）。
///
/// 内嵌字节而不是引素材文件：presenter 的 dev-dependencies 里没有图片编码器，而这条用例要的
/// 正是「**真**解码器能读的真 PNG」——手写的 143 个字节比任何 mock 都硬。
const PNG_8X6: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 8, 0, 0, 0, 6, 8, 6, 0,
    0, 0, 254, 5, 223, 251, 0, 0, 0, 86, 73, 68, 65, 84, 120, 218, 21, 202, 33, 1, 195, 64, 12, 0,
    192, 87, 50, 92, 17, 47, 162, 56, 74, 2, 171, 34, 184, 34, 34, 98, 56, 134, 182, 123, 112, 236,
    214, 90, 207, 239, 195, 38, 72, 138, 102, 88, 235, 18, 216, 4, 73, 209, 204, 117, 194, 45, 176,
    9, 146, 162, 153, 251, 132, 20, 216, 4, 73, 209, 76, 158, 240, 10, 108, 130, 164, 104, 230, 61,
    225, 43, 176, 9, 146, 162, 25, 254, 48, 255, 110, 65, 247, 55, 89, 206, 0, 0, 0, 0, 73, 69, 78,
    68, 174, 66, 96, 130,
];

/// 一次性临时目录（本 crate 不引第三方 tempfile）。
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(tag: &str) -> TempDir {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("ull-thumbs-{tag}-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("建临时目录");
    TempDir(path)
}

/// 目录里放一张真 PNG，返回它的字节数。
fn write_png(dir: &Path, file: &str) -> u64 {
    let path = dir.join(file);
    std::fs::write(&path, PNG_8X6).expect("写 PNG");
    std::fs::metadata(&path).expect("读元数据").len()
}

/// 一张「快照里的截图」（`preview` 由被测代码填）。
fn shot(file: &str, bytes: u64, modified_ms: Option<u64>) -> ScreenshotView {
    ScreenshotView {
        file: file.to_string(),
        bytes,
        modified_ms,
        preview: ScreenshotPreview::NotGenerated,
    }
}

/// **判据 ①**：真 PNG 出缩略图（像素 + 宽高 + 长度自洽）。
///
/// 抓得住：解码这条路断了（回 `NotGenerated`）、缩略图边长没按 `THUMBNAIL_MAX_EDGE` 走、
/// `pixels` 长度与宽高对不上（壳层 `from_raw_rgba` 会画出花屏）。
#[test]
fn a_real_png_becomes_a_thumbnail() {
    clear_cache();
    let dir = temp_dir("ready");
    let bytes = write_png(&dir.0, "2024-05-01_10.00.00.png");
    let mut views = [shot(
        "2024-05-01_10.00.00.png",
        bytes,
        Some(1_714_557_600_000),
    )];
    let calls = std::cell::Cell::new(0u32);
    let mut counting = |path: &Path, size: u64| {
        calls.set(calls.get() + 1);
        decode(path, size)
    };
    attach_with(&dir.0, &mut views, &mut counting);

    assert_eq!(calls.get(), 1, "一张新图应当正好进一次解码器");
    let ready = views[0].preview.thumbnail().expect("真 PNG 应当出缩略图");
    assert_eq!((ready.width, ready.height), (8, 6), "8×6 的图不放大");
    assert!(ready.is_consistent(), "像素长度与宽高对不上");
    assert_eq!(&ready.pixels[..4], &[0, 0, 128, 255], "左上角那一格的颜色");
}

/// **判据 ②**：坏文件给**结构化错误**（逐条可见），不 panic、不静默少一张。
#[test]
fn a_broken_file_is_a_visible_reason() {
    clear_cache();
    let dir = temp_dir("broken");
    std::fs::write(dir.0.join("bad.png"), b"\x89PNG\r\n\x1a\n garbage").expect("写坏文件");
    let bytes = std::fs::metadata(dir.0.join("bad.png"))
        .expect("元数据")
        .len();
    let mut views = [shot("bad.png", bytes, Some(1))];
    let calls = std::cell::Cell::new(0u32);
    let mut counting = |path: &Path, size: u64| {
        calls.set(calls.get() + 1);
        decode(path, size)
    };
    attach_with(&dir.0, &mut views, &mut counting);

    assert_eq!(calls.get(), 1, "坏文件也要真的试一次解码（不许提前判死）");
    match &views[0].preview {
        ScreenshotPreview::Failed(failure) => {
            assert_eq!(failure.kind, ScreenshotThumbKind::Decode);
            assert!(!failure.detail.is_empty(), "原因被吞掉了");
        }
        other => panic!("坏 PNG 应当给 Failed，实际 {other:?}"),
    }
}

/// **判据 ③**：超过 8 MiB 的图**不进解码器**，且原因可见（T27 变异的目标）。
///
/// 抓得住：把上限改成「无上限」或把判断挪到解码之后（`calls` 会从 0 变成 1）。
/// 抓不住：8 MiB 这个数字本身合不合理（那是 D54 定的产品取舍）。
#[test]
fn a_file_over_the_size_cap_never_reaches_the_decoder() {
    clear_cache();
    let dir = temp_dir("toolarge");
    let file = "huge.png";
    std::fs::write(dir.0.join(file), vec![0u8; MAX_SOURCE_BYTES as usize + 1]).expect("写大文件");
    let mut views = [shot(file, MAX_SOURCE_BYTES + 1, Some(1))];
    let calls = std::cell::Cell::new(0u32);
    let mut counting = |path: &Path, size: u64| {
        calls.set(calls.get() + 1);
        decode(path, size)
    };
    attach_with(&dir.0, &mut views, &mut counting);

    assert_eq!(calls.get(), 0, "超过 8 MiB 的图一个字节都不该进解码器");
    match &views[0].preview {
        ScreenshotPreview::Failed(failure) => {
            assert_eq!(failure.kind, ScreenshotThumbKind::TooLarge);
            assert!(
                failure.detail.contains(&MAX_SOURCE_BYTES.to_string()),
                "括号里要带上限，实际 {}",
                failure.detail
            );
        }
        other => panic!("超限的图应当给 Failed(TooLarge)，实际 {other:?}"),
    }
}

/// **判据 ④**：第 61 张不解码（按 mtime 倒序）。
///
/// 抓得住：上限被去掉（`calls` 会变 61）、排序反了（最早的那张反而被解、最新的给占位）、
/// 占位被画成失败（`NotGenerated` 与 `Failed` 是两种状态：一个是「没轮到」，一个是「试过但不行」）。
#[test]
fn the_sixty_first_screenshot_is_not_decoded() {
    clear_cache();
    let dir = temp_dir("cap");
    let mut views: Vec<ScreenshotView> = (0..MAX_PER_REFRESH + 1)
        .map(|index| {
            let file = format!("2024-05-01_10.00.{index:02}.png");
            let bytes = write_png(&dir.0, &file);
            // index 越小越新：排序之后 index 0 应当是 rank 0。
            shot(&file, bytes, Some(1_714_557_600_000 - index as u64))
        })
        .collect();
    let calls = std::cell::Cell::new(0u32);
    let mut counting = |path: &Path, size: u64| {
        calls.set(calls.get() + 1);
        decode(path, size)
    };
    attach_with(&dir.0, &mut views, &mut counting);

    assert_eq!(
        calls.get(),
        MAX_PER_REFRESH as u32,
        "只该解前 {MAX_PER_REFRESH} 张"
    );
    assert!(
        views[..MAX_PER_REFRESH]
            .iter()
            .all(|view| view.preview.thumbnail().is_some()),
        "最新的 {MAX_PER_REFRESH} 张都该有缩略图"
    );
    assert_eq!(
        views[MAX_PER_REFRESH].preview,
        ScreenshotPreview::NotGenerated,
        "第 61 张应当是「未生成预览」占位"
    );
}

/// **判据 ⑤**：缓存命中不重复解码（同一 (路径, mtime, 大小) 第二次不进解码器）。
///
/// 抓得住：缓存根本没接上（`calls` 变 2）、键里多加了会让命中失效的东西（例如把「本次调用
/// 序号」混进键）。
#[test]
fn the_second_identical_refresh_does_not_redecode() {
    clear_cache();
    let dir = temp_dir("hit");
    let bytes = write_png(&dir.0, "2024-05-01_10.00.00.png");
    let calls = std::cell::Cell::new(0u32);
    let mut counting = |path: &Path, size: u64| {
        calls.set(calls.get() + 1);
        decode(path, size)
    };
    let mut first = [shot("2024-05-01_10.00.00.png", bytes, Some(7))];
    attach_with(&dir.0, &mut first, &mut counting);
    let mut second = [shot("2024-05-01_10.00.00.png", bytes, Some(7))];
    attach_with(&dir.0, &mut second, &mut counting);

    assert_eq!(calls.get(), 1, "第二次刷新不许再进解码器");
    assert_eq!(first[0].preview, second[0].preview, "两拍的缩略图必须一致");
}

/// **判据 ⑥（T27 的第一条变异靶子）**：文件被覆盖（mtime 变了）必须重解。
///
/// 这是「截图被覆盖后仍显示旧缩略图」那个 bug 的唯一自动化抓手：把 mtime 从缓存键里去掉，
/// 本用例立刻红（`calls` 停在 1）。大小故意保持不变——只改 mtime，才能把「大小也在键里」这条
/// 排除掉。
#[test]
fn a_changed_file_is_decoded_again() {
    clear_cache();
    let dir = temp_dir("mtime");
    let bytes = write_png(&dir.0, "2024-05-01_10.00.00.png");
    let calls = std::cell::Cell::new(0u32);
    let mut counting = |path: &Path, size: u64| {
        calls.set(calls.get() + 1);
        decode(path, size)
    };
    let mut first = [shot("2024-05-01_10.00.00.png", bytes, Some(1_000))];
    attach_with(&dir.0, &mut first, &mut counting);
    assert_eq!(calls.get(), 1);
    // 同名文件被覆盖：**只有 mtime 变了**（大小相同）。
    let mut second = [shot("2024-05-01_10.00.00.png", bytes, Some(2_000))];
    attach_with(&dir.0, &mut second, &mut counting);
    assert_eq!(calls.get(), 2, "mtime 变了就是另一张图，必须重解");
}

/// **判据 ⑦**：同名同刻但**大小变了**（例如用户替换了文件并保了 mtime）也必须重解。
///
/// 抓得住：缓存键只剩路径（`calls` 停在 1）。与用例 ⑥ 一起把三段键的两段独立钉住。
#[test]
fn a_changed_size_is_decoded_again() {
    clear_cache();
    let dir = temp_dir("size");
    let bytes = write_png(&dir.0, "2024-05-01_10.00.00.png");
    let calls = std::cell::Cell::new(0u32);
    let mut counting = |path: &Path, size: u64| {
        calls.set(calls.get() + 1);
        decode(path, size)
    };
    let mut first = [shot("2024-05-01_10.00.00.png", bytes, Some(5))];
    attach_with(&dir.0, &mut first, &mut counting);
    let mut second = [shot("2024-05-01_10.00.00.png", bytes + 1, Some(5))];
    attach_with(&dir.0, &mut second, &mut counting);
    assert_eq!(calls.get(), 2, "大小变了必须重解");
}

/// 空快照不碰缓存、不 panic（新实例的常态）。
#[test]
fn an_empty_list_is_a_no_op() {
    clear_cache();
    let dir = temp_dir("empty");
    let mut views: Vec<ScreenshotView> = Vec::new();
    let calls = std::cell::Cell::new(0u32);
    let mut counting = |path: &Path, size: u64| {
        calls.set(calls.get() + 1);
        decode(path, size)
    };
    attach_with(&dir.0, &mut views, &mut counting);
    assert_eq!(calls.get(), 0);
}

/// 缓存有界：**策略函数单独测**（不往全局缓存里灌 240 条——那会把并行用例的键清掉）。
///
/// 抓得住：淘汰那段被删掉（250 条 → 还是 250 条）；
/// 抓不住：淘汰顺序（这里只留「本拍碰到的」，没有 LRU 语义，也不该有）。
#[test]
fn the_cache_stays_bounded() {
    let mut store: Cache = HashMap::new();
    let touched: Key = ("/x/keep.png".to_string(), Some(1), 10);
    for round in 0..(MAX_CACHE_ENTRIES + 10) {
        store.insert(
            (format!("/x/{round}.png"), Some(round as u64), 10),
            Ok(sample()),
        );
    }
    store.insert(touched.clone(), Ok(sample()));
    assert!(store.len() > MAX_CACHE_ENTRIES);

    prune(&mut store, std::slice::from_ref(&touched));
    assert_eq!(
        store.len(),
        1,
        "超上限之后只该留这一拍碰到的键（实际 {}）",
        store.len()
    );
    assert!(store.contains_key(&touched), "本拍碰到的键被自己清了");

    // 没超上限时一条都不许掉。
    let mut small: Cache = HashMap::new();
    small.insert(touched.clone(), Ok(sample()));
    prune(&mut small, std::slice::from_ref(&touched));
    assert_eq!(small.len(), 1);
}

/// 一张可复用的小缩略图（`tests` 里只用来占位，不看内容）。
fn sample() -> ThumbnailView {
    ThumbnailView {
        width: 1,
        height: 1,
        pixels: vec![0, 0, 0, 255],
    }
}
