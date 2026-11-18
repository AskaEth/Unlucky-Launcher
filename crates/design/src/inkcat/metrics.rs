//! 控件方框几何（规格卡的 `CARD / DIALOG / SNACKBAR / LIST`、`TEXT FIELDS`、`SWITCH / CHECKBOX /
//! RADIO / CHIPS`、`PROGRESS` 五节）。
//!
//! 口径与 [`crate::component_map::ComponentMetrics`] 一致：`width`/`height` 为 `0.0` 表示
//! **由内容决定**（自适应宽），不是「尺寸为零」。差别只在**来源**——这里是规格卡的值，
//! `component_map` 是官方组件令牌的值，两者不一致处已在 [`super`] 的登记表里逐条写明。
//!
//! 圆角一律引用 [`ShapeScale`]，不写裸数字；`SpecBox::radius_px()` 用**高度**折算
//! （药丸按高的一半、非药丸按刻度值且不超过半高），所以同一个结构既能表达 60 高药丸行（30），
//! 也能表达 150 高圆角卡（16）。

use crate::shape::ShapeScale;

/// 一个控件方框（宽 / 高 / 圆角档）。`0.0` = 由内容决定。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpecBox {
    /// 可见外框宽（px；`0.0` = 自适应）。
    pub width: f32,
    /// 可见外框高（px）。
    pub height: f32,
    /// 圆角档（[`ShapeScale`] 的刻度值）。
    pub shape: ShapeScale,
}

impl SpecBox {
    /// 实际圆角（px）：按高度折算，且不超过高的一半。
    pub fn radius_px(&self) -> f32 {
        self.shape.radius_px(self.height)
    }

    /// 是否是药丸/整圆（圆角 = 半高）。
    pub fn is_pill(&self) -> bool {
        (self.radius_px() - self.height / 2.0).abs() < f32::EPSILON && self.height > 0.0
    }
}

/// 卡片：300×150、圆角 **16**（规格卡 `ELEVATED CARD` 的 `rx=16`）。
///
/// 与官方 `FilledCardTokens` 的 `CornerMedium(12)` 差 4 —— 规格卡口径取 16（Large）。
pub const CARD: SpecBox = SpecBox {
    width: 300.0,
    height: 150.0,
    shape: ShapeScale::Large,
};

/// 对话框：380×190、圆角 **28**（与官方 `DialogTokens.CornerExtraLarge` 一致）。
pub const DIALOG: SpecBox = SpecBox {
    width: 380.0,
    height: 190.0,
    shape: ShapeScale::ExtraLarge,
};

/// Snackbar：340×52、圆角 **16**（官方是 `ExtraSmall(4)`，规格卡取 16 —— 登记差异）。
pub const SNACKBAR: SpecBox = SpecBox {
    width: 340.0,
    height: 52.0,
    shape: ShapeScale::Large,
};

/// 列表行：300×**60**、圆角 **30**（= 药丸；官方 `CornerNone`，本项目 `Small(8)`，规格卡药丸 —— 登记差异）。
pub const LIST_ROW: SpecBox = SpecBox {
    width: 300.0,
    height: 60.0,
    shape: ShapeScale::Full,
};

/// Chip（assist / filter）：高 **32**、圆角 **8**、宽度自适应（标签 13px、左右内距 16）。
///
/// 选中态的**形状不变**（规格卡 `Selected` 只是换填充），所以这里不给形变数据。
pub const CHIP: SpecBox = SpecBox {
    width: 0.0,
    height: 32.0,
    shape: ShapeScale::Small,
};

/// Chip 左右内距（px）。规格卡 chip 自适应宽；16 与 M3 `AssistChipTokens` 的 `HorizontalPadding`
/// 同值，且实测标签订了这个量级。
pub const CHIP_PAD_X: f32 = 16.0;

/// 徽标（数字圆点）：20 Ø、整圆。
pub const BADGE: SpecBox = SpecBox {
    width: 20.0,
    height: 20.0,
    shape: ShapeScale::Full,
};

/// 列表行头像：40 Ø、整圆。
pub const AVATAR: SpecBox = SpecBox {
    width: 40.0,
    height: 40.0,
    shape: ShapeScale::Full,
};

/// 加载指示：56 Ø，圆形态（`rx=28`）与方形态（`rx=12` = [`ShapeScale::Medium`]）两种。
pub const LOADING: SpecBox = SpecBox {
    width: 56.0,
    height: 56.0,
    shape: ShapeScale::Full,
};

/// 加载指示的方形态圆角档（规格卡 `rx=12`）。
pub const LOADING_SQUIRCLE: ShapeScale = ShapeScale::Medium;

// ---- 文本字段（TEXT FIELDS / FILLED - OUTLINED - ERROR + ROUNDED 变体）----

/// 字段高度（px；filled / outlined / error / 搜索框都是 56）。
pub const FIELD_HEIGHT: f32 = 56.0;
/// 默认圆角档：**4**（规格卡标题就写着 `CORNER 4`；与官方 `CornerExtraSmall` 一致）。
pub const FIELD_SHAPE: ShapeScale = ShapeScale::ExtraSmall;
/// 药丸变体圆角档：**28**（规格卡 `ROUNDED  (CORNER 28, BRAND VARIANT)`）。
pub const FIELD_SHAPE_ROUNDED: ShapeScale = ShapeScale::ExtraLarge;
/// 默认字段的内容内距（px）：标签 `x=80` − 字段 `x=64` = **16**。
pub const FIELD_INSET: f32 = 16.0;
/// 药丸字段的内容内距（px）：**32**。
///
/// 规格卡专门写了一行提醒：`label inset 32 inside pills (not 16), otherwise it reads as clipped`——
/// 药丸两端是半圆，沿用 16 会让标签贴到弧上。
pub const FIELD_PILL_INSET: f32 = 32.0;
/// 字段悬浮标签字号（px）与输入值字号（px）。
pub const FIELD_LABEL_PX: f32 = 12.0;
/// 字段输入值字号（px）。
pub const FIELD_VALUE_PX: f32 = 16.0;
/// 错误提示行字号（px）。
pub const FIELD_ERROR_PX: f32 = 12.0;

/// 搜索框：560 宽是页级骨架量（这里只给高度与形状），圆角 **28** = 药丸，图标内距 **32**。
pub const SEARCH_SHAPE: ShapeScale = ShapeScale::Full;
/// 搜索框图标中心距左边（px）：`cx=1048` − 框 `x=1016` = **32**。
pub const SEARCH_INSET: f32 = 32.0;

// ---- 开关 / 复选框 / 单选框 ----

/// 开关：轨道 52×32 药丸，拇指 **24 Ø**、内缩 **4**（关态拇指 16 Ø + 描边 2）。
pub const SWITCH_TRACK: SpecBox = SpecBox {
    width: 52.0,
    height: 32.0,
    shape: ShapeScale::Full,
};
/// 开关拇指直径（px）。
pub const SWITCH_THUMB: f32 = 24.0;
/// 拇指相对轨道的内缩（px）：规格卡轨道 `y=1060..1092`、拇指 `y=1064..1088`，上下各 **4**；
/// 右端 `116−112` 同为 4。
///
/// 位移行程（拇指可从左滑到右的距离）= 轨道宽 52 − 拇指 24 − 2×内缩 4 = **20**（规格卡没写行程，
/// 这是由前三项推出来的唯一值；写死 20 就是第四份副本）。
pub const SWITCH_THUMB_INSET: f32 = 4.0;
/// 关态拇指直径（px）：规格卡 `16×16 rx8`。
pub const SWITCH_THUMB_OFF: f32 = 16.0;

/// 复选框：**18×18**、圆角 **4**、描边 **2**（规格卡 `rect 18×18 rx4` + 勾线 `stroke-width=2`）。
///
/// 与官方 20×20 / 2dp 差 2px 与 2dp —— 4 是 `ShapeScale` 的刻度值，所以走容器层不违反 D32。
pub const CHECKBOX: SpecBox = SpecBox {
    width: 18.0,
    height: 18.0,
    shape: ShapeScale::ExtraSmall,
};
/// 复选框描边/勾线宽度（px）。
pub const CHECKBOX_STROKE: f32 = 2.0;

/// 单选框：**18 Ø**、环线 **2**、内点 **10 Ø**（规格卡 `r=9` 环 + `r=5` 实心）。
pub const RADIO: SpecBox = SpecBox {
    width: 18.0,
    height: 18.0,
    shape: ShapeScale::Full,
};
/// 单选框环线宽度（px）。
pub const RADIO_RING: f32 = 2.0;
/// 单选框内点直径（px）。
pub const RADIO_DOT: f32 = 10.0;
/// 勾选控件与标签的间距（px；规格卡 `18` 框 → 标签 `x=398`，实测 8）。
pub const CHOICE_GAP: f32 = 8.0;

// ---- 进度 ----

/// 线性进度条厚度（px）：`rect height=4`。
pub const PROGRESS_THICKNESS: f32 = 4.0;
/// 波形进度的描边宽度（px）：`stroke-width=6`、`stroke-linecap=round`。
pub const PROGRESS_WAVY_STROKE: f32 = 6.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacing::GRID;

    #[test]
    fn spec_boxes_hit_the_spec_corners() {
        // 规格卡逐个比对（rx 一栏就是 `radius_px()`）。
        assert_eq!(CARD.radius_px(), 16.0, "卡片 rx=16");
        assert_eq!(DIALOG.radius_px(), 28.0, "对话框 rx=28");
        assert_eq!(SNACKBAR.radius_px(), 16.0, "snackbar rx=16");
        assert_eq!(LIST_ROW.radius_px(), 30.0, "列表行 rx=30（药丸）");
        assert_eq!(CHIP.radius_px(), 8.0, "chip rx=8");
        assert_eq!(BADGE.radius_px(), 10.0, "徽标是圆点");
        assert_eq!(AVATAR.radius_px(), 20.0, "头像是圆");
        assert_eq!(LOADING.radius_px(), 28.0, "加载圆形态");
        assert_eq!(LOADING_SQUIRCLE.dp(), 12.0, "加载方形态 rx=12");
        assert_eq!(CHECKBOX.radius_px(), 4.0, "复选框 rx=4");
        assert_eq!(SWITCH_TRACK.radius_px(), 16.0, "开关轨道 rx=16");
    }

    #[test]
    fn pills_are_only_the_ones_the_spec_draws_as_pills() {
        assert!(LIST_ROW.is_pill());
        assert!(BADGE.is_pill());
        assert!(AVATAR.is_pill());
        assert!(LOADING.is_pill());
        assert!(RADIO.is_pill());
        assert!(SWITCH_TRACK.is_pill());
        assert!(!CARD.is_pill());
        assert!(!DIALOG.is_pill());
        assert!(!SNACKBAR.is_pill());
        assert!(!CHECKBOX.is_pill());
    }

    #[test]
    fn field_variants_keep_the_inset_rule() {
        assert_eq!(FIELD_SHAPE.dp(), 4.0);
        assert_eq!(FIELD_SHAPE_ROUNDED.dp(), 28.0);
        // 药丸内距必须是默认内距的两倍——规格卡那行提醒就是这条。
        assert_eq!(FIELD_PILL_INSET, FIELD_INSET * 2.0);
    }

    #[test]
    fn heights_are_on_the_grid() {
        for (name, value) in [
            ("snackbar", SNACKBAR.height),
            ("list-row", LIST_ROW.height),
            ("chip", CHIP.height),
            ("badge", BADGE.height),
            ("avatar", AVATAR.height),
            ("loading", LOADING.height),
            ("field", FIELD_HEIGHT),
            ("switch", SWITCH_TRACK.height),
            ("switch-thumb", SWITCH_THUMB),
            ("progress", PROGRESS_THICKNESS),
            ("choice-gap", CHOICE_GAP),
            ("search-inset", SEARCH_INSET),
        ] {
            assert_eq!(value % GRID, 0.0, "{name} = {value} 不在 4px 栅格上");
        }
        // 微控件 18：规格卡实测值（M3 官方 `CheckboxTokens.ContainerWidth/Height` 也是 18），
        // 它与 2dp/4dp 圆角一样属「微控件刻度」，不受 4px 栅格约束。
        assert_eq!((CHECKBOX.width, CHECKBOX.height), (18.0, 18.0));
        assert_eq!((RADIO.width, RADIO.height), (18.0, 18.0));
        // 卡 / 对话框的 300×150 与 380×190 是规格卡画布上的**示意**尺寸（实际高由内容决定），
        // 所以只钉数值、不参与栅格门禁。
        assert_eq!((CARD.width, CARD.height), (300.0, 150.0));
        assert_eq!((DIALOG.width, DIALOG.height), (380.0, 190.0));
        // 例外：描边类不受栅格约束（它们是一像素/线宽量，不是布局尺寸），单独钉住。
        assert_eq!(CHECKBOX_STROKE, 2.0);
        assert_eq!(RADIO_RING, 2.0);
        assert_eq!(PROGRESS_WAVY_STROKE, 6.0, "波形进度描边 stroke-width=6");
    }
}
