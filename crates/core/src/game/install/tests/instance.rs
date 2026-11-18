//! 实例登记：幂等（同版本同名复用）、`isolation: inherit` 不落盘、两份文件各就各位。

use std::path::PathBuf;

use crate::game::install::instance::{
    read_instance_config, read_profiles, register, InstanceConfig, Isolation,
};
use crate::platform::layout::LauncherLayout;

fn temp_layout(tag: &str) -> (LauncherLayout, PathBuf) {
    let unique = format!(
        "{}-{}-{tag}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let root = std::env::temp_dir().join(format!("ull-install-instance-{unique}"));
    let layout = LauncherLayout::new(&root);
    layout.ensure().expect("建四分区");
    (layout, root)
}

#[test]
fn registers_profile_and_sparse_instance_config() {
    let (layout, root) = temp_layout("new");
    let registration = register(
        &layout,
        "1.20.1",
        "1.20.1",
        Some("2026-10-30T00:00:00.000Z"),
    )
    .expect("登记应成功");
    assert!(registration.created);
    assert!(
        registration.profile.id.is_uuid_shaped(),
        "实例 id 必须是稳定 UUID 形状"
    );
    assert!(registration.config_path.is_file());

    let profiles = read_profiles(&layout).expect("读实例清单");
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles.profiles[0].version_id, "1.20.1");
    assert_eq!(
        profiles.profiles[0].created_at.as_deref(),
        Some("2026-10-30T00:00:00.000Z")
    );

    // 每实例设置：id / versionId / schemaVersion 在，isolation 缺省不落盘。
    let text = std::fs::read_to_string(&registration.config_path).expect("读设置文件");
    assert!(
        text.contains("\"versionId\": \"1.20.1\""),
        "键名用 camelCase：{text}"
    );
    assert!(
        !text.contains("isolation"),
        "inherit 是默认值且不落盘：{text}"
    );
    assert!(!text.contains("gameDir"), "非 custom 不写 gameDir：{text}");

    let config = read_instance_config(&layout, registration.profile.id.as_str())
        .expect("读设置")
        .expect("设置文件应存在");
    assert!(config.inherits_isolation());
    assert_eq!(config.id, registration.profile.id.as_str());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn reinstall_same_version_reuses_instance() {
    let (layout, root) = temp_layout("idempotent");
    let first = register(&layout, "1.20.1", "1.20.1", None).expect("第一次登记");
    let second = register(&layout, "1.20.1", "1.20.1", None).expect("第二次登记");

    assert!(!second.created, "同版本同名应复用");
    assert_eq!(second.profile.id.as_str(), first.profile.id.as_str());
    assert_eq!(read_profiles(&layout).expect("读清单").len(), 1);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn distinct_names_create_distinct_instances_of_same_version() {
    let (layout, root) = temp_layout("names");
    let a = register(&layout, "生存", "1.20.1", None).expect("A");
    let b = register(&layout, "创造", "1.20.1", None).expect("B");

    assert!(a.created && b.created);
    assert_ne!(a.profile.id.as_str(), b.profile.id.as_str());
    assert_eq!(read_profiles(&layout).expect("读清单").len(), 2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn instance_config_roundtrip_keeps_explicit_isolation() {
    let mut config = InstanceConfig::new("11111111-2222-4333-8444-555555555555", "1.20.1");
    config.isolation = Some(Isolation::Isolated);
    let text = config.to_json_string().expect("序列化");
    assert!(text.contains("\"isolation\": \"isolated\""));
    let back = InstanceConfig::from_json_str(&text).expect("反序列化");
    assert_eq!(back.isolation, Some(Isolation::Isolated));
    assert!(!back.inherits_isolation());
    assert_eq!(Isolation::parse("custom"), Some(Isolation::Custom));
    assert_eq!(Isolation::parse("inherit"), None, "inherit 不是显式值");
}

#[test]
fn registers_legacy_profile_without_config_file() {
    let (layout, root) = temp_layout("legacy");
    // 早期 CLI 只写 profiles.json（没有 instances/<id>.json）。
    let profile = crate::game::profiles::InstanceProfile::new("老实例", "1.20.1");
    let id = profile.id.as_str().to_string();
    let mut profiles = crate::game::profiles::ProfileList::default();
    profiles.upsert(profile);
    super::super::instance::write_profiles(&layout, &profiles).expect("写清单");

    let registration = register(&layout, "老实例", "1.20.1", None).expect("补登记");
    assert!(!registration.created, "已有档案应复用");
    assert_eq!(registration.profile.id.as_str(), id);
    assert!(registration.config_path.is_file(), "缺失的设置文件应补齐");
    std::fs::remove_dir_all(&root).ok();
}
