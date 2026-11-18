//! 规格卡字号 → 官方 type scale 的**校核**（不是又一张字号表）。
//!
//! 规格卡上每一处**文字**都写了 `font-size`，去重共 11 档：
//! 40 / 32 / 24 / 16 / 15 / 14 / 13 / 12 / 11 / 10 / 9。
//! 其中一部分**命中** M3 的 15 档（14 / 16 / 24 / 32 / 12 / 11），一部分**落在两档之间**
//! （13 / 15 / 40），还有一部分**低于本项目允许的 UI 下限**（10 / 9）。
//!
//! 规格卡上另有 20 / 26 / 34 三个字号，但它们是 **FAB 的图形符号（`+` 字形）**，不是文字——
//! 归 [`crate::inkcat::size::FabSize::icon_px`] 管，本模块不收录（`SpecText` 里没有 20 这一档）。
//!
//! 本模块的职责就是把「规格卡字级」翻译成这三类结论，让换肤时不用逐个拿卷尺量：
//!
//! | 结论 | 含义 | 处置 |
//! |---|---|---|
//! | [`SpecFit::Exact`] | 恰好是官方某档 | 直接用该 [`TypeRole`] |
//! | [`SpecFit::Between`] | 落在相邻两档之间 | 取**较大**那档（可读性优先），并在报告里登记 1px 级偏离 |
//! | [`SpecFit::BelowFloor`] | 低于 [`UI_TEXT_FLOOR_PX`]（11） | **不进 UI**：只允许出现在规格卡/图注/设计稿里 |
//!
//! ## 为什么下限是 11
//!
//! [`TypeRole::LabelSmall`] 是官方最小档（11/16），且 crate 文档已经写明「中文 11px 可读性差，
//! 谨慎使用」。规格卡的 10（小节标题 `BUTTONS / 5 LEVELS`）与 9（色板角色名）是**设计稿标注**，
//! 不是界面文字——搬进 UI 就是拿中文在图上看蚂蚁。所以下限写成常量，由
//! [`SpecText::in_ui`] 与测试把它钉死。
//!
//! ## 同字号多档怎么办
//!
//! M3 有若干同字号档（14 有 title-small / body-medium / label-large；16 有 title-medium /
//! body-large；12 有 body-small / label-medium）。[`nearest_role`] 在距离相同时按
//! [`TypeRole::ALL`] 的声明序取第一个（title → body → label），这是**稳定的**、与文档一致的选择；
//! 需要别的语义（例如「这是按钮文字」）就显式指定 [`TypeRole`]，不要指望 `nearest_role` 猜。

use crate::typography::{TypeRole, Weight};

/// 允许进 UI 的最小字号（px）。低于它的规格卡字号只能是设计稿标注。
pub const UI_TEXT_FLOOR_PX: f32 = 11.0;

/// 规格卡上出现的文字用途（每个用途一个 `font-size`，值取自规格卡）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecText {
    /// 卡片大标题：40 / 500 / 0（`Ink Cat / M3 Expressive` 那行）。
    PageTitle,
    /// 副标题：15 / 400 / 0.3。
    Subtitle,
    /// 小节标注（全大写 + 字距）：10 / 600 / 1.2。
    SectionCaps,
    /// 大按钮标签：32 / 500 / 0（XL 档）。
    DisplayLabel,
    /// 对话框标题：24 / 400 / 0。
    DialogTitle,
    /// 标准控件标签：16 / 500 / 0（M 档按钮、卡片标题）。
    ControlLabel,
    /// 小控件标签：14 / 500 / 0（XS/S 档按钮、对话框动作、snackbar）。
    ControlLabelSmall,
    /// 正文：13 / 400 / 0（卡片支持文字、chip 标签、列表副文案）。
    Body,
    /// 辅助文字：12 / 400 / 0（字段标签、图标按钮说明）。
    Caption,
    /// 徽标数字：11 / 600 / 0。
    Badge,
    /// 色板角色名（设计稿标注）：9 / 400 / 0。
    SwatchCaption,
}

impl SpecText {
    /// 规格卡里出现的全部用途。
    pub const ALL: [SpecText; 11] = [
        SpecText::PageTitle,
        SpecText::Subtitle,
        SpecText::SectionCaps,
        SpecText::DisplayLabel,
        SpecText::DialogTitle,
        SpecText::ControlLabel,
        SpecText::ControlLabelSmall,
        SpecText::Body,
        SpecText::Caption,
        SpecText::Badge,
        SpecText::SwatchCaption,
    ];

    /// 字号（px，规格卡 `font-size`）。
    pub const fn px(self) -> f32 {
        match self {
            SpecText::PageTitle => 40.0,
            SpecText::Subtitle => 15.0,
            SpecText::SectionCaps => 10.0,
            SpecText::DisplayLabel => 32.0,
            SpecText::DialogTitle => 24.0,
            SpecText::ControlLabel => 16.0,
            SpecText::ControlLabelSmall => 14.0,
            SpecText::Body => 13.0,
            SpecText::Caption => 12.0,
            SpecText::Badge => 11.0,
            SpecText::SwatchCaption => 9.0,
        }
    }

    /// 字重（规格卡 `font-weight`）。
    pub const fn weight(self) -> Weight {
        match self {
            SpecText::SectionCaps | SpecText::Badge => Weight::SemiBold,
            SpecText::PageTitle
            | SpecText::DisplayLabel
            | SpecText::ControlLabel
            | SpecText::ControlLabelSmall => Weight::Medium,
            _ => Weight::Regular,
        }
    }

    /// 字距（px，规格卡 `letter-spacing`）。
    pub const fn tracking_px(self) -> f32 {
        match self {
            SpecText::SectionCaps => 1.2,
            SpecText::Subtitle => 0.3,
            _ => 0.0,
        }
    }

    /// 校核结论（命中 / 落在两档之间 / 低于下限）。
    pub fn fit(self) -> SpecFit {
        fit_of(self.px())
    }

    /// 能否进 UI（≥ [`UI_TEXT_FLOOR_PX`]）。
    pub const fn in_ui(self) -> bool {
        self.px() >= UI_TEXT_FLOOR_PX
    }
}

/// 一个字号与官方 type scale 的关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecFit {
    /// 恰好等于官方某档（同字号多档时取 `TypeRole::ALL` 序里第一个）。
    Exact(TypeRole),
    /// 落在相邻两档之间（小档, 大档）。
    Between(TypeRole, TypeRole),
    /// 低于 [`UI_TEXT_FLOOR_PX`]：只能是设计稿标注。
    BelowFloor,
}

impl SpecFit {
    /// 建议采用的档位（`BelowFloor` 返回 `None`：不该进 UI）。
    ///
    /// `Between` 取**较大**那档——宁可偏大 1px，也不要偏小到中文糊成一团。
    pub fn recommended(self) -> Option<TypeRole> {
        match self {
            SpecFit::Exact(role) => Some(role),
            SpecFit::Between(_, larger) => Some(larger),
            SpecFit::BelowFloor => None,
        }
    }
}

/// `px` 的最近官方档（距离相同取**较大**那档；同字号多档取 `TypeRole::ALL` 序里第一个）。
pub fn nearest_role(px: f32) -> TypeRole {
    let mut best = TypeRole::BodyMedium;
    let mut best_key = (f32::INFINITY, f32::NEG_INFINITY);
    for role in TypeRole::ALL {
        let size = role.style().size_px;
        // 排序键：(距离, −字号)；严格优于当前才换，所以同键时保留 `ALL` 序靠前的那个。
        let key = ((size - px).abs(), -size);
        if key.0 < best_key.0 || (key.0 == best_key.0 && key.1 < best_key.1) {
            best = role;
            best_key = key;
        }
    }
    best
}

/// `px` 与官方 type scale 的关系。
pub fn fit_of(px: f32) -> SpecFit {
    if px < UI_TEXT_FLOOR_PX {
        return SpecFit::BelowFloor;
    }
    let mut exact = None;
    let mut lower: Option<(f32, TypeRole)> = None;
    let mut upper: Option<(f32, TypeRole)> = None;
    for role in TypeRole::ALL {
        let size = role.style().size_px;
        if (size - px).abs() < f32::EPSILON {
            // 同字号多档（14/16/12）：保留 `ALL` 序里第一个（title → body → label）。
            exact = exact.or(Some(role));
        } else if size < px {
            if lower.is_none_or(|(best, _)| size > best) {
                lower = Some((size, role));
            }
        } else if upper.is_none_or(|(best, _)| size < best) {
            upper = Some((size, role));
        }
    }
    match (exact, lower, upper) {
        (Some(role), _, _) => SpecFit::Exact(role),
        (None, Some((_, lower)), Some((_, upper))) => SpecFit::Between(lower, upper),
        // 超出官方区间（px > 57）：取最近档，不硬造一个不存在的「之间」。
        _ => SpecFit::Exact(nearest_role(px)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_sizes_classify_as_documented() {
        // 命中：14 / 16 / 24 / 32 / 12 / 11
        assert_eq!(
            SpecText::ControlLabelSmall.fit(),
            SpecFit::Exact(TypeRole::TitleSmall)
        );
        assert_eq!(
            SpecText::ControlLabel.fit(),
            SpecFit::Exact(TypeRole::TitleMedium)
        );
        assert_eq!(
            SpecText::DialogTitle.fit(),
            SpecFit::Exact(TypeRole::HeadlineSmall)
        );
        assert_eq!(
            SpecText::DisplayLabel.fit(),
            SpecFit::Exact(TypeRole::HeadlineLarge)
        );
        assert_eq!(SpecText::Caption.fit(), SpecFit::Exact(TypeRole::BodySmall));
        assert_eq!(SpecText::Badge.fit(), SpecFit::Exact(TypeRole::LabelSmall));
        // 落在两档之间：13（12/14）、15（14/16）、40（36/45）
        assert_eq!(
            SpecText::Body.fit(),
            SpecFit::Between(TypeRole::BodySmall, TypeRole::TitleSmall)
        );
        assert_eq!(
            SpecText::Subtitle.fit(),
            SpecFit::Between(TypeRole::TitleSmall, TypeRole::TitleMedium)
        );
        assert_eq!(
            SpecText::PageTitle.fit(),
            SpecFit::Between(TypeRole::DisplaySmall, TypeRole::DisplayMedium)
        );
        // 低于下限：10 / 9
        assert_eq!(SpecText::SectionCaps.fit(), SpecFit::BelowFloor);
        assert_eq!(SpecText::SwatchCaption.fit(), SpecFit::BelowFloor);
    }

    #[test]
    fn only_the_two_annotation_sizes_are_out_of_ui() {
        let out: Vec<&str> = SpecText::ALL
            .iter()
            .filter(|text| !text.in_ui())
            .map(|text| match text {
                SpecText::SectionCaps => "section-caps",
                SpecText::SwatchCaption => "swatch-caption",
                _ => "不该出现在这里",
            })
            .collect();
        assert_eq!(out, vec!["section-caps", "swatch-caption"]);
        // 下限就是 LabelSmall 的字号（官方最小档），不是随手拍的数。
        assert_eq!(UI_TEXT_FLOOR_PX, TypeRole::LabelSmall.style().size_px);
    }

    #[test]
    fn recommended_rounds_up_not_down() {
        assert_eq!(SpecFit::BelowFloor.recommended(), None);
        // 13 → 取 14，不取 12。
        assert_eq!(fit_of(13.0).recommended(), Some(TypeRole::TitleSmall));
        // 15 → 取 16。
        assert_eq!(fit_of(15.0).recommended(), Some(TypeRole::TitleMedium));
        // 40 → 取 45，不取 36。
        assert_eq!(fit_of(40.0).recommended(), Some(TypeRole::DisplayMedium));
    }

    #[test]
    fn nearest_role_is_stable_for_equal_distances() {
        // 15 与 14 / 16 等距 → 取较大档 16。
        assert_eq!(nearest_role(15.0), TypeRole::TitleMedium);
        // 官方最小/最大档之外也不越界。
        assert_eq!(nearest_role(4.0), TypeRole::LabelSmall);
        assert_eq!(nearest_role(80.0), TypeRole::DisplayLarge);
    }

    #[test]
    fn spec_weights_and_tracking_are_pinned() {
        assert_eq!(SpecText::SectionCaps.weight(), Weight::SemiBold);
        assert_eq!(SpecText::Badge.weight(), Weight::SemiBold);
        assert_eq!(SpecText::ControlLabel.weight(), Weight::Medium);
        assert_eq!(SpecText::Body.weight(), Weight::Regular);
        assert!((SpecText::SectionCaps.tracking_px() - 1.2).abs() < 1.0e-6);
        assert!((SpecText::Subtitle.tracking_px() - 0.3).abs() < 1.0e-6);
        assert_eq!(SpecText::ControlLabel.tracking_px(), 0.0);
    }
}
