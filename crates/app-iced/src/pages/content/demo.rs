//! 内容库的演示数据（`pages_demo` 出图与单测用；产品路径永远走 `Content::from_snapshot`）。
//!
//! ## 为什么要环境变量选「哪一屏」
//!
//! D30 的验收矩阵要求「四类内容 × 浅/深」加「搜索无结果」「加载器不兼容禁用」两种状态，
//! 共 10 张。`pages_demo` 的 `--screen=` 只够选到**哪一页**，选不到「同一页的哪一态」；
//! 而 `pages/harness.rs` 与 `cli.rs` 不在本 mission 的 scope。
//!
//! 所以沿用本仓既有的取证开关范式（`ULL_START_ROUTE` / `ULL_APPEARANCE_*` / `ULL_TICK_TRACE`）：
//!
//! | 变量 | 取值 | 作用 |
//! |---|---|---|
//! | `ULL_CONTENT_KIND` | `modpack` / `mod` / `resourcepack` / `shader` | 选内容类型（默认 `mod`） |
//! | `ULL_CONTENT_STATE` | `ok` / `empty` / `incompatible` | 选状态（默认 `ok`） |
//!
//! 只影响本来就用演示数据的 harness 路径；产品二进制里 `demo()` 根本不会被调用。

use unlucky_i18n::{Catalog, Lang};
use unlucky_presenter::dto::{ContentHit, ContentKind, ContentSort, ContentSource, LoaderKind};

use super::model::{blockers_of, Content, ContentQueryView, Hit, HitFacts, Status, Target};
use crate::pages::download::Download;

/// 一个固定的演示环境：两个实例（一个 Fabric / 一个 Forge）。
fn targets() -> Vec<Target> {
    vec![
        Target {
            id: "survival".to_string(),
            name: "Survival".to_string(),
            mc_version: "1.20.1".to_string(),
            loader: LoaderKind::Fabric,
        },
        Target {
            id: "modded".to_string(),
            name: "Modded".to_string(),
            mc_version: "1.21.1".to_string(),
            loader: LoaderKind::Forge,
        },
    ]
}

/// 四条命中项：**先造 `ContentHit` 再走 `Hit::of`**，保证演示数据与产品路径逐字段同源
/// （自己拼 `Hit` 的那一版会把「1,200,000 下载量」「Fabric 首字母大写」这类格式化漂开）。
fn hits(kind: ContentKind, catalog: Catalog, compatible_only: bool) -> Vec<Hit> {
    let rows: &[(&str, &str, &str)] = match kind {
        ContentKind::Modpack => &[
            (
                "Fabulously Optimized",
                "robotkoer",
                "Performance-focused vanilla+ pack",
            ),
            ("Adrenaline", "Someone", "Lightweight kitchen-sink pack"),
            ("Better MC", "Luna", "Adventure pack with quests"),
            ("All the Mods 9", "ATMTeam", "Kitchen sink, 400+ mods"),
        ],
        ContentKind::Mod => &[
            ("Sodium", "jellysquid3", "Modern rendering engine"),
            ("Lithium", "jellysquid3", "Game logic optimizations"),
            ("Iris Shaders", "coderbot", "Shader support for Fabric"),
            ("JEI", "mezz", "Item and recipe viewer"),
        ],
        ContentKind::Resourcepack => &[
            (
                "Faithful 32x",
                "Faithful",
                "Double resolution vanilla textures",
            ),
            ("Stay True", "Fred", "Warm, hand-drawn look"),
            ("Classic 3D", "Someone", "3D blocks and items"),
            ("Bare Bones", "Someone", "Flat, minimalist textures"),
        ],
        ContentKind::Shader => &[
            ("Complementary Reimagined", "Emin", "Balanced, high quality"),
            ("BSL Shaders", "capttatsu", "Soft colors, strong weather"),
            ("Photon", "SixthSurge", "Path-traced look"),
            ("MakeUp Ultra Fast", "Someone", "Lightweight shader"),
        ],
    };

    rows.iter()
        .enumerate()
        .map(|(index, (title, author, summary))| {
            // 前两条只支持 1.20.1 + Fabric（能装进 Survival）；后两条是 1.21.1 + Forge
            let compatible = compatible_only || index < 2;
            Hit::of(
                &ContentHit {
                    id: format!("{}-{index}", kind.slug()),
                    slug: format!("{}-{index}", kind.slug()),
                    title: (*title).to_string(),
                    summary: (*summary).to_string(),
                    author: (*author).to_string(),
                    downloads: 1_200_000u64.saturating_sub(index as u64 * 111_111),
                    updated: format!("2026-0{}-1{index}", index + 1),
                    source: if index % 2 == 0 {
                        ContentSource::Modrinth
                    } else {
                        ContentSource::Curseforge
                    },
                    game_versions: if compatible {
                        vec!["1.20.1".to_string()]
                    } else {
                        vec!["1.21.1".to_string()]
                    },
                    loaders: if compatible {
                        vec!["fabric".to_string()]
                    } else {
                        vec!["forge".to_string()]
                    },
                    icon: None,
                },
                catalog,
            )
        })
        .collect()
}

/// 装入中的面板（**复用下载页那块**：`pages::download::install_panel_model` + `install_panel`）。
fn install_panel(catalog: Catalog) -> Download {
    use unlucky_presenter::dto::{InstallProgressView, InstallStage};
    let progress = InstallProgressView {
        version: "Sodium".to_string(),
        instance: "survival".to_string(),
        loader: "Fabric".to_string(),
        stage: InstallStage::Downloading,
        files_done: 0,
        files_total: 1,
        message: "sodium-fabric-0.6.0.jar".to_string(),
        ..InstallProgressView::new()
    };
    crate::pages::download::install_panel_model(&progress, catalog)
        .expect("演示数据的进度是「下载中」，面板必然存在")
}

/// 内建模型（默认 `ok` + 从环境变量读类型/状态）。
pub(super) fn demo(fallback_kind: ContentKind, lang: Lang) -> Content {
    let catalog = lang.catalog();
    let kind = std::env::var("ULL_CONTENT_KIND")
        .ok()
        .and_then(|value| {
            ContentKind::ALL
                .into_iter()
                .find(|item| item.slug() == value.trim())
        })
        .unwrap_or(fallback_kind);
    let state = std::env::var("ULL_CONTENT_STATE").unwrap_or_else(|_| "ok".to_string());

    let targets = targets();
    let target = Some("survival".to_string());
    let empty = state == "empty";
    // 「装入中」那一屏：面板就是下载页那块（同一个函数），用来证明它在内容库语义下也可读
    let installing = state == "install";
    let list = if empty {
        Vec::new()
    } else {
        hits(kind, catalog, state != "incompatible")
    };
    // 「不兼容」那一屏：选中的那条是 1.21.1 + Forge 的项目，而目标是 1.20.1 + Fabric
    // 「CF 未接通」那一屏：选中的那条来自 CurseForge（文件端点还没接，按钮必须禁用）
    let selected_index = match state.as_str() {
        "incompatible" => 2,
        "cf" => 1,
        _ => 0,
    };
    let selected = list.get(selected_index).map(|hit| hit.id.clone());
    let blockers = blockers_of(
        list.get(selected_index).map(|hit| HitFacts {
            versions: hit.versions.as_slice(),
            loaders: hit.loaders.as_slice(),
            source: hit.source,
        }),
        kind,
        &targets,
        target.as_deref(),
    );
    let target_label = targets
        .iter()
        .find(|item| Some(&item.id) == target.as_ref())
        .map(|item| item.label());

    // M113: import/export —— 导入导出区自己那一态由 `ULL_CONTENT_PACK` 选
    // （`importing` / `partial` / `conflict` / `export`；不设 = 只有入口）
    let pack = super::packui::demo_pack(lang).0;

    Content {
        kind,
        query: ContentQueryView {
            source: ContentSource::Modrinth,
            sort: ContentSort::Relevance,
            version: Some("1.20.1".to_string()),
            loader: Some(LoaderKind::Fabric),
            text: if empty {
                "quarkonium".to_string()
            } else {
                String::new()
            },
        },
        hits: list,
        selected,
        status: if empty { Status::Empty } else { Status::Ready },
        total: if empty { 0 } else { 1_284 },
        // 演示数据故意把 `has_more` 置真（真实判据是「本页装满」）：出图要能看到「加载更多」
        has_more: !empty,
        curseforge: false,
        targets,
        target: target.clone(),
        target_label,
        blockers,
        panel: if installing {
            Some(install_panel(catalog))
        } else {
            None
        },
        versions: vec!["1.20.1".to_string(), "1.21.1".to_string()],
        draft: if empty {
            "quarkonium".to_string()
        } else {
            String::new()
        },
        pack,
    }
}
