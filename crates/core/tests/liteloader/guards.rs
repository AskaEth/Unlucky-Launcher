//! 边界与失败面：父版本缺失、组合不支持、数据源形状坏、取消。
//!
//! 网络层的失败映射（`DownloadError` → `loader_network`）由 `core::download` 的引擎测试
//! 与既有加载器测试覆盖，这里**不联网**：外部源的真实可用性在 mission 报告里单独列
//! （含探测结果），不混进单元测试冒充。

use std::sync::atomic::AtomicBool;

use unlucky_core::addon::loader::liteloader::parse_versions;
use unlucky_core::addon::loader::{
    LoaderErrorKind, LoaderInstallPlan, LoaderKind, LoaderPlanInput, LoaderSource,
};
use unlucky_core::download::blocking::block_on;
use unlucky_core::game::arguments::Environment;
use unlucky_core::game::install::{execute_with_cancel, InstallErrorKind};
use unlucky_core::game::layout::GameLayout;

use crate::support::{engine, liteloader_release, prefill, put_version, TempDir, VANILLA_1122};

/// 父版本不是原版、又没装在盘上 → **具名报错**，绝不默默改成叠原版。
#[test]
fn missing_loader_parent_is_a_named_error() {
    let temp = TempDir::new("missing-parent");
    let layout = GameLayout::new(temp.path());
    put_version(&layout, "1.12.2", VANILLA_1122);
    let release = liteloader_release("1.12.2-forge-14.23.5.2860");

    let error = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &layout,
        release: &release,
        environment: &Environment::host(),
        parent_installed: false,
    })
    .expect_err("父版本缺失必须报错");
    assert_eq!(error.kind(), LoaderErrorKind::NotFound);
    assert!(
        error.message().contains("1.12.2-forge-14.23.5.2860"),
        "错误里要点名缺的是哪个父版本：{}",
        error.message()
    );
    assert!(error.hint().is_some(), "要给出下一步怎么办");
}

/// 父版本链：读盘定位原版版本 id（合成 loader id / 定位 jar 都用它）。
#[test]
fn parent_chain_walks_to_the_vanilla_root() {
    let temp = TempDir::new("parent-chain");
    let layout = GameLayout::new(temp.path());
    put_version(&layout, "1.12.2", VANILLA_1122);
    put_version(
        &layout,
        "1.12.2-forge-14.23.5.2860",
        crate::support::FORGE_1122,
    );

    assert_eq!(
        unlucky_core::addon::loader::parent::root_version_id(&layout, "1.12.2-forge-14.23.5.2860")
            .expect("两层链应走得到根"),
        "1.12.2"
    );
    assert_eq!(
        unlucky_core::addon::loader::parent::root_version_id(&layout, "1.12.2").expect("根是自己"),
        "1.12.2"
    );

    // 没装的父版本 → not_found（而不是猜一个原版版本号）。
    let error = unlucky_core::addon::loader::parent::root_version_id(&layout, "9.9.9-forge-1.0.0")
        .expect_err("缺父版本必须报错");
    assert_eq!(error.kind(), LoaderErrorKind::NotFound);
}

/// 只有 LiteLoader 接受「父版本不是原版」；别家**具名拒绝**，不去发请求。
#[test]
fn only_liteloader_accepts_a_non_vanilla_parent() {
    let temp = TempDir::new("combo-guard");
    let engine = engine(temp.path());
    let source = LoaderSource::new(&engine, temp.path());
    let error = block_on(async {
        source
            .resolve_on(
                LoaderKind::Forge,
                "1.12.2",
                Some("14.23.5.2860"),
                Some("1.12.2-liteloader-1.12.2-SNAPSHOT"),
            )
            .await
    })
    .expect_err("Forge 叠在 LiteLoader 上必须被拒");
    assert_eq!(error.kind(), LoaderErrorKind::Unsupported);
    assert!(
        error
            .message()
            .contains("1.12.2-liteloader-1.12.2-SNAPSHOT"),
        "要说清是哪个组合：{}",
        error.message()
    );
}

/// 版本表形状坏 / 空表：各自给具名大类，不当成「没有版本」糊过去。
#[test]
fn broken_version_table_is_reported_distinctly() {
    let shape = parse_versions(r#"{"versions": []}"#).expect_err("形状不符");
    assert_eq!(shape.kind(), LoaderErrorKind::Metadata);
    let empty = parse_versions(r#"{"meta": {}, "versions": {}}"#).expect_err("空表");
    assert_eq!(empty.kind(), LoaderErrorKind::NotFound);

    // 版本段缺失（记录没有 version）：只剩 latest 别名 → 仍当空表处理。
    let only_latest = r#"{"versions": {"1.7.10": {"artefacts": {"com.mumfrey:liteloader":
        {"latest": {"tweakClass": "x", "version": "", "file": "liteloader.jar"}}}}}}"#;
    let error = parse_versions(only_latest).expect_err("只有 http 别名时没有可用版本");
    assert_eq!(error.kind(), LoaderErrorKind::NotFound);
}

/// 取消：执行段看标志就返回 `Cancelled`，**版本 JSON 不落盘**（不留半成品）。
#[test]
fn cancel_leaves_no_manifest() {
    let temp = TempDir::new("cancel");
    let layout = GameLayout::new(temp.path());
    put_version(&layout, "1.12.2", VANILLA_1122);
    let release = liteloader_release("1.12.2");
    let plan = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &layout,
        release: &release,
        environment: &Environment::host(),
        parent_installed: true,
    })
    .expect("计划应可组");

    let engine = engine(temp.path());
    prefill(&plan.install);
    let cancel = AtomicBool::new(true);
    let error = block_on(execute_with_cancel(
        &engine,
        &plan.install,
        4,
        None,
        Some(&cancel),
    ))
    .expect_err("取消必须返回错误");
    assert_eq!(error.kind(), InstallErrorKind::Cancelled);
    assert!(
        !layout.version_json(&release.version_id).exists(),
        "取消之后不许留下版本清单"
    );
}

/// 同一个 loader 版本 id 被两条父链请求：**具名报错**，且盘上那份清单一个字节都不动。
///
/// 为什么必须有这道门：`<原版>-liteloader-<ver>` 对「叠原版」与「叠 Forge」是同一个目录名，
/// 直接覆盖会让已经登记、指向这个版本 id 的旧实例**静默换父链**（启动行为跟着变）。
#[test]
fn parent_collision_is_refused_instead_of_overwriting() {
    let temp = TempDir::new("parent-collision");
    let layout = GameLayout::new(temp.path());
    put_version(&layout, "1.12.2", VANILLA_1122);
    put_version(
        &layout,
        "1.12.2-forge-14.23.5.2860",
        crate::support::FORGE_1122,
    );

    let vanilla_release = liteloader_release("1.12.2");
    let existing = serde_json::json!({
        "id": vanilla_release.version_id,
        "inheritsFrom": "1.12.2",
        "mainClass": "net.minecraft.launchwrapper.Launch",
        "arguments": {"game": ["--tweakClass", "com.mumfrey.liteloader.launch.LiteLoaderTweaker"]},
        "libraries": []
    });
    put_version(
        &layout,
        &vanilla_release.version_id,
        &serde_json::to_string_pretty(&existing).expect("序列化"),
    );
    let before = std::fs::read_to_string(layout.version_json(&vanilla_release.version_id))
        .expect("读旧清单");

    let forge_release = liteloader_release("1.12.2-forge-14.23.5.2860");
    assert_eq!(
        forge_release.version_id, vanilla_release.version_id,
        "两条父链撞同一个 id"
    );
    let error = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &layout,
        release: &forge_release,
        environment: &Environment::host(),
        parent_installed: true,
    })
    .expect_err("撞父链必须报错");
    assert_eq!(error.kind(), LoaderErrorKind::Unsupported);
    assert!(
        error.message().contains("1.12.2-forge-14.23.5.2860") && error.message().contains("1.12.2"),
        "要说清两条父链各是什么：{}",
        error.message()
    );

    let after = std::fs::read_to_string(layout.version_json(&vanilla_release.version_id))
        .expect("读旧清单");
    assert_eq!(before, after, "报错之后盘上那份清单不许被改动");
}
