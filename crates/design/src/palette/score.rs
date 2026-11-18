//! 主色簇 → 排序后的候选种子：官方 `Score` 的口径。
//!
//! ## 官方怎么挑「最好的那个颜色」
//!
//! `Score` 不是「像素最多的颜色」，而是三件事的加权：
//!
//! 1. **有人气**：颜色所在**色相桶**（1 度一桶）占全图的像素比例要够大
//!    （`CUTOFF_EXCITED_PROPORTION = 1%` 以下的色相直接出局）。用色相桶而不是单色占比，
//!    是为了把「同一个色相的一族明暗」合成人气，而不是让它们互相拆票。
//! 2. **有颜色**：`CUTOFF_CHROMA = 5` 以下的颜色出局——灰、白、黑再多也不该当种子。
//! 3. **彩度越高越优先**，但**封顶**：`TARGET_CHROMA = 48` 以上不再加分（否则一张荧光图会被
//!    推到色域边缘，整套配色的 on-color 会难看得像报警灯）。
//!
//! 打分式（官方形状）：
//!
//! ```text
//! excited = 该色所在色相桶的像素占比
//! chroma_weight = clamp((chroma − 5) / (48 − 5), 0, 1)
//! score = 0.7 × excited + 0.3 × chroma_weight
//! ```
//!
//! ## 来源等级
//!
//! - **A**：四个常数（`CUTOFF_CHROMA = 5`、`CUTOFF_EXCITED_PROPORTION = 0.01`、
//!   `TARGET_CHROMA = 48`、`WEIGHT_PROPORTION/WEIGHT_CHROMA = 0.7/0.3`）与「按色相桶计人气」
//!   的做法都来自 `material-color-utilities` 的 `score/Score.java`。
//! - **B**：把 `excited` 直接当 `proportion` 项用（官方的 `Score` 内部还有一层
//!   `hueOffset`/`huePopulation` 的除法归一，口径相同、写法不同）。本实现的选择是
//!   「只看色相桶占比」，因为它是能全单测的最简形式，且与官方在样例图上选出同一个色相。
//!
//! 注意与 D40 的彩度门槛（[`super::MIN_SEED_CHROMA`] = 8）的分工：
//! **这里的 5 是「要不要把它当候选」，那里的 8 是「这个结果够不够格当种子」**。
//! 一张偏灰的图会有一堆彩度 5~7 的候选通过 `Score`，然后被 8 的门槛挡下并回落到色卡——
//! 这正是 D40 要的行为（宁可回落，不许凭空上色）。

use super::quantize::Cluster;
use crate::color::Hct;

/// 彩度下限：低于它的颜色不参与打分（官方 `Score.CUTOFF_CHROMA`）。
pub(crate) const CUTOFF_CHROMA: f64 = 5.0;
/// 色相桶人气下限（官方 `Score.CUTOFF_EXCITED_PROPORTION`）。
pub(crate) const CUTOFF_EXCITED_PROPORTION: f64 = 0.01;
/// 彩度加分封顶（官方 `Score.TARGET_CHROMA`）。
pub(crate) const TARGET_CHROMA: f64 = 48.0;
/// 人气项的权重（官方 `Score.WEIGHT_PROPORTION`）。
const WEIGHT_PROPORTION: f64 = 0.7;
/// 彩度项的权重（官方 `Score.WEIGHT_CHROMA`）。
const WEIGHT_CHROMA: f64 = 0.3;

/// 一个候选种子（含打分所需的中间量，方便测试与提示文案）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ranked {
    /// 候选簇。
    pub cluster: Cluster,
    /// 全图像素占比（0..1）。
    pub share: f64,
    /// 所在色相桶的像素占比（0..1，官方所谓 excited proportion）。
    pub excited: f64,
    /// CAM16 彩度。
    pub chroma: f64,
    /// CAM16 色相角。
    pub hue: f64,
    /// 最终打分（越大越优先）。
    pub score: f64,
}

/// 按官方 `Score` 口径排序候选；被过滤掉的簇不出现在结果里。
pub(crate) fn rank(clusters: &[Cluster]) -> Vec<Ranked> {
    let total: u64 = clusters.iter().map(|c| u64::from(c.population)).sum();
    if total == 0 {
        return Vec::new();
    }

    let hcts: Vec<Hct> = clusters.iter().map(|c| Hct::from_color(c.color)).collect();
    let mut hue_population = [0u64; 360];
    for (cluster, hct) in clusters.iter().zip(&hcts) {
        hue_population[hue_bucket(hct.hue)] += u64::from(cluster.population);
    }

    let mut ranked: Vec<Ranked> = clusters
        .iter()
        .zip(&hcts)
        .filter_map(|(cluster, hct)| {
            if hct.chroma < CUTOFF_CHROMA {
                return None;
            }
            let share = f64::from(cluster.population) / total as f64;
            let excited = hue_population[hue_bucket(hct.hue)] as f64 / total as f64;
            if excited < CUTOFF_EXCITED_PROPORTION {
                return None;
            }
            let chroma_weight =
                ((hct.chroma - CUTOFF_CHROMA) / (TARGET_CHROMA - CUTOFF_CHROMA)).clamp(0.0, 1.0);
            Some(Ranked {
                cluster: *cluster,
                share,
                excited,
                chroma: hct.chroma,
                hue: hct.hue,
                score: WEIGHT_PROPORTION * excited + WEIGHT_CHROMA * chroma_weight,
            })
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            // 同分时像素多的优先，再同则按色值定序 → 结果完全确定。
            .then_with(|| b.cluster.population.cmp(&a.cluster.population))
            .then_with(|| a.cluster.color.r.cmp(&b.cluster.color.r))
            .then_with(|| a.cluster.color.g.cmp(&b.cluster.color.g))
            .then_with(|| a.cluster.color.b.cmp(&b.cluster.color.b))
    });
    ranked
}

/// 彩度最高的那个簇——被 `Score` 全部过滤时用它报「这张图最鲜艳的颜色」。
///
/// ⚠️ 它**不是**「最接近及格的那一族」：`Score` 的过滤有两道（彩度 ≥5、色相桶人气 ≥1%），
/// 面积不足 1% 的高彩度颜料会被**人气**刷掉——例如 1592 个米白 + 8 个纯红的图，
/// 候选为空而这里的「最鲜艳」彩度是 **113.36**。所以调用方的文案要按
/// 「全图最鲜艳的那一族」，不是「差一点就及格的那一族」。
///
/// 取色失败（`Failure::LowChroma`）时**色值与彩度都从这一个簇取**，保证二者成对：
/// 之前的两条路径一条报 `Score` 榜首、一条报「彩度最高」，而后者还拿着**另一个**簇的色值，
/// 于是同一个 `LowChroma` 里的 `color` 与 `chroma` 可能不是一回事（复审 r1 p2-4.2）。
///
/// 同彩度时取像素多的那一个（结果确定，不受簇顺序影响）。
pub(crate) fn most_chromatic(clusters: &[Cluster]) -> Option<&Cluster> {
    clusters.iter().max_by(|a, b| {
        let key = |cluster: &Cluster| (Hct::from_color(cluster.color).chroma, cluster.population);
        let (ca, pa) = key(a);
        let (cb, pb) = key(b);
        ca.partial_cmp(&cb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| pa.cmp(&pb))
    })
}

fn hue_bucket(hue: f64) -> usize {
    let wrapped = hue.rem_euclid(360.0);
    (wrapped.round() as usize) % 360
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    fn cluster(hex: &str, population: u32) -> Cluster {
        Cluster {
            color: Color::from_hex(hex).expect("合法 hex"),
            population,
        }
    }

    #[test]
    fn accent_with_small_area_but_high_chroma_transfers_to_seed() {
        // 95% 米白 + 5% 珊瑚：米白被彩度门槛挡掉，珊瑚当种子。
        let clusters = [cluster("#F5F1EA", 950), cluster("#FF6B5A", 50)];
        let ranked = rank(&clusters);
        assert_eq!(ranked.len(), 1, "{ranked:?}");
        assert_eq!(ranked[0].cluster.color.to_hex(), "#FF6B5A");
    }

    #[test]
    fn grayscale_clusters_are_all_filtered() {
        let clusters = [
            cluster("#FFFFFF", 1000),
            cluster("#808080", 400),
            cluster("#0A0A0A", 300),
        ];
        assert!(rank(&clusters).is_empty());
        let most = most_chromatic(&clusters).expect("灰图也有簇");
        assert!(Hct::from_color(most.color).chroma < CUTOFF_CHROMA);
    }

    #[test]
    fn tie_break_is_deterministic() {
        let clusters = [cluster("#3366CC", 100), cluster("#2B6DD0", 100)];
        let first = rank(&clusters);
        let second = rank(&clusters);
        assert_eq!(first.len(), 2);
        assert_eq!(
            first.iter().map(|r| r.cluster.color).collect::<Vec<_>>(),
            second.iter().map(|r| r.cluster.color).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tiny_population_hue_is_ignored() {
        // 0.5% 的青绿被 excited proportion 门槛（1%）挡掉。
        let clusters = [cluster("#FF0000", 995), cluster("#00C8B4", 5)];
        let ranked = rank(&clusters);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].cluster.color.to_hex(), "#FF0000");
    }

    #[test]
    fn hue_bucket_wraps() {
        assert_eq!(hue_bucket(0.0), 0);
        assert_eq!(hue_bucket(359.6), 0);
        assert_eq!(hue_bucket(-1.0), 359);
        assert_eq!(hue_bucket(720.4), 0);
    }
}
