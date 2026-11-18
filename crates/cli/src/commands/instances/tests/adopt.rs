use super::*;
use unlucky_core::game::AdoptTarget;

use crate::cli::Globals;

/// 一份临时数据根，盘上一个最小版本（`versions/1.20.1/1.20.1.json`）。零网络、零外部依赖。
fn data_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("ull346-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let versions = root.join(".minecraft/versions/1.20.1");
    std::fs::create_dir_all(&versions).expect("建版本目录");
    std::fs::write(
        versions.join("1.20.1.json"),
        r#"{"id":"1.20.1","mainClass":"net.minecraft.client.main.Main"}"#,
    )
    .expect("写版本清单");
    root
}

/// **M346 r2 P2-1**：`adopt` 那处登记行的**实参映射**（名称 / 实例 id / 版本 id）。
///
/// 走真的 `run()`（真写盘到临时数据根，零网络），所以它同时钉住"这一行确实出自这条命令"。
/// 抓得住什么（变异判据，实测过）：三个实参换位或换错字段（例如把 `version_id` 放进 id 那一位）
/// ⇒ 整行对不上 ⇒ 红——实例 id 是运行时生成的，本用例用 `--json` 的 `instance.id` 与它比对，
/// 于是"印的是三样里的哪一样"被钉死。抓不住：`--dry-run` 那一档（按设计不印登记行）。
#[test]
fn the_registered_line_names_the_adopted_instance() {
    let root = data_root("adopt");
    let ctx = Ctx::resolve_in(&Globals::default(), Some(root.clone())).expect("解析数据根");
    let args = InstancesAdoptArgs {
        version_id: "1.20.1".to_string(),
        name: Some("Survival".to_string()),
        target: AdoptTarget::InstanceDir,
        dry_run: false,
    };
    let report = run(&ctx, &args).expect("登记应成功");
    let line = report.human.first().expect("没有登记行");
    let id = report.data["instance"]["id"]
        .as_str()
        .expect("--json 的 instance.id 该在");
    assert_eq!(
        line,
        &format!("已登记实例 Survival（id {id}）→ 版本 1.20.1"),
        "名称/实例 id/版本 id 三位必须各就各位"
    );
    std::fs::remove_dir_all(&root).ok();
}
