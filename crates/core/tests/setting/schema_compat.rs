//! schema 版本标识、8 态兼容判定、三档访问权映射。

use serde_json::json;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::schema::{
    FileAccess, SchemaCompat, SchemaId, SchemaRef, SchemaVersion, SCHEMA_KEY,
};

fn expected() -> SchemaRef {
    SchemaRef::new(SchemaId::LAUNCHER, SchemaVersion::new(1, 2, 3))
}

fn compat_of(version: SchemaVersion) -> SchemaCompat {
    let url = SchemaRef::new(SchemaId::LAUNCHER, version).url();
    SchemaCompat::check(&json!({ SCHEMA_KEY: url }), expected())
}

#[test]
fn version_parsing_and_ordering() {
    let version = SchemaVersion::parse("1.2.3").expect("解析");
    assert_eq!(version, SchemaVersion::new(1, 2, 3));
    assert_eq!(version.to_string(), "1.2.3");
    assert_eq!(version.next_major(), SchemaVersion::new(2, 0, 0));
    assert!(SchemaVersion::parse("1.2").is_none());
    assert!(SchemaVersion::parse("1.2.3.4").is_none());
    assert!(SchemaVersion::new(1, 2, 3) < SchemaVersion::new(1, 3, 0));
}

#[test]
fn schema_url_round_trip() {
    let reference = launcher_schema().schema_ref();
    let url = reference.url();
    assert!(
        url.ends_with(&format!("launcher/{}", reference.version)),
        "{url}"
    );
    assert_eq!(SchemaRef::parse_url(&url), Some(reference));
    assert!(SchemaRef::parse_url("https://example.com/x").is_none());
    assert!(SchemaRef::parse_url("garbage").is_none());
    assert_eq!(SchemaId::parse("instance"), Some(SchemaId::INSTANCE));
    assert!(SchemaId::parse("nope").is_none());
}

#[test]
fn same_or_older_versions_are_read_write() {
    assert_eq!(
        compat_of(SchemaVersion::new(1, 2, 3)),
        SchemaCompat::ReadWrite
    );
    assert_eq!(
        compat_of(SchemaVersion::new(1, 2, 2)),
        SchemaCompat::ReadWrite
    );
    assert_eq!(
        compat_of(SchemaVersion::new(1, 0, 0)),
        SchemaCompat::ReadWrite
    );
    assert_eq!(
        compat_of(SchemaVersion::new(0, 9, 0)),
        SchemaCompat::ReadWrite
    );
}

#[test]
fn newer_patch_keeps_original_schema() {
    let compat = compat_of(SchemaVersion::new(1, 2, 4));
    assert_eq!(compat, SchemaCompat::ReadWritePreserveSchema);
    assert!(compat.preserve_schema());
    assert_eq!(compat.access(), FileAccess::ReadWrite);
}

#[test]
fn newer_minor_is_read_only() {
    let compat = compat_of(SchemaVersion::new(1, 3, 0));
    assert_eq!(compat, SchemaCompat::ReadOnlyPreserveSchema);
    assert!(compat.preserve_schema());
    assert_eq!(compat.access(), FileAccess::ReadOnly);
    assert!(!compat.access().can_save());
    assert!(compat.access().blocks_editing());
}

#[test]
fn newer_major_is_unreadable() {
    let compat = compat_of(SchemaVersion::new(2, 0, 0));
    assert_eq!(compat, SchemaCompat::UnsupportedMajor);
    assert!(!compat.readable());
    assert_eq!(compat.access(), FileAccess::Unreadable);
    assert!(!compat.access().can_read());
}

#[test]
fn other_ids_and_broken_urls_are_rejected() {
    let instance_url = SchemaRef::new(SchemaId::INSTANCE, SchemaVersion::new(1, 0, 0)).url();
    assert_eq!(
        SchemaCompat::check(&json!({ SCHEMA_KEY: instance_url }), expected()),
        SchemaCompat::UnexpectedId
    );
    assert_eq!(
        SchemaCompat::check(&json!({ SCHEMA_KEY: "not-a-url" }), expected()),
        SchemaCompat::Invalid
    );
    assert_eq!(
        SchemaCompat::check(&json!({ "schemaVersion": 1 }), expected()),
        SchemaCompat::Missing
    );
    assert_eq!(SchemaCompat::Unparseable.access(), FileAccess::Unreadable);
}

#[test]
fn access_three_tiers() {
    assert!(FileAccess::ReadWrite.can_read() && FileAccess::ReadWrite.can_save());
    assert!(FileAccess::ReadOnly.can_read() && !FileAccess::ReadOnly.can_save());
    assert!(!FileAccess::Unreadable.can_read());
    assert!(FileAccess::Unreadable.blocks_editing());
    assert_eq!(FileAccess::ReadOnly.as_str(), "read_only");
    assert_eq!(
        SchemaCompat::ReadOnlyPreserveSchema.as_str(),
        "read_only_preserve_schema"
    );
}
