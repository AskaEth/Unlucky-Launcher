//! M216 ③：装同一版本时**不再静默覆盖** `versions/<id>/<id>.json`。
//!
//! 三条判据各一条用例（都走真的 `execute`，所以报告字段是端到端验过的，不是只测私有函数）：
//!
//! | 盘上状态 | 期望 |
//! |---|---|
//! | 没有清单 | `ManifestWrite::Fresh`，无备份、无告警 |
//! | 有、且内容**逐字节相同** | `Identical`，不写、不备份、不告警（重装同版本是正常操作）|
//! | 有、且内容**不同** | `Replaced{backup}`，旧的那份进 `<dest>.ullbak-<pid>`，告警里点名 |
//!
//! 全部零网络：客户端 jar 事先按 sha1 放好，跳过判定第一级直接命中。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::download::verify;
use crate::download::{
    ContentCache, DownloadEngine, EngineConfig, HashAlgo, HealthRegistry, MirrorTable,
    RegionProfile,
};
use crate::game::arguments::Environment;
use crate::game::install::execute::execute;
use crate::game::install::plan::{InstallPlan, PlanInput};
use crate::game::install::{InstallReport, ManifestWrite};
use crate::game::layout::GameLayout;
use crate::game::manifest::VersionJson;

fn temp_root(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ull-m216-manifest-{}-{tag}", std::process::id()))
}

fn engine(root: &Path) -> DownloadEngine {
    DownloadEngine::new(
        EngineConfig::default(),
        Arc::new(MirrorTable::builtin()),
        Arc::new(RegionProfile::direct(0)),
        Arc::new(HealthRegistry::in_memory()),
        Arc::new(ContentCache::open(root.join("cache"))),
    )
}

/// 客户端 jar 已在盘上的一份计划（于是整次安装零下载，只有清单这一步会写盘）。
fn plan_with_existing_jar(root: &Path) -> InstallPlan {
    let payload = b"already installed client jar".to_vec();
    let layout = GameLayout::new(root);
    let sha1 = verify::digest_bytes(HashAlgo::Sha1, &payload);
    std::fs::create_dir_all(root.join("versions/1.20.1")).expect("建版本目录");
    std::fs::write(layout.version_jar("1.20.1"), &payload).expect("放客户端 jar");
    let resolved = VersionJson::from_json_str(&format!(
        r#"{{"id": "1.20.1", "mainClass": "net.minecraft.client.main.Main",
            "downloads": {{"client": {{"sha1": "{sha1}", "size": {}, "url": "https://piston-data.mojang.com/v1/objects/{sha1}/client.jar"}}}}}}"#,
        payload.len()
    ))
    .expect("清单应可解析");
    InstallPlan::build(PlanInput {
        layout: &layout,
        version_id: "1.20.1",
        resolved: &resolved,
        asset_index: None,
        environment: &Environment::host(),
    })
    .expect("组计划应成功")
}

/// 版本目录下的备份文件（`<dest>.ullbak-<pid>[-n]`）。
fn backups(plan: &InstallPlan) -> Vec<PathBuf> {
    let Some(dir) = plan.version_json_path.parent() else {
        return Vec::new();
    };
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.to_string_lossy().contains(".ullbak-"))
                .collect()
        })
        .unwrap_or_default()
}

fn says_replaced(report: &InstallReport) -> bool {
    report
        .warnings
        .iter()
        .any(|line| line.contains("已替换版本清单"))
}

/// **不存在 ⇒ 正常写**（不能因为加了检测就把「第一次装」也变成需要备份的路径）。
///
/// 抓得住：Fresh 被误标成 Replaced/Identical（那会让报告对第一次安装说谎）、凭空多出备份、
/// 以及「检测逻辑把没有清单的路径判成失败」。抓不住：写完之后别的启动器能不能读（实机才验）。
#[tokio::test]
async fn a_version_without_a_manifest_gets_one_written() {
    let root = temp_root("fresh");
    let plan = plan_with_existing_jar(&root);
    let report = execute(&engine(&root), &plan, 4, None)
        .await
        .expect("执行应成功");

    assert_eq!(report.manifest, Some(ManifestWrite::Fresh));
    assert!(plan.version_json_path.is_file(), "清单该落盘");
    assert!(backups(&plan).is_empty(), "没覆盖就没什么可备份");
    assert!(!says_replaced(&report), "第一次装不该说「替换了」");
    std::fs::remove_dir_all(&root).ok();
}

/// **存在且相同 ⇒ 不写、不备份、不提示**（重装同版本是正常操作，不是冲突）。
///
/// 抓得住：内容相同也走备份/告警（`backups` 非空或 `says_replaced` 为真就红）、清单内容被改。
/// 抓不住：是不是真的没调 `write_atomic`——内容本来就一样，盘上看不出来；这一条靠
/// `Identical` 早退那段代码 review 兜住（Windows 上另加一半证据：只读属性下 rename 会失败，
/// 所以「只读 + 仍然成功」在 Windows 上等于没写）。
#[tokio::test]
async fn reinstalling_the_same_version_leaves_the_manifest_alone() {
    let root = temp_root("identical");
    let plan = plan_with_existing_jar(&root);
    execute(&engine(&root), &plan, 4, None)
        .await
        .expect("第一次执行应成功");
    let before = std::fs::read(&plan.version_json_path).expect("读清单");

    // Windows 上把这枚文件设成只读：写它的唯一途径是 rename 盖过去，那会失败。
    // （只读这条证据只在 Windows 上成立；Unix 下目录权限才管 rename，所以那里它不证明什么。）
    let original = std::fs::metadata(&plan.version_json_path)
        .expect("读元数据")
        .permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&plan.version_json_path, readonly).expect("设只读");

    let again = execute(&engine(&root), &plan, 4, None).await;
    // 复原**原来的**权限（不是 `set_readonly(false)`：那会把 Unix 的位改成 world-writable）。
    std::fs::set_permissions(&plan.version_json_path, original).expect("恢复权限");

    let report = again.expect("内容相同 ⇒ 不该尝试写盘（只读也不该失败）");
    assert_eq!(report.manifest, Some(ManifestWrite::Identical));
    assert_eq!(
        std::fs::read(&plan.version_json_path).expect("读清单"),
        before,
        "内容相同 ⇒ 清单一个字节都不该变"
    );
    assert!(backups(&plan).is_empty(), "内容相同不该备份");
    assert!(!says_replaced(&report), "内容相同不该提示替换");
    std::fs::remove_dir_all(&root).ok();
}

/// **存在且不同 ⇒ 备份 + 报告里可见**（这是「不静默破坏别人」的核心一条）。
///
/// 抓得住：去掉备份只覆盖（`backup` 为空、`.ullbak-*` 不存在 ⇒ 红）、旧内容丢失（备份内容
/// 不等于别人的原文 ⇒ 红）、报告不说（`ManifestWrite::Replaced` 与告警都缺 ⇒ 红）。
/// 抓不住：真实另一个启动器**正开着**这枚文件时的失败落点（Windows 共享冲突），那要实机。
#[tokio::test]
async fn a_different_manifest_is_backed_up_before_being_replaced() {
    let root = temp_root("replaced");
    let plan = plan_with_existing_jar(&root);
    // 「别人铺的那份」：同路径、不同内容（这里用另一份 id/mainClass 模拟）。
    let foreign = br#"{"id":"1.20.1","mainClass":"org.pcl.Launcher","note":"not ours"}"#.to_vec();
    std::fs::write(&plan.version_json_path, &foreign).expect("放别人的清单");

    let report = execute(&engine(&root), &plan, 4, None)
        .await
        .expect("执行应成功");
    let Some(ManifestWrite::Replaced { backup }) = report.manifest.clone() else {
        panic!("应当报 Replaced，实际 {:?}", report.manifest);
    };

    assert_eq!(
        std::fs::read(&backup).expect("读备份"),
        foreign,
        "备份=别人原文"
    );
    assert_eq!(
        backups(&plan),
        vec![backup.clone()],
        "备份只有一份、在版本目录里"
    );
    assert_ne!(
        std::fs::read(&plan.version_json_path).expect("读清单"),
        foreign,
        "目标应当是刚解析出来的那份"
    );
    let written = std::fs::read_to_string(&plan.version_json_path).expect("读清单");
    assert!(
        VersionJson::from_json_str(&written).is_ok_and(|json| json.id.as_deref() == Some("1.20.1")),
        "{written}"
    );
    assert!(says_replaced(&report), "报告里必须点名「替换了已有清单」");
    std::fs::remove_dir_all(&root).ok();
}
