//! 演示模型（`pages::demo_model`）的基线判据（M84 出图基线）。
//!
//! M301 从 `pages/tests.rs` 搬出来：那份文件要装轨宽活值的判据，按语义拆一块正好守 400 行门禁。

use unlucky_presenter::router::NavSection;

use super::super::*;

/// 演示模型：12 行 + 1 条选中详情（M84 的出图基线就靠这份数据）。
#[test]
fn the_demo_model_carries_the_m84_baseline() {
    let model = demo_model(Screen::Instances, 1180.0, 760.0);
    assert_eq!(model.instances.rows.len(), 12);
    let detail = model.instances.detail.as_ref().expect("演示数据有选中项");
    assert_eq!(detail.meta.len(), 6, "元信息 6 格");
    assert!(detail.note.is_some(), "演示数据带正文段");
    assert_eq!(model.active, Some(NavSection::Instances));

    // 其余三页不带实例数据（占位视觉不读它）
    for screen in [Screen::Download, Screen::Settings, Screen::Logs] {
        let model = demo_model(screen, 1180.0, 760.0);
        assert!(
            model.instances.rows.is_empty(),
            "{} 不该带实例数据",
            screen.slug()
        );
    }
}
