use super::human_bytes;
use super::*;
use unlucky_core::game::arguments::Environment;
use unlucky_core::game::install::{ManifestWrite, PlanInput};
use unlucky_core::game::layout::GameLayout;
use unlucky_core::game::manifest::VersionJson;
use unlucky_core::game::{InstanceProfile, ProfileId};

/// **M216 P2-2**：清单被替换这件事必须出现在**人读**行里（以前只进 `--json`，
/// 复审 r1 抓的就是「对玩家仍然静默」）。
///
/// 抓得住：`human_lines` 退回只遍历 `plan.warnings`（那时找不到「已替换版本清单」⇒ 红，
/// 正是修前状态）；`manifest` 的备份路径没渲出来。抓不住：真实终端排版与颜色、
/// 以及 `ull install` 的整条命令链（本用例直接调排版函数，不落盘、不联网）。
#[test]
fn a_replaced_manifest_is_said_out_loud_in_the_human_output() {
    let root = std::env::temp_dir().join(format!("ull216-report-{}", std::process::id()));
    let layout = GameLayout::new(&root);
    let resolved = VersionJson::from_json_str(
            r#"{"id":"1.20.1","mainClass":"net.minecraft.client.main.Main",
                "downloads":{"client":{"sha1":"aa","size":1,"url":"https://example.invalid/c.jar"}}}"#,
        )
        .expect("清单应可解析");
    let plan = InstallPlan::build(PlanInput {
        layout: &layout,
        version_id: "1.20.1",
        resolved: &resolved,
        asset_index: None,
        environment: &Environment::host(),
    })
    .expect("组计划应成功");

    let backup = root.join("versions/1.20.1/1.20.1.json.ullbak-4242");
    let mut warnings = plan.warnings.clone();
    warnings.push("已替换版本清单 <dest>（旧的那份备份在 <backup>）".to_string());
    let report = InstallReport {
        files: Vec::new(),
        version_json: Some(plan.version_json_path.clone()),
        manifest: Some(ManifestWrite::Replaced {
            backup: backup.clone(),
        }),
        warnings,
    };

    let text = human_lines(false, &plan, Some(&report), None, &[], "ignored", "1.20.1").join(
        "
",
    );
    assert!(
        text.contains("已替换版本清单"),
        "运行期告警没进人读输出：{text}"
    );
    assert!(
        text.contains(&backup.display().to_string()),
        "备份路径没进人读输出：{text}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn formats_sizes_without_fake_precision() {
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(1023), "1023 B");
    assert_eq!(human_bytes(1024), "1 KB");
    assert_eq!(human_bytes(1024 * 1024), "1.0 MB");
    assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
}

/// **M346 r2 P2-1**：登记行这一处的**实参映射**（`name` / `profile.id` / 版本名 / `created`）。
///
/// 为什么非得有这一条：`pack/wording.rs` 的用例只证明那句模板是人话，证明不了**调用方传对了
/// 字段**——把这里三个实参换个位，全仓 20 行 `test result` 照样全绿（r2 复审原话）。
///
/// 抓得住什么（变异判据，逐条实测过）：
/// - `(name, profile.id, version)` 换个位 ⇒ 整行对不上 ⇒ 红；
/// - `registration.created` 取反 ⇒ 末尾 `·复用已有档案` 该有的没有 / 不该有的多出来 ⇒ 红；
/// - 把 `version` 换成 `profile.version_id`（本链印的是**调用方给的版本名**）⇒ 红——本用例故意
///   让两者不同（`version = "1.20.1"`，`profile.version_id = "1.20.1-forge-47.2.0"`）。
///
/// 抓不住：这一行的颜色/对齐，以及 `install` 整条命令链（直接调排版函数，零网络、零落盘——
/// `--dry-run` 那一档按设计不印这行，真二进制验它要下载几百 MB，见 r2 报告）。
#[test]
fn the_registered_line_names_the_instance_and_the_version_it_used() {
    let root = std::env::temp_dir().join(format!("ull346-registered-{}", std::process::id()));
    let layout = GameLayout::new(&root);
    let resolved = VersionJson::from_json_str(
            r#"{"id":"1.20.1","mainClass":"net.minecraft.client.main.Main",
                "downloads":{"client":{"sha1":"aa","size":1,"url":"https://example.invalid/c.jar"}}}"#,
        )
        .expect("清单应可解析");
    let plan = InstallPlan::build(PlanInput {
        layout: &layout,
        version_id: "1.20.1",
        resolved: &resolved,
        asset_index: None,
        environment: &Environment::host(),
    })
    .expect("组计划应成功");
    let registration = Registration {
        profile: InstanceProfile {
            id: ProfileId::from("3f2a1c".to_string()),
            name: "档案里的名字（这一行不印它）".to_string(),
            version_id: "1.20.1-forge-47.2.0".to_string(),
            ..InstanceProfile::default()
        },
        config_path: root.join("config/instances/3f2a1c.json"),
        created: true,
    };

    let line_of = |registration: &Registration| {
        human_lines(
            false,
            &plan,
            None,
            Some(registration),
            &[],
            "Survival",
            "1.20.1",
        )
        .into_iter()
        .find(|line| line.starts_with("已登记实例"))
        .unwrap_or_else(|| panic!("没有登记行"))
    };
    assert_eq!(
        line_of(&registration),
        "已登记实例 Survival（id 3f2a1c）→ 版本 1.20.1",
        "名称/id/版本名三位必须各就各位（换位即红）"
    );
    assert_eq!(
        line_of(&Registration {
            created: false,
            ..registration
        }),
        "已登记实例 Survival（id 3f2a1c）→ 版本 1.20.1·复用已有档案",
        "复用已有档案那半句只许在 created == false 时出现（取反即红）"
    );
    std::fs::remove_dir_all(&root).ok();
}
