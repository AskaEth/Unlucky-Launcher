//! 下载页的**演示数据**（截图 harness 与单测的输入，产品路径不读）。
//!
//! 与 `pages/instances/data.rs` 同一条纪律：演示条目是**页面侧常量**，不是业务 DTO——
//! 于是核心 DTO 每改一次字段，基线图不会跟着漂。真页面的数据只走
//! [`Download::from_snapshot`](super::model::Download::from_snapshot)。
//!
//! 演示条目本身也是一步「DTO → 视图模型」的真转换（[`VersionRow::of`](super::model::VersionRow)、
//! [`InstallView::of`](super::model::InstallView)）：图上看到的副文案 / 进度行就是产品路径
//! 那条转换的产物，不是另写一套拼串。
//!
//! ## 为什么要一个环境变量
//!
//! 状态矩阵（加载 / 空 / 有数据 / 安装中 / 安装失败）都要出图，而 harness 的
//! `--screen` 只选页面、选不了页面状态：
//!
//! ```text
//! ULL_DL_DEMO=empty cargo run -p unlucky-app-iced --example pages_demo -- \
//!   --screen=download --screenshot=out.png --screenshot-theme=light --software-renderer
//! ```
//!
//! 取值：`ready`（缺省）/ `loading` / `empty` / `selected` / `category-release` / `installing` /
//! `installing-late` / `installing-empty` / `failed` / `done` / `done-next` /
//! **`options` / `options-fabric` / `options-incompat`**（M105 的安装选项区三态）/
//! **`options-liteloader`**（M222：LiteLoader 已可装，取样点用它的最高原版 1.12.2）/
//! **`category-ancient` / `search` / `search-major` / `search-empty` / `search-elsewhere`**
//! （M107 的两级侧栏与搜索取样点）。
//!
//! M107 那几个取样点**都走产品那条真转换**（`Download::from_snapshot`，查询词写在快照的
//! `download_query` 里）：图证明的是产品路径的行为，不是演示的特权。
//!
//! `category-release` / `done` / `done-next` 都**不手搓结果**，而是拿一份 `AppState` 跑
//! [`Download::from_snapshot`]：图证明的是产品路径的行为（分类本地过滤、装完后面板让回列表），
//! 不是演示的特权。
//!
//! `done` / `done-next` 是「**刚装完**」的两个采样点：它们不手搓模型，而是拿一份
//! `AppState`（版本列表 + `InstallStage::Done`）跑 [`Download::from_snapshot`]，所以图上看到的是
//! 产品路径的真行为（复审 p2：`Done` 之后面板必须让回列表，否则第二个版本装不了）。
//! **只有 harness 与演示走这里**：产品路径由 `app/view.rs` 从 presenter 快照建模型。

use unlucky_presenter::dto::{
    AncientKind, InstallProgressView, InstallStage, VersionKind, VersionSummary,
};

use unlucky_presenter::dto::{AppEvent, CoreEvent};
use unlucky_presenter::state::AppState;

use unlucky_i18n::Catalog;

use super::model::{Category, Download, InstallView, VersionRow};
use super::options::{InstallDraft, Loader};

/// 取证开关的名字（[`demo`] 读它）。
pub const DEMO_STATE_ENV: &str = "ULL_DL_DEMO";

/// 演示模型：按 [`DEMO_STATE_ENV`] 选状态，缺省是「有数据」。
pub fn demo(catalog: Catalog) -> Download {
    match std::env::var(DEMO_STATE_ENV).ok().as_deref() {
        Some("loading") => Download::loading(),
        Some("empty") => Download {
            loaded: true,
            ..Download::loading()
        },
        Some("installing") => Download {
            install: Some(install(InstallStage::Downloading, 0.42, catalog)),
            selected: Some("1.21.9".to_string()),
            ..ready(catalog)
        },
        // 同一状态的第二个采样点：进度条随**事件**推进（不是动画），出图时两帧对照
        Some("installing-late") => Download {
            install: Some(install(InstallStage::Downloading, 0.8, catalog)),
            selected: Some("1.21.9".to_string()),
            ..ready(catalog)
        },
        // M105：安装选项区三态（未选加载器 / 已选 Fabric / 不兼容组合）。
        // 三条都走**产品那条真转换**（`from_snapshot` + 草稿），图里看到的是页面真会画的东西。
        Some("options") => options(catalog, "1.21.9", Loader::Vanilla, None),
        Some("options-fabric") => options(
            catalog,
            "1.21.9",
            Loader::Fabric,
            Some(vec!["0.16.5", "0.16.4", "0.15.11"]),
        ),
        // NeoForge 最早只到 1.20.1，远古版（列表里的 `rd-132211`）根本没有构建 ⇒ 选中即报原因
        Some("options-incompat") => options(catalog, "rd-132211", Loader::NeoForge, None),
        // M222 的取样点：选中 1.12.2 + LiteLoader（外加「这里只叠原版」那条中性说明）
        Some("options-liteloader") => options_liteloader(catalog),
        Some("selected") => Download {
            selected: Some("1.21.9".to_string()),
            ..ready(catalog)
        },
        Some("failed") => Download {
            install: Some(install(InstallStage::Failed, 0.0, catalog)),
            selected: Some("1.21.9".to_string()),
            ..ready(catalog)
        },
        // 装完之后：走**产品那条真转换**（见 `finished`），图里看到的就是页面真会画的东西
        Some("done") => finished(catalog, None),
        // 装完之后再点下一个版本：面板必须重新给出实心「安装」
        Some("done-next") => finished(catalog, Some("1.21.9-pre3")),
        // 安装进行中、但列表被过滤空：面板要跟着占位态一起在（复审 p3 的边角）
        // 分类筛选（本页本地过滤：真 host 丢弃 ListVersions 的分类参数）——混合清单里只留正式版
        Some("category-release") => category(catalog, Category::Release),
        // M107 的两级侧栏 + 搜索：这三种状态**都走产品那条真转换**（from_snapshot），
        // 图里看到的是页面真会画的东西，不是演示的特权。
        Some("category-ancient") => searched(catalog, Category::Ancient, ""),
        Some("search") => searched(catalog, Category::All, "1.20"),
        Some("search-major") => majored(searched(catalog, Category::All, ""), "1.21"),
        Some("search-empty") => searched(catalog, Category::All, "zzz"),
        Some("search-elsewhere") => searched(catalog, Category::Release, "24w"),
        // 复审 P2-1 的取样点：**选中 1.21 桶的同时搜 1.20**。修好之后列表里是 2 行命中、
        // 侧栏点亮「全部版本」（第二级在搜索期间放宽），不会出现「2 个匹配 + 没有版本」。
        Some("search-over-bucket") => majored(searched(catalog, Category::All, "1.20"), "1.21"),
        // 复审 P2-2 的取样点：**22 个主版本桶**（MOJANG 清单里 `Release` 就是这个量级）。
        // chips 档一行放不下 ⇒ 主版本行必须能横向滚，否则后面的桶永远点不到。
        Some("many-buckets") => many(catalog),
        Some("installing-empty") => Download {
            install: Some(install(InstallStage::Downloading, 0.42, catalog)),
            ..Download::loading()
        },
        _ => ready(catalog),
    }
}

/// 有数据：八行（四条正式、两条预览、两条远古），覆盖四种版本形态与三个主版本桶。
///
/// 不复用 `mock_data::mock_versions()`：演示数据是**出图的基线**，得比 mock 稳定
/// （mock 随 P3 的假数据一起漂）。
fn ready(catalog: Catalog) -> Download {
    let versions = demo_versions();
    let rows: Vec<VersionRow> = versions
        .iter()
        .map(|version| VersionRow::of(version, catalog))
        .collect();

    Download {
        // 混合分类 → 没有单个 chip 该亮（这正是 `Category::of` 的反推结果）
        category: Category::of(&versions),
        buckets: super::group::buckets(&rows),
        hits: rows.len(),
        rows,
        selected: None,
        options: None,
        install: None,
        loaded: true,
        query: String::new(),
        selected_major: None,
        type_rail_folded: false,
    }
}

/// 「用户选了版本、正在填安装选项」的演示（M105）。
///
/// 走真转换：`AppState`（版本列表 + 可选的加载器版本缓存）+ 草稿 → [`Download::from_snapshot`]。
/// 于是图上「默认取最新加载器版本」「不兼容给具名原因」「确认键禁用」都是产品行为，不是演示特权。
fn options(catalog: Catalog, version: &str, loader: Loader, cached: Option<Vec<&str>>) -> Download {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(
        demo_versions(),
    )));
    if let Some(items) = cached {
        state.apply(AppEvent::Core(CoreEvent::LoaderVersions {
            kind: loader.kind(),
            mc_version: version.to_string(),
            result: Ok(items.into_iter().map(str::to_string).collect()),
        }));
    }
    let draft = InstallDraft {
        selected: Some(version.to_string()),
        loader,
        ..InstallDraft::default()
    };
    Download::from_snapshot(&state, Category::All, &draft, catalog)
}

/// M222 的取样点：**选中 1.12.2 + LiteLoader**（安装选项区的第四个状态）。
///
/// 为什么单独一个函数而不是复用 [`options`]：LiteLoader 的官方版本表只覆盖 `1.5.2`~`1.12.2`，
/// 而 [`demo_versions`] 是别处基线图的行集（`tests/view.rs` 钉着 8 行），不能为这个取样点改它。
/// 这里只是给**本状态**的版本列表补一条 `1.12.2`，其余照旧走产品那条真转换
/// [`Download::from_snapshot`]：图上的 chip 行、中性说明、确认键状态都是产品行为。
fn options_liteloader(catalog: Catalog) -> Download {
    const MC: &str = "1.12.2";
    let mut versions = demo_versions();
    versions.push(VersionSummary {
        id: MC.to_string(),
        kind: VersionKind::Release,
        released: "2017-09-18".to_string(),
        installed: false,
    });
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(versions)));
    // 真版本表里 LiteLoader 对 1.12.2 只有这一条（快照分支），照实写。
    state.apply(AppEvent::Core(CoreEvent::LoaderVersions {
        kind: Loader::LiteLoader.kind(),
        mc_version: MC.to_string(),
        result: Ok(vec!["1.12.2-SNAPSHOT".to_string()]),
    }));
    let draft = InstallDraft {
        selected: Some(MC.to_string()),
        loader: Loader::LiteLoader,
        ..InstallDraft::default()
    };
    Download::from_snapshot(&state, Category::All, &draft, catalog)
}

/// 22 个主版本桶（`1.21 … 1.0`）的清单：chips 档的横向滚动取证用。
fn many(catalog: Catalog) -> Download {
    let versions = (0..=21)
        .rev()
        .map(|minor| VersionSummary {
            id: format!("1.{minor}.4"),
            kind: VersionKind::Release,
            released: "2026-09-20".to_string(),
            installed: false,
        })
        .collect();

    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(versions)));
    Download::from_snapshot(&state, Category::All, &InstallDraft::default(), catalog)
}

/// 演示用的版本表（`ready` 与 `finished` 共用；与 presenter 的 `mock_versions` 同形）。
///
/// 四种形态各有样本：`1.21.9`（正式）、`1.21.9-pre3`（预发布）、`24w14a`（周更）、
/// `rd-132211` / `a1.2.6`（远古，没有主版本）。
fn demo_versions() -> Vec<VersionSummary> {
    fn summary(id: &str, kind: VersionKind, released: &str, installed: bool) -> VersionSummary {
        VersionSummary {
            id: id.to_string(),
            kind,
            released: released.to_string(),
            installed,
        }
    }

    vec![
        summary("1.21.9", VersionKind::Release, "2026-09-20", false),
        summary("1.21.8", VersionKind::Release, "2026-07-11", true),
        summary("1.20.4", VersionKind::Release, "2023-12-07", true),
        summary("1.20.1", VersionKind::Release, "2023-06-12", false),
        summary("1.21.9-pre3", VersionKind::Snapshot, "2026-09-01", false),
        summary("24w14a", VersionKind::Snapshot, "2024-04-03", false),
        summary(
            "rd-132211",
            VersionKind::Ancient(AncientKind::Classic),
            "2009-05-13",
            false,
        ),
        summary(
            "a1.2.6",
            VersionKind::Ancient(AncientKind::OldAlpha),
            "2010-08-11",
            false,
        ),
    ]
}

/// 「刚装完一个版本」的演示：**走产品那条真转换**（`AppState` → `Download::from_snapshot`）。
///
/// 为什么不用手搓模型：本轮复审的 bug 正是「`Done` 之后 `install` 从不清」——手搓一个
/// `install: None` 的模型会让图**假绿**（图上好看，真路径照样卡死）。这里真的走一遍快照转换，
/// 于是图能证明「装完之后面板让回列表」是产品路径的行为，不是演示的特权。
///
/// `select_next`：演「用户接着点了下一个版本」，面板应当重新给出实心「安装」。
fn finished(catalog: Catalog, select_next: Option<&str>) -> Download {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(
        demo_versions(),
    )));
    state.apply(AppEvent::Core(CoreEvent::InstallProgress(Some(
        InstallProgressView {
            version: "1.21.8".to_string(),
            stage: InstallStage::Done,
            files_done: 512,
            files_total: 512,
            ..InstallProgressView::new()
        },
    ))));

    let mut model =
        Download::from_snapshot(&state, Category::All, &InstallDraft::default(), catalog);
    model.selected = select_next.map(str::to_string);
    model
}

/// 「用户点了某个分类」的演示：走真转换（`from_snapshot` 的分类过滤），不手搓过滤结果。
fn category(catalog: Catalog, wanted: Category) -> Download {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(
        demo_versions(),
    )));
    Download::from_snapshot(&state, wanted, &InstallDraft::default(), catalog)
}

/// 「用户在搜索框里敲了几个字符」的演示：查询词写进**快照字段**（产品路径就是这么活的）。
fn searched(catalog: Catalog, wanted: Category, query: &str) -> Download {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(
        demo_versions(),
    )));
    state.download_query = query.to_string();
    Download::from_snapshot(&state, wanted, &InstallDraft::default(), catalog)
}

/// 「用户点了第二级侧栏里的一个主版本桶」的演示（纯视图态，与 `on_download` 的落点一致）。
fn majored(mut model: Download, key: &str) -> Download {
    model.selected_major = Some(key.to_string());
    model
}

/// 一次安装的演示快照（文件计数 / 字节 / 速度 / 剩余时间都给真值，进度条画得出来）。
fn install(stage: InstallStage, ratio: f32, catalog: Catalog) -> InstallView {
    let total = 512;
    let done = (f64::from(ratio) * total as f64) as u64;
    let failed = matches!(stage, InstallStage::Failed);

    InstallView::of(
        &InstallProgressView {
            version: "1.21.9".to_string(),
            instance: "1.21.9".to_string(),
            // 演示也**不**搬 core 的中文串：`loader` 字段这里给的是结构化徽标的英文名
            loader: "Vanilla".to_string(),
            stage,
            files_done: if failed { 137 } else { done },
            files_total: total,
            bytes_done: 0,
            bytes_total: 0,
            speed_bps: if failed { 0 } else { 1_258_291 },
            eta_secs: if failed { 0 } else { 75 },
            message: String::new(),
            // 演示里这次失败不是内容库那条路来的，不给分类（面板照旧画「失败 + 文案」）。
            failure_kind: None,
            // 同理：没有失败名单 ⇒ 内容库那块「只重试这些」不出现（下载页也不画它）。
            retry: Vec::new(),
        },
        catalog,
    )
}
