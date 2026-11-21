//! 远端条目 → `ModRecord`：数据层真的能落地（并进既有 `report` 的判据）。

use crate::addon::curseforge::{Mod, ModFile};
use crate::addon::mods::report;

fn project() -> Mod {
    Mod {
        id: 238_222,
        name: "Just Enough Items".to_string(),
        slug: "jei".to_string(),
        ..Default::default()
    }
}

fn file(display: &str, name: &str) -> ModFile {
    ModFile {
        id: 4_455_667,
        mod_id: 238_222,
        display_name: display.to_string(),
        file_name: name.to_string(),
        ..Default::default()
    }
}

#[test]
fn a_remote_file_becomes_a_mod_record() {
    let record = project().record_of(&file(
        "jei-1.20.1-forge-15.2.0.27",
        "jei-1.20.1-forge-15.2.0.27.jar",
    ));
    assert_eq!(record.id, "jei", "site slug 就是我们要的 id");
    assert_eq!(record.name.as_deref(), Some("Just Enough Items"));
    assert_eq!(
        record.version.as_deref(),
        Some("jei-1.20.1-forge-15.2.0.27"),
        "版本取站点的 displayName"
    );
    assert_eq!(
        record.file.as_deref(),
        Some("jei-1.20.1-forge-15.2.0.27.jar"),
        "装上去就叫这个名字"
    );
    assert!(record.enabled, "远端条目装上就是启用的");
    assert!(
        record.deps.is_empty(),
        "CF 的前置端点本切片没接：空是安全的一侧（不谎报缺失）"
    );
}

#[test]
fn a_sparse_remote_entry_falls_back_to_the_file_name_heuristic() {
    let bare = Mod {
        id: 7,
        ..Default::default()
    };
    let record = bare.record_of(&file("", "sodium-fabric-0.6.0.jar"));
    assert_eq!(record.id, "sodium-fabric", "没 slug 时退到文件名");
    assert_eq!(record.name.as_deref(), Some("sodium-fabric"));
    assert_eq!(
        record.version.as_deref(),
        Some("0.6.0"),
        "displayName 是空串时退到文件名的版本号部分"
    );
}

#[test]
fn two_files_of_one_project_are_a_duplicate_as_the_report_sees_it() {
    let project = project();
    let records = vec![
        project.record_of(&file("15.2.0.27", "jei-1.20.1-forge-15.2.0.27.jar")),
        project.record_of(&file("19.0.0.1", "jei-1.21.1-neoforge-19.0.0.1.jar")),
    ];
    let outcome = report(&records);
    assert_eq!(
        outcome.duplicates.len(),
        1,
        "同一个项目的两个版本，既有报告必须看见一条重复"
    );
    assert_eq!(outcome.duplicates[0].id, "jei");
    assert_eq!(outcome.duplicates[0].copies.len(), 2);
    assert!(!outcome.is_clean());
    assert!(outcome.missing.is_empty(), "deps 为空时不许谎报前置缺失");
}

#[test]
fn the_same_project_installed_twice_only_once_is_clean() {
    let project = project();
    let records = vec![project.record_of(&file("15.2.0.27", "jei-1.20.1-forge-15.2.0.27.jar"))];
    assert!(report(&records).is_clean(), "只有一份时没有任何问题");
}
