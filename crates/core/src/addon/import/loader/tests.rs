//! 第六跳的端到端用例（M311）：**导入 → 装载器真的被请求安装 → 实例登记 → 版本 JSON 能读到**。
//!
//! # 断言的都是"机器位"，不是文案
//!
//! `loader_installed` / `report.registered` / `profiles.json` / `versions/<id>/<id>.json` 四处
//! 事实互相独立，任何一处被写成假的，这里就红。
//!
//! # 顺序判据（复审 r1 P2-1）
//!
//! 逐个阶段断言"我是第几步"钉不住**发射顺序**：`ALL` 与管线里真实的先后完全可能相反（r1 实测
//! 过 `loader` 先于 `commit`，界面却把 `Loader` 排在 `Commit` 之后 ⇒ 步号从 5/6 跳回 4/6）。
//! 所以 [`the_stages_really_fire_in_the_declared_order`] 收**实际发出**的序列，与
//! [`ImportStage::ALL`] 逐条比对并要求严格递增（用枚举自己的 `Ord`）。夹具与假件在
//! [`support`]（那边贴着 400 行硬线）。

mod support;

#[path = "tests/processors.rs"]
mod processors;
#[path = "tests/progress.rs"]
mod progress;
#[path = "tests/version.rs"]
mod version;

#[path = "tests/engine_pin.rs"]
mod engine_pin;

use crate::addon::import::keys;
use crate::addon::import::{
    import_pack, ImportProgress, ImportProgressSink, ImportRequest, ImportStage, ImportStatus,
};
use crate::addon::temp::TempRoot;
use crate::download::blocking::block_on;
use crate::game::layout::GameLayout;
use crate::platform::layout::LauncherLayout;

use support::{
    launch_stage_version_manifest, mrpack, mrpack_with_remote, registered_count, FakeInstaller,
    LOADER_VERSION, MC_VERSION, VERSION_ID,
};

// ============================ 用例 ============================

#[test]
fn old_behaviour_registers_an_instance_that_cannot_launch() {
    // **旧行为的复现**（M310 盘查的结论，也是这条 mission 要消灭的假功能）：只翻译计划、不装
    // 装载器 ⇒ 导入成功、实例登记、界面一切正常，点启动在「读版本 JSON」这一步就失败。
    // 这条与下一条是**同一份夹具、同一个断言函数**，只差 `.with_loader_install(...)`。
    let root = TempRoot::new("m311-old");
    let pack = mrpack("m311-old-pack");
    let target = root.join("instance");
    let layout = LauncherLayout::new(root.join("data"));
    let game = GameLayout::new(root.join("game"));

    let report = block_on(import_pack(
        &ImportRequest::new(pack.path(), &target).with_registration(&layout, Some("m311 old")),
    ));

    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    assert!(report.registered.is_some(), "旧行为确实登记了实例");
    assert!(
        !report.loader_installed,
        "没接安装器就必须如实说没装（`loader_installed = false`）"
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|note| note.message_key == keys::NOTE_LOADER_NOT_INSTALLED),
        "没装加载器必须有一条具名注记：{:?}",
        report.warnings
    );

    // 点启动的那一步：版本 JSON 根本不在盘上 ⇒ `VersionManifest` + 路径。
    let error =
        launch_stage_version_manifest(&game, VERSION_ID).expect_err("旧行为读不到版本 JSON");
    assert!(
        error.contains(VERSION_ID) && error.contains("versions"),
        "要指到那个不存在的版本 JSON：{error}"
    );
}

#[test]
fn the_declared_loader_is_really_requested_and_the_version_json_reads_back() {
    // 新行为：清单声明了加载器 ⇒ 真的请求装载器安装（版本 id 与清单一致），装完版本 JSON 读得回来。
    let root = TempRoot::new("m311-new");
    let pack = mrpack("m311-new-pack");
    let target = root.join("instance");
    let layout = LauncherLayout::new(root.join("data"));
    let game = GameLayout::new(root.join("game"));
    let installer = FakeInstaller::watching(&target);

    let report = block_on(import_pack(
        &ImportRequest::new(pack.path(), &target)
            .with_loader_install(&installer, &game)
            .with_registration(&layout, Some("m311 new")),
    ));

    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    // ①装载器**真的被请求安装**，且要的就是清单声明的那个版本（不是"最近的一版"）。
    assert_eq!(installer.called(), vec![VERSION_ID.to_string()]);
    // ②装载器排在落盘之前：被调时目标实例目录还不存在（失败时才能说"目标目录未被写入"）。
    assert_eq!(
        installer.target_seen(),
        vec![false],
        "装载器必须排在落盘之前"
    );
    // ③真的装上了（机器位 + 注记），实例登记用的就是那个版本 id。
    assert!(report.loader_installed, "{:?}", report.warnings);
    assert!(
        report
            .warnings
            .iter()
            .any(|note| note.message_key == keys::NOTE_LOADER_INSTALLED),
        "{:?}",
        report.warnings
    );
    let registered = report.registered.as_ref().expect("要登记实例");
    assert_eq!(registered.version_id, VERSION_ID);
    assert_eq!(registered_count(&layout), 1);
    // ④版本 JSON 真在盘上、真读得回来（= 启动侧那一步不再报 `VersionManifest`）。
    assert!(game.is_version_installed(VERSION_ID));
    let manifest = launch_stage_version_manifest(&game, VERSION_ID).expect("版本 JSON 要读得回来");
    assert_eq!(manifest.inherits_from.as_deref(), Some(MC_VERSION));
    // ⑤包内内容照旧落地（第六跳不许动前五跳的行为）。
    assert_eq!(
        std::fs::read_to_string(target.join("config/demo.toml")).expect("覆盖内容"),
        "k = 1\n"
    );
}

#[test]
fn a_failing_loader_install_keeps_the_instance_out_of_the_launcher() {
    // **失败路径**（这条 mission 的命门）：加载器装不上 ⇒ 实例不许被登记成"看起来能启动"的样子。
    let root = TempRoot::new("m311-fail");
    let pack = mrpack("m311-fail-pack");
    let target = root.join("instance");
    let layout = LauncherLayout::new(root.join("data"));
    let game = GameLayout::new(root.join("game"));
    let installer = FakeInstaller {
        fail: Some("连不上 meta.fabricmc.net（fetch_network）".to_string()),
        ..FakeInstaller::default()
    };

    let report = block_on(import_pack(
        &ImportRequest::new(pack.path(), &target)
            .with_loader_install(&installer, &game)
            .with_registration(&layout, Some("m311 fail")),
    ));

    assert_eq!(report.status, ImportStatus::Failed, "{:?}", report.error);
    let error = report.error.as_ref().expect("必须给具名错误");
    assert_eq!(error.code(), "import_loader");
    // 原因要指到"哪个加载器、哪个版本、哪一步"，不是一句"失败了"。
    assert!(
        error.message().contains("Fabric")
            && error.message().contains(LOADER_VERSION)
            && error.message().contains("fetch_network"),
        "{}",
        error.message()
    );
    // 恢复动作要在（重试装加载器 / 装到已有实例），不是只说失败。
    let hint = error.hint().expect("失败要指向下一步");
    assert!(hint.contains("重导") || hint.contains("装好"), "{hint}");
    // ①不登记：档案里一条都没有（不许留下指向不存在版本 JSON 的实例）。
    assert!(report.registered.is_none(), "装不上加载器时不许登记实例");
    assert_eq!(registered_count(&layout), 0, "profiles.json 里不许有它");
    // ②目标目录零残留（装载器排在落盘之前，所以这句"未被写入"是真的）。
    assert!(!target.exists(), "目标目录不该被建出来");
    assert!(report.rolled_back, "{:?}", report.rollback);
    assert!(
        report
            .warnings
            .iter()
            .any(|note| note.message.contains("未被写入")),
        "{:?}",
        report.warnings
    );
    assert!(launch_stage_version_manifest(&game, VERSION_ID).is_err());
}

#[test]
fn a_loader_version_other_than_the_declared_one_is_refused() {
    // 版本一致性闸：装载器"用最近的版本顶上"必须被拒——清单说要 0.15.11，装回来 0.16.9 也不行。
    let root = TempRoot::new("m311-mismatch");
    let pack = mrpack("m311-mismatch-pack");
    let target = root.join("instance");
    let layout = LauncherLayout::new(root.join("data"));
    let game = GameLayout::new(root.join("game"));
    let installer = FakeInstaller {
        different: Some("1.20.1-fabric-0.16.9".to_string()),
        ..FakeInstaller::default()
    };

    let report = block_on(import_pack(
        &ImportRequest::new(pack.path(), &target)
            .with_loader_install(&installer, &game)
            .with_registration(&layout, Some("m311 mismatch")),
    ));

    assert_eq!(report.status, ImportStatus::Failed);
    let error = report.error.as_ref().expect("必须给具名错误");
    assert_eq!(error.code(), "import_loader");
    assert!(
        error.message().contains(VERSION_ID)
            && error.message().contains("1.20.1-fabric-0.16.9")
            && error.message().contains("不一致"),
        "要说清「清单要的」与「装回来的」：{}",
        error.message()
    );
    assert!(report.registered.is_none(), "被拒的那次不许登记实例");
    assert_eq!(registered_count(&layout), 0);
    assert!(!target.exists());
}

#[test]
fn a_cancel_during_the_loader_install_is_a_cancellation_not_a_failure() {
    // 取消不是失败：装载器装到一半被取消 ⇒ `Cancelled`、`error` 为空、不登记、目标目录未写入。
    let root = TempRoot::new("m311-cancel");
    let pack = mrpack("m311-cancel-pack");
    let target = root.join("instance");
    let layout = LauncherLayout::new(root.join("data"));
    let game = GameLayout::new(root.join("game"));
    let installer = FakeInstaller {
        cancel: true,
        ..FakeInstaller::default()
    };

    let report = block_on(import_pack(
        &ImportRequest::new(pack.path(), &target)
            .with_loader_install(&installer, &game)
            .with_registration(&layout, Some("m311 cancel")),
    ));

    assert_eq!(report.status, ImportStatus::Cancelled, "{:?}", report.error);
    assert!(report.error.is_none(), "取消不是失败");
    assert!(report.registered.is_none());
    assert_eq!(registered_count(&layout), 0);
    assert!(!target.exists());
    assert!(!report.loader_installed);
}

#[test]
fn the_stages_really_fire_in_the_declared_order() {
    // **顺序判据**（复审 r1 P2-1 真正的缺口）：逐个阶段独立断言步号，钉不住"谁先谁后"。
    // 这条在同一份夹具上收**实际发出**的阶段序列（`with_progress` 的回调就是界面拿到的那条），
    // 要求它：①与 `ImportStage::ALL` 逐条相同 ②严格递增（用枚举自己的 `Ord`）。
    //
    // 抓得住：把 `Loader` 与 `Commit` 的顺序在**枚举**或 **`ALL`** 任一处调换 ⇒ 本条红
    // （要么逐条比对不等，要么 `loader > commit` 破坏递增）。
    let root = TempRoot::new("m311-order");
    // 带一条补不出地址的远端条目：`Fetch` 也会真的发射一次（六个阶段全在才谈得上顺序）。
    let pack = mrpack_with_remote("m311-order-pack");
    let target = root.join("instance");
    let layout = LauncherLayout::new(root.join("data"));
    let game = GameLayout::new(root.join("game"));
    let installer = FakeInstaller::default();

    let seen: std::sync::Arc<std::sync::Mutex<Vec<ImportStage>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorder = std::sync::Arc::clone(&seen);
    let sink: ImportProgressSink = std::sync::Arc::new(move |progress: &ImportProgress| {
        let mut seen = recorder.lock().expect("锁");
        // 同一阶段会被逐文件重复上报，这里只记"进入新阶段"的那一次。
        if seen.last().copied() != Some(progress.stage) {
            seen.push(progress.stage);
        }
    });

    let report = block_on(import_pack(
        &ImportRequest::new(pack.path(), &target)
            .with_loader_install(&installer, &game)
            .with_registration(&layout, Some("m311 order"))
            .with_progress(sink),
    ));
    // 那条远端条目补不出地址 ⇒ 缺文件（可容忍），正好让 `Fetch` 真的发射一次。
    assert_eq!(report.status, ImportStatus::Partial, "{:?}", report.error);

    let emitted = seen.lock().expect("锁").clone();
    assert_eq!(
        emitted,
        ImportStage::ALL.to_vec(),
        "实际发射的阶段顺序与 `ImportStage::ALL` 不一致（界面步号会跳号）"
    );
    assert!(
        emitted.windows(2).all(|pair| pair[0] < pair[1]),
        "阶段必须严格往前走（枚举 `Ord` = 执行顺序）：{emitted:?}"
    );
}

// ------------------------------- M321 r1 P2-1：原版那一层的计划告警不许丢

/// `install_vanilla` 的计划告警（缺某类资源、已降级）在**成功**路径上也要进报告。
///
/// `install_vanilla` 本身要真引擎（网络 + 计划），离线够不着，所以把「两份都留下」抽成
/// `engine::merge_warnings` 钉住：把 `warnings.extend(vanilla)` 那行删掉 ⇒ 含 2 条的那条断言红。
#[test]
fn the_vanilla_plan_warnings_are_kept_alongside_the_loader_ones() {
    let plan = vec!["loader 库缺下载地址，已降级".to_string()];
    let vanilla = vec!["原版 1.20.1 的 assets 索引缺失，已降级".to_string()];

    let merged = super::engine::merge_warnings(&plan, vanilla);
    assert_eq!(merged.len(), 2, "{merged:?}");
    assert_eq!(merged[0], "loader 库缺下载地址，已降级");
    assert_eq!(merged[1], "原版 1.20.1 的 assets 索引缺失，已降级");

    // 没有原版那一跳（vanilla 已装）时只有计划期那一条，不许凭空多。
    assert_eq!(super::engine::merge_warnings(&plan, Vec::new()), plan);
}
