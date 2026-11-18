//! M69（TOKEXT）的数值钉死：G9~G13 五项缺口逐条关门。
//!
//! 期望值全部**独立于实现重写**（照 `research/15` §18~§23 与 `00-decision-log.md` D32/D33 抄），
//! 实现改一个数、这里不改就红。覆盖：
//! - **G13** `ComponentRadius` 白名单 + 尺寸闸（2 不在 `ShapeScale` 里，刻度不许被加宽）；
//! - **G11** `LayoutMetrics` 的 4 个新骨架值 + nav 语义修正；
//! - **G10** `ComponentMetrics` 逐字段；
//! - **G9** `Component` 25 个变体与档位表一行对一行；
//! - **G12** 一像素线单一真源。

use unlucky_design::component_map::{Component, ComponentMetrics};
use unlucky_design::elevation::Elevation;
use unlucky_design::icons::IconSize;
use unlucky_design::shape::{radius_layer_for, ComponentRadius, RadiusLayer, ShapeScale};
use unlucky_design::spacing::{stroke, LayoutMetrics, Space};
use unlucky_design::typography::TypeRole;

// ---------- G13：圆角分层（D32） ----------

#[test]
fn shape_scale_is_unchanged_and_never_contains_two() {
    // 刻度一个字都不许动（D32 第 1 条：ShapeScale 保持严格）。
    let ladder: [(ShapeScale, f32); 9] = [
        (ShapeScale::None, 0.0),
        (ShapeScale::ExtraSmall, 4.0),
        (ShapeScale::Small, 8.0),
        (ShapeScale::Medium, 12.0),
        (ShapeScale::Large, 16.0),
        (ShapeScale::LargeIncreased, 20.0),
        (ShapeScale::ExtraLarge, 28.0),
        (ShapeScale::ExtraLargeIncreased, 32.0),
        (ShapeScale::ExtraExtraLarge, 48.0),
    ];
    for (scale, dp) in ladder {
        assert_eq!(scale.dp(), dp, "{scale:?} 的刻度被改动了");
        // 2 是官方 Checkbox 的圆角，但它**不在**刻度里——加进来就等于废掉 K1 守门。
        assert_ne!(dp, 2.0, "2dp 不许进 ShapeScale（走 ComponentRadius）");
    }
    assert_eq!(ShapeScale::Full.radius_px(64.0), 32.0);
}

#[test]
fn component_radius_is_a_one_item_whitelist() {
    assert_eq!(ComponentRadius::ALL, [ComponentRadius::Micro]);
    // 2dp 只能通过带尺寸闸的 `radius_px` 拿到（`dp()` 是 crate 内部口，外部看不见）。
    assert_eq!(
        ComponentRadius::Micro.radius_px(20.0),
        Some(2.0),
        "官方 Checkbox RadiusSize = 2.dp"
    );
    // 尺寸闸：>24dp 一律拿不到微半径（D32 的「只允许 ≤24dp 微控件」）。
    assert_eq!(ComponentRadius::Micro.radius_px(24.0), Some(2.0));
    assert_eq!(ComponentRadius::Micro.radius_px(25.0), None);
    assert_eq!(ComponentRadius::Micro.radius_px(240.0), None);
    assert_eq!(ComponentRadius::Micro.radius_px(0.0), None);
    // 边长小于两倍半径时按边长一半折算（2dp 元素不能有 2dp 圆角）。
    assert_eq!(ComponentRadius::Micro.radius_px(2.0), Some(1.0));
}

#[test]
fn radius_layer_for_routes_by_size() {
    assert_eq!(radius_layer_for(20.0), RadiusLayer::Component);
    assert_eq!(radius_layer_for(24.0), RadiusLayer::Component);
    assert_eq!(radius_layer_for(28.0), RadiusLayer::Scale);
    assert_eq!(radius_layer_for(360.0), RadiusLayer::Scale);
    // 抽屉/面板这类容器永远不该被判成微控件。
    for container in [
        LayoutMetrics::DESKTOP.nav_drawer_width,
        LayoutMetrics::DESKTOP.ai_panel_width,
    ] {
        assert_eq!(radius_layer_for(container), RadiusLayer::Scale);
    }
}

// ---------- G11：骨架值 + nav 语义修正 ----------

#[test]
fn layout_metrics_new_skeleton_values_are_pinned() {
    let m = LayoutMetrics::DESKTOP;
    // A 级：SearchBarTokens.ContainerHeight = 56。
    assert_eq!(m.search_bar_height, 56.0);
    // A 级：NavigationDrawer 下限 240（官方 ContainerWidth = 360，1180 窗下过宽，D33 取 240）。
    assert_eq!(m.nav_drawer_width, 240.0);
    // 12b §5.1 登记偏离：官方 rail 96 → 本项目 88。
    assert_eq!(m.nav_rail_width, 88.0);
    // A 级：DividerTokens.Thickness = 1.0dp（一像素线，不受 4px 栅格约束）。
    assert_eq!(m.divider_thickness, 1.0);
}

#[test]
fn nav_widths_mean_what_their_names_say() {
    // D33：展开（drawer）比折叠（rail）宽；旧字段 nav_expanded_width=88 / nav_collapsed_width=48
    // 的名字与值相反，已在 M69 改名并删除，这里钉住「新名字不会再次反向」。
    let m = LayoutMetrics::DESKTOP;
    assert!(
        m.nav_drawer_width > m.nav_rail_width,
        "展开态（drawer {}）必须比折叠态（rail {}）宽",
        m.nav_drawer_width,
        m.nav_rail_width
    );
    // rail 里 56 宽的选中指示器要放得下（88 − 2×16）。
    assert_eq!(m.nav_rail_width - Space::X4.dp() * 2.0, 56.0);
    // drawer 里 216 宽的指示器同样由「宽 − 2×12」推出（见 ComponentMetrics::indicator_width）。
    assert_eq!(
        ComponentMetrics::NAVIGATION_DRAWER.indicator_width(m.nav_drawer_width),
        216.0
    );
}

// ---------- G10：组件级微度量 ----------

#[test]
fn checkbox_metrics_are_pinned() {
    let cm = ComponentMetrics::CHECKBOX;
    // 20×20（不是 token 文件里那个死值 18，见 research/15 §26.2 矛盾 ①）。
    assert_eq!((cm.width, cm.height), (20.0, 20.0));
    assert_eq!(
        cm.radius,
        Some(ComponentRadius::Micro),
        "圆角 2dp 走 D32 第二层"
    );
    assert_eq!(cm.stroke, 2.0, "CheckboxTokens.UnselectedOutlineWidth = 2");
    assert_eq!(cm.state_layer, 40.0, "StateLayerSize = 40");
    assert_eq!(cm.inner_dot, 0.0);
    assert_eq!(cm.gap, Space::X2.dp(), "框 ↔ 标签 8（团队口径）");
    assert_eq!(
        Component::Checkbox.metrics(),
        Some(ComponentMetrics::CHECKBOX)
    );
}

#[test]
fn radio_metrics_are_pinned() {
    let cm = ComponentMetrics::RADIO;
    assert_eq!(
        (cm.width, cm.height),
        (20.0, 20.0),
        "RadioButtonTokens.IconSize = 20"
    );
    assert_eq!(cm.stroke, 2.0, "RadioStrokeWidth = 2.dp");
    // 内点 10 不是 12：12 是含描边前的名义值（绘制半径 = 6 − 1 = 5）。
    assert_eq!(cm.inner_dot, 10.0);
    assert_eq!(cm.state_layer, 40.0);
    assert_eq!(cm.radius, None, "radio 是真圆，圆角走 ShapeScale::Full");
    assert_eq!(Component::Radio.spec().shape, ShapeScale::Full);
}

#[test]
fn search_bar_metrics_are_pinned() {
    let cm = ComponentMetrics::SEARCH_BAR;
    // 高 56 属骨架级：CM 里必须留 0，不许重复一份。
    assert_eq!((cm.width, cm.height), (0.0, 0.0));
    assert_eq!(cm.icon, IconSize::Toolbar.px(), "leading/trailing 图标 24");
    // A 级（§20.4）：图标可视左缘 16 = 文本框内容内边距 12 + `SearchBarIconOffsetX = 4`。
    assert_eq!(cm.pad_start, Space::X3.dp(), "内容内边距 12");
    assert_eq!(cm.icon_offset, 4.0, "SearchBarIconOffsetX = 4");
    assert_eq!(cm.pad_start + cm.icon_offset, 16.0, "图标可视左缘必须是 16");
    // 右侧同式：图标右缘也是 16。
    assert_eq!(cm.pad_end, Space::X3.dp());
    assert_eq!(cm.pad_end + cm.icon_offset, 16.0, "图标可视右缘必须是 16");
    // 守门：不许把 pad 本身写成 16（否则 pad + offset = 20，越过 rubric 的 ±2px）。
    assert_ne!(cm.pad_start, 16.0);
    assert_ne!(cm.pad_end, 16.0);
    // 搜索条圆角 = 高/2 = 28，等于官方 SearchBarCornerRadius。
    assert_eq!(
        ShapeScale::Full.radius_px(LayoutMetrics::DESKTOP.search_bar_height),
        28.0
    );
}

#[test]
fn navigation_drawer_metrics_are_pinned() {
    let cm = ComponentMetrics::NAVIGATION_DRAWER;
    assert_eq!((cm.width, cm.height), (0.0, 0.0), "宽/项高属骨架级");
    assert_eq!(
        cm.inset, 12.0,
        "NavigationDrawerItemDefaults.ItemPadding = 12"
    );
    assert_eq!(cm.pad_start, Space::X4.dp(), "项内 start 16");
    assert_eq!(cm.pad_end, Space::X6.dp(), "项内 end 24");
    assert_eq!(cm.icon, IconSize::Toolbar.px());
    // 与 search bar 同一条公式：图标边 = pad + icon_offset（抽屉的 offset 是 0）。
    assert_eq!(cm.pad_start + cm.icon_offset, 16.0, "图标起点 = 项内 start");
    assert_eq!(cm.gap, Space::X3.dp(), "图标 ↔ label 12");
    // 指示器宽 = 容器宽 − 24；官方 360 − 24 = 336，本项目 240 − 24 = 216。
    assert_eq!(cm.indicator_width(360.0), 336.0);
    assert_eq!(cm.indicator_width(240.0), 216.0);
}

#[test]
fn component_metrics_never_duplicate_layout_metrics() {
    // 同一量只许一份：CM 里出现的骨架级尺寸必须留 0。
    let m = LayoutMetrics::DESKTOP;
    for component in [Component::Search, Component::NavigationDrawer] {
        let cm = component.metrics().expect("这两个控件必须有 CM");
        assert_eq!(cm.width, 0.0, "{} 的宽度属 LM", component.name());
        assert_eq!(cm.height, 0.0, "{} 的高度属 LM", component.name());
        assert_ne!(cm.height, m.search_bar_height);
        assert_ne!(cm.width, m.nav_drawer_width);
    }
    // Divider 的厚度也是 LM（一像素线）→ 它没有 CM。
    assert_eq!(Component::Divider.metrics(), None);
    assert_eq!(m.divider_thickness, stroke::HAIRLINE);
}

// ---------- G12：一像素线 ----------

#[test]
fn hairline_is_the_single_source_of_one_pixel_lines() {
    assert_eq!(stroke::HAIRLINE, 1.0);
    // 分隔线厚度直接引用它，不是另写一个字面量。
    assert_eq!(LayoutMetrics::DESKTOP.divider_thickness, stroke::HAIRLINE);
    // 一像素线不受 4px 栅格约束（栅格测试里显式豁免，这里反向钉一次）。
    assert_ne!(stroke::HAIRLINE % unlucky_design::spacing::GRID, 0.0);
}

// ---------- G9：枚举 25 变体 ↔ 表槽位 ----------

#[test]
fn component_enum_is_the_frozen_twenty_five() {
    let names = [
        "top-app-bar",
        "navigation-item",
        "primary-button",
        "secondary-button",
        "text-button",
        "icon-button",
        "outlined-card",
        "filled-card",
        "list-item",
        "text-field",
        "segmented-button",
        "chip",
        "switch",
        "slider",
        "progress-indicator",
        "dialog",
        "menu",
        "snackbar",
        "tooltip",
        "empty-state",
        // M69 末尾追加的 5 个
        "checkbox",
        "radio",
        "search",
        "navigation-drawer",
        "divider",
    ];
    assert_eq!(Component::ALL.len(), names.len(), "M66 的 G9：20 → 25");
    for (i, component) in Component::ALL.iter().enumerate() {
        assert_eq!(component.name(), names[i], "第 {i} 个组件被换位/改名了");
        // 表槽位与枚举判别值一一对应（`spec()` 就是按下标取的）。
        assert_eq!(*component as usize, i);
        assert_eq!(component.spec().component, *component);
    }
}

#[test]
fn new_components_carry_the_official_elevation_and_shape() {
    // Checkbox / Radio / Search / NavigationDrawer / Divider：全部 L0（都不是浮层）。
    for component in [
        Component::Checkbox,
        Component::Radio,
        Component::Search,
        Component::NavigationDrawer,
        Component::Divider,
    ] {
        assert_eq!(
            component.spec().elevation,
            Elevation::Level0,
            "{} 不该有阴影（R4：只有浮层允许）",
            component.name()
        );
    }
    // Checkbox 的圆角真源是 micro_radius，shape 只是占位 None。
    let checkbox = Component::Checkbox.spec();
    assert_eq!(checkbox.micro_radius, Some(ComponentRadius::Micro));
    assert_eq!(checkbox.shape, ShapeScale::None);
    // 其余 4 个不碰微半径层。
    for component in [
        Component::Radio,
        Component::Search,
        Component::NavigationDrawer,
        Component::Divider,
    ] {
        assert_eq!(component.spec().micro_radius, None, "{}", component.name());
    }
    // NavigationDrawer = CornerLargeEnd 的内侧角 16；Divider 无圆角。
    assert_eq!(Component::NavigationDrawer.spec().shape, ShapeScale::Large);
    assert_eq!(Component::Divider.spec().shape, ShapeScale::None);
    // Checkbox / Radio 的标签字级是团队口径（官方无「框 + 标签」token）。
    assert_eq!(Component::Checkbox.spec().text, TypeRole::BodyLarge);
    assert_eq!(Component::Radio.spec().text, TypeRole::BodyLarge);
    // Search 的字级：输入 BodyLarge、占位/辅助 BodySmall。
    assert_eq!(Component::Search.spec().text, TypeRole::BodyLarge);
    assert_eq!(Component::Search.spec().support, Some(TypeRole::BodySmall));
}
