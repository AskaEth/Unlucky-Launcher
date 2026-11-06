//! 下载页的**文案守门**（M105 从 `download/tests.rs` 搬出：那边加了安装选项的用例之后顶破 400 行）。
//!
//! 两条各守一件事：死 key（目录里有、页面没用）与「模型说的是目录的语言」。

use unlucky_i18n::{Key, Lang};
use unlucky_presenter::dto::VersionKind;

use super::*;

// --------------------------------------------------------------- 文案（i18n）

/// 每个 `Download*` key 都**真的被页面引用**（死 key 守门，M94 r1 / P2-6）。
///
/// 复审抓到的 `DownloadSelected` 就是这条要拦的东西：目录里定义了、页面没人用。所以这里把页面
/// 源码用 `include_str!` 编进来扫一遍。**抓不住什么也说清**：反向（引用了不存在的 key）是编译错误，
/// 不需要测试；译文对不对、两套语言都填了没由 `i18n` 自己的 `built_in_catalogs_have_every_key` 守；
/// 它只看字面出现，用宏拼 key 名能骗过它。
#[test]
fn every_download_key_is_referenced_by_the_page() {
    const SOURCES: [&str; 12] = [
        include_str!("../../download.rs"),
        include_str!("../model.rs"),
        // M107：分类枚举搬进 `model/category.rs`（`Category::label_key` 在那儿，漏扫会误判死 key）
        include_str!("../model/category.rs"),
        include_str!("../rows.rs"),
        // M107：两级侧栏与分组规则（`VersionRailTitle` / `VersionMajorAll` 的落点）
        include_str!("../rails.rs"),
        // M125：主版本栏搬进 `rails/major.rs`（`VersionMajorTitle` / `VersionMajorAll` 的落点）
        include_str!("../rails/major.rs"),
        include_str!("../group.rs"),
        include_str!("../install.rs"),
        include_str!("../states.rs"),
        include_str!("../demo.rs"),
        // M105：安装选项区的两条填页线（漏扫的话新 key 会被判成死 key）
        include_str!("../options/model.rs"),
        include_str!("../options/view.rs"),
    ];

    let mut checked = 0;
    for key in Key::ALL
        .iter()
        .filter(|key| key.name().starts_with("Download") || key.name().starts_with("Version"))
    {
        let needle = format!("Key::{}", key.name());
        assert!(
            SOURCES.iter().any(|source| source.contains(&needle)),
            "{needle} 在目录里定义了，但下载页没有任何地方用它（死 key，删掉或接上）"
        );
        checked += 1;
    }
    assert!(
        checked > 20,
        "下载页的 key 不该被成批删掉（只查到 {checked} 条）"
    );
    assert!(
        checked > 50,
        "M107 之后这一页的 key（Download* + Version*）不该少于 50 条，只查到 {checked}"
    );
}

/// 页面级的文案全部按 `Catalog` 走：中文目录出中文，英文目录出英文，**且不含 core 的中文业务串**。
///
/// 抓得住：页面里又出现硬编码英文/中文字面量、或把 `InstallStage::label()` / `progress.message`
/// （core 的中文串）画到界面上。
/// 抓不住：目录里那条译文本身写错了。
#[test]
fn the_model_speaks_the_catalog_language() {
    let versions = vec![version("1.21.9", VersionKind::Release, false)];
    let state = state_with(versions);

    let en = Download::from_snapshot(
        &state,
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    let zh = Download::from_snapshot(
        &state,
        Category::All,
        &InstallDraft::default(),
        Lang::ZhCn.catalog(),
    );

    assert_eq!(en.rows[0].supporting, "Release · released 2026-09-20");
    assert_eq!(zh.rows[0].supporting, "正式版 · 发布于 2026-09-20");

    let progress = InstallProgressView {
        version: "1.21.9".to_string(),
        stage: InstallStage::Planning,
        message: "正在解析版本清单…".to_string(),
        ..InstallProgressView::new()
    };
    let install_en = crate::pages::download::model::InstallView::of(&progress, Lang::En.catalog());
    assert_eq!(install_en.stage_label, "Planning");
    assert_eq!(install_en.detail, "Waiting for the first file…");
    assert!(
        !install_en.stage_label.contains('解'),
        "core 的中文阶段串不许直接上屏"
    );
    assert!(
        !install_en.detail.contains('正'),
        "`InstallProgressView::message` 是中文业务串，页面不许画它"
    );

    let install_zh =
        crate::pages::download::model::InstallView::of(&progress, Lang::ZhCn.catalog());
    assert_eq!(install_zh.stage_label, "解析版本");
}
