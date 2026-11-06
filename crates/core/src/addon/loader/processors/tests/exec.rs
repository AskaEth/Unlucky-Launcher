//! 执行器：幂等跳过（不碰文件系统）、依赖缺失分流、起不来 java 的分类、脱敏器。

use std::path::{Path, PathBuf};

use crate::addon::loader::forge::InstallProfile;
use crate::addon::loader::processors::{host_redactor, run};
use crate::addon::loader::processors::{ProcessorErrorKind, ProcessorPlan, ProcessorPlanInput};
use crate::addon::loader::LoaderKind;
use crate::game::arguments::Environment;
use crate::game::java::{JavaOrigin, JavaRuntime};
use crate::game::layout::GameLayout;

use super::temp_root;

/// 一个最小 profile：一步 processor，产物是临时目录里的 `built.jar`（只按存在性判定）。
const MINIMAL_PROFILE: &str = r#"{
  "spec": 1,
  "minecraft": "1.20.1",
  "json": "/version.json",
  "processors": [{
    "jar": "net.minecraftforge:installertools:1.3.0",
    "classpath": ["net.minecraftforge:jarsplitter:1.1.4"],
    "args": ["--task", "SPLIT"],
    "outputs": {"{OUT}": "'not-a-sha1'"}
  }],
  "libraries": [],
  "data": {"OUT": {"client": "'built.jar'"}}
}"#;

fn minimal_profile() -> InstallProfile {
    serde_json::from_str(MINIMAL_PROFILE).expect("夹具应可解析")
}

/// 组装计划（依赖库不落盘：执行器不负责下载，缺失由用例自己摆）。
fn plan(root: &Path) -> ProcessorPlan {
    let profile = minimal_profile();
    let layout = GameLayout::new(root);
    ProcessorPlan::build(ProcessorPlanInput {
        kind: LoaderKind::Forge,
        mc_version: "1.20.1",
        version_id: "1.20.1-forge-47.2.0",
        profile: &profile,
        layout: &layout,
        installer: &root.join("installer.jar"),
        scratch: &root.join("scratch"),
        environment: &Environment::host(),
    })
    .expect("计划应能组出来")
}

/// 一个「必然起不来」的 java：这些用例只验证分类与顺序，不验证 java 行为。
fn missing_java() -> JavaRuntime {
    JavaRuntime::unknown(
        PathBuf::from("/nonexistent/ull-test-java"),
        JavaOrigin::Path,
    )
}

fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建目录");
    }
    std::fs::write(path, b"stub").expect("写夹具文件");
}

/// 写一个最小 jar（可选带 `Main-Class`），给"主类从 MANIFEST 读"的用例用。
fn write_jar(path: &Path, main_class: Option<&str>) {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建目录");
    }
    let file = std::fs::File::create(path).expect("建 jar");
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    if let Some(main_class) = main_class {
        writer
            .start_file("META-INF/MANIFEST.MF", options)
            .expect("写 manifest 条目");
        write!(
            writer,
            "Manifest-Version: 1.0\r\nMain-Class: {main_class}\r\n\r\n"
        )
        .expect("写 manifest");
    }
    writer.finish().expect("收尾 jar");
}

#[test]
fn ready_outputs_skip_the_whole_step_without_touching_the_disk() {
    let root = temp_root("exec-idempotent");
    let plan = plan(&root);
    // 产物已就绪：原版 client jar 压根不存在也无所谓——幂等快路径不该去复制它。
    touch(&plan.steps[0].outputs[0].path);

    let run = run(&plan, &missing_java(), None).expect("产物齐备时不该跑任何进程");
    assert_eq!(run.executed(), 0);
    assert_eq!(run.skipped(), 1);
    assert_eq!(run.steps[0].status.as_str(), "skipped");
    assert_eq!(run.to_json()["steps"][0]["status"], "skipped");
    // prepare 不该跑：原版 jar 压根不存在、client jar 副本没被复制。
    assert!(!plan.minecraft_jar_copy.exists());
}

#[test]
fn missing_minecraft_jar_is_reported_before_any_java_runs() {
    let root = temp_root("exec-no-vanilla");
    let plan = plan(&root);
    let error = run(&plan, &missing_java(), None).expect_err("缺原版 jar 必须报错");
    assert_eq!(error.kind(), ProcessorErrorKind::Io);
    assert!(
        error.message().contains("原版 client jar"),
        "{}",
        error.message()
    );
}

#[test]
fn missing_processor_jar_is_a_library_error_naming_the_file() {
    let root = temp_root("exec-no-lib");
    let plan = plan(&root);
    touch(&plan.minecraft_jar);

    let error = run(&plan, &missing_java(), None).expect_err("缺依赖必须报错");
    assert_eq!(error.kind(), ProcessorErrorKind::Library);
    assert!(
        error.message().contains("installertools-1.3.0.jar"),
        "{}",
        error.message()
    );
}

#[test]
fn a_java_that_cannot_start_is_an_io_error_with_a_hint() {
    let root = temp_root("exec-no-java");
    let plan = plan(&root);
    touch(&plan.minecraft_jar);
    // 可执行 jar 得是真的 zip + MANIFEST：主类只从那儿来（profile 没写 mainClass）。
    write_jar(
        &plan.steps[0].jar,
        Some("net.minecraftforge.installertools.Main"),
    );
    touch(&plan.steps[0].classpath[0]);

    let error = run(&plan, &missing_java(), None).expect_err("java 起不来必须报错");
    assert_eq!(error.kind(), ProcessorErrorKind::Io);
    assert!(
        error.message().contains("起 java 失败"),
        "{}",
        error.message()
    );
    assert!(error.hint().is_some());
    // 走到这一步说明临时目录、MC jar 副本都准备好了（顺序：prepare → 起进程）。
    assert!(plan.minecraft_jar_copy.is_file());
}

#[test]
fn a_jar_without_a_main_class_is_a_plan_error() {
    let root = temp_root("exec-no-main");
    let plan = plan(&root);
    touch(&plan.minecraft_jar);
    write_jar(&plan.steps[0].jar, None);
    touch(&plan.steps[0].classpath[0]);

    let error = run(&plan, &missing_java(), None).expect_err("拿不到主类必须报错");
    assert_eq!(error.kind(), ProcessorErrorKind::Plan);
    assert!(error.message().contains("主类"), "{}", error.message());
}

#[test]
fn host_redactor_masks_the_user_home_directory() {
    let redactor = host_redactor();
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    let Some(home) = home else {
        return; // 极简环境（无 HOME/USERPROFILE）时跳过
    };
    let text = format!("{}/game/libraries/x.jar", home.to_string_lossy());
    let scrubbed = redactor.scrub(&text);
    assert!(scrubbed.contains("<home>"), "{scrubbed}");
    assert!(
        !scrubbed.contains(&home.to_string_lossy().to_string()),
        "{scrubbed}"
    );
}
