//! 三个设置键的真读写（M197）：落盘 / 冷读回 / 生效值与来源层。
//!
//! 每一条都能变红（报告的「变异证明」逐条给了改哪一行会红）：
//!
//! | 守卫 | 断言 | 改回什么会红 |
//! |---|---|---|
//! | 三键真落盘 | [`the_three_keys_are_written_and_read_back`] | 删掉 `prefs_store::save` 里任一 `store.set(...)` |
//! | 回读即真值 | 同上（`adopt` 之后草稿 = 磁盘值） | 让 `save` 返回草稿而不是 `load(config)` |
//! | 落点层级 | [`an_empty_install_root_follows_the_data_mode`] | 让 `effective` 忽略 `java.installRoot` |
//! | 顺序层级 | [`an_empty_priority_follows_the_region`] | 让 `order_layer` 恒为 `Region` |
//! | 超长路径不写盘 | [`an_overlong_install_root_is_refused_before_writing`] | 删掉 `Prefs::validate` 的长度判据 |

use std::path::Path;

use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use super::super::prefs::{OrderLayer, Prefs, RootLayer, MAX_INSTALL_ROOT};
use super::super::prefs_store;
use super::super::state::Refusal;
use super::super::store::WriteError;
use super::{config, root};

/// 把 `core.data_mode` 显式写成某一档（测试不依赖这台机器的出厂默认）。
fn write_data_mode(file: &Path, mode: &str) {
    let mut store = SettingStore::load(file, launcher_schema());
    store
        .set("core.data_mode", mode.to_string().into())
        .expect("schema 认识 core.data_mode");
    store.save().expect("写盘");
}

/// 三键真写进 `launcher.json`，再从盘上冷读一次（= 重启后）仍一致。
///
/// 变异证明：删掉 `prefs_store::save` 里 `store.set("java.auto_install", …)` 那一行 ⇒ 本用例红
/// （`snapshot.auto_install` 回到 schema 默认 `true`）；把 `Ok(load(config))` 改成 `Ok(*prefs)`
/// 之类「回显草稿」⇒ 前后空格那条断言红。
#[test]
fn the_three_keys_are_written_and_read_back() {
    let dir = root("prefs-save");
    let file = config(&dir);

    // 前后带空格：落盘值会被 trim，回读的草稿必须显示**磁盘上那份**。
    let mut prefs = Prefs {
        install_root: "  D:/JDKs  ".to_string(),
        auto_install: false,
        priority: vec!["zulu", "temurin"],
        ..Prefs::default()
    };
    assert!(prefs.dirty(), "改过三键就该是脏的");

    let snapshot = prefs_store::save(&file, &prefs).expect("三键应能落盘");
    assert_eq!(snapshot.install_root, "D:/JDKs", "落盘值已 trim");
    assert!(!snapshot.auto_install);
    assert_eq!(snapshot.priority, vec!["zulu", "temurin"]);

    prefs.adopt(snapshot.clone());
    assert_eq!(prefs.install_root, "D:/JDKs", "回读后的草稿 = 磁盘值");
    assert!(!prefs.dirty(), "回读之后不再脏");

    // 冷读（另一个 store 实例，模拟重启）。
    let again = prefs_store::load(&file);
    assert_eq!(again, snapshot, "从盘上读回来的与写进去的一致");
}

/// `java.installRoot` 留空 ⇒ 生效落点来自 `core.data_mode`；填了 ⇒ 来自设置键。
///
/// 变异证明：把 `prefs_store::effective` 里 `override_root` 那段删掉（当作永远没设）⇒ 后半段红。
#[test]
fn an_empty_install_root_follows_the_data_mode() {
    let dir = root("prefs-root");
    let file = config(&dir);
    write_data_mode(&file, "portable");

    let prefs = Prefs::default();
    let effective = prefs_store::effective(&file, &prefs);
    assert_eq!(effective.root_layer, RootLayer::DataMode);
    assert_eq!(effective.root_mode, "portable");
    assert!(
        effective.root.ends_with("java-managed"),
        "portable 落点是数据根里的 java-managed：{}",
        effective.root
    );
    assert!(
        effective.root.starts_with(&dir.display().to_string()),
        "portable 落点必须在这个数据根里：{}",
        effective.root
    );

    let custom = Prefs {
        install_root: "D:/JDKs".to_string(),
        saved_install_root: "D:/JDKs".to_string(),
        ..Prefs::default()
    };
    let effective = prefs_store::effective(&file, &custom);
    assert_eq!(effective.root_layer, RootLayer::Setting);
    assert_eq!(effective.root_mode, "custom");
    assert_eq!(effective.root, "D:/JDKs", "设了就按它来，不再拼数据根");
}

/// `java.vendorPriority` 留空 ⇒ 地区默认序；排过 ⇒ 顺序与来源层都变。
///
/// 变异证明：把 `order_layer` 的判据改成恒 `Region` ⇒ 后半段红。
#[test]
fn an_empty_priority_follows_the_region() {
    let dir = root("prefs-order");
    let file = config(&dir);

    let prefs = Prefs::default();
    let effective = prefs_store::effective(&file, &prefs);
    assert_eq!(effective.order_layer, OrderLayer::Region);
    assert_eq!(effective.order.len(), 4, "四家都要在链上");

    let sorted = Prefs {
        priority: vec!["zulu"],
        saved_priority: vec!["zulu"],
        ..Prefs::default()
    };
    let effective = prefs_store::effective(&file, &sorted);
    assert_eq!(effective.order_layer, OrderLayer::Setting);
    assert_eq!(effective.order[0], "Azul Zulu", "排过的在第一");
    assert_eq!(effective.order.len(), 4, "补集追加，其余仍在链上");
}

/// 超长路径在**写盘前**被拒：文件都不该被创建。
///
/// 变异证明：删掉 `Prefs::validate` 的长度判据 ⇒ 本用例红（会走 schema 的路径长度上限，
/// 得到 `WriteError::Schema` 而不是 `Refused`，且断言 `!file.exists()` 仍成立但错误类型不符）。
#[test]
fn an_overlong_install_root_is_refused_before_writing() {
    let dir = root("prefs-long");
    let file = config(&dir);
    let prefs = Prefs {
        install_root: "x".repeat(MAX_INSTALL_ROOT + 1),
        ..Prefs::default()
    };
    let error = prefs_store::save(&file, &prefs).expect_err("超长路径应被拒");
    assert_eq!(error, WriteError::Refused(Refusal::InstallRootTooLong));
    assert!(!file.exists(), "被本地校验拒掉的保存不许碰磁盘");
}
