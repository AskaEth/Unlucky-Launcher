//! Ink Cat 容器层可视性门禁（M228 债务裁决的可测一半）：状态容器 1.20 门禁、阶梯单调、
//! 以及「容器层不能当状态指示」的实测钉子。
//!
//! 与 `ink_cat_boards.rs` / `ink_cat_geometry.rs` 分开是单文件 ≤400 行的硬约束。

use unlucky_design::color::{BrandBoard, Color, Role, SchemeFlavor};
use unlucky_design::palette::visibility::{
    check_surface_hierarchy, LAYER_ROLES, STATE_CONTAINERS, STATE_CONTAINER_MIN,
};
use unlucky_design::palette::wcag::AA_TEXT;

#[test]
fn state_containers_clear_the_visibility_floor() {
    // 债务台账「SecondaryContainer 对 Surface 仅 1.06」的令牌层裁决：状态容器 ≥1.20:1。
    for board in BrandBoard::ALL {
        let report = check_surface_hierarchy(&board.scheme());
        assert!(
            report.state_failures().is_empty(),
            "{} 的状态容器未达 {STATE_CONTAINER_MIN}:1：{:?}",
            board.name(),
            report.state_failures()
        );
        let (role, ratio) = report.worst_state();
        assert!(
            ratio >= STATE_CONTAINER_MIN,
            "{} 最弱的状态容器 {} 只有 {ratio:.3}",
            board.name(),
            role.name()
        );
        // 旧 baseline 的现场值 1.057 必须被显著甩开，否则「修了但没修好」。
        assert!(
            ratio > 1.20,
            "{} 只是刚好卡线（{ratio:.3}），等于没修",
            board.name()
        );
    }
    // 两套板覆盖 4 个状态容器角色，一个不落。
    assert_eq!(STATE_CONTAINERS.len(), 4);
}

#[test]
fn surface_container_ladder_is_monotone_with_visible_steps() {
    for board in BrandBoard::ALL {
        let report = check_surface_hierarchy(&board.scheme());
        assert!(
            report.layer_violations().is_empty(),
            "{} 的容器阶梯有断层或方向错：{:?}",
            board.name(),
            report.layer_violations()
        );
        assert!(report.is_clean(), "{} 的容器层体检没过", board.name());
        // 方向：深色板越靠上越亮、浅色板越靠上越暗。
        let steps = report.layer_steps();
        for step in steps {
            let expect_positive = board.flavor() == SchemeFlavor::Dark;
            assert_eq!(
                step.delta_lstar > 0.0,
                expect_positive,
                "{} 的 {} 阶梯方向错",
                board.name(),
                step.role.name()
            );
        }
        assert_eq!(LAYER_ROLES.len(), 5);
    }
}

#[test]
fn container_layers_are_not_state_indicators() {
    // 裁决第 3 条的可测化一半：把「状态容器压在容器层上」的最差值钉住，提醒后来者
    // 别把它当唯一指示（浅色板必然贴到 1.02，深色板 1.54）。
    let worst_over_layers = |board: BrandBoard| {
        let scheme = board.scheme();
        STATE_CONTAINERS
            .iter()
            .flat_map(|role| {
                LAYER_ROLES
                    .iter()
                    .map(move |layer| (*role, *layer, scheme.contrast(*role, *layer)))
            })
            .map(|(_, _, ratio)| ratio)
            .fold(f32::INFINITY, f32::min)
    };
    let silver = worst_over_layers(BrandBoard::Silver);
    assert!(
        (1.0..1.1).contains(&silver),
        "Silver 的状态容器对容器层最低 = {silver:.3}，与「浅色 pale-on-pale 必然贴近」的登记（1.02）不符"
    );
    // Ink 相反：深色板的状态容器与容器层拉得开，所以深色可以靠底色差做弱指示。
    let ink = worst_over_layers(BrandBoard::Ink);
    assert!(
        ink >= 1.5,
        "Ink 最弱的状态容器对容器层 = {ink:.3}，应明显高于浅色板"
    );
    // 两条一起说明「为什么这不是调色问题而是构成问题」：同一套算法下深色能做的弱指示，
    // 浅色做不到——所以门禁不能按容器层设，只能按 surface 设。
    assert!(silver < ink);
}

#[test]
fn field_backdrop_is_the_same_role_on_both_boards() {
    // P2-1 的回归钉子：字段底在两套板都是 `surface-container-highest`。
    // 若把 Silver 的字段底挪回 `surface-container-high`（#E4E4E5），错误标签会从 4.69 掉到 4.38（AA 失守）。
    for board in BrandBoard::ALL {
        let scheme = board.scheme();
        for (fg, usage) in [
            (Role::OnSurfaceVariant, "字段标签"),
            (Role::OnSurface, "字段内容"),
            (Role::Error, "错误字段标签"),
        ] {
            let ratio = scheme.contrast(fg, Role::SurfaceContainerHighest);
            assert!(
                ratio >= AA_TEXT,
                "{} 的{usage}（{} on surface-container-highest）= {ratio:.3}",
                board.name(),
                fg.name()
            );
        }
    }
    // 浅色板的 high 必须比 highest 更亮、且是另一个色（阶梯方向 + 不许两档同值）。
    let silver = BrandBoard::Silver.scheme();
    let high = silver.get(Role::SurfaceContainerHigh);
    let highest = silver.get(Role::SurfaceContainerHighest);
    assert_ne!(high, highest);
    assert!(
        high.lstar() > highest.lstar(),
        "Silver 的 surface-container-high 应比 highest 更亮（高 {} vs 最高 {}）",
        high.lstar(),
        highest.lstar()
    );
    // 规格卡给的字段底就是 highest 那个值。
    assert_eq!(highest, Color::from_hex("#DCDDDE").expect("规格卡字段底"));
}
