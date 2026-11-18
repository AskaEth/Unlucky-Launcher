//! 版本管理页的**三栏几何**：版本类型栏 + 主版本栏 + 列表列。纯函数，不碰 iced 控件树。
//!
//! 这一页是 `PaneKit::Single` 之外的第二种「页内分栏」：外层仍是单列（页头铺满页宽），
//! 正文自己切成三栏。所以它不能复用 [`super::PaneKit`]（那是**页面级**版面决议），
//! 而是在可用宽上再做一次**页内**裁决 —— 本文件就是那次裁决的唯一真源。
//!
//! ## 1. 两个侧栏的宽度
//!
//! | 栏 | 展开 | 收成图标条 | 依据 |
//! |---|---|---|---|
//! | 版本类型栏 | **140** | **64** | 140 = `12b §7` drawer 项「图标 24 + 间距 12 + 8 字符标签」；`Snapshot` 是最长标签（8 字符 ≈ 62px），24+12+62+2×16 内边距 = 130，取 4px 栅格 **140** |
//! | 主版本栏 | **140** | **64** | 同一档宽度：主版本是 `1.21` 这种 3~5 字符的短串，140 里放得下「主版本 + 该组版本数」两行；64 里只放得下版本号本身（40 可用宽够 4 字符 `LabelMedium`） |
//!
//! 64 = `LayoutMetrics::nav_rail_width` 的一半再对齐栅格？**不是**：64 是 4px 栅格上的
//! 「图标 24 + 两侧内边距 2×16 + 余量 8」，比导航轨 88 窄一档（导航轨要放 56 宽的选中药丸，
//! 页内侧栏只要 40 宽的选中条）。
//!
//! ## 2. 四档与各自的门槛（门槛都是**算出来的**）
//!
//! 记 `A` = **页面块宽**（[`super::block::body_width`](super::block::body_width) = 可用宽
//! 封顶 1440，M102 的口径；≤1440 时它就是可用宽，所以下面这张表在 1180/1024/900/700 上不变）。
//!
//! | 档 | 门槛 | 类型栏 | 主版本栏 | 列表列 |
//! |---|---|---|---|---|
//! | `Wide` | `A ≥ 888` | 140 | 140 | `A − 328` |
//! | `Narrow` | `676 ≤ A < 888` | **64** | 140 | `A − 252` |
//! | `Compact` | `600 ≤ A < 676` | 64 | **64** | `A − 176` |
//! | `Chips` | `A < 600` | 0 | 0 | `A` |
//!
//! 三个门槛的来源（`L` = 列表列下限 [`version_list_min_width`] = 行宽上限 **424**）：
//!
//! - `888` = `available_width(1024)`：任务给的产品口径「≫1024 两栏都展开」，换成可用宽就是它；
//! - `676` = `64 + 24 + 140 + 24 + 424`：**类型栏收成图标条之后**三栏仍能让列表停在 424；
//! - `600` = `64 + 24 + 64 + 24 + 424`：两条栏都收成 64 也救不回来时，三栏这件事就不成立了。
//!
//! ## 3. 列表列下限为什么是 424（这条是硬的）
//!
//! 版本行要**单行不折**地放「版本号 + 类型 + 日期 + 安装按钮」。424 是 [`super::measure_cap_px`]
//! ——正文行宽上限（逐行实测 67 字符 ≤72），比这一行真正需要的宽得多：
//! `1.21.9-pre3`（11 字符 `BodyMedium` 14px ≈ 67）+ `Snapshot`（`LabelMedium` 12px ≈ 50）
//! + `2026-09-20`（10 字符 ≈ 52）+ 安装按钮（`control_height` 40 高的 text 按钮 ≈ 76）
//! + 两侧内边距 2×16 + 三处间距 3×12 = **293**。取 424 不是「凑」：它是本仓既有的
//! 行宽红线，列表列比它还窄就等于在 424 的正文列旁边又开了一条更窄的正文列。
//!
//! ## 4. 数值表（报告里贴的是同一张）
//!
//! 页面拿到的宽度 → 可用宽 `A`（扣 rail 88 与 2×24 页边距）→ 档位 → 三列。
//! 「右栏收起 / 展开」列按 M104 的 `content_width`（窗口 − 右栗 − 拖拽缝）计算：
//!
//! | 窗口 | 右栏 | 页面拿到 | `A` | 档 | 类型 | 主版本 | 列表 |
//! |---|---|---|---|---|---|---|---|
//! | 1180 | 无（本分支） | 1180 | 1044 | Wide | 140 | 140 | **716** |
//! | 1180 | 收起 48 | 1132 | 996 | Wide | 140 | 140 | **668** |
//! | 1180 | 展开 320 | 852 | 716 | Narrow | 64 | 140 | **464** |
//! | 1024 | 无 | 1024 | 888 | Wide | 140 | 140 | **560** |
//! | 900 | 无 | 900 | 764 | Narrow | 64 | 140 | **512** |
//! | 880 | 无 | 880 | 744 | Narrow | 64 | 140 | **492** |
//! | 700 | 无 | 700 | 564 | Chips | — | — | **564** |
//!
//! **任务书那句「1180−88−320−140−140 = 492」算错了**：它漏掉 2×24 页边距与 2×24 栏间距
//! （共 96）。按真实链路，1180 展开右栏后页面拿到 **852**，可用宽 **716**，
//! 两条 140 栏 + 两处间距要 328 ⇒ 列表只剩 **388 < 424**，**那正是「一条栏必须收成 64」的
//! 触发条件**；收到 64 之后列表是 **464**（不是 492）。492 这个数在 **880 窗宽**上成立
//! （744 − 252 = 492），两者差 28 就是 1180−852−88−48 里被漏掉的那部分。
//!
//! ## 5. 为什么 Narrow 档收的是**类型栏**而不是主版本栏
//!
//! 两栏都放 3~5 个字符的短串，但可辨性不同：类型栏是**四个固定项**（有图标可认：列表 / 星 /
//! 相机 / 历史），主版本栏放的是**用户正在找的那串版本号**——同一屏里两种 64 栏，先收有图标的那个。
//! `Compact` 档才把主版本栏也收成 64（`1.21` 在 40 可用宽里仍是完整串，不会被截断）。

use unlucky_design::spacing::Space;

use super::{available_width, measure_cap_px, FrameMetrics};

/// 版本类型栏（第一栏）展开宽。
pub const TYPE_RAIL_WIDTH: f32 = 140.0;
/// 主版本栏（第二栏）展开宽。
pub const MAJOR_RAIL_WIDTH: f32 = 140.0;
/// 两条侧栏收成图标条时的宽度。
pub const RAIL_ICON_WIDTH: f32 = 64.0;
/// 栏与栏之间的间距（与两列版面的 `PANES_GAP` 同值：同一屏里不出现两种分栏节奏）。
pub const RAIL_GAP: f32 = Space::X6.dp();

/// 列表列下限 = 行宽上限 **424**（见模块文档 §3）。
pub fn version_list_min_width() -> f32 {
    measure_cap_px()
}

/// `Wide` 档门槛：`available_width(1024)` = **888**（产品口径「≫1024 两栏都展开」）。
pub fn wide_min_available() -> f32 {
    available_width(1024.0)
}

/// `Narrow` 档门槛：类型栏 64 + 间距 + 主版本栏 140 + 间距 + 列表下限 = **676**。
pub fn narrow_min_available() -> f32 {
    RAIL_ICON_WIDTH + RAIL_GAP + MAJOR_RAIL_WIDTH + RAIL_GAP + version_list_min_width()
}

/// `Compact` 档门槛：两条 64 栏 + 两处间距 + 列表下限 = **600**。
pub fn compact_min_available() -> f32 {
    RAIL_ICON_WIDTH + RAIL_GAP + RAIL_ICON_WIDTH + RAIL_GAP + version_list_min_width()
}

/// 版本行里「类型」一格的宽度：最长标签 `Snapshot`（8 字符 × `LabelMedium` 12px ≈ 50）+
/// 一格间距余量，取 4px 栅格 **84**。
pub const LIST_KIND_WIDTH: f32 = 84.0;

/// 版本行里「日期」一格的宽度：ISO 日期 `2026-09-20`（10 字符 ≈ 52）+ 余量，取 **88**。
pub const LIST_DATE_WIDTH: f32 = 88.0;

/// 版本行尾的安装图标按钮格子宽 = 图标按钮边长 **40**（`IconGeometry::side`；命中区 48 由行高 44 补）。
pub const ROW_ICON_BUTTON: f32 = 40.0;

/// 版本行内三处间距（`Space::X2` = 8）：比栏间距小一档，行内节奏不与分栏节奏打架。
pub const ROW_GAP: Space = Space::X2;

/// 版本行的最小内容宽（报告里的推导用它当校验）：
/// `Listitem 最小格 176 + 84 + 88 + 40 + 3×8 = 412 < 424`。
///
/// 412 略小于行宽下限 424，说明「424 下四样仍在同一行」不是估计而是有 12px 余量的结论；
/// 版本号那一格最窄拿到 `424 − 84 − 88 − 40 − 24 = 188`（比推导里取的 176 还宽 12）。
pub fn version_row_min_width() -> f32 {
    176.0 + LIST_KIND_WIDTH + LIST_DATE_WIDTH + ROW_ICON_BUTTON + 3.0 * ROW_GAP.dp()
}

/// 版本管理页此刻用哪一档（四档，见模块文档 §2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionRailTier {
    /// 两栏都展开（`A ≥ 888`）。
    Wide,
    /// 类型栏收成图标条（`676 ≤ A < 888`）。
    Narrow,
    /// 两条栏都收成图标条（`600 ≤ A < 676`）。
    Compact,
    /// 三栏不成立：退回顶部 chips（`A < 600`）。
    Chips,
}

impl VersionRailTier {
    /// 稳定名字（日志 / 测试 / 出图文件名用）。
    pub const fn name(self) -> &'static str {
        match self {
            Self::Wide => "wide",
            Self::Narrow => "narrow",
            Self::Compact => "compact",
            Self::Chips => "chips",
        }
    }

    /// 这一档还画不画页内侧栏。
    pub const fn is_three_column(self) -> bool {
        !matches!(self, Self::Chips)
    }
}

/// 三栏的宽度裁决结果（页面直接消费，不再自己算）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VersionRails {
    /// 档位。
    pub tier: VersionRailTier,
    /// 版本类型栏宽（`Chips` 档为 0）。
    pub type_rail: f32,
    /// 主版本栏宽（`Chips` 档为 0）。
    pub major_rail: f32,
    /// 列表 / 内容列宽。
    pub list: f32,
}

impl VersionRails {
    /// 按可用宽裁决（门槛见模块文档 §2）。
    ///
    /// 降级是**逐档下沉**的，不做「挤一挤」：每一档都保证 `list ≥ `[`version_list_min_width`]，
    /// 除非已经沉到 `Chips`（那时列表就是整条可用宽）。
    pub fn of(available: f32) -> Self {
        let (tier, type_rail, major_rail) = if available >= wide_min_available() {
            (VersionRailTier::Wide, TYPE_RAIL_WIDTH, MAJOR_RAIL_WIDTH)
        } else if available >= narrow_min_available() {
            (VersionRailTier::Narrow, RAIL_ICON_WIDTH, MAJOR_RAIL_WIDTH)
        } else if available >= compact_min_available() {
            (VersionRailTier::Compact, RAIL_ICON_WIDTH, RAIL_ICON_WIDTH)
        } else {
            (VersionRailTier::Chips, 0.0, 0.0)
        };

        let list = if tier.is_three_column() {
            (available - type_rail - RAIL_GAP - major_rail - RAIL_GAP).max(0.0)
        } else {
            available.max(0.0)
        };

        Self {
            tier,
            type_rail,
            major_rail,
            list,
        }
    }

    /// 一屏的版本管理页几何（`metrics` 来自 [`super::frame_metrics`]）。
    ///
    /// **读的是块宽**（[`super::block::body_width`] = pane 宽封顶 1440，M102 的口径），
    /// 不是 `metrics.available`：三栏都长在 `frame::body` 那个块里，超宽窗（≥1576）上
    /// 可用宽 2424 > 块宽 1440，按 available 切会让三列之和超出块、右侧被裁。
    pub fn of_metrics(metrics: FrameMetrics) -> Self {
        Self::of(super::block::body_width(metrics))
    }

    /// 叠上「用户手动折叠了类型栏」这条覆盖。
    ///
    /// 只动第一栏：收窄到 [`RAIL_ICON_WIDTH`]，省下来的宽**原样进列表列**（折叠只会让列表更宽，
    /// 所以不需要再校验一次 424 下限）。`Chips` 档没有第一栏，返回原值。
    #[must_use]
    pub fn with_type_folded(self) -> Self {
        if !self.tier.is_three_column() || self.type_rail <= RAIL_ICON_WIDTH {
            return self;
        }
        Self {
            type_rail: RAIL_ICON_WIDTH,
            list: self.list + (self.type_rail - RAIL_ICON_WIDTH),
            ..self
        }
    }
}

#[cfg(test)]
mod tests;
