//! **M358**：内容库页在最小窗口下的纵向预算（`ScrollWhole` 档的列头收敛 + 导入区改序）。
//!
//! 判据、算式与「改回旧列头必红」的数据都在各条用例的文档里；像素证据在
//! `DevDocs/artifacts/contentbudget/`。

use unlucky_i18n::Lang;
use unlucky_presenter::dto::ContentKind;

use super::{demo, Status};
use crate::pages::content::layout::{
    compact_below, pack_before_results, BodyMode, MIN_BODY_HEIGHT,
};
use crate::pages::content::parts::{self, head};

/// 测试夹具：标准令牌下的 `Ui` + 最小窗口（880 宽）的页几何。
pub(super) fn fixture() -> (crate::theme::Ui, crate::pages::layout::FrameMetrics) {
    use unlucky_design::color::SchemeFlavor;
    use unlucky_design::typography::Platform;

    use crate::pages::layout::{frame_metrics, PaneKit};
    use crate::theme::{Families, Ui};

    let ui = Ui::with_platform(
        SchemeFlavor::Light,
        Platform::Windows,
        Families {
            text: "Segoe UI",
            display: "Segoe UI",
            mono: "Consolas",
        },
    );
    // 880 宽 = 窗口最小尺寸（`app::MIN_SIZE.width`），属 Expanded 单列。
    (ui, frame_metrics(PaneKit::Single, 880.0))
}

/// 880×560 的两种正文高：无提示条 380 / 带提示条 304（提示条 64 + 间距 12）。
pub(super) fn bodies(
    ui: &crate::theme::Ui,
    metrics: crate::pages::layout::FrameMetrics,
) -> (f32, f32) {
    let bare = crate::pages::frame::body_height(ui, metrics, 560.0, true);
    let with_banner = crate::pages::frame::body_height(ui, metrics.with_banner(76.0), 560.0, true);
    (bare, with_banner)
}

/// **M358 ①**：列头五项逐项按令牌相加，且与**像素实测**的 281 对上。
///
/// 这条同时钉两个来源：令牌（改搜索条高 / chip 高 / 行高 / 分隔线厚度 / 间距就红）与像素
/// （`DevDocs/artifacts/narrowfix/after/content-880-dark-zh.png`：正文顶 156、首卡顶 437）。
/// M357 r2 的错数之所以活了一轮，正是因为只有「常量 == 字面量」这一个来源。
#[test]
fn the_head_is_five_token_rows_and_matches_the_pixels() {
    use unlucky_design::spacing::Space;
    use unlucky_design::typography::TypeRole;

    use crate::app::components::selection::geometry::ChipGeometry;

    let (ui, _) = fixture();
    let model = demo(ContentKind::Mod, Lang::En);
    let gap = ui.space(Space::X2);

    // 逐项：搜索条 / 筛选四排 / 提示行 / 分隔线 / 计数行 + 五条 X2
    let search = ui.metrics().search_bar_height;
    let four_rows = 4.0 * ChipGeometry::of(&ui).height + 3.0 * gap;
    let notice = TypeRole::BodySmall.style().line_height_px;
    let divider = ui.metrics().divider_thickness;
    let count = TypeRole::LabelMedium.style().line_height_px;

    let full = parts::head::chrome_of(&ui, &model, parts::head::Kind::Full);
    assert_eq!(
        full,
        search + four_rows + notice + divider + count + 5.0 * gap
    );
    assert_eq!(
        (search, four_rows, notice, divider, count, 5.0 * gap),
        (56.0, 152.0, 16.0, 1.0, 16.0, 40.0),
        "令牌自身漂了：这一行是「令牌 → 像素」的唯一锚点"
    );
    assert_eq!(full, 281.0, "像素实测（437 − 156）与令牌相加必须同值");

    // 收敛档：筛选从四排（152）变成一排横滚（control_height + X2 = 48），其余四项不动
    let compact = parts::head::chrome_of(&ui, &model, parts::head::Kind::Compact);
    assert_eq!(
        compact,
        search + parts::strip_height(&ui) + notice + divider + count + 5.0 * gap
    );
    assert_eq!(full - compact, four_rows - parts::strip_height(&ui));
    assert_eq!(compact, 177.0);
}

/// **M358 ②（判据 · 带算式）**：880×560 要「正文 − 列头 ≥ 一张整卡」。
///
/// ```text
/// 无提示条：正文 380；旧列头 281 + 卡 80 = 361 ≤ 380  ✔（不收，与 M357 的图逐像素相同）
/// 带提示条：正文 304；旧列头 281 + 卡 80 = 361 > 304  ✘（改前现场：卡只露 304 − 281 = 23px）
///                    收敛 177 + 卡 80 = 257 ≤ 304  ✔
/// ```
///
/// **改回旧列头 ⇒ 这条必红**：`assert!(full + card > with_banner)` 反过来就是 361 ≤ 304。
#[test]
fn the_third_visible_card_arithmetic_pins_the_compact_head() {
    use head::Kind;

    let (ui, metrics) = fixture();
    let (bare, with_banner) = bodies(&ui, metrics);
    assert_eq!((bare, with_banner), (380.0, 304.0));

    let model = demo(ContentKind::Mod, Lang::En);
    assert_eq!(model.status, Status::Ready, "演示夹具的缺省态");
    // 一张整卡 = 卡高 76 + 行距 4（`ListItem` 三行档），由 `row_pitches` 量出来，不写死
    let card: f32 = parts::row_pitches(&ui, &model).take(2).sum();
    let full = head::chrome_of(&ui, &model, Kind::Full);

    // 无提示条 380：常规列头就够 ⇒ 不收敛（M357 的 after 图因此逐像素不变）
    let tall = head::plan(&ui, &model, BodyMode::ScrollWhole, bare);
    assert_eq!(tall.kind, Kind::Full);
    assert!(
        tail_ok(full, card, bare),
        "常规列头 {full} + 一张卡 {card} 装不进正文 {bare}"
    );
    assert_eq!(tall.viewport, Some(full + card), "视口 = 列头 + 一张整卡");

    // 带提示条 304：常规列头**不成立**（这就是改前必红的那一行）
    assert!(
        !tail_ok(full, card, with_banner),
        "算式不成立就别换列头：{full} + {card} ≤ {with_banner}"
    );
    assert_eq!(
        compact_below(full, card),
        361.0,
        "门槛 = 常规列头 + 一张整卡（两个入参都来自令牌）"
    );

    // 收敛列头：判据成立，且视口正好停在整卡边界上
    let short = head::plan(&ui, &model, BodyMode::ScrollWhole, with_banner);
    assert_eq!(short.kind, Kind::Compact);
    let compact = short.chrome.expect("Ready 态要按列头取整");
    assert!(
        tail_ok(compact, card, with_banner),
        "收敛列头 {compact} + 一张卡 {card} > 正文 {with_banner}"
    );
    assert_eq!(short.viewport, Some(compact + card));
    assert!(
        compact + card < full + card,
        "收敛必须真的把列头收下来（不然这条用例只是在描述现状）"
    );

    // 门槛永远落在 `ScrollWhole` 里 ⇒ `Split` 档（含 1180×900）根本不进这一档
    assert!(
        compact_below(full, card) < MIN_BODY_HEIGHT,
        "收敛门槛必须低于两档切点，否则 1180×900 的基线会被这条改动碰到"
    );
}

/// 判据本身（`正文 − 列头 ≥ 一张整卡`），单独写出来，算式两边都引用它。
fn tail_ok(chrome: f32, card: f32, body: f32) -> bool {
    body - chrome >= card
}

/// **M358 ③**：收敛档只在「装不下」时出现，其余三条前提照旧（M357 的 P2-1 / P1 兜底）。
#[test]
fn the_compact_head_only_shows_up_when_the_card_does_not_fit() {
    use head::Kind;

    let (ui, metrics) = fixture();
    let (bare, with_banner) = bodies(&ui, metrics);
    let model = demo(ContentKind::Mod, Lang::En);

    // 门槛两侧各一档：断言用**真令牌算出来的门槛**，不写 361 之外的数
    let full = head::chrome_of(&ui, &model, Kind::Full);
    for (body, expected) in [
        (bare, Kind::Full),
        (with_banner, Kind::Compact),
        (MIN_BODY_HEIGHT, Kind::Full),
        (MIN_BODY_HEIGHT - 1.0, Kind::Full),
    ] {
        assert_eq!(
            head::plan(&ui, &model, BodyMode::ScrollWhole, body).kind,
            expected,
            "正文 {body}（常规列头 {full}）选错了列头档"
        );
    }

    // 非 `Ready`：列表没画（`body` 是 `status_body`），`hits` 可能还留着上一批结果 ⇒ **不取整**
    // （M357 r2 的 P2-1）。但**档位照收**——列头是画的，收它是看得见的改动（v1 的 P2-1，r2 修）。
    let mut loading = demo(ContentKind::Mod, Lang::En);
    loading.status = Status::Loading;
    assert!(
        !loading.hits.is_empty(),
        "加载更多时旧结果还在（这正是危险条件）"
    );
    let plan = head::plan(&ui, &loading, BodyMode::ScrollWhole, with_banner);
    assert_eq!(
        plan.kind,
        Kind::Compact,
        "Loading 态列头也要收（v1 在这里退回 Full）"
    );
    assert_eq!(plan.chrome, None);
    assert_eq!(plan.viewport, None);

    // 装不下一张整卡（正文 < 收敛列头 + 卡）：退回 `Fill`，不把取整值当定高
    let tiny = head::chrome_of(&ui, &model, Kind::Compact) / 2.0;
    assert_eq!(
        head::plan(&ui, &model, BodyMode::ScrollWhole, tiny).viewport,
        None
    );

    // `Split` 档：列表自己滚，这里**从不**给定高（1180×900 的基线不变）
    let split = head::plan(&ui, &model, BodyMode::Split, 720.0);
    assert_eq!(split.kind, Kind::Full);
    assert_eq!(split.viewport, None, "Split 档不许走定高视口");
}

/// **M358 ④**：整合包的导入/导出区在矮窗里排到结果之后，于是列头重新从正文顶开始——
/// 首屏才放得下一张结果卡。`Split` 档仍在顶部，所以那里照旧不取整（M357 的判据保留）。
#[test]
fn the_modpack_import_block_moves_below_the_results_in_the_short_body() {
    use head::Kind;

    let (ui, metrics) = fixture();
    let (bare, with_banner) = bodies(&ui, metrics);
    let model = demo(ContentKind::Modpack, Lang::En);
    let card: f32 = parts::row_pitches(&ui, &model).take(2).sum();

    // 演示夹具是**纯入口**态（没有进行中 / 待决策 / 结果 / 展开的导出）
    assert!(model.pack.is_entry_only(), "演示夹具的导入区必须是纯入口");

    // **M374**：`Split` 档的导入区已经进了列表的滚动区 ⇒ 它不再挡在列头之上，列头重新在正文顶
    // （`chrome` 算得出来）；但 `Split` 档从不给定高视口 —— 列表自己滚（1180×900 的基线不变）。
    assert!(!pack_before_results(true, true));
    let split = head::plan(&ui, &model, BodyMode::Split, 720.0);
    assert_eq!((split.kind, split.viewport), (Kind::Full, None));
    assert!(split.chrome.is_some(), "Split 档列头在正文顶，账目对得上");

    // `ScrollWhole` + 纯入口：那块区排到结果之后 ⇒ 列头在正文顶，账目对得上
    let short = head::plan(&ui, &model, BodyMode::ScrollWhole, with_banner);
    assert_eq!(short.kind, Kind::Compact);
    let chrome = short.chrome.expect("列头在正文顶上，应当能取整");
    assert!(
        tail_ok(chrome, card, with_banner),
        "{chrome} + {card} > {with_banner}"
    );
    assert_eq!(short.viewport, Some(chrome + card));
    // 无提示条 380：常规列头就够（这一档与 Mod 一致）
    assert_eq!(
        head::plan(&ui, &model, BodyMode::ScrollWhole, bare).viewport,
        Some(head::chrome_of(&ui, &model, Kind::Full) + card)
    );

    // `ScrollWhole` + 进行中/待决策/有结果：那块区仍在顶部 ⇒ 不取整（高由文案撑，不在账上）
    let mut active = demo(ContentKind::Modpack, Lang::En);
    active.pack.export.open = true;
    assert!(!active.pack.is_entry_only());
    assert!(pack_before_results(true, false));
    assert_eq!(
        head::plan(&ui, &active, BodyMode::ScrollWhole, with_banner).chrome,
        None
    );
}

/// **M358 ⑤**：`is_entry_only` 的判据逐条对上 `packui::view::section` 的分支——
/// 只有「入口」那一支才算纯入口；其余四态（进行中 / 待决策 / 结果 / 取消占位 / 展开的导出）
/// 都要留在顶部。
#[test]
fn the_import_block_counts_as_entry_only_until_any_panel_takes_over() {
    let base = demo(ContentKind::Modpack, Lang::En).pack;
    assert!(base.is_entry_only());

    let mut export = base.clone();
    export.export.open = true;
    assert!(!export.is_entry_only(), "展开的导出面板不是入口");

    let mut cancelled = base.clone();
    cancelled.cancelled = true;
    assert!(!cancelled.is_entry_only(), "取消占位不是入口");

    // 输了一半路径仍是入口（与 `is_idle` 的差别就在这里）
    let mut drafting = base;
    drafting.draft = "D:/随便.mrpack".to_string();
    assert!(drafting.is_entry_only());
    assert!(!drafting.is_idle());
}

/// 接线：主列必须真的按 `plan` 画（列头两档 + 定高视口），别只把纯函数写出来没人用。
#[test]
fn the_main_column_wires_the_head_plan() {
    let page = include_str!("../view.rs");
    for needle in [
        "parts::head::plan(ui, model, mode, body_height)",
        "parts::head::element(ui, catalog, model, plan.kind)",
        "Length::Fixed(height)",
        // M374：导入区的位置与归属合成一条纯函数（`stack::pack_place`，判据与整张落点表在那里）
        "stack::pack_place(",
    ] {
        assert!(
            page.contains(needle),
            "`pages/content/view.rs` 里少了 `{needle}`：列头两档 / 定高视口 / 导入区位置有一处没接线"
        );
    }
    // 整卡节距必须真的来自 `ListItem` 的行数档（三行档 76 + 行距 4），不是拍一个常数
    assert!(
        include_str!("../parts.rs").contains("geometry.height(item.lines()), gap"),
        "结果卡的整行节距必须由 `ListItemGeometry` 量，不许写死"
    );
}
