//! 渲染层几何断言（D30 / M68 的教训：**纯函数对 ≠ 图画对**）。
//!
//! ## 为什么这个文件在 `inputs/` 而不是 `gallery/probe.rs`
//!
//! 像素工具 `gallery::scan` 是 `mod scan;`（**私有**于 `gallery`），我的 probe 拿不到它，
//! 而 `gallery/mod.rs` 不在本 mission 的 scope 里。所以这里自带一份最小的读图器
//! （约 40 行：读 PNG / 取点 / 连通块 / 行覆盖），判据全在本文件——已作为 finding 报给 tower
//! （建议把 `gallery::scan` 提升为 `pub(crate)`，四个家族就能共用一份）。
//!
//! ## 怎么跑（worktree 根目录）
//!
//! ```text
//! # ① 出图：副本工程只把 inputs 段提到首位（组件代码逐字节相同），见 target/m77/sync.py
//! python target/m77/sync.py && (cd /d/tmp/m77-shot && cargo build --no-default-features ...)
//! M77_PART=text ./target/debug/unlucky-launcher-iced.exe --software-renderer --screen=gallery \
//!     --screenshot target/m77/text-light.png --screenshot-theme=light
//! # ② 量图
//! ULL_INPUTS_DIR=target/m77 cargo test -p unlucky-app-iced inputs::probe
//! ```
//!
//! 没给 `ULL_INPUTS_DIR` 就**大声跳过**（不静默通过），与 `gallery::probe` 的口径一致。
//!
//! ## 量什么（每条都对着一个「已经可能画错」的形态）
//!
//! | 断言 | 抓的错误 |
//! |---|---|
//! | 文本框高 **56**、宽 **320** | 高度被内容/内边距吃掉（不是 56 的 A 级值） |
//! | Filled 底部指示线 静息 **1.0** / 聚焦 **2.0** 墨量 | 把 2dp 画成 1dp（或反过来） |
//! | Outlined 描边 静息 **1.0** / 聚焦 **2.0** 墨量 | 同上 |
//! | 错误态：标签 / 线 / 辅助文本全 `Error` | 只把线染红、忘了标签与辅助文本 |
//! | 搜索条高 **56**、图标墨迹偏移 **20**（= 16 + viewBox 4） | 手抄 16/20 当图标边、或 pad 与 offset 少加一段 |
//! | docked 面板 宽 **360** / 头 **56** / 分隔线 **1.0 `Outline`** / 体 **240** | 用 `OutlineVariant`、或头部高度与 bar 不一致 |
//! | Slider 0%（竖条贴左）、50%（分界在中央）、100%（无未激活段） | 位移区间取错（0..W 而不是 handle/2..W−handle/2） |
//! | Slider 竖条 **4×44** | 画成圆点或撑满命中区 |
//! | 菜单项 pitch **48**、选中指示器内缩 **4**、容器高 **161** | 项高/内边距抄错 |
//! | 分组分隔线 **1.0** 墨量 | 分隔线变成 2px |

use std::path::{Path, PathBuf};

use unlucky_design::color::{Role, SchemeFlavor};
use unlucky_design::typography::Platform;

use crate::theme::{Families, Ui};

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 颜色比对容差（抗锯齿/取整）。
const TOLERANCE: u8 = 10;

type Rgb = [u8; 3];

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(flavor, Platform::Windows, FAMILIES)
}

fn rgb(color: iced::Color) -> Rgb {
    [
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8,
    ]
}

fn matches(pixel: Rgb, expected: Rgb) -> bool {
    matches_within(pixel, expected, TOLERANCE)
}

/// 指定容差的同色判断（量**阴影边**时要用更严的容差：阴影叠上去会落在 10 的容差内）。
fn matches_within(pixel: Rgb, expected: Rgb, tolerance: u8) -> bool {
    pixel
        .iter()
        .zip(expected)
        .all(|(a, b)| a.abs_diff(b) <= tolerance)
}

/// 一张图的最小访问器（读 PNG / 取点 / 连通块）。
struct Shot {
    width: usize,
    height: usize,
    pixels: Vec<Rgb>,
}

/// 一张图里某颜色的连通块外接框（闭区间）。
#[derive(Debug, Clone, Copy)]
struct BBox {
    left: usize,
    right: usize,
    top: usize,
    bottom: usize,
}

impl BBox {
    fn width(&self) -> usize {
        self.right - self.left + 1
    }

    fn height(&self) -> usize {
        self.bottom - self.top + 1
    }
}

impl Shot {
    fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|error| format!("读不动 {path:?}：{error}"))?;
        let image = image::load_from_memory(&bytes)
            .map_err(|error| format!("解 PNG 失败：{error}"))?
            .to_rgb8();
        let (width, height) = (image.width() as usize, image.height() as usize);
        Ok(Self {
            width,
            height,
            pixels: image.pixels().map(|p| p.0).collect(),
        })
    }

    fn at(&self, x: usize, y: usize) -> Rgb {
        self.pixels[y * self.width + x]
    }

    fn bbox_of(&self, color: Rgb, min_area: usize) -> Vec<BBox> {
        let mut seen = vec![false; self.width * self.height];
        let mut boxes = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if seen[y * self.width + x] || !matches(self.at(x, y), color) {
                    continue;
                }
                let mut stack = vec![(x, y)];
                seen[y * self.width + x] = true;
                let mut bbox = BBox {
                    left: x,
                    right: x,
                    top: y,
                    bottom: y,
                };
                while let Some((cx, cy)) = stack.pop() {
                    bbox.left = bbox.left.min(cx);
                    bbox.right = bbox.right.max(cx);
                    bbox.top = bbox.top.min(cy);
                    bbox.bottom = bbox.bottom.max(cy);
                    for (nx, ny) in [
                        (cx + 1, cy),
                        (cx.saturating_sub(1), cy),
                        (cx, cy + 1),
                        (cx, cy.saturating_sub(1)),
                    ] {
                        if nx < self.width
                            && ny < self.height
                            && !seen[ny * self.width + nx]
                            && matches(self.at(nx, ny), color)
                        {
                            seen[ny * self.width + nx] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
                if bbox.width() * bbox.height() >= min_area {
                    boxes.push(bbox);
                }
            }
        }
        boxes
    }

    fn count_in(&self, color: Rgb, x0: usize, x1: usize, y0: usize, y1: usize) -> usize {
        let mut total = 0;
        for y in y0..y1.min(self.height) {
            for x in x0..x1.min(self.width) {
                if matches(self.at(x, y), color) {
                    total += 1;
                }
            }
        }
        total
    }
}

/// 一行像素相对底色向 `line` 靠拢的比例（0..=1）——1dp 的线在各行上摊开时总和 ≈ 1.0。
fn coverage(pixel: Rgb, surface: Rgb, line: Rgb) -> f32 {
    let channel = |i: usize| {
        let full = f32::from(line[i]) - f32::from(surface[i]);
        if full.abs() < 1.0 {
            return 0.0;
        }
        ((f32::from(pixel[i]) - f32::from(surface[i])) / full).clamp(0.0, 1.0)
    };
    (channel(0) + channel(1) + channel(2)) / 3.0
}

/// 竖直方向上的**线墨量**：`y0..y1` 每行在 x 区间上的平均覆盖之和（1dp ⇒ ≈1.0）。
fn line_ink(
    shot: &Shot,
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
    surface: Rgb,
    line: Rgb,
) -> f32 {
    let mut total = 0.0;
    for y in y0..y1.min(shot.height) {
        let mut sum = 0.0;
        for x in x0..x1.min(shot.width) {
            sum += coverage(shot.at(x, y), surface, line);
        }
        total += sum / (x1 - x0) as f32;
    }
    total
}

/// 某一行里「两色之一」的最长连续段 `(start, end)`，允许 ≤`max_gap` 的空隙。
///
/// 用它而不是「本行最左/最右」：深色档的说明文字里有零星像素落在令牌色容差内，
/// 取 min/max 会把轨道量成从 x=24 起（实测）。
fn longest_run(shot: &Shot, y: usize, a: Rgb, b: Rgb, max_gap: usize) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut start: Option<usize> = None;
    let mut last: Option<usize> = None;
    for x in 0..shot.width {
        if matches(shot.at(x, y), a) || matches(shot.at(x, y), b) {
            if start.is_none() {
                start = Some(x);
            }
            last = Some(x);
        } else if let (Some(from), Some(to)) = (start, last) {
            if x - to > max_gap {
                if best.is_none_or(|(p, q)| to - from > q - p) {
                    best = Some((from, to));
                }
                start = None;
                last = None;
            }
        }
    }
    if let (Some(from), Some(to)) = (start, last) {
        if best.is_none_or(|(p, q)| to - from > q - p) {
            best = Some((from, to));
        }
    }
    best
}

/// `bbox` 范围内「整行至少 `min_pixels` 个同色像素」的行区间（闭区间）。
///
/// 菜单的阴影会把 bbox 外扩 ~5px，而阴影行绝不会有 100 个容器色像素——用它把阴影滤掉。
fn row_extent(shot: &Shot, bbox: &BBox, color: Rgb, min_pixels: usize) -> (usize, usize) {
    let mut first = None;
    let mut last = None;
    for y in bbox.top..=bbox.bottom {
        let mut count = 0;
        for x in bbox.left..=bbox.right {
            // 容差收到 2：**阴影边**会落在默认容差 10 内，会把容器量高 ~5px
            if matches_within(shot.at(x, y), color, 2) {
                count += 1;
            }
        }
        if count >= min_pixels {
            first.get_or_insert(y);
            last = Some(y);
        }
    }
    (
        first.expect("行内没有足够的同色像素"),
        last.expect("行内没有足够的同色像素"),
    )
}

/// `pixel` 更接近 `fill` 还是 `surface`（含平局都算 `fill`）。
fn nearer(pixel: Rgb, fill: Rgb, surface: Rgb) -> bool {
    let dist = |a: Rgb, b: Rgb| -> u32 {
        a.iter()
            .zip(b)
            .map(|(x, y)| (i32::from(*x) - i32::from(y)).unsigned_abs())
            .sum()
    };
    dist(pixel, fill) <= dist(pixel, surface)
}

/// 截图目录（`ULL_INPUTS_DIR`）；没给就返回 `None`（调用方大声跳过）。
fn shot_dir() -> Option<PathBuf> {
    let raw = std::env::var_os("ULL_INPUTS_DIR")?;
    let path = PathBuf::from(raw);
    if path.is_absolute() || path.is_dir() {
        return Some(path);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Some(root.join(path))
}

/// 取一张图；缺图直接 panic（给了目录就说明这轮要认真量）。
fn shot(dir: &Path, part: &str, flavor: SchemeFlavor) -> Shot {
    let slug = match flavor {
        SchemeFlavor::Light => "light",
        SchemeFlavor::Dark => "dark",
    };
    let path = dir.join(format!("{part}-{slug}.png"));
    let shot = Shot::load(&path).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        (shot.width, shot.height),
        (1180, 760),
        "验收图必须是 D30 的 1180×760 @100% DPI"
    );
    shot
}

/// 一个矩阵格在某列的竖直「实色高度」：从 `bbox.top-4` 往上/下走到不再接近 `fill` 为止。
fn vertical_extent(
    shot: &Shot,
    bbox: &BBox,
    fill: Rgb,
    surface: Rgb,
    column: usize,
) -> (usize, usize) {
    let mut top = bbox.top;
    while top > 0 && nearer(shot.at(column, top - 1), fill, surface) {
        top -= 1;
    }
    let mut bottom = bbox.bottom;
    while bottom + 1 < shot.height && nearer(shot.at(column, bottom + 1), fill, surface) {
        bottom += 1;
    }
    (top, bottom)
}

macro_rules! require_dir {
    () => {
        match shot_dir() {
            Some(dir) => dir,
            None => {
                eprintln!(
                    "[inputs::probe] SKIPPED：未设置 ULL_INPUTS_DIR。这是**渲染层**断言，\
                     需要先按本文件模块文档出 8 张图（target/m77/*.png）。"
                );
                return;
            }
        }
    };
}

mod menu;
mod search;
mod slider;
mod text;
