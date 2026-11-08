//! 下载复核：sha1/sha512 对不对、失败重试几次、会不会留下半截文件。

use crate::addon::online::download::{download_verified, RETRY_ATTEMPTS};
use crate::addon::online::OnlineErrorKind;
use crate::addon::temp::TempRoot;
use crate::download::blocking::block_on;

use super::support::{hashes_of, FakeArtifact};

const BYTES: &[u8] = b"PK\x03\x04 fake mod jar bytes for offline fixtures";

fn spec() -> (String, String) {
    hashes_of(BYTES)
}

#[test]
fn a_matching_download_is_written_and_reported() {
    let root = TempRoot::new("online-ok");
    let dest = root.join("sodium.jar");
    let fetcher = FakeArtifact::new(BYTES);
    let (sha1, sha512) = spec();
    let done = block_on(download_verified(
        &fetcher,
        "https://cdn.modrinth.com/data/AANobbMI/sodium.jar",
        Some(&sha1),
        Some(&sha512),
        Some(BYTES.len() as u64),
        &dest,
    ))
    .expect("摘要对得上就该成功");
    assert_eq!(done.bytes, BYTES.len() as u64);
    assert_eq!(done.digest.sha512, sha512);
    assert_eq!(std::fs::read(&dest).expect("文件应当落盘"), BYTES);
    assert_eq!(fetcher.attempts(), 1);
}

#[test]
fn a_sha512_mismatch_deletes_the_file_and_retries_then_fails() {
    let root = TempRoot::new("online-bad-sha512");
    let dest = root.join("sodium.jar");
    let fetcher = FakeArtifact::new(BYTES);
    let (sha1, _) = spec();
    let wrong = "0".repeat(128);
    let error = block_on(download_verified(
        &fetcher,
        "https://cdn.modrinth.com/data/AANobbMI/sodium.jar",
        Some(&sha1),
        Some(&wrong),
        None,
        &dest,
    ))
    .expect_err("摘要不符必须失败");
    assert_eq!(error.kind(), OnlineErrorKind::Integrity);
    assert_eq!(fetcher.attempts(), RETRY_ATTEMPTS, "摘要不符要重试到上限");
    assert!(!dest.exists(), "不符的内容不许留在盘上");
}

#[test]
fn a_sha1_mismatch_is_caught_even_when_the_fetcher_skips_verification() {
    let root = TempRoot::new("online-bad-sha1");
    let dest = root.join("sodium.jar");
    let fetcher = FakeArtifact::new(BYTES);
    let (_, sha512) = spec();
    let wrong = "1".repeat(40);
    let error = block_on(download_verified(
        &fetcher,
        "https://cdn.modrinth.com/data/AANobbMI/sodium.jar",
        Some(&wrong),
        Some(&sha512),
        None,
        &dest,
    ))
    .expect_err("sha1 不符必须失败");
    assert_eq!(error.kind(), OnlineErrorKind::Integrity);
    assert!(error.message().contains("SHA-1"), "{}", error.message());
    assert!(!dest.exists());
}

#[test]
fn a_transient_network_failure_is_retried_and_succeeds() {
    let root = TempRoot::new("online-retry");
    let dest = root.join("sodium.jar");
    let fetcher = FakeArtifact::new(BYTES).failing(2);
    let (sha1, sha512) = spec();
    let done = block_on(download_verified(
        &fetcher,
        "https://cdn.modrinth.com/data/AANobbMI/sodium.jar",
        Some(&sha1),
        Some(&sha512),
        Some(BYTES.len() as u64),
        &dest,
    ))
    .expect("第三次该成功");
    assert_eq!(done.bytes, BYTES.len() as u64);
    assert_eq!(fetcher.attempts(), 3);
    assert!(dest.exists());
}

#[test]
fn exhausting_the_retries_leaves_no_half_file() {
    let root = TempRoot::new("online-exhausted");
    let dest = root.join("sodium.jar");
    let fetcher = FakeArtifact::new(BYTES).failing(RETRY_ATTEMPTS + 5);
    let (sha1, sha512) = spec();
    let error = block_on(download_verified(
        &fetcher,
        "https://cdn.modrinth.com/data/AANobbMI/sodium.jar",
        Some(&sha1),
        Some(&sha512),
        None,
        &dest,
    ))
    .expect_err("一直断网必须失败");
    assert_eq!(error.kind(), OnlineErrorKind::NotConnected);
    assert_eq!(fetcher.attempts(), RETRY_ATTEMPTS);
    assert!(!dest.exists(), "失败后不许留文件");
}

#[test]
fn a_truncated_body_is_rejected_by_the_declared_size() {
    let root = TempRoot::new("online-truncated");
    let dest = root.join("sodium.jar");
    let fetcher = FakeArtifact::new(BYTES).truncated();
    // 声明的是全量大小，实际只写了一半 ⇒ 按字节数就该拒
    let (sha1, sha512) = spec();
    let error = block_on(download_verified(
        &fetcher,
        "https://cdn.modrinth.com/data/AANobbMI/sodium.jar",
        Some(&sha1),
        Some(&sha512),
        Some(BYTES.len() as u64),
        &dest,
    ))
    .expect_err("半截文件必须失败");
    assert_eq!(error.kind(), OnlineErrorKind::Integrity);
    assert!(!dest.exists());
}

#[test]
fn a_malformed_declared_hash_never_touches_the_disk_or_the_network() {
    let root = TempRoot::new("online-bad-decl");
    let dest = root.join("sodium.jar");
    let fetcher = FakeArtifact::new(BYTES);
    let error = block_on(download_verified(
        &fetcher,
        "https://cdn.modrinth.com/data/AANobbMI/sodium.jar",
        None,
        Some("abc"),
        None,
        &dest,
    ))
    .expect_err("摘要形状不对必须失败");
    assert_eq!(error.kind(), OnlineErrorKind::Unsupported);
    assert_eq!(fetcher.attempts(), 0);
    assert!(!dest.exists());
}

#[test]
fn a_malformed_sha1_is_dropped_and_the_sha512_still_guards_the_file() {
    // r1 顺手项：sha1 形状写坏不该让整条装不上——sha512 才是 mrpack 的权威摘要。
    let root = TempRoot::new("online-bad-sha1-good-sha512");
    let dest = root.join("sodium.jar");
    let fetcher = FakeArtifact::new(BYTES);
    let (_, sha512) = spec();
    let done = block_on(download_verified(
        &fetcher,
        "https://cdn.modrinth.com/data/AANobbMI/sodium.jar",
        Some("not-a-sha1-at-all"),
        Some(&sha512),
        Some(BYTES.len() as u64),
        &dest,
    ))
    .expect("sha512 合法就该能校验并装下");
    assert_eq!(done.digest.sha512, sha512);
    assert!(dest.exists());
}

#[test]
fn an_empty_url_is_unsupported() {
    let root = TempRoot::new("online-no-url");
    let dest = root.join("sodium.jar");
    let fetcher = FakeArtifact::new(BYTES);
    let error = block_on(download_verified(&fetcher, "   ", None, None, None, &dest))
        .expect_err("没有地址必须失败");
    assert_eq!(error.kind(), OnlineErrorKind::Unsupported);
    assert_eq!(fetcher.attempts(), 0);
}
