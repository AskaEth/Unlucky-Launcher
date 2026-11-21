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

use std::path::{Path, PathBuf};

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
/// ## 为什么不再写死 `D:/JDKs`（M262）
///
/// `D:/JDKs` 在 Windows 上是**绝对路径**、在 Linux 上是**相对路径**（`D:` 只是个普通目录名），
/// 而 `JavaInstallRoot::resolve`（`crates/core/src/game/java/location.rs:128`）对两者的处理是
/// 两件事：绝对路径原样用，相对路径拼到数据根后面。于是同一条 `assert_eq!(root, "D:/JDKs")`
/// 在两个平台上验的是**两个不同的语义**：Windows 上验「设了就不拼数据根」（对），Linux 上验
/// 「绝对路径被当成相对路径拼了」——ubuntu CI 因此必红。落点现在由临时目录给出，两个平台上都是
/// 绝对路径，断言回到它本来的意思。
///
/// 抓得住：
/// - `effective` 忽略 `java.installRoot`（整段红：落点会变成数据根下的 `java-managed`，
///   `root_layer` 也退回 `DataMode`。实测：把 `override_root` 恒置空 ⇒ 红在 `root_layer`）；
/// - 「设了却仍按数据根下的 `java-managed` 来」（实测：让 `JavaInstallRoot::resolve` 的 custom 分支
///   返回 `launcher.java_managed()` ⇒ 红在 `root` 那句，错误的落点被打出来）；
/// - 相对路径的落点规则（第二段：相对路径**必须**拼数据根）。
///
/// 抓不住：
/// - **「绝对路径被当成相对路径拼」**：`Path::join` 对一个绝对路径返回的就是它自己，于是
///   `is_absolute()` 那一支在两个平台上都**观测不到**——实测把那一支删成恒 `home().join(root)`，
///   本用例仍然全绿。上一版写死 `D:/JDKs` 之所以在 Windows 上绿而在 Linux 上红，靠的不是
///   「拼不拼」这条分支，而是 `D:/JDKs` 在两个平台上**根本不是一类路径**。要让这一支有守卫，
///   得换成一个「绝对路径 join 之后会变」的平台（没有），或者直接断言 `is_absolute` 的判据本身；
/// - 路径归一化（`D:/JDKs` → `D:\JDKs` 这类）与分隔符风格；
/// - Standard 数据模式下拿不到系统标准目录的那一臂（`root_unresolved`，本用例钉的是 portable）。
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

    let custom_root = custom_install_root();
    assert!(
        !custom_root.starts_with(&dir),
        "custom 落点必须不在数据根里（否则「拼了数据根」这条也能过）：{} vs {}",
        custom_root.display(),
        dir.display()
    );

    let custom = Prefs {
        install_root: custom_root.display().to_string(),
        saved_install_root: custom_root.display().to_string(),
        ..Prefs::default()
    };
    let effective = prefs_store::effective(&file, &custom);
    assert_eq!(effective.root_layer, RootLayer::Setting);
    assert_eq!(effective.root_mode, "custom");
    assert_eq!(
        effective.root,
        custom_root.display().to_string(),
        "设了就按它来，不再拼数据根"
    );
    assert!(
        !effective.root_unresolved,
        "custom 落点不该是「解析失败」：{effective:?}"
    );

    // 相对路径那一档（上一版写死的 `D:/JDKs` 在 Linux 上就落进这里）：按数据根解析，
    // 但来源层仍是设置键。两个平台上语义相同——这正是上一版抓不住的那一半。
    let relative = Prefs {
        install_root: "jdks-relative".to_string(),
        saved_install_root: "jdks-relative".to_string(),
        ..Prefs::default()
    };
    let effective = prefs_store::effective(&file, &relative);
    assert_eq!(effective.root_layer, RootLayer::Setting);
    assert_eq!(effective.root_mode, "custom");
    assert_eq!(
        effective.root,
        dir.join("jdks-relative").display().to_string(),
        "相对路径按数据根解析（绝对路径才原样用）"
    );

    let _ = std::fs::remove_dir_all(&custom_root);
}

/// 一个**平台无关的绝对路径**（且不在数据根里）：custom 落点的替身。
///
/// 不能写死 `D:/JDKs` / `/opt/jdks`：前者在 Linux 上是相对路径，后者在 Windows 上是相对路径
/// （见 [`an_empty_install_root_follows_the_data_mode`] 的文档）。临时目录两边都是绝对路径。
fn custom_install_root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ull-java-custom-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建 custom 落点");
    dir
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
