use super::*;
use std::path::PathBuf;
use unlucky_core::addon::loader::{
    LoaderInstallPlan, LoaderKind, LoaderPlanInput, LoaderRelease, LoaderSupport,
};
use unlucky_core::game::arguments::Environment;
use unlucky_core::game::install::ManifestWrite;
use unlucky_core::game::layout::GameLayout;
use unlucky_core::game::manifest::VersionJson;
use unlucky_core::game::{InstanceProfile, ProfileId};

/// 一份最小的 Fabric 加载器计划（零网络：只算路径，不落盘）。
fn loader_plan(root: &std::path::Path) -> LoaderInstallPlan {
    let layout = GameLayout::new(root);
    let release = LoaderRelease {
            kind: LoaderKind::Fabric,
            mc_version: "1.20.1".to_string(),
            parent_version_id: "1.20.1".to_string(),
            loader_version: "0.15.11".to_string(),
            version_id: "1.20.1-fabric-0.15.11".to_string(),
            version_json: VersionJson::from_json_str(
                r#"{"id":"1.20.1-fabric-0.15.11","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","inheritsFrom":"1.20.1","libraries":[]}"#,
            )
            .expect("loader 清单应可解析"),
            support: LoaderSupport::JsonRoute,
            installer: None,
            install_profile: None,
            warnings: Vec::new(),
        };
    LoaderInstallPlan::build(LoaderPlanInput {
        layout: &layout,
        release: &release,
        environment: &Environment::host(),
        parent_installed: true,
    })
    .expect("计划应可组")
}

/// **M216 r2 P2-2**：装加载器时「先装原版」那一步替换了共享 `.minecraft` 里的原版清单，
/// 这句话必须出现在**加载器安装**的人读输出里（此前只出现在纯原版安装的输出里）。
///
/// 抓得住：`human_lines` 只遍历 `plan.warnings()`（= r1 状态）；备份路径没渲出来；
/// `--json` 的 `vanilla` 段没了。抓不住：真实 `ull install --loader …` 整条链路（要网络），
/// 以及「原版 report 是否真被 `loader.rs` 传进来」那步接线（见报告）。
#[test]
fn a_replaced_vanilla_manifest_is_said_out_loud_in_the_loader_output() {
    let root = std::env::temp_dir().join(format!("ull216-loader-{}", std::process::id()));
    let plan = loader_plan(&root);
    let backup = PathBuf::from("/g/.minecraft/versions/1.20.1/1.20.1.json.ullbak-99");
    let vanilla = InstallReport {
        files: Vec::new(),
        version_json: Some(PathBuf::from("/g/.minecraft/versions/1.20.1/1.20.1.json")),
        manifest: Some(ManifestWrite::Replaced {
            backup: backup.clone(),
        }),
        warnings: vec!["已替换版本清单 /g/.minecraft/versions/1.20.1/1.20.1.json".to_string()],
    };
    let text = human_lines(false, &plan, None, Some(&vanilla), None, &[], "n", None).join("\n");
    assert!(
        text.contains("已替换版本清单"),
        "原版那段的告警没进人读输出：{text}"
    );
    assert!(
        text.contains(&backup.display().to_string()),
        "原版清单的备份路径没进人读输出：{text}"
    );
    // 没先装原版时不许凭空多说话。
    let plain = human_lines(false, &plan, None, None, None, &[], "n", None).join("\n");
    assert!(!plain.contains("原版版本清单已替换"), "{plain}");
    // `--json` 的 `vanilla` 段（结构化，脚本可读）。
    let block = vanilla_json(Some(&vanilla)).expect("应当有 vanilla 段");
    assert_eq!(block["manifest"]["kind"], Value::String("replaced".into()));
    assert_eq!(
        block["manifest"]["backup"].as_str(),
        Some(backup.to_string_lossy().as_ref())
    );
    assert!(vanilla_json(None).is_none());
    std::fs::remove_dir_all(&root).ok();
}

/// **M346 r2 P2-1**：登记行的实参映射（名称 / 档案 id / **档案里的**版本 id / `created`）。
/// 三个值互不相同 ⇒ 换位、换错字段、`created` 取反都会让整行对不上；抓不住整条命令链。
#[test]
fn the_registered_line_names_the_loader_instance() {
    let root = std::env::temp_dir().join(format!("ull346-loader-reg-{}", std::process::id()));
    let registration = Registration {
        profile: InstanceProfile {
            id: ProfileId::from("3f2a1c".to_string()),
            version_id: "1.20.1-fabric".to_string(),
            ..InstanceProfile::default()
        },
        config_path: root.join("c.json"),
        created: false,
    };
    let human = human_lines(
        false,
        &loader_plan(&root),
        None,
        None,
        Some(&registration),
        &[],
        "Survival",
        None,
    );
    let line = human.last().map(String::as_str).unwrap_or_default();
    assert_eq!(
        line,
        "已登记实例 Survival（id 3f2a1c）→ 版本 1.20.1-fabric·复用已有档案"
    );
}
