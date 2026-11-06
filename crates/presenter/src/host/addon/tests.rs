//! 内容库宿主的守门测试（M109）：编码、文件名穿越、退避判定、两个站点的方言映射。

use std::time::Duration;

use super::http::{encode, retry_after, retryable};
use super::install::safe_file_name;
use super::sources::{all_kinds, curseforge_loader, date_part};
use crate::dto::ContentKind;

#[test]
fn percent_encoding_keeps_the_query_readable_and_escapes_the_rest() {
    assert_eq!(encode("1.20.1"), "1.20.1");
    assert_eq!(
        encode("[\"versions:1.20.1\"]"),
        "%5B%22versions%3A1.20.1%22%5D"
    );
    assert_eq!(encode("中文"), "%E4%B8%AD%E6%96%87");
    assert_eq!(encode("a b"), "a%20b");
}

#[test]
fn file_names_never_carry_a_path() {
    // 抓得住：站点给的 `../../evil.jar` 落到实例目录之外（穿越）。
    assert_eq!(safe_file_name("sodium.jar"), "sodium.jar");
    assert_eq!(safe_file_name("../../evil.jar"), "evil.jar");
    assert_eq!(safe_file_name("mods\\jei.jar"), "jei.jar");
    assert_eq!(safe_file_name(".."), "download.bin");
    assert_eq!(safe_file_name(""), "download.bin");
}

#[test]
fn dates_are_sliced_not_guessed() {
    assert_eq!(date_part("2024-05-01T12:00:00Z"), "2024-05-01");
    assert_eq!(date_part("2024-05"), "2024-05");
    assert_eq!(date_part(""), "");
}

#[test]
fn only_rate_limited_and_transient_failures_are_retried() {
    assert!(retryable("HTTP 429 (retry_after=Some(3))"));
    assert!(retryable("HTTP 503 (retry_after=None)"));
    assert!(!retryable("HTTP 404 (retry_after=None)"));
    assert!(
        !retryable("HTTP 401 (retry_after=None)"),
        "密钥错重试只会更糟"
    );
}

#[test]
fn retry_after_prefers_the_server_and_falls_back_to_exponential() {
    assert_eq!(
        retry_after("HTTP 429 (retry_after=Some(7))", 0),
        Duration::from_secs(7)
    );
    assert_eq!(
        retry_after("HTTP 429 (retry_after=None)", 0),
        Duration::from_secs(1)
    );
    assert_eq!(
        retry_after("HTTP 429 (retry_after=None)", 2),
        Duration::from_secs(4)
    );
    // 服务端要得再多也不等超过 30s（交互与截图不能被一次退避挂死）
    assert_eq!(
        retry_after("HTTP 429 (retry_after=Some(900))", 0),
        Duration::from_secs(30)
    );
}

#[test]
fn curseforge_loader_ids_follow_the_official_enum() {
    assert_eq!(curseforge_loader(1), Some("forge"));
    assert_eq!(curseforge_loader(4), Some("fabric"));
    assert_eq!(curseforge_loader(5), Some("quilt"));
    assert_eq!(curseforge_loader(6), Some("neoforge"));
    assert_eq!(curseforge_loader(99), None);
}

#[test]
fn the_four_content_kinds_are_the_four_side_entries() {
    assert_eq!(
        all_kinds().map(ContentKind::slug),
        ["modpack", "mod", "resourcepack", "shader"]
    );
}
