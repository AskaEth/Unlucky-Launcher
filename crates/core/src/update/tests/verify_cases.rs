//! 校验层：摘要不符、包体截断都必须被挡下，且暂存文件不许留下。

use std::fs;
use std::path::Path;

use crate::update::manifest::ReleaseManifest;
use crate::update::{stage, DirSource, UpdateErrorKind};

use super::support::{artifact, manifest, release, sha256, TempDir};

/// 暂存目录里的文件名（断言「失败后没留垃圾」用）。
fn staging_files(dir: &Path) -> Vec<String> {
    match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn artifact_of(json: &str) -> crate::update::Artifact {
    ReleaseManifest::from_json(json)
        .expect("清单应可解析")
        .releases[0]
        .artifacts[0]
        .clone()
}

#[test]
fn digest_mismatch_is_rejected_and_staged_file_removed() {
    let tmp = TempDir::new("verify-digest");
    let source = DirSource::new(tmp.path());
    // 清单声明的摘要按 A 算，实际发的是 B：长度一样，只有摘要能抓住。
    let declared = b"payload-aaaa";
    let delivered = b"payload-bbbb";
    assert_eq!(declared.len(), delivered.len());
    tmp.write("ull-test.zip", delivered);
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact(
                "win-x64",
                "portable-zip",
                "ull-test.zip",
                declared,
            )],
        )],
    );
    let artifact = artifact_of(&json);
    assert_eq!(artifact.integrity.hex, sha256(declared));

    let staging = tmp.join("staging");
    let error = stage(&source, &artifact, &staging).expect_err("摘要不符必须拒绝");
    assert_eq!(error.kind, UpdateErrorKind::DigestMismatch);
    assert!(
        error
            .path
            .as_ref()
            .is_some_and(|path| path.starts_with(&staging)),
        "错误应指向暂存文件：{error}"
    );
    assert!(
        staging_files(&staging).is_empty(),
        "校验失败的暂存文件必须被删掉，实际：{:?}",
        staging_files(&staging)
    );
}

#[test]
fn truncated_payload_is_rejected_as_size_mismatch() {
    let tmp = TempDir::new("verify-truncated");
    let source = DirSource::new(tmp.path());
    let full = b"a full portable zip payload";
    // 发布侧发一半就断了：长度先抓到，不用读摘要就能拒绝。
    tmp.write("ull-test.zip", &full[..8]);
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact("win-x64", "portable-zip", "ull-test.zip", full)],
        )],
    );
    let staging = tmp.join("staging");
    let error = stage(&source, &artifact_of(&json), &staging).expect_err("截断的包必须拒绝");
    assert_eq!(error.kind, UpdateErrorKind::SizeMismatch);
    assert!(error.to_string().contains("截断"));
    assert!(staging_files(&staging).is_empty());
}

#[test]
fn oversized_payload_is_rejected_too() {
    let tmp = TempDir::new("verify-oversized");
    let source = DirSource::new(tmp.path());
    tmp.write("ull-test.zip", b"payload and then some");
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact(
                "win-x64",
                "portable-zip",
                "ull-test.zip",
                b"payload",
            )],
        )],
    );
    let error = stage(&source, &artifact_of(&json), &tmp.join("staging"))
        .expect_err("多出来的字节同样不许装");
    assert_eq!(error.kind, UpdateErrorKind::SizeMismatch);
}

#[test]
fn matching_payload_is_staged_and_cleaned_on_drop() {
    let tmp = TempDir::new("verify-ok");
    let source = DirSource::new(tmp.path());
    let payload = b"a real payload, short but real";
    tmp.write("ull-test.zip", payload);
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact("win-x64", "portable-zip", "ull-test.zip", payload)],
        )],
    );
    let staged = stage(&source, &artifact_of(&json), &tmp.join("staging")).expect("校验应通过");
    assert_eq!(staged.size(), payload.len() as u64);
    assert_eq!(fs::read(staged.path()).expect("暂存文件应存在"), payload);
    assert_eq!(staged.integrity().hex, sha256(payload));
    let path = staged.path().to_path_buf();
    drop(staged);
    assert!(!path.exists(), "StagedPackage 落地后应自行清理");
}
