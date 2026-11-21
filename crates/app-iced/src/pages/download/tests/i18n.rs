//! 下载页的**文案守门**（M105 从 `download/tests.rs` 搬出：那边加了安装选项的用例之后顶破 400 行）。
//!
//! 两条各守一件事：死 key（目录里有、页面没用）与「模型说的是目录的语言」。

use unlucky_i18n::{Key, Lang};
use unlucky_presenter::dto::VersionKind;

use crate::pages::tests::scan;

use super::*;

// --------------------------------------------------------------- 文案（i18n）

/// M338 实测：`pages/download.rs` + `pages/download/**` 下的非测试文件有 18 个。
///
/// 下界而不是等号：加文件不该假红；**清空 / 扫不到**必须红（空集合上的全称断言恒真）。
const KNOWN_FAMILY_SOURCES: usize = 18;

/// 每个 `Download*` / `Version*` key 都**真的被页面引用**（死 key 守门，M94 r1 / P2-6）。
///
/// 复审抓到的 `DownloadSelected` 就是这条要拦的东西：目录里定义了、页面没人用。所以这里把页面
/// 源码扫一遍。**抓不住什么也说清**：反向（引用了不存在的 key）是编译错误，
/// 不需要测试；译文对不对、两套语言都填了没由 `i18n` 自己的 `built_in_catalogs_have_every_key` 守；
/// 它只看字面出现，用宏拼 key 名能骗过它。
///
/// ## M338：参加者从**目录派生**（不再手写 14 条 `include_str!`）
///
/// 原状：这 14 条清单是手写的，下载页新增一个源文件（`rows/tail.rs` / `options/fade.rs` 就是后来的）
/// 而忘了登记，那一族的 key 引用就**扫不到**——判据会把它误判成「死 key」，或者反过来：
/// 真正的死 key 因为别的文件还留着引用而无声通过。现在扫的是**本页那一族**
/// （`pages/download.rs` + `pages/download/**`），新文件自动参加；`include_str!` 当年「文件没了就
/// 编译不过」的保护由 [`scan`] 补回：读不到目录 / 文件一律 panic。
#[test]
fn every_download_key_is_referenced_by_the_page() {
    let sources = scan::family("download");
    assert!(
        sources.len() >= KNOWN_FAMILY_SOURCES,
        "下载页这一族只扫到 {} 个源文件（已知至少 {KNOWN_FAMILY_SOURCES} 个）：目录派生读不到\
         源码树了，这条判据会变成空转",
        sources.len()
    );

    let mut checked = 0;
    for key in Key::ALL
        .iter()
        .filter(|key| key.name().starts_with("Download") || key.name().starts_with("Version"))
    {
        let needle = format!("Key::{}", key.name());
        assert!(
            sources.iter().any(|file| file.text.contains(&needle)),
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
