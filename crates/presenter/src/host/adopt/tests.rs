//! `host::adopt` 的用例（M223）：真盘夹具 + 三条命令的回程形状。
//!
//! 抓得住：扫描只读（`.ULL/config` 不出现）、预检与确认的落点一致且都来自 core、
//! 坏清单失败时 `problems[]` 逐条带出（带行列号）、三档映射。
//! 抓不住：真启动游戏用不用这个运行目录（那是 launch 链路的活）；D30 视觉（另走截图通道）。
//!
//! 为什么单独一个文件：`host/adopt.rs` 带用例会到 440 行，越过 400 行硬线（AGENTS §3.1）。

use super::*;
use crate::dto::AdoptStage;

/// 真盘夹具：`<root>/.minecraft/versions/<id>/`（清单 + jar）+ `<root>/.ULL`。
struct Fixture {
    root: std::path::PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "ull-presenter-adopt-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&root).expect("建临时根");
        Self { root }
    }

    fn env(&self) -> HostEnv {
        HostEnv::at_root(&self.root)
    }

    /// 铺一个「别的启动器装好的」版本（`manifest` 原样写，便于造坏清单）。
    fn version(&self, id: &str, manifest: &str) {
        let dir = self.root.join(".minecraft/versions").join(id);
        std::fs::create_dir_all(&dir).expect("建版本目录");
        std::fs::write(dir.join(format!("{id}.json")), manifest).expect("写清单");
        std::fs::write(dir.join(format!("{id}.jar")), b"jar-bytes").expect("写 jar");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const MANIFEST: &str = r#"{
  "id": "1.20.1",
  "type": "release",
  "mainClass": "net.minecraft.main.Main",
  "libraries": []
}"#;

fn request(version_id: &str, target: AdoptTargetView) -> AdoptRequestView {
    AdoptRequestView {
        version_id: version_id.to_string(),
        name: None,
        target,
    }
}

/// 收一条 `HostTask::Adopt`（同步等后台线程交卷）。
fn recv(rx: &super::super::job::JobReceiver) -> AdoptEvent {
    match rx.recv().expect("后台线程应回一条") {
        HostTask::Adopt { event, .. } => *event,
        other => panic!("期望 HostTask::Adopt，收到 {other:?}"),
    }
}

#[test]
fn scan_lists_the_foreign_versions_without_writing() {
    let fixture = Fixture::new("scan");
    fixture.version("1.20.1", MANIFEST);
    let env = fixture.env();
    let (tx, rx) = job::channel();
    spawn_scan(env.clone(), tx, 1);

    let AdoptEvent::Scan(scan) = recv(&rx) else {
        panic!("扫描应回 AdoptEvent::Scan");
    };
    assert_eq!(scan.versions_dir, fixture.root.join(".minecraft/versions"));
    assert_eq!(scan.total(), 1);
    assert_eq!(scan.adoptable(), 1);
    assert!(scan.candidates[0].jar_present);
    // 扫描零写盘：`.ULL/config` 下不该出现任何东西
    assert!(!fixture.root.join(".ULL/config/profiles.json").exists());
}

#[test]
fn confirm_registers_and_reports_the_core_run_dir() {
    let fixture = Fixture::new("confirm");
    fixture.version("1.20.1", MANIFEST);
    let env = fixture.env();

    // 先预检：零写盘，落点与后果原文都来自 core
    let (tx, rx) = job::channel();
    spawn_adopt(
        env.clone(),
        tx,
        request("1.20.1", AdoptTargetView::InstanceDir),
        Mode::Preview,
        1,
    );
    let AdoptEvent::Planned(plan) = recv(&rx) else {
        panic!("预检应回 AdoptEvent::Planned");
    };
    assert_eq!(plan.target, AdoptTargetView::InstanceDir);
    assert_eq!(plan.isolation, AdoptIsolationView::Custom);
    // 缺省档的运行目录落数据区（**不是** `versions/<id>`）
    assert!(
        plan.run_dir
            .starts_with(fixture.root.join(".ULL/data/instances")),
        "落点应在 ULL 数据区：{}",
        plan.run_dir.display()
    );
    assert!(!plan.notes.is_empty(), "缺省档也要说清后果");
    assert!(!fixture.root.join(".ULL/config/profiles.json").exists());

    // 确认：同一份请求发回去，真的登记
    let (tx, rx) = job::channel();
    spawn_adopt(
        env,
        tx,
        request("1.20.1", AdoptTargetView::InstanceDir),
        Mode::Confirm,
        2,
    );
    let AdoptEvent::Done(done) = recv(&rx) else {
        panic!("确认应回 AdoptEvent::Done");
    };
    assert_eq!(done.version_id, "1.20.1");
    assert_eq!(done.run_dir, plan.run_dir, "确认的落点必须与预检一致");
    assert!(done.dir_created);
    let profiles = std::fs::read_to_string(fixture.root.join(".ULL/config/profiles.json"))
        .expect("profiles.json 应被写出来");
    assert!(profiles.contains("1.20.1"));
    // 游戏根的版本目录一个字节都没写（只有夹具自己那两个文件）
    let dir = fixture.root.join(".minecraft/versions/1.20.1");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("读版本目录")
        .map(|entry| {
            entry
                .expect("条目")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(names, ["1.20.1.jar", "1.20.1.json"]);
}

#[test]
fn a_bad_manifest_comes_back_as_every_problem_not_one_sentence() {
    let fixture = Fixture::new("problems");
    fixture.version("1.20.1", "{ this is not json");
    let (tx, rx) = job::channel();
    spawn_adopt(
        fixture.env(),
        tx,
        request("1.20.1", AdoptTargetView::InstanceDir),
        Mode::Preview,
        1,
    );
    let AdoptEvent::Failed(failure) = recv(&rx) else {
        panic!("坏清单应回 AdoptEvent::Failed");
    };
    assert_eq!(failure.code, "adopt_not_adoptable");
    assert_eq!(failure.problems.len(), 1);
    assert_eq!(failure.problems[0].code, "manifest_invalid_json");
    assert!(
        failure.problems[0].message.contains("line"),
        "清单问题要带行列号：{}",
        failure.problems[0].message
    );
}

#[test]
fn targets_map_to_core_and_custom_stays_out_of_the_gui() {
    assert_eq!(
        target_of(AdoptTargetView::InstanceDir),
        AdoptTarget::InstanceDir
    );
    assert_eq!(target_of(AdoptTargetView::Shared), AdoptTarget::Shared);
    assert_eq!(
        target_of(AdoptTargetView::VersionDir),
        AdoptTarget::VersionDir
    );
    // `Custom` 只在 core 侧出现（CLI 的 `--game-dir`）：界面按缺省档显示
    assert_eq!(
        target_view(&AdoptTarget::Custom(std::path::PathBuf::from("/x"))),
        AdoptTargetView::InstanceDir
    );
    assert_eq!(
        isolation_view(Isolation::Custom),
        AdoptIsolationView::Custom
    );
    assert_eq!(
        isolation_view(Isolation::Shared),
        AdoptIsolationView::Shared
    );
    assert_eq!(
        isolation_view(Isolation::Isolated),
        AdoptIsolationView::Isolated
    );
    // 空名回落到版本 id 由 core 负责；这里只钉「空白名不进请求」
    let request = core_request(AdoptRequestView {
        version_id: "1.20.1".to_string(),
        name: Some("   ".to_string()),
        target: AdoptTargetView::Shared,
    });
    assert_eq!(request.name, None);
    assert!(request.created_at.is_some());
    assert_eq!(AdoptStage::Pick.slug(), "pick");
}
