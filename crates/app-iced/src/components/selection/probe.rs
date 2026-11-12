//! 选择类家族的**渲染层**几何断言：对着真截图量像素（M68 的教训：纯函数对、图可能是错的）。
//!
//! 为什么住在 `selection/` 而不是 `gallery/`：`gallery/selection.rs` 只负责「把矩阵摆进 harness」，
//! 断言加上去会把它顶到 400 行红线。像素扫描的 4 个纯函数在 [`super::pixel`]（同一目录、`cfg(test)`）。
//!
//! 量什么（每条对一个具体错误，容差 ±1）：
//!
//! - **Checkbox 外缘 20，且选中/未选中同尺寸**（§18.3：描边向内收；向外画会变 22）——逐块断言；
//! - **Radio 内点 10 不是 12**（§19.2）· 环是真圆、不是圆角方（§19.6）；
//! - **状态层直径 40 且四角无墨**（§18.5 / §19.7）——**每一块**都查；
//! - 勾形墨迹 ≈14.8（端点 12 + 方头外扩）· **indeterminate 是短横**（§18.4/§18.6）；
//! - Switch 轨道**恰好一块** 52×32（§10.1）· Chip **恰好一块** 32 高（§13.1）；
//! - 禁用：chip 只降不删（P2-1）· 选中禁用的 Checkbox 容器是 38% 不是 12%（P2-3）。
//!
//! 复现（worktree 根目录）：
//!
//! ```text
//! cargo build -p unlucky-app-iced --bin unlucky-launcher-iced
//! ULL_GALLERY_SECTION=selection ./target/debug/unlucky-launcher-iced \
//!   --software-renderer --screen=gallery \
//!   --screenshot target/m76/selection-light.png --screenshot-theme=light
//! ULL_GALLERY_SHOT=target/m76/selection-light.png cargo test -p unlucky-app-iced selection::probe
//! ```

use unlucky_design::color::{Role, SchemeFlavor};
use unlucky_design::typography::Platform;

use super::super::interaction::{self, Base, Interaction};
use super::pixel::{center, dims, ink_boxes, inner_ink, matches, near, rgb, shot_path, Shot};
use crate::theme::{Families, Ui};

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(flavor, Platform::Windows, FAMILIES)
}

/// 一条测试覆盖全部几何断言（浅/深各跑一次）。
#[test]
fn selection_geometry_matches_the_reference() {
    let Some(path) = shot_path() else {
        eprintln!(
            "[selection-probe] SKIPPED：未设置 ULL_GALLERY_SHOT。这是**渲染层**几何断言，\
             需要先出图（见本文件模块文档，注意 ULL_GALLERY_SECTION=selection）。"
        );
        return;
    };
    let shot = Shot::load(&path).expect("读 selection 截图");
    assert_eq!(
        (shot.width, shot.height),
        (1180, 760),
        "验收图必须 1180×760 @100% DPI"
    );
    let light = matches(
        shot.at(2, 2),
        rgb(ui(SchemeFlavor::Light).color(Role::Surface)),
    );
    let flavor = if light {
        SchemeFlavor::Light
    } else {
        SchemeFlavor::Dark
    };
    let ui = ui(flavor);
    let surface = ui.color(Role::Surface);
    let primary = ui.color(Role::Primary);
    let on_primary = ui.color(Role::OnPrimary);
    let unselected = ui.color(Role::OnSurfaceVariant);

    // ---- §18.1 / §18.3 / §19.1：20 的外缘，选中与未选中**同尺寸** -----------------
    // 逐块断言（复审 P2-4）：**每一块** Primary 都必须是 20(±1)——任何一块到 22 就说明描边
    // 画到了框外。只数「18×18 有几块」的时候，「把其中一格放大成 22」照样绿。
    let filled = ink_boxes(&shot, primary, surface, 15, 24);
    for bbox in &filled {
        assert!(
            bbox.width() <= 21 && bbox.height() <= 21,
            "18.3：Primary 块不得超过 21（22 = 描边画到框外），实测 {}×{} @ {:?}",
            bbox.width(),
            bbox.height(),
            (bbox.left, bbox.top)
        );
    }
    let twenty: Vec<_> = filled
        .iter()
        .copied()
        .filter(|bbox| near(bbox.width(), 20) && near(bbox.height(), 20))
        .collect();
    assert_eq!(
        twenty.len(),
        12,
        "18.1：应**恰好** 12 块 20(±1) 的 Primary（checkbox 选中 4 + 部分选中 4 + radio 环 4），\
         实际 {}：{}",
        twenty.len(),
        dims(&filled)
    );

    // 未选中（OnSurfaceVariant 描边）：逐块都必须 20(±1)，且内部透底
    let candidates = ink_boxes(&shot, unselected, surface, 17, 24);
    let rings: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|bbox| !matches(shot.at(center(bbox).0, bbox.top + 3), rgb(unselected)))
        .collect();
    assert!(
        !rings.is_empty(),
        "18.2：没找到未选中的复选框；候选：{}",
        dims(&candidates)
    );
    for bbox in &rings {
        assert!(
            near(bbox.width(), 20) && near(bbox.height(), 20),
            "18.3：未选中外缘必须与选中同尺寸 20（描边向内收），实测 {}×{} @ {:?}",
            bbox.width(),
            bbox.height(),
            (bbox.left, bbox.top)
        );
        assert!(
            !matches(shot.at(center(bbox).0, center(bbox).1), rgb(unselected)),
            "18.2：未选中框内部必须透出底色"
        );
    }
    // A1：hover/pressed/focused 三列的未选中描边提到 `OnSurface`
    let on_surface = ui.color(Role::OnSurface);
    let lifted: Vec<_> = ink_boxes(&shot, on_surface, surface, 17, 24)
        .into_iter()
        .filter(|bbox| !matches(shot.at(center(bbox).0, bbox.top + 3), rgb(on_surface)))
        .collect();
    assert!(
        lifted.len() >= 3,
        "A1：未选中复选框在 hover/pressed/focused 的描边应是 OnSurface（3 个），实际 {}",
        lifted.len()
    );
    for bbox in &lifted {
        assert!(
            near(bbox.width(), 20) && near(bbox.height(), 20),
            "18.3：提到 OnSurface 之后外缘仍是 20，实测 {}×{}",
            bbox.width(),
            bbox.height()
        );
    }

    // ---- §19.2：Radio 内点 **10 不是 12**；§19.6：环是真圆 ------------------------
    // 环/实心看**上沿内缩 3px 的点**（环在那里是内孔）；不看正中心——短横正好压中心。
    let mut dots = 0;
    for bbox in &twenty {
        if matches(shot.at(center(bbox).0, bbox.top + 3), rgb(primary)) {
            continue;
        }
        assert!(
            !matches(shot.at(bbox.left + 1, bbox.top + 1), rgb(primary)),
            "19.6：Radio 外圈必须是真圆（不是圆角方）@ {:?}",
            (bbox.left, bbox.top)
        );
        let dot = inner_ink(&shot, bbox, primary, surface, 4).expect("选中的 radio 必须有内点");
        assert!(
            near(dot.width(), 10) && near(dot.height(), 10),
            "19.2：Radio 内点可视直径 10（12 是含描边前的名义值），实测 {}×{}",
            dot.width(),
            dot.height()
        );
        assert_ne!(dot.width(), 12, "19.2：照名义值抄 12 会明显偏大");
        dots += 1;
    }
    assert_eq!(
        dots, 4,
        "19.2：应恰好 4 个选中的 radio（四态），实际 {dots}"
    );

    // ---- §18.4 / §18.6：勾形墨迹跨度；indeterminate 是**短横**不是勾 --------------
    let mut bars = 0;
    let mut checks = 0;
    for bbox in &twenty {
        let Some(glyph) = inner_ink(&shot, bbox, on_primary, primary, 3) else {
            continue;
        };
        if glyph.height() <= 4 {
            bars += 1;
            assert!(
                glyph.width() >= 12,
                "18.6：indeterminate 必须是居中短横，实测 {}×{}",
                glyph.width(),
                glyph.height()
            );
        } else {
            checks += 1;
            // 端点 12 + 方头两侧各 2/√2 ≈ 1.41 → 墨迹 ≈14.8（见 `ControlGeometry`）
            assert!(
                (13..=16).contains(&glyph.width()),
                "18.4：勾形墨迹横向应 ≈14.8（端点 12 + 方头外扩），实测 {}",
                glyph.width()
            );
            assert!(
                glyph.height() >= 6,
                "18.4：勾形有折点，实测高 {}",
                glyph.height()
            );
        }
    }
    assert!(
        bars >= 4 && checks >= 4,
        "18.4/18.6：勾 {checks} 个、短横 {bars} 个（各应 ≥4）"
    );

    // ---- §18.5 / §19.7：状态层直径 40，且**每一块**都是圆（四角无墨）----------------
    // hover 是 8%、pressed/focus 是 10%：**两种颜色都要查**。只查 8% 会漏掉「按下那一列被画成
    // 方块」——代码 reviewer 的第二次变异正好落在这个盲点上（实测它当时是绿的）。
    let mut circles = 0;
    for state in [Interaction::Hovered, Interaction::Pressed] {
        let layer = interaction::overlay_color(&ui, state, Role::OnSurfaceVariant, Role::Surface)
            .expect("可交互态有状态层");
        let found: Vec<_> = ink_boxes(&shot, layer, surface, 37, 43)
            .into_iter()
            .filter(|bbox| near(bbox.width(), 40) && near(bbox.height(), 40))
            .collect();
        assert!(
            found.len() >= 2,
            "18.5：{state:?} 的 Ø40 状态层应 ≥2 块（checkbox×3 + radio + switch），实测 {}：{}",
            found.len(),
            dims(&found)
        );
        for circle in &found {
            for (x, y) in [
                (circle.left, circle.top),
                (circle.right, circle.top),
                (circle.left, circle.bottom),
                (circle.right, circle.bottom),
            ] {
                assert!(
                    !matches(shot.at(x, y), rgb(layer)),
                    "18.5：{state:?} 状态层必须是圆（外接框四角不该有墨）@ ({x},{y})"
                );
            }
        }
        circles += found.len();
    }

    // ---- §10.1 Switch 轨道 52×32 · §13.1 Chip 高 32 -------------------------------
    let on_tracks: Vec<_> = ink_boxes(&shot, primary, surface, 28, 57)
        .into_iter()
        .filter(|bbox| near(bbox.width(), 52) && near(bbox.height(), 32))
        .collect();
    assert_eq!(
        on_tracks.len(),
        1,
        "10.1：选中的 switch 轨道应**恰好一块** 52×32，实测 {}：{}",
        on_tracks.len(),
        dims(&on_tracks)
    );
    let chips: Vec<_> = ink_boxes(&shot, ui.color(Role::SecondaryContainer), surface, 20, 140)
        .into_iter()
        .filter(|bbox| near(bbox.height(), 32))
        .collect();
    // 选中的 chip 在**四态**里容器都是 `SecondaryContainer`（状态层只叠在未选中的透明底上），
    // 所以这里应恰好 4 块——多于 4 说明别的组件的容器也是这个角色、且高 32。
    assert_eq!(
        chips.len(),
        4,
        "13.1：选中的 filter chip 应**恰好四块**（enabled/hover/pressed/focused 各一，32 高），\
         实测 {}：{}",
        chips.len(),
        dims(&chips)
    );
    assert!(
        chips[0].width() >= 58,
        "13.1：chip 宽应 ≥58（图标 18 + 两侧各 8 + 文字），实测 {}",
        chips[0].width()
    );

    // ---- 复审 P2-1：禁用的 chip **只降不删**（选中态与图标都要在）------------------
    // 判据：禁用列里必须**恰好有一块** 12% 填充的 chip（= filter·on）。旧 API 把 selected
    // 一起删掉，这一块根本不存在（实测 0 块）；能查到它、且宽度 ≥58，说明选中态与图标都还在。
    // （未选中的那三个禁用 chip 只有 1px 描边，落在半像素上时覆盖率不足 50%，量不稳，故不在此断言。）
    let disabled_container = interaction::disabled_colors(&ui, Base::CANVAS).0;
    let disabled_chips: Vec<_> = ink_boxes(&shot, disabled_container, surface, 28, 140)
        .into_iter()
        .filter(|bbox| near(bbox.height(), 32) && bbox.width() >= 58)
        .collect();
    assert_eq!(
        disabled_chips.len(),
        1,
        "P2-1：禁用列应**恰好一块**填充 chip（filter·on 的选中态不能被删掉），实测 {}：{}",
        disabled_chips.len(),
        dims(&disabled_chips)
    );
    assert!(
        disabled_chips[0].width() >= 58,
        "P2-1：禁用 chip 宽度应 ≥58（图标还在），实测 {}",
        disabled_chips[0].width()
    );

    // ---- 复审 P2-3：**选中且禁用**的 Checkbox 容器是 38%，不是 12% -----------------
    let disabled_ink =
        interaction::disabled_content_over_role(&ui, Base::CANVAS, Role::OnSurface, false);
    let ink38: Vec<_> = ink_boxes(&shot, disabled_ink, surface, 17, 24)
        .into_iter()
        .filter(|bbox| near(bbox.width(), 20) && near(bbox.height(), 20))
        .collect();
    assert!(
        ink38.len() >= 3,
        "18.8：禁用列应有 ≥3 块 20(±1) 的 38% 叠色（选中禁用填充 + 未选中禁用描边 + radio 环），\
         实测 {}：{}",
        ink38.len(),
        dims(&ink38)
    );
    let container_20: Vec<_> = ink_boxes(&shot, disabled_container, surface, 19, 21)
        .into_iter()
        .filter(|bbox| near(bbox.width(), 20) && near(bbox.height(), 20))
        .collect();
    assert!(
        container_20.is_empty(),
        "P2-3：18×18 的禁用块不许是 12% 容器色（那是普通禁用容器，选中禁用是 38%），实测 {}",
        dims(&container_20)
    );

    eprintln!(
        "[selection-probe] OK flavor={flavor:?}：20(±1) Primary 块 {} · 未选中环 {} · 内点 {dots} · \
         勾 {checks} / 短横 {bars} · 状态层 {} · 禁用 38% {} · 禁用 chip {}",
        twenty.len(),
        rings.len(),
        circles,
        ink38.len(),
        disabled_chips.len()
    );
}
