//! 规则库加载 / 热重载 / 坏文件的降级行为。

use std::fs;

use crate::prelude::*;

const CUSTOM_MEMORY: &str = "\
version: 1
category: memory
rules:
  - id: memory.custom_only
    severity: warning
    pattern: 'CUSTOM TEST SIGNATURE'
    confidence: 0.9
    priority: 70
    cause:
      key: diagnose.cause.memory.custom_only
      summary: '测试用自定义规则'
    fix_action:
      - kind: seek_help
    sample: samples/memory/out_of_memory.log
";

#[test]
fn embedded_library_is_complete() {
    let loader = RuleLoader::embedded();
    assert_eq!(EMBEDDED_RULES.len(), 10, "10 个根因分组各一个 YAML");
    // 64 = HMCL 59 条全量平移 + 5 条 Vulkan 组（graphics.yaml 末尾的 4 个原因码
    // + no_graphics_backend）。加规则要同时加样本并同步这个数。
    assert_eq!(loader.rules().len(), 64);
    assert!(loader.last_errors().is_empty());
    assert!(loader.dir().is_none());
}

#[test]
fn missing_directory_falls_back_to_embedded() {
    let dir = temp_dir("loader-missing").join("nope");
    let loader = RuleLoader::from_dir(&dir);
    assert_eq!(loader.rules().len(), 64);
    assert!(loader.last_errors().is_empty());
}

#[test]
fn disk_file_replaces_embedded_group() {
    let dir = temp_dir("loader-override");
    fs::write(dir.join("memory.yaml"), CUSTOM_MEMORY).expect("写规则文件");
    let loader = RuleLoader::from_dir(&dir);

    assert_eq!(loader.rules().len(), 63, "memory.yaml 的 2 条被 1 条替换");
    assert!(loader.last_errors().is_empty());
    assert!(loader.rule_set().get("memory.custom_only").is_some());
    assert!(
        loader.rule_set().get("memory.out_of_memory").is_none(),
        "同组文件被整份替换"
    );
}

#[test]
fn refresh_picks_up_edits() {
    let dir = temp_dir("loader-refresh");
    let mut loader = RuleLoader::from_dir(&dir);
    assert_eq!(
        loader.refresh(),
        ReloadOutcome::Unchanged,
        "没变就不该重解析"
    );

    fs::write(dir.join("memory.yaml"), CUSTOM_MEMORY).expect("写规则文件");
    assert_eq!(
        loader.refresh(),
        ReloadOutcome::Reloaded { rules: 63 },
        "指纹变了应重新加载"
    );
    assert!(loader.rule_set().get("memory.custom_only").is_some());
}

#[test]
fn broken_yaml_keeps_last_good_rules() {
    let dir = temp_dir("loader-broken");
    fs::write(dir.join("memory.yaml"), CUSTOM_MEMORY).expect("写规则文件");
    let mut loader = RuleLoader::from_dir(&dir);
    assert_eq!(loader.rules().len(), 63);

    // 用户手改坏了 YAML：保留上一份好规则 + 报错，而不是让诊断能力归零
    fs::write(
        dir.join("memory.yaml"),
        "version: 1\ncategory: memory\nrules: [ oops\n",
    )
    .expect("写坏文件");
    match loader.refresh() {
        ReloadOutcome::Rejected { errors } => {
            assert!(!errors.is_empty());
            assert_eq!(
                errors[0].kind,
                unlucky_core::diagnose::loader::LoadErrorKind::Yaml
            );
        }
        other => panic!("应被拒绝：{other:?}"),
    }
    assert_eq!(loader.rules().len(), 63, "坏文件不该清空规则库");
    assert!(loader.rule_set().get("memory.custom_only").is_some());
    assert!(!loader.last_errors().is_empty());
}

#[test]
fn duplicate_rule_id_across_files_is_rejected() {
    let dir = temp_dir("loader-duplicate");
    let duplicated = "\
version: 1
category: other
rules:
  - id: memory.out_of_memory
    pattern: 'dup'
    cause:
      key: diagnose.cause.dup
      summary: '重复 id'
";
    fs::write(dir.join("other.yaml"), duplicated).expect("写规则文件");
    let loader = RuleLoader::from_dir(&dir);

    assert_eq!(loader.rules().len(), 64, "整体失败后回退内嵌规则集");
    assert!(loader
        .last_errors()
        .iter()
        .any(|error| error.kind == unlucky_core::diagnose::loader::LoadErrorKind::DuplicateId));
}

#[test]
fn unknown_field_is_rejected() {
    let dir = temp_dir("loader-typo");
    let typo = "\
version: 1
category: other
rules:
  - id: other.typo
    patternn: 'typo in field name'
    cause:
      key: diagnose.cause.typo
      summary: '字段拼错'
";
    fs::write(dir.join("other.yaml"), typo).expect("写规则文件");
    let loader = RuleLoader::from_dir(&dir);
    assert_eq!(loader.rules().len(), 64);
    assert!(
        !loader.last_errors().is_empty(),
        "deny_unknown_fields 应把拼错的字段报出来"
    );
}

#[test]
fn rule_set_rejects_invalid_regex() {
    let sources = vec![(
        "bad.yaml".to_owned(),
        "\
version: 1
category: other
rules:
  - id: other.bad_regex
    pattern: '(unclosed'
    cause:
      key: diagnose.cause.bad
      summary: '坏正则'
"
        .to_owned(),
    )];
    let errors = RuleSet::from_sources(&sources).expect_err("坏正则应失败");
    assert_eq!(
        errors[0].kind,
        unlucky_core::diagnose::loader::LoadErrorKind::Regex
    );
}
