//! 一次配色的「结果解释」：最终用了哪一级、前几级为什么没用、对比度被改了什么。
//!
//! 这些类型是**给 UI 看的契约**（[`Provenance::hint_key`] 的键 → 文案的映射见
//! `DevDocs/SubAgentsTalk/palette-source-contract.md`），所以刻意做得比内部需要更细：
//! 每一种失败都带足以写出一句具体提示的数据（彩度是多少、图有多小、哪一级坏了）。
//!
//! 与 [`super`] 拆开只为单文件 ≤400 行；对外路径仍是 `palette::resolve::*`。

use super::super::card::CardKey;
use super::super::seed::{Failure, Seed};
use super::super::wcag::Adjustment;
use crate::color::{Color, SchemeFlavor};

/// 方案的来源层级（D38 的三级）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    /// ① 启动器背景图取色。
    BackgroundImage,
    /// ② 桌面壁纸取色。
    Wallpaper,
    /// ③ 内置色卡。
    Preset,
}

impl Level {
    /// 稳定码（日志/设置里用，别用 `Debug` 输出）。
    pub const fn code(self) -> &'static str {
        match self {
            Level::BackgroundImage => "background-image",
            Level::Wallpaper => "wallpaper",
            Level::Preset => "preset",
        }
    }

    /// 在本模块内部当索引用（0 = ①）。
    ///
    /// `pub(super)`：`resolve` 的编排函数要按下标填 `Provenance` 的三个格子，
    /// 但它不该出现在对外 API 里（外部只该问「哪一级」，不该拿索引当协议）。
    pub(super) const fn index(self) -> usize {
        match self {
            Level::BackgroundImage => 0,
            Level::Wallpaper => 1,
            Level::Preset => 2,
        }
    }
}

/// 某一级的处置结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    /// 用了这一级。
    Used,
    /// 用户没提供这一级（没设背景图 / 没开莫奈取色）。
    ///
    /// 注意与 [`Outcome::NotReached`] 的区别：`Absent` 是「用户没有这一级的输入」，
    /// 而「用户提供了、但更高级已经成功所以没轮到」是 `NotReached`。两者对 UI 的含义完全不同——
    /// 把后者说成「未选择」，用户明明选了卡却被告知「色卡：未选择」。
    Absent,
    /// 有输入、但因为更高级已经成功，**没轮到**这一级。
    ///
    /// 只可能出现在低优先级那一侧：背景图成功 → 壁纸与色卡都是 `NotReached`。
    /// 色卡即使 `NotReached` 也可能参与生成（它的中性彩度决定表面色温），所以 UI 要看
    /// [`Provenance::card`] 而不是把 `NotReached` 当成「没用它」。
    NotReached,
    /// 有输入但不合格，已跳过。
    Failed(Skip),
}

/// 「有输入但不合格」的具体原因——每一条都要能变成一句人话。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Skip {
    /// 解码失败（原因在适配层，这里只知道失败）。
    Undecodable,
    /// 图太小（< 4 像素）。
    TooSmall {
        /// 宽。
        width: u32,
        /// 高。
        height: u32,
    },
    /// 像素缓冲长度与宽高不符（调用方给错了数据）。
    BadDimensions {
        /// 期望字节数。
        expected: usize,
        /// 实际字节数。
        actual: usize,
    },
    /// 全是透明像素。
    NoOpaquePixels,
    /// 彩度不足（D40：`< MIN_SEED_CHROMA`），硬取色会凭空上色。
    LowChroma {
        /// 实测彩度。
        chroma: f64,
        /// 最接近的候选色（提示文案里显示成色块）。
        color: Color,
    },
    /// 取色成功但对比度收敛后仍不达标。
    Contrast {
        /// 仍不达标的对数。
        failures: usize,
    },
}

impl Skip {
    /// 稳定原因码（`Provenance::hint_key` 的后半段）。
    pub const fn code(self) -> &'static str {
        match self {
            Skip::Undecodable => "undecodable",
            Skip::TooSmall { .. } => "too-small",
            Skip::BadDimensions { .. } => "bad-dimensions",
            Skip::NoOpaquePixels => "no-opaque-pixels",
            Skip::LowChroma { .. } => "low-chroma",
            Skip::Contrast { .. } => "contrast",
        }
    }
}

impl From<Failure> for Skip {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::BadDimensions {
                expected, actual, ..
            } => Skip::BadDimensions { expected, actual },
            Failure::TooSmall { width, height } => Skip::TooSmall { width, height },
            Failure::NoOpaquePixels => Skip::NoOpaquePixels,
            Failure::LowChroma { color, chroma } => Skip::LowChroma { color, chroma },
        }
    }
}

/// 这次配色「最后用了什么、前几级为什么没用」。
#[derive(Debug, Clone, PartialEq)]
pub struct Provenance {
    /// 最终生效的层级。
    pub level: Level,
    /// 当前生效的色卡（取色成功时它仍然参与：中性彩度来自它）。
    pub card: CardKey,
    /// 取色成功时的种子（色卡档为 `None`）。
    pub seed: Option<Seed>,
    /// 深浅模式。
    pub flavor: SchemeFlavor,
    /// 对比度收敛改了哪些角色（空 = 没动）。
    pub adjustments: Vec<Adjustment>,
    /// 三级各自的处置结果（`pub(super)`：只能由 `resolve::resolve_source` 填，
    /// 外部一律通过 [`Provenance::outcome`] / [`Provenance::background`] 等访问器读）。
    pub(super) outcomes: [Outcome; 3],
}

impl Provenance {
    /// 某一级发生了什么。
    pub fn outcome(&self, level: Level) -> Outcome {
        self.outcomes[level.index()]
    }

    /// 背景图那一级。
    pub fn background(&self) -> Outcome {
        self.outcome(Level::BackgroundImage)
    }

    /// 壁纸那一级。
    pub fn wallpaper(&self) -> Outcome {
        self.outcome(Level::Wallpaper)
    }

    /// 色卡那一级。
    pub fn preset(&self) -> Outcome {
        self.outcome(Level::Preset)
    }

    /// 这次配色是否发生过回落（用了 `level` 以下的层级）。
    pub fn did_fall_back(&self) -> bool {
        (0..self.level.index()).any(|i| !matches!(self.outcomes[i], Outcome::Absent))
    }

    /// 第一个「有输入但不合格」的层级（提示文案优先说它）。
    pub fn first_failure(&self) -> Option<(Level, Skip)> {
        const LEVELS: [Level; 3] = [Level::BackgroundImage, Level::Wallpaper, Level::Preset];
        LEVELS
            .into_iter()
            .find_map(|level| match self.outcome(level) {
                Outcome::Failed(skip) => Some((level, skip)),
                _ => None,
            })
    }

    /// UI 提示键（`None` = 无需提示）。
    ///
    /// 形如 `palette.hint.background.low-chroma`，`key` 稳定、可持久化进日志；
    /// 本 crate 不产出人话文案（文案属于 UI 层，D40 的分层纪律），UI 只做查表：
    ///
    /// | 键 | 建议文案（`{card}` = `provenance.card.name()`） |
    /// |---|---|
    /// | `palette.hint.background.undecodable` | 背景图读取失败，已改用{card}配色 |
    /// | `palette.hint.background.too-small` | 背景图太小，无法取色，已改用{card}配色 |
    /// | `palette.hint.background.bad-dimensions` | 背景图数据异常，已改用{card}配色 |
    /// | `palette.hint.background.no-opaque-pixels` | 背景图几乎全透明，无法取色，已改用{card}配色 |
    /// | `palette.hint.background.low-chroma` | 背景图颜色太淡，取不出主题色，已改用{card}配色 |
    /// | `palette.hint.background.contrast` | 背景图配色达不到可读性要求，已改用{card}配色 |
    /// | `palette.hint.wallpaper.*` | 同上，把「背景图」换成「桌面壁纸」 |
    /// | `palette.hint.preset.contrast` | 当前配色可读性不足，已自动微调 |
    /// | `palette.hint.adjusted` | 为可读性微调了少量颜色（`adjustments` 非空但无失败） |
    ///
    /// 完整契约（含可选补充提示、持久化纪律、性能账）见
    /// `DevDocs/SubAgentsTalk/palette-source-contract.md`（该目录是本地文档，不入库）。
    ///
    /// 优先级：① 先解释「为什么回落」（第一个失败的层级）→ ② 再说「微调过对比度」。
    /// 前者对用户更重要：用户看到的是「我的壁纸怎么没生效」，而不是「颜色被调了 2%」。
    pub fn hint_key(&self) -> Option<&'static str> {
        if let Some((level, skip)) = self.first_failure() {
            return Some(match (level, skip) {
                (Level::BackgroundImage, Skip::Undecodable) => {
                    "palette.hint.background.undecodable"
                }
                (Level::BackgroundImage, Skip::TooSmall { .. }) => {
                    "palette.hint.background.too-small"
                }
                (Level::BackgroundImage, Skip::BadDimensions { .. }) => {
                    "palette.hint.background.bad-dimensions"
                }
                (Level::BackgroundImage, Skip::NoOpaquePixels) => {
                    "palette.hint.background.no-opaque-pixels"
                }
                (Level::BackgroundImage, Skip::LowChroma { .. }) => {
                    "palette.hint.background.low-chroma"
                }
                (Level::BackgroundImage, Skip::Contrast { .. }) => {
                    "palette.hint.background.contrast"
                }
                (Level::Wallpaper, Skip::Undecodable) => "palette.hint.wallpaper.undecodable",
                (Level::Wallpaper, Skip::TooSmall { .. }) => "palette.hint.wallpaper.too-small",
                (Level::Wallpaper, Skip::BadDimensions { .. }) => {
                    "palette.hint.wallpaper.bad-dimensions"
                }
                (Level::Wallpaper, Skip::NoOpaquePixels) => {
                    "palette.hint.wallpaper.no-opaque-pixels"
                }
                (Level::Wallpaper, Skip::LowChroma { .. }) => "palette.hint.wallpaper.low-chroma",
                (Level::Wallpaper, Skip::Contrast { .. }) => "palette.hint.wallpaper.contrast",
                (Level::Preset, Skip::Contrast { .. }) => "palette.hint.preset.contrast",
                // 色卡档不存在「解码/彩度」问题；落到这里说明是将来新增的原因，别乱编键。
                (Level::Preset, _) => "palette.hint.preset.unavailable",
            });
        }
        if !self.adjustments.is_empty() {
            return Some("palette.hint.adjusted");
        }
        None
    }

    /// 原因码列表（`[级别码, 原因码]`），写日志/上报用；给不了键的细节都在这里。
    pub fn reason_codes(&self) -> Vec<(&'static str, &'static str)> {
        const LEVELS: [Level; 3] = [Level::BackgroundImage, Level::Wallpaper, Level::Preset];
        LEVELS
            .into_iter()
            .filter_map(|level| match self.outcome(level) {
                Outcome::Failed(skip) => Some((level.code(), skip.code())),
                _ => None,
            })
            .collect()
    }
}
