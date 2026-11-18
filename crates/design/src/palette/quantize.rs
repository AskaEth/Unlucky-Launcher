//! 像素 → 主色簇：Monet（Material You）取色管线里**量化那一半**（打分那一半在 [`super::score`]）。
//!
//! ## 为什么不是「取平均色」
//!
//! 平均色是两个独立错误的合成：
//!
//! 1. **它不存在的颜色**。平均会把「天空 60% + 树叶 30% + 纯白 10%」混成一块脏灰绿，
//!    这个色值在图上**一个像素都不是**，拿它当种子，用户看到的整套配色跟壁纸不像。
//! 2. **它没有「主色」概念**。一张 95% 是米白、5% 是珊瑚的图，平均色几乎等于米白，
//!    而人眼记住的、能当强调色用的是那 5%。官方 Monet 的立场正好相反：**按面积 + 彩度
//!    挑一个「有人气又有色」的代表色**（`Score`），不是把整体摊平。
//!
//! ## 采用的策略（三层）
//!
//! | 步 | 做什么 | 官方对应 | 等级 |
//! |---|---|---|---|
//! | 1 | 5 bit/通道直方图（32³ = 32768 桶），桶内记像素数与各通道原值和 | `QuantizerWu` 的 `INDEX_BITS = 5` | A |
//! | 2 | 桶当带权点，在 **CIELAB** 里跑 k-means（≤10 轮；簇心用线性 RGB 加权均值更新） | `QuantizerWsmeans`（`MAX_ITERATIONS = 10`、`MIN_DELTA_E = 3.0`） | A（结构）/ B（用直方图桶代替 Wu 的分割盒播种） |
//! | 3 | 每簇的代表色 = 簇内像素最多的那个桶里**像素的原值平均** | `QuantizerCelebi` = 量化 + `Score` | A（管线）/ B（代表色取桶内均值） |
//!
//! 也就是说：本模块 = `QuantizerCelebi` 的形状（直方图量化 + k-means 精修），
//! 与官方的差别在**播种方式**——官方用 `QuantizerWu` 的动态规划分割盒当初始簇心，
//! 这里用「像素数最多的 24 个桶」。差别可测量（见各模块单测：5% 面积的强调色簇不会被
//! 95% 的底色吞掉），换来的是一条可全单测、零依赖、确定性的实现。
//!
//! ## 代表色为什么要「桶内原值平均」而不是桶中心
//!
//! 只取桶中心（5 bit 量化点）会让纯色 `#3C8DBC` 的图取出 `#3B8BBB`——量化误差 1~3/通道。
//! 单看数字无所谓，但这是**用户能看出来**的：「我选的是这个蓝，凭什么给我的强调色差一点？」
//! 桶内原值平均对纯色图精确等于输入色（误差 0），对复杂图就是该桶的真实均值，
//! 仍然不是「全图平均色」（只有一个桶那么多像素参与）。
//!
//! ## 来源等级
//!
//! - **A**：`material-color-utilities`（Apache-2.0）里确实有 `QuantizerWu` / `QuantizerWsmeans` /
//!   `QuantizerCelebi` / `Score` 四个类，且 `INDEX_BITS = 5`、`MAX_ITERATIONS = 10`、
//!   `MIN_DELTA_E = 3.0` 是官方常数——这些是「照抄口径」的部分。
//! - **B**：用「top-N 桶」代替 Wu 的分割盒当播种；代表色取桶内原值均值。
//! - **C**：不做。拿不准的常数一律写成带注释的「我们的选择」，不假装是官方值。

use crate::color::space;
use crate::color::Color;

/// 每通道保留的位数：官方 `QuantizerWu` 用 5 bit/通道。
pub(crate) const INDEX_BITS: u32 = 5;
/// 直方图桶数（32³）。
const BIN_COUNT: usize = 1 << (INDEX_BITS as usize * 3);
/// 每通道的右移量（8 − 5 = 3）。
const SHIFT: u32 = 8 - INDEX_BITS;
/// k-means 最多迭代轮数（官方 `QuantizerWsmeans.MAX_ITERATIONS`）。
const MAX_ITERATIONS: usize = 10;
/// 收敛阈值：簇心在 CIELAB 里移动小于这个距离就停（官方 `MIN_DELTA_E`）。
const MIN_DELTA_E: f64 = 3.0;
/// 最多保留的簇数（喂给打分的候选数量）。官方 `Score` 通常拿到 256 个候选；
/// 24 个足够覆盖「底色 + 两三个强调色簇」，也让色相直方图不被噪声塞满。
pub(crate) const MAX_CLUSTERS: usize = 24;

/// 一个主色簇：图里**真实存在**的颜色 + 被归到它名下的像素数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cluster {
    /// 簇的代表色（簇内像素最多的桶里，像素原值的平均）。
    pub color: Color,
    /// 归到这个簇的采样像素数。
    pub population: u32,
}

/// 一个非空桶。
#[derive(Debug, Clone, Copy)]
struct Bin {
    index: usize,
    color: Color,
    count: u32,
}

/// 把采样像素量化成主色簇（按像素数降序，同数按色值排序 → 结果确定）。
pub(crate) fn quantize(samples: &[Color]) -> Vec<Cluster> {
    let bins = histogram(samples);
    if bins.is_empty() {
        return Vec::new();
    }
    let mut clusters = if bins.len() <= MAX_CLUSTERS {
        bins.iter()
            .map(|bin| Cluster {
                color: bin.color,
                population: bin.count,
            })
            .collect()
    } else {
        kmeans(&bins)
    };
    clusters.sort_by(|a, b| {
        b.population
            .cmp(&a.population)
            .then_with(|| packed(a.color).cmp(&packed(b.color)))
    });
    clusters
}

/// 直方图（按桶下标升序，空的桶不出现）。
fn histogram(samples: &[Color]) -> Vec<Bin> {
    let mut counts = vec![0u32; BIN_COUNT];
    // 每桶各通道的原值和（`u32` 够用：单桶最多 65536 个采样点 × 255 < 2^32）。
    let mut sums = vec![[0u32; 3]; BIN_COUNT];
    for color in samples {
        let index = bin_index(*color);
        counts[index] = counts[index].saturating_add(1);
        for (channel, value) in [color.r, color.g, color.b].into_iter().enumerate() {
            sums[index][channel] = sums[index][channel].saturating_add(u32::from(value));
        }
    }
    counts
        .iter()
        .zip(&sums)
        .enumerate()
        .filter(|(_, (count, _))| **count > 0)
        .map(|(index, (count, sum))| Bin {
            index,
            color: average_color(*count, *sum),
            count: *count,
        })
        .collect()
}

fn average_color(count: u32, sum: [u32; 3]) -> Color {
    let channel = |value: u32| ((f64::from(value) / f64::from(count)).round()) as u8;
    Color::rgb(channel(sum[0]), channel(sum[1]), channel(sum[2]))
}

fn bin_index(color: Color) -> usize {
    (usize::from(color.r >> SHIFT) << (INDEX_BITS * 2))
        | (usize::from(color.g >> SHIFT) << INDEX_BITS)
        | usize::from(color.b >> SHIFT)
}

/// CIELAB 欧氏距离的平方。
fn distance_squared(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dl = a[0] - b[0];
    let da = a[1] - b[1];
    let db = a[2] - b[2];
    dl * dl + da * da + db * db
}

/// 带权 k-means（在 CIELAB 里分配、在线性 RGB 里更新簇心）。
fn kmeans(bins: &[Bin]) -> Vec<Cluster> {
    let mut by_population: Vec<usize> = (0..bins.len()).collect();
    by_population.sort_by(|&a, &b| {
        bins[b]
            .count
            .cmp(&bins[a].count)
            .then_with(|| bins[a].index.cmp(&bins[b].index))
    });
    by_population.truncate(MAX_CLUSTERS);

    let mut centers: Vec<Color> = by_population.iter().map(|&i| bins[i].color).collect();
    let mut assignments = vec![usize::MAX; bins.len()];

    for _ in 0..MAX_ITERATIONS {
        let center_lab: Vec<[f64; 3]> = centers.iter().map(|c| space::lab_from_color(*c)).collect();
        let mut changed = false;
        for (i, bin) in bins.iter().enumerate() {
            let lab = space::lab_from_color(bin.color);
            let mut best = 0;
            let mut best_distance = f64::INFINITY;
            for (k, center) in center_lab.iter().enumerate() {
                let distance = distance_squared(lab, *center);
                if distance < best_distance {
                    best_distance = distance;
                    best = k;
                }
            }
            if assignments[i] != best {
                changed = true;
                assignments[i] = best;
            }
        }

        let mut sums = vec![[0.0_f64; 3]; centers.len()];
        let mut weights = vec![0.0_f64; centers.len()];
        for (i, bin) in bins.iter().enumerate() {
            let weight = f64::from(bin.count);
            let linear = [
                space::linearized(bin.color.r),
                space::linearized(bin.color.g),
                space::linearized(bin.color.b),
            ];
            let k = assignments[i];
            for channel in 0..3 {
                sums[k][channel] += linear[channel] * weight;
            }
            weights[k] += weight;
        }

        let mut shift = 0.0_f64;
        for (k, center) in centers.iter_mut().enumerate() {
            if weights[k] <= 0.0 {
                continue;
            }
            let mean = [
                sums[k][0] / weights[k],
                sums[k][1] / weights[k],
                sums[k][2] / weights[k],
            ];
            let next = space::color_from_linrgb(mean);
            shift = shift.max(distance_squared(space::lab_from_color(next), center_lab[k]).sqrt());
            *center = next;
        }
        if !changed || shift < MIN_DELTA_E {
            break;
        }
    }

    // 代表色取「簇内像素数最多的真实桶」，**不是**簇心均值：均值色往往在图上不存在，
    // 用它当种子会让色卡与壁纸看起来不像（见模块文档第 1 条理由）。
    let mut representatives: Vec<Option<(u32, Color)>> = vec![None; centers.len()];
    for (i, bin) in bins.iter().enumerate() {
        let k = assignments[i];
        let better = match representatives[k] {
            None => true,
            Some((best, _)) => bin.count > best,
        };
        if better {
            representatives[k] = Some((bin.count, bin.color));
        }
    }

    let mut totals = vec![0u32; centers.len()];
    for (i, bin) in bins.iter().enumerate() {
        totals[assignments[i]] = totals[assignments[i]].saturating_add(bin.count);
    }

    let mut clusters: Vec<Cluster> = representatives
        .iter()
        .enumerate()
        .filter_map(|(k, entry)| {
            entry.map(|(_, color)| Cluster {
                color,
                population: totals[k],
            })
        })
        .collect();
    // 不同簇可能选出同一个代表桶（极少见），合并以免重复喂给打分。
    clusters.sort_by_key(|cluster| packed(cluster.color));
    clusters.dedup_by(|a, b| {
        if a.color == b.color {
            b.population = b.population.saturating_add(a.population);
            true
        } else {
            false
        }
    });
    clusters
}

fn packed(color: Color) -> u32 {
    (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bin_index_groups_neighbours() {
        // 同一个 5 bit 桶内的像素共享桶下标（桶宽 8）。
        assert_eq!(
            bin_index(Color::rgb(0x50, 0x80, 0x10)),
            bin_index(Color::rgb(0x57, 0x87, 0x17))
        );
        assert_ne!(
            bin_index(Color::rgb(0x50, 0x80, 0x10)),
            bin_index(Color::rgb(0x60, 0x80, 0x10))
        );
    }

    #[test]
    fn solid_color_keeps_its_exact_value() {
        // 桶内原值平均 → 纯色图取回的种子与输入色**逐通道相等**（量化误差被平均消掉）。
        for hex in ["#3C8DBC", "#C0392B", "#00F0FF", "#123456"] {
            let color = Color::from_hex(hex).expect("合法 hex");
            let clusters = quantize(&[color; 500]);
            assert_eq!(clusters.len(), 1);
            assert_eq!(clusters[0].color, color, "{hex}");
            assert_eq!(clusters[0].population, 500);
        }
    }

    #[test]
    fn empty_input_has_no_cluster() {
        assert!(quantize(&[]).is_empty());
    }

    #[test]
    fn cluster_populations_add_up() {
        let mut samples = Vec::new();
        for i in 0..60 {
            let shade = (i * 4) as u8;
            samples.push(Color::rgb(shade, 40, 200));
            samples.push(Color::rgb(200, shade, 40));
            samples.push(Color::rgb(40, 200, shade));
        }
        let clusters = quantize(&samples);
        let total: u32 = clusters.iter().map(|c| c.population).sum();
        assert_eq!(total, samples.len() as u32);
        assert!(clusters.len() <= MAX_CLUSTERS, "{} 簇", clusters.len());
    }

    #[test]
    fn kmeans_split_is_deterministic() {
        let mut samples = Vec::new();
        for i in 0..90 {
            let shade = (i * 2) as u8;
            samples.push(Color::rgb(shade, 40, 200));
            samples.push(Color::rgb(200, shade, 40));
            samples.push(Color::rgb(40, 200, shade));
        }
        assert_eq!(quantize(&samples), quantize(&samples));
    }

    #[test]
    fn kmeans_keeps_a_small_distinct_cluster() {
        // 3% 面积的红点 + 97% 的蓝底：k-means 不能把红点并进蓝簇。
        let mut samples = vec![Color::rgb(0x20, 0x40, 0xC0); 970];
        samples.extend(vec![Color::rgb(0xD0, 0x30, 0x20); 30]);
        let clusters = quantize(&samples);
        assert!(
            clusters
                .iter()
                .any(|c| c.color.r > 0xB0 && c.color.b < 0x60),
            "{clusters:?}"
        );
        assert!(
            clusters
                .iter()
                .any(|c| c.color.b > 0xA0 && c.color.r < 0x60),
            "{clusters:?}"
        );
    }
}
