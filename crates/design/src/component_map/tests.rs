//! `component_map` 的守门测试：表槽位 ↔ 枚举次序、官方高程/形状、圆角分层（D32）、微度量入口。
//!
//! 拆成独立文件是为了让 `mod.rs` 留出文档空间（项目硬约束：单文件 ≤400 行）。

use super::*;

#[test]
fn table_matches_component_order() {
    for (i, spec) in table::SPECS.iter().enumerate() {
        assert_eq!(
            spec.component as usize, i,
            "SPECS 第 {i} 行顺序与枚举不一致"
        );
    }
}

#[test]
fn every_component_resolves() {
    assert_eq!(Component::ALL.len(), table::SPECS.len());
    for component in Component::ALL {
        let spec = component.spec();
        assert_eq!(spec.component, component);
        assert!(spec.text.style().size_px >= 11.0);
    }
}

#[test]
fn names_are_unique() {
    let mut seen = std::collections::BTreeSet::new();
    for component in Component::ALL {
        assert!(
            seen.insert(component.name()),
            "组件名重复：{}",
            component.name()
        );
    }
}

#[test]
fn elevation_matches_official_component_tokens() {
    // A 级来源（androidx.compose.material3.tokens）：
    //   FilledCardTokens/OutlinedCardTokens.ContainerElevation = Level0
    //   MenuTokens.ContainerElevation = Level2
    //   DialogTokens.ContainerElevation = Level3
    //   SnackbarTokens.ContainerElevation = Level3
    //   AppBarTokens = Level0、ListTokens.ItemContainerElevation = Level0
    // 偏离只允许出现在有登记的地方（Tooltip 是 B 级：token 文件里没有 Elevation 行）。
    for (component, expect) in [
        (Component::FilledCard, Elevation::Level0),
        (Component::OutlinedCard, Elevation::Level0),
        (Component::Menu, Elevation::Level2),
        (Component::Dialog, Elevation::Level3),
        (Component::Snackbar, Elevation::Level3),
        (Component::TopAppBar, Elevation::Level0),
        (Component::ListItem, Elevation::Level0),
        (Component::TextField, Elevation::Level0),
    ] {
        assert_eq!(
            component.spec().elevation,
            expect,
            "{} 的高程与官方组件令牌不一致",
            component.name()
        );
    }
    // R4「全屏都在飘」的守门：一屏常见元素里带阴影（>Level0）的只允许是浮层。
    for component in Component::ALL {
        if component.spec().elevation > Elevation::Level0 {
            assert!(
                matches!(
                    component,
                    Component::Menu | Component::Dialog | Component::Snackbar | Component::Tooltip
                ),
                "{} 不该有阴影（只有浮层允许）",
                component.name()
            );
        }
    }
}

#[test]
fn shapes_match_official_component_tokens() {
    // 官方：卡片 CornerMedium(12)、对话框 CornerExtraLarge(28)、菜单 CornerExtraSmall(4)、
    // 按钮/导航指示条 CornerFull、chip CornerSmall(8)。
    assert_eq!(Component::FilledCard.spec().shape, ShapeScale::Medium);
    assert_eq!(Component::OutlinedCard.spec().shape, ShapeScale::Medium);
    assert_eq!(Component::Dialog.spec().shape, ShapeScale::ExtraLarge);
    assert_eq!(Component::Menu.spec().shape, ShapeScale::ExtraSmall);
    assert_eq!(Component::PrimaryButton.spec().shape, ShapeScale::Full);
    assert_eq!(Component::Chip.spec().shape, ShapeScale::Small);
}

#[test]
fn only_primary_actions_use_filled_emphasis_and_one_dialog_layer() {
    // 契约 §1：状态色不复用 accent、每屏最多一个实心 accent → 只有 primary-button 是
    // 「实心强调」，其余按钮的形态由页面决定但不能是 Full + Level0 + Emphasized 的组合。
    let primary = Component::PrimaryButton.spec();
    assert_eq!(primary.shape, ShapeScale::Full);
    assert_eq!(primary.text, TypeRole::LabelLarge);
    // 浮层类必须比卡片层高。
    assert!(Component::Dialog.spec().elevation > Component::OutlinedCard.spec().elevation);
    assert!(Component::Menu.spec().elevation > Component::FilledCard.spec().elevation);
}

#[test]
fn morphs_always_expand_upwards() {
    for component in Component::ALL {
        if let Some(morph) = component.spec().morph {
            assert_ne!(
                morph.from,
                morph.to,
                "{} 的形变起止不能相同",
                component.name()
            );
            // 方向必须「沿阶梯胀开」（morph 对 `Full` 也没问题，rank 把它排在末尾），
            // 不比较像素跨度：`Full` 的半径依赖边长，用固定边长比会被 `None` 之外的大档拉爆。
            assert!(
                morph.from.rank() < morph.to.rank(),
                "{} 的形变方向反了：{:?} → {:?}",
                component.name(),
                morph.from,
                morph.to
            );
        }
    }
}

#[test]
fn appended_variants_keep_the_frozen_order() {
    // 枚举冻结为「只可末尾追加」：前 20 个的次序不能动（M69 只在末尾加 5 个）。
    assert_eq!(Component::ALL.len(), 25);
    assert_eq!(Component::ALL[0], Component::TopAppBar);
    assert_eq!(Component::ALL[19], Component::EmptyState);
    assert_eq!(Component::ALL[20], Component::Checkbox);
    assert_eq!(Component::ALL[24], Component::Divider);
    for (i, component) in Component::ALL.iter().enumerate() {
        assert_eq!(*component as usize, i, "ALL 的次序与枚举判别值不一致");
    }
}

#[test]
fn micro_radius_only_on_micro_controls() {
    // D32：微半径只能给 ≤24dp 的微控件，且此时 `shape` 必须是占位 None（不参与渲染）。
    let mut owners = Vec::new();
    for component in Component::ALL {
        let spec = component.spec();
        let Some(radius) = spec.micro_radius else {
            continue;
        };
        owners.push(component);
        assert_eq!(radius, ComponentRadius::Micro, "白名单外的微半径不许过表");
        assert_eq!(
            spec.shape,
            ShapeScale::None,
            "{} 有微半径时 shape 必须占位 None（2dp 不在刻度里）",
            component.name()
        );
        let metrics = component
            .metrics()
            .expect("带微半径的组件必须有 ComponentMetrics");
        assert!(
            metrics.width <= crate::shape::MICRO_CONTROL_MAX_SIZE
                && metrics.height <= crate::shape::MICRO_CONTROL_MAX_SIZE,
            "{} 超出微控件尺寸上限",
            component.name()
        );
        assert_eq!(
            radius.radius_px(metrics.width),
            Some(radius.dp()),
            "{} 的微半径按自身尺寸取不到",
            component.name()
        );
    }
    assert_eq!(
        owners,
        vec![Component::Checkbox],
        "当前只允许 Checkbox 用微半径"
    );
}

#[test]
fn metrics_resolve_only_for_controls_with_inner_geometry() {
    for component in [
        Component::Checkbox,
        Component::Radio,
        Component::Search,
        Component::NavigationDrawer,
    ] {
        assert!(
            component.metrics().is_some(),
            "{} 缺 ComponentMetrics",
            component.name()
        );
    }
    // Divider 只有厚度，而厚度是一像素线（LM），不该出现在 CM 里。
    assert!(Component::Divider.metrics().is_none());
    // 骨架级尺寸不许在 CM 里重复一份（Single Source of Truth）。
    for component in [Component::Search, Component::NavigationDrawer] {
        let metrics = component.metrics().unwrap();
        assert_eq!(metrics.width, 0.0, "{} 的宽度属骨架级", component.name());
        assert_eq!(metrics.height, 0.0, "{} 的高度属骨架级", component.name());
    }
}
