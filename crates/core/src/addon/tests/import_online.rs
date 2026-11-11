//! 远端条目补全（M162）：**有 path 但没有 `hashes.sha1` 的 mrpack 条目，补得出就判，补不出照旧计数**。
//!
//! 三条纪律：
//!
//! 1. 补全靠 `PackFetcher::modrinth_file_by_sha512`（默认 `Unsupported` ⇒ 离线导入补不出）；
//! 2. 补出来的条目与自带 `sha1` 的条目走**同一个** `judge`（M160 的判据一行没改）；
//! 3. 补出的 `sha1` 会真的用在下载校验上——所以「站点给错摘要」必须是 `Failed`（内容不符），
//!    不能被当成「暂时搞不到」混过去。
//!
//! 全部离线：`StubFetcher` 里挂一条 sha512 → 元数据，没有一个真实请求。

use super::support::{sha1, StubFetcher};
use super::{read_text, write_file, zip_dir, TempRoot};
use crate::addon::import::{
    import_pack, scan_pack, scan_pack_online, ImportRequest, ImportStatus, ModrinthFileMeta,
};
use crate::download::blocking::block_on;

const PATH: &str = "mods/no-sha1.jar";
const DISK_BYTES: &[u8] = b"what the user already has";
const PACK_BYTES: &[u8] = b"what the content source serves";
const DOWNLOAD: &str = "stub://mods/no-sha1.jar";
const SHA512: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// 一个只有 `hashes.sha512` 的 mrpack（sha1 缺失 = 预检判不了）。
fn mrpack_without_sha1(tag: &str) -> (TempRoot, std::path::PathBuf) {
    let root = TempRoot::new(tag);
    let dir = root.join("src");
    let index = format!(
        r#"{{"game":"minecraft","formatVersion":1,"versionId":"1.0.0","name":"缺 sha1 的包",
             "files":[{{"path":"{PATH}",
                        "hashes":{{"sha512":"{SHA512}"}},
                        "downloads":["{DOWNLOAD}"],"fileSize":{size}}}],
             "dependencies":{{"minecraft":"1.20.1"}}}}"#,
        size = PACK_BYTES.len()
    );
    write_file(&dir.join("modrinth.index.json"), index.as_bytes());
    let pack = root.join("pack.mrpack");
    zip_dir(&dir, &pack);
    (root, pack)
}

/// 站点给出这个文件的 sha1/字节数（`sha1` 由调用方决定对不对）。
fn meta(pack_sha1: &str) -> ModrinthFileMeta {
    ModrinthFileMeta {
        sha1: Some(pack_sha1.to_string()),
        size: Some(PACK_BYTES.len() as u64),
        url: Some(DOWNLOAD.to_string()),
        file_name: Some("no-sha1.jar".to_string()),
    }
}

#[test]
fn offline_scan_keeps_the_unknown_remote_count() {
    // 对照：没有 fetcher（离线）时口径与 M160 完全一致。
    let (root, pack) = mrpack_without_sha1("online-scan-offline");
    let target = root.join("instance");
    write_file(&target.join(PATH), DISK_BYTES);
    let scan = scan_pack(&ImportRequest::new(&pack, &target));
    assert!(scan.is_ok(), "{:?}", scan.error);
    assert_eq!(scan.unknown_remote, 1, "缺 sha1 就是判不了，如实计数");
    assert!(scan.conflicts.is_empty());
}

#[test]
fn an_online_scan_completes_the_sha1_and_judges_the_conflict() {
    let (root, pack) = mrpack_without_sha1("online-scan-filled");
    let target = root.join("instance");
    write_file(&target.join(PATH), DISK_BYTES);
    let fetcher = StubFetcher::new().with_modrinth(SHA512, meta(&sha1(PACK_BYTES)));
    let scan = block_on(scan_pack_online(
        &ImportRequest::new(&pack, &target).with_fetcher(&fetcher),
    ));
    assert!(scan.is_ok(), "{:?}", scan.error);
    assert_eq!(scan.unknown_remote, 0, "补出来了就不再是「判不了」");
    let paths: Vec<&str> = scan.conflicts.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, vec![PATH], "补出来的摘要要真的用上：{:?}", paths);
    assert_eq!(scan.conflicts[0].pack_sha1, sha1(PACK_BYTES));
    assert!(scan
        .notes
        .iter()
        .any(|note| note.message.contains("补全了 1 条远端条目的 sha1")));
}

#[test]
fn a_scan_whose_source_does_not_know_the_hash_still_counts_it() {
    let (root, pack) = mrpack_without_sha1("online-scan-unknown");
    let target = root.join("instance");
    write_file(&target.join(PATH), DISK_BYTES);
    // 站点不认得这个 sha512（stub 里没挂）⇒ 补不出，口径回到 M160
    let fetcher = StubFetcher::new();
    let scan = block_on(scan_pack_online(
        &ImportRequest::new(&pack, &target).with_fetcher(&fetcher),
    ));
    assert_eq!(scan.unknown_remote, 1);
    assert!(scan.conflicts.is_empty());
}

#[test]
fn a_completed_sha1_is_actually_used_for_the_download_check() {
    let (root, pack) = mrpack_without_sha1("online-import-ok");
    let target = root.join("instance");
    let fetcher = StubFetcher::new()
        .with_modrinth(SHA512, meta(&sha1(PACK_BYTES)))
        .with_file(DOWNLOAD, PACK_BYTES);
    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target).with_fetcher(&fetcher),
    ));
    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    assert_eq!(
        read_text(&target.join(PATH)),
        String::from_utf8_lossy(PACK_BYTES)
    );
    assert!(report.missing.is_empty(), "{:?}", report.missing);
    assert!(
        report
            .warnings
            .iter()
            .any(|note| note.message.contains("补全了 1 条远端条目的 sha1")),
        "报告要说清补全过：{:?}",
        report.warnings
    );
}

#[test]
fn a_wrong_completed_sha1_fails_the_import_instead_of_installing() {
    // 站点给了错的 sha1 ⇒ 下载校验必须拦住（这是"内容坏了"，不是"暂时搞不到"）。
    let (root, pack) = mrpack_without_sha1("online-import-bad");
    let target = root.join("instance");
    let fetcher = StubFetcher::new()
        .with_modrinth(SHA512, meta(&"f".repeat(40)))
        .with_file(DOWNLOAD, PACK_BYTES);
    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target).with_fetcher(&fetcher),
    ));
    assert_eq!(report.status, ImportStatus::Failed, "{:?}", report.status);
    assert_eq!(
        report.error.as_ref().map(|error| error.code()),
        Some("import_integrity"),
        "{:?}",
        report.error
    );
    assert!(!target.join(PATH).exists(), "失败的导入不许留文件");
}

// ---------------------------------------------------------------- r1 P1-1 后果 B 的守卫

const CF_SHA512: &str = "c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1\
c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1";

/// 两条远端条目：第一条**站点不认得**（等价于托管在 CurseForge CDN），第二条认得。
///
/// 真实 mrpack 大多是这个形状——所以「第一条补不出就掐掉整轮」是 r1 抓到的后果 B。
fn pack_with_one_unknown_sha512(tag: &str) -> (TempRoot, std::path::PathBuf) {
    let root = TempRoot::new(tag);
    let dir = root.join("src");
    let index = format!(
        r#"{{"game":"minecraft","formatVersion":1,"versionId":"1.0.0","name":"混托管",
             "files":[
               {{"path":"mods/hosted-on-cf.jar","hashes":{{"sha512":"{CF_SHA512}"}},
                 "downloads":["stub://mods/hosted-on-cf.jar"],"fileSize":10}},
               {{"path":"{PATH}","hashes":{{"sha512":"{SHA512}"}},
                 "downloads":["{DOWNLOAD}"],"fileSize":{size}}}
             ],
             "dependencies":{{"minecraft":"1.20.1"}}}}"#,
        size = PACK_BYTES.len()
    );
    write_file(&dir.join("modrinth.index.json"), index.as_bytes());
    let pack = root.join("pack.mrpack");
    zip_dir(&dir, &pack);
    (root, pack)
}

#[test]
fn a_404_on_one_entry_does_not_stop_the_completion_round() {
    // 第一次查（CF 那一条）站点回 404 ⇒ **不许**停：第二条仍然要补出来并参与判定。
    let (root, pack) = pack_with_one_unknown_sha512("online-mixed-hosting");
    let target = root.join("instance");
    write_file(&target.join(PATH), DISK_BYTES);
    write_file(&target.join("mods/hosted-on-cf.jar"), b"user's own copy");

    let fetcher = StubFetcher::new().with_modrinth(SHA512, meta(&sha1(PACK_BYTES)));
    let scan = block_on(scan_pack_online(
        &ImportRequest::new(&pack, &target).with_fetcher(&fetcher),
    ));
    assert!(scan.is_ok(), "{:?}", scan.error);
    let paths: Vec<&str> = scan.conflicts.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![PATH],
        "第二条（托管在 Modrinth 的那条）必须补出来并判出冲突：{:?}",
        paths
    );
    assert_eq!(scan.unknown_remote, 1, "补不出的那一条仍如实计数");
    assert!(
        !scan
            .notes
            .iter()
            .any(|note| note.message.contains("未联网")),
        "404 不是「没联网」，注记不许这么说：{:?}",
        scan.notes.iter().map(|n| &n.message).collect::<Vec<_>>()
    );
}

#[test]
fn only_a_real_offline_stops_the_completion_round() {
    let (root, pack) = pack_with_one_unknown_sha512("online-mixed-offline");
    let target = root.join("instance");
    write_file(&target.join(PATH), DISK_BYTES);
    write_file(&target.join("mods/hosted-on-cf.jar"), b"user's own copy");

    let fetcher = StubFetcher::new()
        .with_modrinth(SHA512, meta(&sha1(PACK_BYTES)))
        .offline_modrinth();
    let scan = block_on(scan_pack_online(
        &ImportRequest::new(&pack, &target).with_fetcher(&fetcher),
    ));
    assert!(scan.is_ok(), "{:?}", scan.error);
    assert_eq!(scan.unknown_remote, 2, "没联网 ⇒ 两条都判不了");
    assert!(scan.conflicts.is_empty());
    // 断网且一条也没补到：注记不许说「补全了 0 条」（读起来像成功），也不许带「缺省保留」
    // ——那句是导入场景的尾缀，预检里没有这回事（r3 登记项）。
    let note = scan
        .notes
        .iter()
        .find(|note| note.message.contains("sha512"))
        .expect("断网要留一条补全注记");
    assert!(note.message.contains("一条也没补到"), "{}", note.message);
    assert!(!note.message.contains("补全了 0 条"), "{}", note.message);
    assert!(!note.message.contains("缺省保留"), "{}", note.message);
}

/// 清单自带 `sha1` 时补全不许插手（也不许改写清单里的摘要）。
fn pack_with_sha1_and_sha512(tag: &str) -> (TempRoot, std::path::PathBuf) {
    let root = TempRoot::new(tag);
    let dir = root.join("src");
    let index = format!(
        r#"{{"game":"minecraft","formatVersion":1,"versionId":"1.0.0","name":"两档摘要都有",
             "files":[{{"path":"{PATH}",
                        "hashes":{{"sha1":"{sha1}","sha512":"{SHA512}"}},
                        "downloads":["{DOWNLOAD}"],"fileSize":{size}}}],
             "dependencies":{{"minecraft":"1.20.1"}}}}"#,
        sha1 = sha1(PACK_BYTES),
        size = PACK_BYTES.len()
    );
    write_file(&dir.join("modrinth.index.json"), index.as_bytes());
    let pack = root.join("pack.mrpack");
    zip_dir(&dir, &pack);
    (root, pack)
}

#[test]
fn an_entry_that_already_has_a_sha1_is_left_alone() {
    let (root, pack) = pack_with_sha1_and_sha512("online-keeps-sha1");
    let target = root.join("instance");
    write_file(&target.join(PATH), DISK_BYTES);
    // 站点那边挂一个**不一样**的 sha1：真去反查并写回的话，这条就会被改写。
    let fetcher = StubFetcher::new().with_modrinth(SHA512, meta(&"e".repeat(40)));
    let scan = block_on(scan_pack_online(
        &ImportRequest::new(&pack, &target).with_fetcher(&fetcher),
    ));
    assert!(scan.is_ok(), "{:?}", scan.error);
    let conflict = scan.conflicts.first().expect("磁盘内容不同 ⇒ 要判出冲突");
    assert_eq!(
        conflict.pack_sha1,
        sha1(PACK_BYTES),
        "清单里的 sha1 不许被改写"
    );
    assert!(
        !scan
            .notes
            .iter()
            .any(|note| note.message.contains("补全了")),
        "没有条目需要补 ⇒ 不该写「补全了 N 条」的注记"
    );
}

/// 清单没写 `downloads`：补全给的直链要能真的用上（否则这条只会进 missing）。
fn pack_without_downloads(tag: &str) -> (TempRoot, std::path::PathBuf) {
    let root = TempRoot::new(tag);
    let dir = root.join("src");
    let index = format!(
        r#"{{"game":"minecraft","formatVersion":1,"versionId":"1.0.0","name":"没有直链",
             "files":[{{"path":"{PATH}","hashes":{{"sha512":"{SHA512}"}},
                        "downloads":[],"fileSize":{size}}}],
             "dependencies":{{"minecraft":"1.20.1"}}}}"#,
        size = PACK_BYTES.len()
    );
    write_file(&dir.join("modrinth.index.json"), index.as_bytes());
    let pack = root.join("pack.mrpack");
    zip_dir(&dir, &pack);
    (root, pack)
}

#[test]
fn a_completed_entry_can_supply_the_missing_download_url() {
    let (root, pack) = pack_without_downloads("online-url-from-source");
    let target = root.join("instance");
    let fetcher = StubFetcher::new()
        .with_modrinth(SHA512, meta(&sha1(PACK_BYTES)))
        .with_file(DOWNLOAD, PACK_BYTES);
    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target).with_fetcher(&fetcher),
    ));
    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    assert_eq!(
        read_text(&target.join(PATH)),
        String::from_utf8_lossy(PACK_BYTES)
    );
}
