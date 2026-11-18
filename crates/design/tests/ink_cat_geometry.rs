//! Ink Cat 规格卡的**几何与字级**门禁（M228）：按钮 XS~XL、FAB 四态、控件方框、字号校核。
//!
//! 与 `ink_cat_boards.rs` 分开是单文件 ≤400 行的硬约束；两侧都只用公开 API。

use unlucky_design::inkcat::metrics;
use unlucky_design::inkcat::size::{ControlSize, FabSize};
use unlucky_design::inkcat::type_scale::{SpecFit, SpecText, UI_TEXT_FLOOR_PX};
use unlucky_design::shape::ShapeScale;
use unlucky_design::typography::{TypeRole, Weight};

#[test]
fn buttons_and_fabs_hit_the_spec_geometry() {
    // 按钮五档：32/40/56/96/136，圆角一律半高（药丸）。
    let heights: Vec<f32> = ControlSize::ALL.iter().map(|s| s.height_px()).collect();
    assert_eq!(heights, vec![32.0, 40.0, 56.0, 96.0, 136.0]);
    for size in ControlSize::ALL {
        assert_eq!(
            size.radius_px(),
            size.height_px() / 2.0,
            "{size:?} 该是药丸"
        );
    }
    // M 档就是规格卡 `BUTTONS / 5 LEVELS` 那一行的 56 高。
    assert_eq!(ControlSize::M.height_px(), 56.0);
    assert_eq!(ControlSize::M.label_px(), 16.0);

    // FAB 四态圆角 12/16/20/16（都不是半高，所以不用 Full）。
    let radii: Vec<f32> = FabSize::ALL.iter().map(|s| s.radius_px()).collect();
    assert_eq!(radii, vec![12.0, 16.0, 20.0, 16.0]);
    let sizes: Vec<f32> = FabSize::ALL.iter().map(|s| s.height_px()).collect();
    assert_eq!(sizes, vec![40.0, 56.0, 80.0, 56.0]);
}

#[test]
fn component_boxes_hit_the_spec_corners() {
    assert_eq!(metrics::CARD.radius_px(), 16.0, "ELEVATED CARD rx=16");
    assert_eq!(metrics::DIALOG.radius_px(), 28.0, "对话框 rx=28");
    assert_eq!(metrics::SNACKBAR.radius_px(), 16.0, "snackbar rx=16");
    assert_eq!(metrics::SNACKBAR.height, 52.0);
    assert_eq!(metrics::LIST_ROW.radius_px(), 30.0, "列表行药丸");
    assert_eq!(metrics::LIST_ROW.height, 60.0);
    assert_eq!(metrics::CHIP.height, 32.0);
    assert_eq!(metrics::CHIP.radius_px(), 8.0);
    // 输入框：默认圆角 4、药丸变体 28，内距 16 / 32（规格卡专门提醒过）。
    assert_eq!(metrics::FIELD_SHAPE, ShapeScale::ExtraSmall);
    assert_eq!(metrics::FIELD_SHAPE_ROUNDED, ShapeScale::ExtraLarge);
    assert_eq!(metrics::FIELD_HEIGHT, 56.0);
    assert_eq!(metrics::FIELD_INSET, 16.0);
    assert_eq!(metrics::FIELD_PILL_INSET, 32.0);
    // 开关 / 勾选 / 进度。
    assert_eq!(
        (metrics::SWITCH_TRACK.width, metrics::SWITCH_TRACK.height),
        (52.0, 32.0)
    );
    assert_eq!(metrics::SWITCH_THUMB, 24.0);
    assert_eq!(metrics::CHECKBOX.radius_px(), 4.0);
    assert_eq!(metrics::PROGRESS_THICKNESS, 4.0);
}

#[test]
fn spec_font_sizes_map_onto_the_official_scale() {
    // 命中官方档的六档。
    for (text, role) in [
        (SpecText::ControlLabelSmall, TypeRole::TitleSmall),
        (SpecText::DialogTitle, TypeRole::HeadlineSmall),
        (SpecText::DisplayLabel, TypeRole::HeadlineLarge),
        (SpecText::Caption, TypeRole::BodySmall),
        (SpecText::Badge, TypeRole::LabelSmall),
    ] {
        assert_eq!(text.fit(), SpecFit::Exact(role), "{text:?}");
    }
    // 落在两档之间的三档：取较大那档，登记 1px 级偏离。
    for (text, lower, upper) in [
        (SpecText::Body, TypeRole::BodySmall, TypeRole::TitleSmall),
        (
            SpecText::Subtitle,
            TypeRole::TitleSmall,
            TypeRole::TitleMedium,
        ),
        (
            SpecText::PageTitle,
            TypeRole::DisplaySmall,
            TypeRole::DisplayMedium,
        ),
    ] {
        assert_eq!(text.fit(), SpecFit::Between(lower, upper), "{text:?}");
        assert_eq!(text.fit().recommended(), Some(upper));
    }
    // 低于 UI 下限的两档：只能是设计稿标注。
    for text in [SpecText::SectionCaps, SpecText::SwatchCaption] {
        assert!(!text.in_ui(), "{text:?} 不该进 UI");
        assert_eq!(text.fit(), SpecFit::BelowFloor);
        assert_eq!(text.fit().recommended(), None);
    }
    assert_eq!(UI_TEXT_FLOOR_PX, 11.0);
    // 按钮标签字重：规格卡是 500（官方 body/headline 是 400，这是按钮语义）。
    for size in ControlSize::ALL {
        assert_eq!(size.label().weight, Weight::Medium);
    }
}
