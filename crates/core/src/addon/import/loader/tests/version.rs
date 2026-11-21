//! M332（P36）：老协议装载器的**版本一致性闸**——判据是「同一个装载器版本」，不是逐字相等。
//!
//! 三组断言各管一层：
//!
//! 1. [`the_two_spellings_of_one_legacy_forge_build_are_the_same_version`] /
//!    [`a_different_build_is_still_refused`]：判据本身（拒与不拒的分界线，逐例）；
//! 2. [`a_legacy_pack_is_no_longer_stopped_at_the_version_gate`]：**穿过产品路径**（真管线 +
//!    真清单解析 + 真登记 + 启动侧读版本 JSON），一份 1.7.10 整合包不再被这道闸拒；
//! 3. [`a_declared_build_that_differs_from_the_installed_one_is_still_refused`]：闸**还能红**
//!    （声明 10.13.4.1615 而装回 1614 ⇒ 必须拒）。
//!
//! 字符串原文的出处写在每条用例里（真 installer / 真 maven 元数据 / M330 干跑），不凭猜。

use crate::addon::import::{import_pack, ImportRequest, ImportStatus, LoaderTarget};
use crate::addon::loader::{LoaderKind, LoaderRelease, LoaderSupport};
use crate::addon::temp::TempRoot;
use crate::download::blocking::block_on;
use crate::game::layout::GameLayout;
use crate::game::manifest::VersionJson;
use crate::platform::layout::LauncherLayout;

use super::support::{launch_stage_version_manifest, mrpack_for, registered_count, FakeInstaller};

use super::super::version::{adopt_declared_id, same_loader_id, same_loader_version};

// 真 installer 里读出来的字符串（`C:/Temp/ull-forge1122/installer-1.7.10.jar` 的
// `install_profile.json`：`install.path = net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10`、
// `versionInfo.id = 1.7.10-Forge10.13.4.1614-1.7.10`），以及真 maven 元数据里那条
// (`1.7.10-10.13.4.1614-1.7.10`)。
const MC_1710: &str = "1.7.10";
/// 清单声明的短形（CF `modLoaders[].id = "forge-10.13.4.1614"` 去掉前缀后的那一段）。
const SHORT_1710: &str = "10.13.4.1614";
/// 同一份发布在 maven 上的写法（双后缀，末尾再写一次原版版本）。
const DOUBLE_1710: &str = "10.13.4.1614-1.7.10";
/// 安装器装回来、实例会登记的那个 id（清单短形 + 核心自己的 `<原版>-<种类>-` 词根）。
const ID_1710_SHORT: &str = "1.7.10-forge-10.13.4.1614";
/// 生产实现在钉到清单 id **之前**算出来的 id（双后缀写法）。
const ID_1710_DOUBLE: &str = "1.7.10-forge-10.13.4.1614-1.7.10";

// ======================= ① 判据：拒与不拒的分界线 =======================

#[test]
fn the_two_spellings_of_one_legacy_forge_build_are_the_same_version() {
    // 放行的例子（复审按这张表逐例核）：
    // 短形 ⇔ 双后缀形；短形 ⇔ 带原版前缀；NeoForge / Fabric 的版本号只有一种写法。
    assert!(same_loader_version(
        LoaderKind::Forge,
        MC_1710,
        SHORT_1710,
        DOUBLE_1710
    ));
    assert!(same_loader_version(
        LoaderKind::Forge,
        MC_1710,
        SHORT_1710,
        "1.7.10-10.13.4.1614"
    ));
    assert!(same_loader_version(
        LoaderKind::Forge,
        MC_1710,
        DOUBLE_1710,
        "1.7.10-10.13.4.1614-1.7.10"
    ));
    // 1.8.9 也有双后缀坐标，但**只有 3 条**（114 条里：11.15.1.2318 / 1902 / 1890），
    // 其余 111 条是单后缀 —— 别把「老版就双后缀」当规则（见 `version` 模块文档的实测表）。
    assert!(same_loader_version(
        LoaderKind::Forge,
        "1.8.9",
        "11.15.1.2318",
        "11.15.1.2318-1.8.9"
    ));
    // 1.20.1 的既有形状一字不变（单后缀，两种写法本来就相同）。
    assert!(same_loader_version(
        LoaderKind::Forge,
        "1.20.1",
        "47.2.0",
        "1.20.1-47.2.0"
    ));
    assert!(same_loader_version(
        LoaderKind::NeoForge,
        "1.21.1",
        "21.1.80",
        "21.1.80"
    ));
    assert!(same_loader_version(
        LoaderKind::Fabric,
        "1.20.1",
        "0.15.11",
        "0.15.11"
    ));

    // 版本 id 层（闸实际比的那一层）：
    assert!(same_loader_id(
        LoaderKind::Forge,
        MC_1710,
        ID_1710_SHORT,
        ID_1710_DOUBLE
    ));
    assert!(same_loader_id(
        LoaderKind::Fabric,
        "1.20.1",
        "1.20.1-fabric-0.15.11",
        "1.20.1-fabric-0.15.11"
    ));
}

#[test]
fn a_different_build_is_still_refused() {
    // 拒的例子（**这条是闸存在的理由**：差一个 build 就是另一次发布）。
    assert!(!same_loader_version(
        LoaderKind::Forge,
        MC_1710,
        SHORT_1710,
        "10.13.4.1615"
    ));
    assert!(!same_loader_version(
        LoaderKind::Forge,
        MC_1710,
        SHORT_1710,
        "10.13.4.161"
    ));
    assert!(!same_loader_id(
        LoaderKind::Forge,
        MC_1710,
        ID_1710_SHORT,
        "1.7.10-forge-10.13.4.1615"
    ));
    // 原版段不同 / 种类不同 / 纯原版 id 都不算「同一个装载器版本」。
    assert!(!same_loader_id(
        LoaderKind::Forge,
        MC_1710,
        ID_1710_SHORT,
        "1.8.9-forge-10.13.4.1614"
    ));
    assert!(!same_loader_id(
        LoaderKind::Forge,
        MC_1710,
        ID_1710_SHORT,
        "1.7.10-neoforge-10.13.4.1614"
    ));
    assert!(!same_loader_id(
        LoaderKind::Fabric,
        "1.20.1",
        "1.20.1-fabric-0.15.11",
        "1.20.1"
    ));
    // 非 maven 族的版本号没有别的写法：只认逐字相等（不因为"看着像"就放行）。
    assert!(!same_loader_version(
        LoaderKind::Fabric,
        "1.20.1",
        "0.15.11",
        "0.15.12"
    ));
}

#[test]
fn the_installed_id_is_pinned_to_the_declared_spelling() {
    // 生产实现（`EngineLoaderInstaller`）在闸放行之后把**落盘 id 钉成清单声明的那个**：
    // 实例登记用的是清单 id，`versions/<id>/` 必须就是它，否则导入"成功"、点启动读不到 JSON。
    let mut release = fake_release();
    let declared = declared_target(SHORT_1710, ID_1710_SHORT);
    adopt_declared_id(&mut release, &declared);
    assert_eq!(release.version_id, ID_1710_SHORT);
    assert_eq!(release.loader_version, SHORT_1710);
    assert_eq!(release.version_json.id.as_deref(), Some(ID_1710_SHORT));
    // 「官方 maven 上叫另一个名字」这件事要进报告，不许悄悄抹掉。
    assert_eq!(release.warnings.len(), 1, "{:?}", release.warnings);
    assert!(
        release.warnings[0].contains(DOUBLE_1710),
        "{:?}",
        release.warnings
    );

    // 写法本来就相同（1.20.1 / Fabric / NeoForge 那条路）⇒ 一个字都不动、不加告警。
    let mut same = fake_release();
    let before = (same.version_id.clone(), same.loader_version.clone());
    adopt_declared_id(&mut same, &declared_target(DOUBLE_1710, ID_1710_DOUBLE));
    assert_eq!((same.version_id, same.loader_version), before);
    assert!(same.warnings.is_empty(), "{:?}", same.warnings);
}

// ======================= ② 穿过产品路径：1.7.10 不再被拒 =======================

#[test]
fn a_legacy_pack_is_no_longer_stopped_at_the_version_gate() {
    // 清单写短形（CF 的 `forge-10.13.4.1614`），装回来的是双后缀形（真 installer 的 maven 坐标
    // 只能拼出 `1.7.10-10.13.4.1614-1.7.10`，M330 干跑 `out-1710short.json` 实测）。
    //
    // 这里的假件**按契约把版本 JSON 落在清单 id 下**（`installs_as`）——即"实现方钉了 id"的那一
    // 支；「engine 自己是否真钉」由 `engine_pin` 那条用例走真 `EngineLoaderInstaller` 钉住，
    // 「没钉的实现方会被谁拦」由下一条用例（`the_declared_id_must_really_hold_a_version_json`）钉住。
    let root = TempRoot::new("m332-legacy");
    let pack = mrpack_for("m332-legacy-pack", MC_1710, "forge", SHORT_1710);
    let target_dir = root.join("instance");
    let layout = LauncherLayout::new(root.join("data"));
    let game = GameLayout::new(root.join("game"));
    let installer = FakeInstaller {
        different: Some(ID_1710_DOUBLE.to_string()),
        installs_as: Some(ID_1710_SHORT.to_string()),
        ..FakeInstaller::default()
    };

    let report = block_on(import_pack(
        &ImportRequest::new(pack.path(), &target_dir)
            .with_loader_install(&installer, &game)
            .with_registration(&layout, Some("m332 legacy")),
    ));

    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    assert!(report.loader_installed, "{:?}", report.warnings);
    // ①装载器被请求的仍是清单声明的那个 id（清单版本没被实现方改写）。
    assert_eq!(installer.called(), vec![ID_1710_SHORT.to_string()]);
    // ②登记用清单 id，且那个 id 的版本 JSON 读得回来（启动侧那一步不再报 VersionManifest）。
    let registered = report.registered.as_ref().expect("要登记实例");
    assert_eq!(registered.version_id, ID_1710_SHORT);
    assert_eq!(registered_count(&layout), 1);
    let manifest =
        launch_stage_version_manifest(&game, ID_1710_SHORT).expect("版本 JSON 要读得回来");
    assert_eq!(manifest.inherits_from.as_deref(), Some(MC_1710));
    // ③不是"整包失败"：目标目录的内容照旧落地。
    assert_eq!(
        std::fs::read_to_string(target_dir.join("config/demo.toml")).expect("覆盖内容"),
        "k = 1\n"
    );
}

// ======================= ②b 落盘判据：清单 id 下必须真有版本 JSON（r1 P2-2）=======================

#[test]
fn the_declared_id_must_really_hold_a_version_json() {
    // **这条是 r1 P2-2 要的机器判据**：实现方报回"同一个装载器版本的另一种写法"，并且**真的
    // 落在了那个写法下**（`installs_as: None` = 假件不替它钉 id）。闸② 会放行（两者是同一个
    // 版本），但清单声明 id 下没有版本 JSON ⇒ 管线会把实例登记成一个读不到的版本。
    //
    // 所以闸③（`version::check_landed`）必须把这次导入**拒掉**：没有它，这条用例就是
    // "ImportStatus::Success + 登记了 ID_1710_SHORT + 那个 id 下读不到 JSON"——正是本 mission
    // 要消灭的假功能。变异（删掉 `loader.rs` 里的 `check_landed` 调用）⇒ 本条红。
    let root = TempRoot::new("m332-dangling");
    let pack = mrpack_for("m332-dangling-pack", MC_1710, "forge", SHORT_1710);
    let target_dir = root.join("instance");
    let layout = LauncherLayout::new(root.join("data"));
    let game = GameLayout::new(root.join("game"));
    let installer = FakeInstaller {
        different: Some(ID_1710_DOUBLE.to_string()),
        ..FakeInstaller::default()
    };

    let report = block_on(import_pack(
        &ImportRequest::new(pack.path(), &target_dir)
            .with_loader_install(&installer, &game)
            .with_registration(&layout, Some("m332 dangling")),
    ));

    assert_eq!(report.status, ImportStatus::Failed, "{:?}", report.error);
    let error = report.error.as_ref().expect("必须给具名错误");
    assert_eq!(error.code(), "import_loader");
    assert!(
        error.message().contains(ID_1710_SHORT) && error.message().contains("没有版本 JSON"),
        "要说清「清单 id 下没有版本 JSON」：{}",
        error.message()
    );
    // 出路提示不许丢（删掉 versions/… 后重试）。
    assert!(
        error
            .hint()
            .is_some_and(|hint| hint.contains("删掉 versions/") && hint.contains(ID_1710_DOUBLE)),
        "{:?}",
        error.hint()
    );
    // 失败即不登记、目标目录未被写入（装载器排在落盘之前）。
    assert!(report.registered.is_none());
    assert_eq!(registered_count(&layout), 0);
    assert!(!target_dir.exists());
    // 反证：盘上有的只是那个"另一种写法"的 JSON，清单 id 下确实什么都没有。
    assert!(game.is_version_installed(ID_1710_DOUBLE));
    assert!(!game.is_version_installed(ID_1710_SHORT));
}

// ======================= ③ 闸还能红 =======================
#[test]
fn a_declared_build_that_differs_from_the_installed_one_is_still_refused() {
    // 变异：清单声明 `10.13.4.1615`，装回来的是 `…1614`（另一个 build）⇒ **必须红**。
    // 这条在"把归一函数写成恒真"或"把比较删掉"之后就会变绿——那正是它要抓的。
    let root = TempRoot::new("m332-mismatch");
    let pack = mrpack_for("m332-mismatch-pack", MC_1710, "forge", "10.13.4.1615");
    let target_dir = root.join("instance");
    let layout = LauncherLayout::new(root.join("data"));
    let game = GameLayout::new(root.join("game"));
    let installer = FakeInstaller {
        different: Some(ID_1710_DOUBLE.to_string()),
        ..FakeInstaller::default()
    };

    let report = block_on(import_pack(
        &ImportRequest::new(pack.path(), &target_dir)
            .with_loader_install(&installer, &game)
            .with_registration(&layout, Some("m332 mismatch")),
    ));

    assert_eq!(report.status, ImportStatus::Failed);
    let error = report.error.as_ref().expect("必须给具名错误");
    assert_eq!(error.code(), "import_loader");
    assert!(
        error.message().contains(ID_1710_DOUBLE)
            && error.message().contains("1.7.10-forge-10.13.4.1615")
            && error.message().contains("不一致"),
        "要说清「清单要的」与「装回来的」：{}",
        error.message()
    );
    // 被拒的那次不登记、目标目录零残留（装载器排在落盘之前）。
    assert!(report.registered.is_none());
    assert_eq!(registered_count(&layout), 0);
    assert!(!target_dir.exists());
}

// ============================ 夹具 ============================

/// 生产实现解析出来的那份 release（双后缀写法，真 engine 的中间态）。
fn fake_release() -> LoaderRelease {
    LoaderRelease {
        kind: LoaderKind::Forge,
        mc_version: MC_1710.to_string(),
        parent_version_id: MC_1710.to_string(),
        loader_version: DOUBLE_1710.to_string(),
        version_id: ID_1710_DOUBLE.to_string(),
        version_json: VersionJson::from_json_str(
            r#"{"id": "1.7.10-forge-10.13.4.1614-1.7.10", "inheritsFrom": "1.7.10",
                "mainClass": "net.minecraft.launchwrapper.Launch"}"#,
        )
        .expect("版本 JSON"),
        support: LoaderSupport::JsonRoute,
        installer: None,
        install_profile: None,
        warnings: Vec::new(),
    }
}

/// 清单声明的那一侧（判据只看 `kind` / `mc_version` / `loader_version` / `version_id`）。
fn declared_target(loader_version: &str, version_id: &str) -> LoaderTarget {
    LoaderTarget {
        kind: LoaderKind::Forge,
        mc_version: MC_1710.to_string(),
        loader_version: loader_version.to_string(),
        version_id: version_id.to_string(),
        raw: format!("forge-{loader_version}"),
    }
}
