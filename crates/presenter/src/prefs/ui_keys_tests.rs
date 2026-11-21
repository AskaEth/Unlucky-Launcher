//! 四个「UI 档位」键的往返用例（M383 / GAPS P4）。
//!
//! 判据是「写入 → **新建一份桥**（= 重启）→ 读回仍是新值」，外加「`launcher.json` 里真的出现
//! 这四个键」。两条缺一不可：只验桥的内存字段会绿着一个「其实没落盘」的实现（`persist` 返回
//! `true` 但 `save` 被 skip 过），只验 JSON 会绿着一个「写得进去、读不回来」的实现。
//!
//! 抓不住什么：真机上点一下按钮是否真的走到 `app-iced` 的写端点（那由 `app-iced` 的用例管：
//! `app/update/agent/rail_persist.rs` 与 `app/update/download_wiring.rs`）。本文件只到 presenter 层。

use super::ui_keys::{category_from_key_value, category_word};
use super::{keys, InstanceView, TileSize, UiKeys, UiPrefs};
use crate::dto::{AncientKind, VersionCategory};
use crate::settings_bridge::SettingsBridge;

/// 一次性临时目录（用例之间不共享；名字带 pid 以免并发跑测试时撞车）。
fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ull-prefs-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建临时目录");
    dir
}

/// 一条与出厂值**逐项不同**的取值：任何一格漏写/漏读都会让它红。
fn non_factory() -> UiKeys {
    UiKeys {
        ai_collapsed: true,
        tile_size: TileSize::Large,
        instance_view: InstanceView::List,
        download_category: VersionCategory::Ancient(AncientKind::OldBeta),
    }
}

/// **P4 的主证据**：四个键写入 → 重启 → 读回；且 `launcher.json` 里真的有这四个键。
#[test]
fn the_four_shelf_keys_survive_a_restart() {
    let dir = temp_dir("ui-keys");
    let path = dir.join("launcher.json");

    // 1) 空配置文件：缺键 ⇒ 出厂值（老配置不会被锁进某个档位）。
    let mut bridge = SettingsBridge::with_path(&path);
    assert_eq!(bridge.load_ui_keys(), UiKeys::default(), "缺键 → 出厂");

    // 2) 写入：四个键都要真的落盘。
    let wanted = non_factory();
    assert!(bridge.persist_ui_keys(&wanted), "四个键必须真的写进磁盘");

    // 3) 「重启」= 同一个路径重新 load 一份桥（进程内的长生命 store 不算）。
    let restarted = SettingsBridge::with_path(&path);
    assert_eq!(
        restarted.load_ui_keys(),
        wanted,
        "重启后 AI 栏 / 磁贴尺寸 / 视图 / 下载分类都必须还是新值"
    );

    // 4) 文件里真的出现这四个键（不是只在内存里生效）。
    let raw = std::fs::read_to_string(&path).expect("读回 launcher.json");
    let json: serde_json::Value =
        serde_json::from_str(&raw).expect("launcher.json 必须是合法 JSON");
    assert_eq!(json["ui"]["ai"]["collapsed"], serde_json::json!(true));
    assert_eq!(
        json["ui"]["instances"]["tile_size"],
        serde_json::json!("large")
    );
    assert_eq!(json["ui"]["instances"]["view"], serde_json::json!("list"));
    assert_eq!(
        json["ui"]["download"]["category"],
        serde_json::json!("ancient_old_beta")
    );

    // 5) 幂等：同一个值再写一次不该产生一次写盘（`mtime` 抖动会让热重读白跑一趟）。
    let mut again = SettingsBridge::with_path(&path);
    assert!(!again.persist_ui_keys(&wanted), "值没变 ⇒ 不写盘");

    let _ = std::fs::remove_dir_all(&dir);
}

/// core 的词表与 presenter 的枚举**逐值同序**（两处各有一份，这是唯一的一致性判据）。
///
/// 抓得住：有人给 `TileSize` 加了第四档却忘了登记 schema（或改了落盘词而没改 core 那张表）。
/// 抓不住：core 那张表自己写错（那由 `core::setting::keys::ui` 的用例管）。
#[test]
fn the_word_lists_are_the_core_schema_lists() {
    use unlucky_core::setting::keys::ui::{DOWNLOAD_CATEGORIES, INSTANCE_VIEWS, TILE_SIZES};
    let tiles: Vec<&str> = [TileSize::Small, TileSize::Medium, TileSize::Large]
        .iter()
        .map(|size| size.as_str())
        .collect();
    assert_eq!(tiles, TILE_SIZES);
    let views: Vec<&str> = [InstanceView::Tiles, InstanceView::List]
        .iter()
        .map(|view| view.as_str())
        .collect();
    assert_eq!(views, INSTANCE_VIEWS);

    // `VersionCategory` 的每个值逐一对应（五个变体、`Ancient` 摊平成四个 ⇒ 八项；
    // 顺序也照 `DOWNLOAD_CATEGORIES` 定）。
    let all = [
        VersionCategory::Release,
        VersionCategory::Snapshot,
        VersionCategory::Ancient(AncientKind::Classic),
        VersionCategory::Ancient(AncientKind::OldAlpha),
        VersionCategory::Ancient(AncientKind::OldBeta),
        VersionCategory::Ancient(AncientKind::Unlisted),
        VersionCategory::Modpack,
        VersionCategory::Mod,
    ];
    let words: Vec<&str> = all.iter().copied().map(category_word).collect();
    assert_eq!(words, DOWNLOAD_CATEGORIES);
    assert_eq!(words.len(), 8, "Ancient 的四个子类必须摊平成四个值");

    // 一张词表必须自己闭合：写出去再读回来等于原值（`Ancient` 的每个子类都要过）。
    for category in all {
        assert_eq!(
            category_from_key_value(category_word(category)),
            category,
            "{category:?} 的往返断了"
        );
    }
    for size in [TileSize::Small, TileSize::Medium, TileSize::Large] {
        assert_eq!(TileSize::from_key_value(size.as_str()), size);
    }
    for view in [InstanceView::Tiles, InstanceView::List] {
        assert_eq!(InstanceView::from_key_value(view.as_str()), view);
    }
}

/// 手改坏的值不许把界面锁在认不出的档位上：三格都回落出厂值。
#[test]
fn unknown_values_fall_back_to_the_factory_value() {
    let dir = temp_dir("ui-keys-bad");
    let path = dir.join("launcher.json");
    std::fs::write(
        &path,
        r#"{"$schema":"ull://launcher/1.10.0","ui":{"instances":{"tile_size":"huge","view":"grid"},"download":{"category":"ancient"}}}"#,
    )
    .expect("写坏配置");
    let bridge = SettingsBridge::with_path(&path);
    let keys = bridge.load_ui_keys();
    assert_eq!(keys.tile_size, TileSize::Medium);
    assert_eq!(keys.instance_view, InstanceView::Tiles);
    assert_eq!(keys.download_category, VersionCategory::Release);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `UiPrefs` 的四个字段与 [`UiKeys`] 是同一件事的两种表示——不许分叉。
#[test]
fn the_two_representations_of_the_shelf_keys_agree() {
    assert_eq!(UiPrefs::default().ui_keys(), UiKeys::default());
    let mut prefs = UiPrefs::default();
    prefs.adopt_ui_keys(non_factory());
    assert_eq!(prefs.ui_keys(), non_factory());
    // 另外三族字段一个都没被动过（`adopt_ui_keys` 只该碰那四格）。
    let untouched = UiPrefs::default();
    assert_eq!(prefs.nav_collapsed, untouched.nav_collapsed);
    assert_eq!(prefs.theme, untouched.theme);
    assert_eq!(prefs.appearance, untouched.appearance);
}

/// 四个键名与 `prefs::keys::ALL` 是同一组（改名等于迁移：两边必须一起改）。
#[test]
fn the_four_key_names_are_registered_in_all() {
    for key in [
        keys::AI_COLLAPSED,
        keys::TILE_SIZE,
        keys::INSTANCE_VIEW,
        keys::DOWNLOAD_CATEGORY,
    ] {
        assert!(keys::ALL.contains(&key), "{key} 不在 keys::ALL 里");
    }
    assert_eq!(keys::AI_COLLAPSED, "ui.ai.collapsed");
    assert_eq!(keys::TILE_SIZE, "ui.instances.tile_size");
    assert_eq!(keys::INSTANCE_VIEW, "ui.instances.view");
    assert_eq!(keys::DOWNLOAD_CATEGORY, "ui.download.category");
}

/// **`ui_key_present` 分得清「缺键」与「用户选了出厂值」**（M452 首启判据的地基）。
///
/// 为什么这三位必须存在：`ui.ai.collapsed` 的 schema 出厂是 `false`，而右栏首启是**折叠**；
/// `ui.download.category` 的 schema 出厂是 `release`，而下载页首启是「全部」。没有这一位，
/// 界面只能二选一：要么首启版面被 schema 出厂值改掉，要么永远读不回用户的选择。
///
/// 抓得住：把 `ui_key_present` 写成「`load_ui_keys` 的值等于出厂值就算没有」
/// （那样显式写 `false` / `release` 会被当成缺键，第二段红）。
#[test]
fn ui_key_present_discriminates_absent_from_factory() {
    let dir = temp_dir("ui-keys-present");
    let path = dir.join("launcher.json");
    let bridge = SettingsBridge::with_path(&path);
    for key in [
        keys::AI_COLLAPSED,
        keys::TILE_SIZE,
        keys::INSTANCE_VIEW,
        keys::DOWNLOAD_CATEGORY,
    ] {
        assert!(!bridge.ui_key_present(key), "空文件里 {key} 不该算出现");
    }

    // **显式写厂值的另一边**（`false` / `release`）：值是出厂，但键真的出现过。
    //
    // 注意不能拿 `persist_ui_keys(&UiKeys::default())` 造这份文件：那一次写会被
    // `persist_ui_keys` 判成 `Unchanged`（值没变 ⇒ 不写盘，这一条本身也是纪律），
    // 于是键仍旧「没出现过」。这里直接落一份显式写了出厂值的文件——那正是
    // `ull config set ui.ai.collapsed false` 会造出来的形状。
    std::fs::write(
        &path,
        r#"{"$schema":"ull://launcher/1.10.0","ui":{"ai":{"collapsed":false},"instances":{"tile_size":"medium","view":"tiles"},"download":{"category":"release"}}}"#,
    )
    .expect("写显式出厂值");
    let reopened = SettingsBridge::with_path(&path);
    for key in [
        keys::AI_COLLAPSED,
        keys::TILE_SIZE,
        keys::INSTANCE_VIEW,
        keys::DOWNLOAD_CATEGORY,
    ] {
        assert!(
            reopened.ui_key_present(key),
            "写了出厂值之后 {key} 必须算「出现过」"
        );
    }
    assert_eq!(
        reopened.load_ui_keys(),
        UiKeys::default(),
        "值是出厂值（所以「缺键」与「显式出厂值」只能靠 `ui_key_present` 分开）"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// **显式写出厂值也要真的落盘**（`ui.ai.collapsed = false` 是最容易被 `Unchanged` 短路吞掉的那一档）。
///
/// 为什么单独一条：`ui.ai.collapsed` 的 schema 出厂是 `false`，所以「用户显式展开」时**生效值**
/// 与缺键完全一样。若判据只看生效值，这次写会被当成「没改」跳过 ⇒ 盘上永远没有这一键 ⇒
/// 重启回到界面出厂档（折叠），也就是本 mission 要修的那个缺陷原封不动地留在最常用的一档上。
///
/// 抓得住：`persist_ai_collapsed` 改回「`persist_ui_keys` + 默认快照」的写法（第一次就 `false`）。
#[test]
fn writing_the_factory_value_materialises_the_key() {
    let dir = temp_dir("ui-keys-factory");
    let path = dir.join("launcher.json");
    let mut bridge = SettingsBridge::with_path(&path);
    assert!(!bridge.ui_key_present(keys::AI_COLLAPSED), "前提：缺键");
    assert!(
        bridge.persist_ai_collapsed(false),
        "缺键时显式写出厂值（false）必须真的落盘"
    );
    assert!(
        SettingsBridge::with_path(&path).ui_key_present(keys::AI_COLLAPSED),
        "盘上必须出现这一键"
    );
    assert!(
        !bridge.persist_ai_collapsed(false),
        "键已在且值没变 ⇒ 不写盘"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// **单键写入口只碰它自己那一键**：别人写下的另外三键原样留在盘上。
///
/// 抓得住：`persist_ai_collapsed` 改成「拿 `UiKeys::default()` 改一格再 `persist_ui_keys`」——
/// 那会把 `ull config set ui.instances.tile_size large` 抹回 `medium`（M174 同类缺陷缩到四键）。
#[test]
fn the_single_key_writers_leave_the_other_keys_alone() {
    let dir = temp_dir("ui-keys-single");
    let path = dir.join("launcher.json");
    let mut bridge = SettingsBridge::with_path(&path);
    assert!(bridge.persist_ui_keys(&non_factory()));

    // 只改 AI 栏：另外三键必须还是 non_factory 那份
    assert!(bridge.persist_ai_collapsed(false), "值变了就得落盘");
    let after_ai = SettingsBridge::with_path(&path).load_ui_keys();
    assert!(!after_ai.ai_collapsed);
    assert_eq!(after_ai.tile_size, TileSize::Large);
    assert_eq!(after_ai.instance_view, InstanceView::List);
    assert_eq!(
        after_ai.download_category,
        VersionCategory::Ancient(AncientKind::OldBeta)
    );

    // 只改下载分类：磁贴尺寸 / 视图依旧
    assert!(bridge.persist_download_category(VersionCategory::Modpack));
    let after_category = SettingsBridge::with_path(&path).load_ui_keys();
    assert_eq!(after_category.download_category, VersionCategory::Modpack);
    assert_eq!(after_category.tile_size, TileSize::Large);
    assert_eq!(after_category.instance_view, InstanceView::List);
    assert!(!after_category.ai_collapsed);

    // 幂等：同一个值再写一次不产生写盘
    assert!(!bridge.persist_ai_collapsed(false), "值没变 ⇒ 不写盘");
    let _ = std::fs::remove_dir_all(&dir);
}
