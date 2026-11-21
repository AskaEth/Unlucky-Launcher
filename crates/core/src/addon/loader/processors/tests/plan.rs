//! 执行计划：client 侧过滤、参数展开、产物/摘要、依赖库补齐、Java 要求。

use crate::addon::loader::forge::InstallProfile;
use crate::addon::loader::processors::{ProcessorErrorKind, ProcessorPlan, ProcessorPlanInput};
use crate::addon::loader::LoaderKind;
use crate::game::arguments::Environment;
use crate::game::layout::GameLayout;

use super::{profile, temp_root};

fn plan_for(root: &std::path::Path, profile: &InstallProfile) -> ProcessorPlan {
    let layout = GameLayout::new(root);
    ProcessorPlan::build(ProcessorPlanInput {
        kind: LoaderKind::Forge,
        mc_version: "1.20.1",
        version_id: "1.20.1-forge-47.2.0",
        profile,
        layout: &layout,
        installer: &root.join("cache/forge-installer.jar"),
        scratch: &root.join("cache/processors"),
        environment: &Environment::host(),
    })
    .expect("计划应能组出来")
}

#[test]
fn only_client_side_processors_become_steps_in_declared_order() {
    let root = temp_root("plan-order");
    let plan = plan_for(&root, &profile());

    assert_eq!(plan.step_count(), 2, "server-only 的 processor 必须被过滤");
    assert_eq!(plan.steps[0].index, 1);
    assert_eq!(
        plan.steps[0].label,
        "net.minecraftforge:installertools:1.3.0"
    );
    assert_eq!(plan.steps[1].index, 2);
    assert_eq!(plan.steps[1].label, "net.minecraftforge:jarsplitter:1.1.4");
    assert_eq!(plan.steps[1].classpath.len(), 1);
    assert!(!plan.is_empty());
}

#[test]
fn args_are_expanded_with_paths_and_the_client_side() {
    let root = temp_root("plan-args");
    let plan = plan_for(&root, &profile());
    let args = &plan.steps[0].args;

    assert_eq!(args[0], "--task");
    assert_eq!(args[1], "MCP_DATA");
    assert!(
        args[3].ends_with("mcp_config-1.20.1-20230612.114412.zip"),
        "输入构件应展开成 libraries 路径：{}",
        args[3]
    );
    assert!(
        args[5].ends_with("client-1.20.1-20230612.114412-slim.jar"),
        "产物路径应按 data 表展开：{}",
        args[5]
    );
    assert_eq!(args[7], "client");
    // 第二步用的是临时目录里的 client jar 副本，不是共享层的原版 jar。
    assert_eq!(
        plan.steps[1].args[1],
        plan.minecraft_jar_copy.to_string_lossy()
    );
    assert!(plan
        .minecraft_jar_copy
        .to_string_lossy()
        .ends_with("minecraft.jar"));
    assert!(!plan.minecraft_jar_copy.starts_with(root.join("versions")));
}

#[test]
fn outputs_carry_paths_and_only_real_sha1s() {
    let root = temp_root("plan-outputs");
    let plan = plan_for(&root, &profile());

    let first = &plan.steps[0].outputs;
    assert_eq!(first.len(), 1);
    assert_eq!(
        first[0].sha1.as_deref(),
        Some("aa11bb22cc33dd44ee55ff660011223344556677")
    );
    assert!(first[0].path.to_string_lossy().ends_with("-slim.jar"));
    // 非 SHA-1 的 values 只算"存在性校验"，并留一条告警。
    assert!(plan.steps[1].outputs[0].sha1.is_none());
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.contains("没给 SHA-1")),
        "{:?}",
        plan.warnings
    );
}

#[test]
fn processor_dependencies_are_planned_once_even_when_libraries_omit_them() {
    let root = temp_root("plan-libs");
    let lean = serde_json::from_str(
        r#"{"spec": 1, "minecraft": "1.20.1", "json": "/version.json",
            "processors": [{"jar": "net.minecraftforge:installertools:1.3.0",
                            "classpath": ["net.minecraftforge:jarsplitter:1.1.4"],
                            "args": [], "outputs": {}}],
            "libraries": [],
            "data": {}}"#,
    )
    .expect("夹具应可解析");
    let layout = GameLayout::new(&root);
    let plan = ProcessorPlan::build(ProcessorPlanInput {
        kind: LoaderKind::NeoForge,
        mc_version: "1.20.1",
        version_id: "1.20.1-neoforge-21.1.80",
        profile: &lean,
        layout: &layout,
        installer: &root.join("installer.jar"),
        scratch: &root.join("tmp"),
        environment: &Environment::host(),
    })
    .expect("计划应能组出来");

    assert_eq!(plan.libraries.len(), 2, "jar 与 classpath 各补一条");
    for file in &plan.libraries {
        assert!(
            file.url.contains("neoforged") || file.url.contains("minecraftforge"),
            "补库应落在加载器的 maven 上：{}",
            file.url
        );
        assert!(file.dest.starts_with(root.join("libraries")));
    }
    assert!(plan
        .warnings
        .iter()
        .any(|warning| warning.contains("不在 install profile 的 libraries 节里")));
}

#[test]
fn java_requirement_follows_the_target_minecraft_version() {
    let root = temp_root("plan-java");
    let plan = plan_for(&root, &profile());
    assert_eq!(plan.java_requirement.minimum, 17);
    assert_eq!(plan.java_requirement.maximum, None);
    assert_eq!(plan.java_requirement.describe(), "Java 17+");
}

#[test]
fn json_view_lists_every_step_for_dry_run() {
    let root = temp_root("plan-json");
    let plan = plan_for(&root, &profile());
    let json = plan.to_json();

    assert_eq!(json["count"], 2);
    assert_eq!(json["java_requirement"]["describe"], "Java 17+");
    assert_eq!(
        json["steps"][0]["jar"],
        "net.minecraftforge:installertools:1.3.0"
    );
    assert_eq!(json["steps"][0]["classpath"], 1);
    assert_eq!(
        json["steps"][1]["outputs"][0]["sha1"],
        serde_json::Value::Null
    );
}

#[test]
fn duplicated_library_declarations_are_deduped_by_destination() {
    // NeoForge 21.1.80 实测：install profile 把 cli-utils / srgutils 等声明了 2~3 次。
    // 同一落点下两次会在同一个 `.part` 中间文件上互相抢（真机 os error 2），必须去重。
    let root = temp_root("plan-dedup");
    let duplicated: InstallProfile = serde_json::from_str(
        r#"{"spec": 1, "minecraft": "1.20.1", "json": "/version.json",
            "processors": [{"jar": "net.neoforged.installertools:cli-utils:2.1.2@jar"}],
            "libraries": [
              {"name": "net.neoforged.installertools:cli-utils:2.1.2@jar", "url": "https://maven.neoforged.net/releases/"},
              {"name": "net.neoforged.installertools:cli-utils:2.1.2@jar", "url": "https://maven.neoforged.net/releases/"},
              {"name": "net.neoforged.installertools:cli-utils:2.1.2@jar", "url": "https://maven.neoforged.net/releases/"}
            ],
            "data": {}}"#,
    )
    .expect("夹具应可解析");
    let layout = GameLayout::new(&root);
    let plan = ProcessorPlan::build(ProcessorPlanInput {
        kind: LoaderKind::NeoForge,
        mc_version: "1.20.1",
        version_id: "1.20.1-neoforge-21.1.80",
        profile: &duplicated,
        layout: &layout,
        installer: &root.join("installer.jar"),
        scratch: &root.join("tmp"),
        environment: &Environment::host(),
    })
    .expect("计划应能组出来");

    assert_eq!(plan.libraries.len(), 1, "同一落点只应留一条");
    assert!(
        plan.warnings.iter().any(|warning| warning.contains("去重")),
        "{:?}",
        plan.warnings
    );
}

#[test]
fn processor_without_jar_is_a_plan_error() {
    let root = temp_root("plan-nojar");
    let broken: InstallProfile = serde_json::from_str(
        r#"{"spec": 1, "minecraft": "1.20.1", "json": "/version.json",
            "processors": [{"args": ["--task", "X"]}], "libraries": [], "data": {}}"#,
    )
    .expect("夹具应可解析");
    let layout = GameLayout::new(&root);
    let error = ProcessorPlan::build(ProcessorPlanInput {
        kind: LoaderKind::Forge,
        mc_version: "1.20.1",
        version_id: "1.20.1-forge-47.2.0",
        profile: &broken,
        layout: &layout,
        installer: &root.join("installer.jar"),
        scratch: &root.join("tmp"),
        environment: &Environment::host(),
    })
    .expect_err("缺 jar 必须报错");
    assert_eq!(error.kind(), ProcessorErrorKind::Plan);
    assert!(error.message().contains("jar"), "{}", error.message());
}
