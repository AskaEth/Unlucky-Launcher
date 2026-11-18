//! Ink Cat 双主题的**消费方视角**门禁（M228）：色板完整性、双主题切换、对比度。
//!
//! 只用公开 API（`unlucky_design::...`），所以它同时是「iced 适配层能拿到什么」的样本：
//! 换肤时只需要 `BrandBoard` → `Scheme` → `Role`，一个色值字面量都不用抄。
//! 几何与字级校核在 `ink_cat_geometry.rs`，容器层可视性在 `ink_cat_visibility.rs`
//! （拆开是单文件 ≤400 行的硬约束）。

use unlucky_design::color::BrandBoard::{Ink, Silver};
use unlucky_design::color::{BrandBoard, Color, Role, SchemeFlavor};
use unlucky_design::palette::wcag::{check, AA_TEXT};
use unlucky_design::DesignTokens;

// ---------------------------------------------------------------- 色板完整性

#[test]
fn every_board_covers_all_49_roles_and_reports_its_flavor() {
    for board in BrandBoard::ALL {
        let scheme = board.scheme();
        assert_eq!(scheme.flavor(), board.flavor());
        assert_eq!(scheme.iter().count(), Role::ALL.len());
        for role in Role::ALL {
            let color = scheme.get(role);
            assert!(
                color.is_opaque(),
                "{} 的 {} 不是不透明色",
                board.name(),
                role.name()
            );
            assert_eq!(color, board.get(role), "两个入口必须同源");
        }
    }
}

#[test]
fn spec_anchors_survive_the_public_api() {
    // 规格卡上写死的色值（两套板各自的一批），一份都不许漂。
    let cases: [(BrandBoard, Role, &str); 12] = [
        (BrandBoard::Ink, Role::Primary, "#E8FFEF"),
        (BrandBoard::Ink, Role::OnPrimary, "#256B4B"),
        (BrandBoard::Ink, Role::Surface, "#0F150F"),
        (BrandBoard::Ink, Role::SurfaceContainerHighest, "#262B25"),
        (BrandBoard::Ink, Role::Error, "#FF716C"),
        (BrandBoard::Ink, Role::Outline, "#7B8A7B"),
        (BrandBoard::Silver, Role::Primary, "#006499"),
        (BrandBoard::Silver, Role::PrimaryContainer, "#87C7FF"),
        (BrandBoard::Silver, Role::Surface, "#F9F9FA"),
        (BrandBoard::Silver, Role::SecondaryContainer, "#D4E4F6"),
        (BrandBoard::Silver, Role::OnSurface, "#1A1C1D"),
        (BrandBoard::Silver, Role::Error, "#AC3434"),
    ];
    for (board, role, hex) in cases {
        assert_eq!(
            board.scheme().get(role),
            Color::from_hex(hex).expect("规格卡色值"),
            "{} / {} 与规格卡的 {hex} 不符",
            board.name(),
            role.name()
        );
    }
}

// ---------------------------------------------------------------- 双主题切换

#[test]
fn follow_system_resolves_to_the_right_board() {
    // 「跟随系统」是设置层的第三种态，令牌层只提供映射。
    assert_eq!(
        BrandBoard::for_flavor(SchemeFlavor::Dark),
        BrandBoard::Ink,
        "系统深色 → Ink"
    );
    assert_eq!(
        BrandBoard::for_flavor(SchemeFlavor::Light),
        BrandBoard::Silver,
        "系统浅色 → Silver"
    );
    for board in BrandBoard::ALL {
        assert_eq!(
            BrandBoard::for_flavor(board.flavor()),
            board,
            "映射必须可逆"
        );
    }
}

#[test]
fn switching_boards_swaps_all_the_surface_roles() {
    let ink = BrandBoard::Ink.scheme();
    let silver = BrandBoard::Silver.scheme();
    // 窗口底 / 正文 / 字段底 / 分层容器：切板后全都得换。
    for role in [
        Role::Surface,
        Role::OnSurface,
        Role::SurfaceContainerLow,
        Role::SurfaceContainerHighest,
        Role::Primary,
        Role::SecondaryContainer,
        Role::Error,
        Role::InverseSurface,
    ] {
        assert_ne!(
            ink.get(role),
            silver.get(role),
            "{} 在两套板之间没变——换肤会露出旧颜色",
            role.name()
        );
    }
    // 深浅判定：Ink 是真的深、Silver 是真的浅。
    assert!(ink.get(Role::Surface).relative_luminance() < 0.02);
    assert!(silver.get(Role::Surface).relative_luminance() > 0.90);
}

#[test]
fn design_tokens_brand_entry_is_the_product_default() {
    let ink = DesignTokens::brand(BrandBoard::Ink);
    assert_eq!(ink.flavor, SchemeFlavor::Dark);
    assert_eq!(ink.color(Role::Surface), BrandBoard::Ink.get(Role::Surface));
    // 品牌板与「取色链」两条路径互不干扰：baseline 仍是 tone 派生（D38 的锚点）。
    let baseline = DesignTokens::baseline(SchemeFlavor::Dark);
    assert_eq!(baseline.flavor, SchemeFlavor::Dark);
    assert_ne!(
        baseline.color(Role::Surface),
        ink.color(Role::Surface),
        "baseline 若已变成品牌板，D38 取色链的表面色锚点就没了"
    );
}

// ---------------------------------------------------------------- 对比度

#[test]
fn both_boards_pass_the_full_wcag_table() {
    for board in BrandBoard::ALL {
        let report = check(&board.scheme());
        assert!(
            report.is_clean(),
            "{} 有 {} 项未达 AA：{:?}",
            board.name(),
            report.failures.len(),
            report.failures
        );
        assert!(
            report.worst >= AA_TEXT,
            "{} 最紧的一对只有 {:.2}",
            board.name(),
            report.worst
        );
    }
}

/// 规格卡里真实出现的「文字压在什么色上」组合（25 组，含 `on-*-container` 全族）。
///
/// 只列规格卡上画出来的用法——不列「有可能这么用」的组合，否则就是在替页面线编需求。
const SPEC_TEXT_ON_SURFACE: [(BrandBoard, Role, Role, &str); 25] = [
    (Ink, Role::OnPrimary, Role::Primary, "Filled 按钮"),
    (
        Ink,
        Role::OnSecondaryContainer,
        Role::SecondaryContainer,
        "Tonal 按钮",
    ),
    (Ink, Role::OnSurface, Role::Surface, "Outlined/Text 按钮"),
    (Ink, Role::Primary, Role::Surface, "Text 按钮标签"),
    (
        Ink,
        Role::Primary,
        Role::SurfaceContainerLowest,
        "Elevated 标签",
    ),
    (
        Ink,
        Role::OnPrimaryContainer,
        Role::PrimaryContainer,
        "FAB 图标",
    ),
    (
        Ink,
        Role::OnSurfaceVariant,
        Role::SurfaceContainerHighest,
        "字段标签",
    ),
    (
        Ink,
        Role::OnSurface,
        Role::SurfaceContainerHighest,
        "字段内容",
    ),
    (
        Ink,
        Role::Error,
        Role::SurfaceContainerHighest,
        "错误字段标签",
    ),
    (Ink, Role::Error, Role::Surface, "错误提示"),
    (Ink, Role::OnError, Role::Error, "徽标数字"),
    (
        Ink,
        Role::OnSurfaceVariant,
        Role::Surface,
        "chip 未选 / 次要图标",
    ),
    (
        Ink,
        Role::InverseOnSurface,
        Role::InverseSurface,
        "snackbar 文字",
    ),
    (
        Ink,
        Role::InversePrimary,
        Role::InverseSurface,
        "snackbar 动作",
    ),
    (
        Ink,
        Role::OnSurfaceVariant,
        Role::SurfaceContainer,
        "卡片副文案",
    ),
    (
        Ink,
        Role::OnSurfaceVariant,
        Role::SurfaceContainerHigh,
        "对话框正文",
    ),
    (Silver, Role::OnPrimary, Role::Primary, "Filled 按钮"),
    (
        Silver,
        Role::OnSecondaryContainer,
        Role::SecondaryContainer,
        "Tonal 按钮",
    ),
    (Silver, Role::OnSurface, Role::Surface, "Outlined 按钮"),
    (Silver, Role::Primary, Role::Surface, "Text 按钮标签"),
    (
        Silver,
        Role::OnSurfaceVariant,
        Role::SurfaceContainerHighest,
        "字段标签",
    ),
    (
        Silver,
        Role::OnSurface,
        Role::SurfaceContainerHighest,
        "字段内容",
    ),
    (
        Silver,
        Role::Error,
        Role::SurfaceContainerHighest,
        "错误字段标签",
    ),
    (
        Silver,
        Role::OnSurfaceVariant,
        Role::SurfaceContainer,
        "卡片副文案",
    ),
    (
        Silver,
        Role::InverseOnSurface,
        Role::InverseSurface,
        "snackbar 文字",
    ),
];

#[test]
fn spec_text_pairs_meet_aa() {
    let mut failures = Vec::new();
    for (board, fg, bg, usage) in SPEC_TEXT_ON_SURFACE {
        let ratio = board.scheme().contrast(fg, bg);
        if ratio < AA_TEXT {
            failures.push(format!(
                "{} / {usage}：{} on {} = {ratio:.2}",
                board.name(),
                fg.name(),
                bg.name()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "规格卡用法未达 AA（4.5:1）：\n{}",
        failures.join("\n")
    );
}
