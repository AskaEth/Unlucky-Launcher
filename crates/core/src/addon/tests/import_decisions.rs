//! 逐文件冲突决策（M160）：预检列清单 → 用户点头 → 导入按决策处置。
//!
//! # 一张表看完全部用例
//!
//! | 用例 | 钉住的口径 |
//! |------|------------|
//! | [`scan_lists_only_files_that_exist_and_differ`] | 进清单的条件：**已存在且内容不同**；来源记胜者侧 |
//! | [`scan_is_silent_about_identical_and_missing_files`] | 内容相同 / 目标不存在 ⇒ 一行都不列 |
//! | [`scan_does_not_ask_about_files_the_user_deleted`] | 用户删过的不算"已存在且内容不同" |
//! | [`no_decision_means_keep_and_nothing_is_overwritten`] | **缺省保留**（报告的 choice 与磁盘内容双证） |
//! | [`per_item_overwrite_touches_only_the_named_path`] | 逐项决策：只动点名那条，最终文件集逐字节断言 |
//! | [`overwrite_all_is_the_batch_channel_from_the_same_table`] | 整批档来自同一张表（`ImportScan::overwrite_all`） |
//! | [`a_kept_file_is_still_a_conflict_next_time`] | 基线记的是**包内**哈希，所以保留过一次的文件不会被静默覆盖 |
//! | [`curseforge_entries_without_a_path_are_counted_not_guessed`] | 补全前没有路径 ⇒ 不猜，只计数 |
//!
//! 全部离线：远端走 [`support::StubFetcher`]，没有一条网络请求。

use super::decisions_support::{
    layered_pack, paths, seeded, KNOWN_CLIENT, SAME, USER_KNOWN, USER_LAYER,
};
use super::support::{mrpack_stub, sha1};
use super::{read_text, write_file, zip_dir, TempRoot};
use crate::addon::import::{
    import_pack, scan_pack, ConflictChoice, ConflictDecisions, ConflictReason, FileAction,
    FileSource, ImportRequest, ImportStatus,
};
use crate::download::blocking::block_on;

#[test]
fn scan_lists_only_files_that_exist_and_differ() {
    let (pack, _) = layered_pack("dec-scan");
    let target = seeded("dec-scan");
    let scan = scan_pack(&ImportRequest::new(&pack, &target));

    assert!(scan.is_ok(), "预检不许失败：{:?}", scan.error);
    assert_eq!(
        paths(&scan),
        vec!["config/known.toml", "mods/layer.jar"],
        "只列「已存在且内容不同」的两条（same.toml 与 remote.jar 不该出现）"
    );
    let known = &scan.conflicts[0];
    assert_eq!(
        known.source,
        FileSource::ClientOverrides,
        "叠层的胜者是 client-overrides 侧，来源要记它"
    );
    assert_eq!(known.reason, ConflictReason::PreExisting);
    assert_eq!(known.pack_sha1, sha1(KNOWN_CLIENT));
    assert_eq!(known.pack_size, KNOWN_CLIENT.len() as u64);
    assert_eq!(known.disk_sha1, sha1(USER_KNOWN));
    assert_eq!(
        known.disk_size,
        USER_KNOWN.len() as u64,
        "要给出目标现有体积"
    );
    assert_eq!(known.base_sha1, None, "首次导入没有基线");
    assert_eq!(
        scan.lost_bytes(),
        (USER_KNOWN.len() + USER_LAYER.len()) as u64
    );
    assert!(
        scan.needs_decision(),
        "有冲突就必须让用户点头，不能自己决定"
    );
}

#[test]
fn scan_is_silent_about_identical_and_missing_files() {
    let (pack, _) = layered_pack("dec-same");
    let root = TempRoot::new("dec-same");
    let target = root.join("instance");
    // 只放一个**内容与包内一致**的文件：一行都不该列出来。
    write_file(&target.join("config/same.toml"), SAME);
    let scan = scan_pack(&ImportRequest::new(&pack, &target));

    assert!(scan.is_ok(), "{:?}", scan.error);
    assert!(
        paths(&scan).is_empty(),
        "内容一致不算冲突：{:?}",
        paths(&scan)
    );
    assert!(!scan.needs_decision(), "没有冲突就不该打扰用户");
    assert_eq!(scan.unknown_remote, 0, "mrpack 条目自带路径，判得了");
}

#[test]
fn scan_does_not_ask_about_files_the_user_deleted() {
    let (pack, fetcher) = layered_pack("dec-deleted");
    let target = seeded("dec-deleted");
    // 先完整导入一次（有基线了），再删掉其中两个文件。
    let first = block_on(import_pack(
        &ImportRequest::new(&pack, &target)
            .with_fetcher(&fetcher)
            .with_decisions(ConflictDecisions::overwrite_all()),
    ));
    assert_eq!(first.status, ImportStatus::Success, "{:?}", first.error);
    std::fs::remove_file(target.join("config/known.toml")).expect("删文件");
    std::fs::remove_file(target.join("mods/remote.jar")).expect("删文件");

    let scan = scan_pack(&ImportRequest::new(&pack, &target));
    assert!(
        paths(&scan).is_empty(),
        "用户删过的文件不是「已存在且内容不同」，不该问：{:?}",
        paths(&scan)
    );

    // 导入同样不把它们装回来（原有的 `UserDeleted` 判据仍然生效）。
    let second = block_on(import_pack(
        &ImportRequest::new(&pack, &target).with_fetcher(&fetcher),
    ));
    assert_eq!(second.status, ImportStatus::Partial);
    assert!(!target.join("config/known.toml").exists(), "不许偷偷装回来");
    assert!(
        second.conflicts.iter().any(|conflict| {
            conflict.path == "config/known.toml" && conflict.reason == ConflictReason::UserDeleted
        }),
        "但仍要按事记账：{:?}",
        second.conflicts
    );
}

#[test]
fn no_decision_means_keep_and_nothing_is_overwritten() {
    let (pack, fetcher) = layered_pack("dec-keep");
    let target = seeded("dec-keep");
    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target).with_fetcher(&fetcher),
    ));

    // 磁盘内容逐字节没变（**这是"缺省不覆盖"的直接证据**）。
    assert_eq!(
        read_text(&target.join("config/known.toml")),
        "shared = mine\n"
    );
    assert_eq!(read_text(&target.join("mods/layer.jar")), "my layer\n");
    // 报告里这两条都记成 KeptUser + KeepUser。
    for path in ["config/known.toml", "mods/layer.jar"] {
        let conflict = report
            .conflicts
            .iter()
            .find(|conflict| conflict.path == path)
            .unwrap_or_else(|| panic!("{path} 要进冲突清单：{:?}", report.conflicts));
        assert_eq!(
            conflict.choice,
            crate::addon::import::ConflictChoice::KeepUser
        );
        let file = report
            .files
            .iter()
            .find(|file| file.path == path)
            .expect("逐文件结果里也要有它");
        assert_eq!(file.action, FileAction::KeptUser);
    }
    // 不冲突的那些照常落地（「保留」不会把整次导入变成什么都不做）。
    assert_eq!(
        read_text(&target.join("mods/remote.jar")),
        "remote jar bytes"
    );
}

#[test]
fn per_item_overwrite_touches_only_the_named_path() {
    let (pack, fetcher) = layered_pack("dec-item");
    let target = seeded("dec-item");
    let scan = scan_pack(&ImportRequest::new(&pack, &target));
    assert_eq!(paths(&scan), vec!["config/known.toml", "mods/layer.jar"]);

    // 只对 `config/known.toml` 说"覆盖"，另一条不给决策。
    let decisions =
        ConflictDecisions::keep_all().with("config/known.toml", ConflictChoice::Overwrite);
    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target)
            .with_fetcher(&fetcher)
            .with_decisions(decisions),
    ));

    // 最终文件集：点名那条被覆盖，没点名那条原样保留。
    assert_eq!(
        read_text(&target.join("config/known.toml")),
        "shared = client\n"
    );
    assert_eq!(read_text(&target.join("mods/layer.jar")), "my layer\n");
    assert_eq!(read_text(&target.join("config/same.toml")), "same = 1\n");
    assert_eq!(
        read_text(&target.join("mods/remote.jar")),
        "remote jar bytes"
    );
    // 有一条冲突按「保留」处置 ⇒ `Partial`（**不是** `Success`：实例可用，但内容与包不全一致）。
    assert_eq!(report.status, ImportStatus::Partial, "{:?}", report.error);
    let mut chosen: Vec<(&str, ConflictChoice)> = report
        .conflicts
        .iter()
        .map(|conflict| (conflict.path.as_str(), conflict.choice))
        .collect();
    // 报告里的顺序是落盘顺序（overrides 组在前、client-overrides 在后），比较前按路径排序。
    chosen.sort_by(|left, right| left.0.cmp(right.0));
    assert_eq!(
        chosen,
        vec![
            ("config/known.toml", ConflictChoice::Overwrite),
            ("mods/layer.jar", ConflictChoice::KeepUser),
        ]
    );
}

#[test]
fn overwrite_all_is_the_batch_channel_from_the_same_table() {
    let (pack, fetcher) = layered_pack("dec-all");
    let target = seeded("dec-all");
    let scan = scan_pack(&ImportRequest::new(&pack, &target));
    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target)
            .with_fetcher(&fetcher)
            .with_decisions(scan.overwrite_all()),
    ));

    assert_eq!(
        read_text(&target.join("config/known.toml")),
        "shared = client\n"
    );
    assert_eq!(
        read_text(&target.join("mods/layer.jar")),
        "layer from overrides\n"
    );
    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    assert!(report
        .conflicts
        .iter()
        .all(|conflict| conflict.choice == ConflictChoice::Overwrite));
}

#[test]
fn a_kept_file_is_still_a_conflict_next_time() {
    // 基线里必须记**包内**的哈希：若记的是磁盘哈希，下一次导入会判成"用户没动过 ⇒ 整合包升级"
    // 而静默覆盖——那正是本轮要禁的行为。
    let (pack, fetcher) = layered_pack("dec-baseline");
    let target = seeded("dec-baseline");
    for _ in 0..2 {
        let report = block_on(import_pack(
            &ImportRequest::new(&pack, &target).with_fetcher(&fetcher),
        ));
        let conflict = report
            .conflicts
            .iter()
            .find(|conflict| conflict.path == "config/known.toml")
            .expect("第二次也必须报冲突");
        assert_eq!(conflict.choice, ConflictChoice::KeepUser);
        assert_eq!(
            read_text(&target.join("config/known.toml")),
            "shared = mine\n"
        );
    }
    // 第二次的成因变成"用户改过"（有基线了），而不是"没有基线"。
    let scan = scan_pack(&ImportRequest::new(&pack, &target));
    let known = &scan.conflicts[0];
    assert_eq!(known.reason, ConflictReason::UserModified);
    assert_eq!(
        known.base_sha1.as_deref(),
        Some(sha1(KNOWN_CLIENT).as_str())
    );
}

#[test]
fn curseforge_entries_without_a_path_are_counted_not_guessed() {
    // CF 的 `files[]` 只有 projectID/fileID：补全前没有路径 ⇒ 预检判不了，只记条数。
    let fixture = super::fixtures::curseforge("dec-cf");
    let root = TempRoot::new("dec-cf");
    let target = root.join("instance");
    let scan = scan_pack(&ImportRequest::new(&fixture.zip, &target));

    assert!(scan.is_ok(), "{:?}", scan.error);
    assert_eq!(scan.unknown_remote, 2, "两条 CF 远端条目都判不了");
    assert!(
        scan.notes
            .iter()
            .any(|note| note.message_key == crate::addon::import::keys::NOTE_REMOTE_PATH_UNKNOWN),
        "要如实说「还有 N 条判不了」：{:?}",
        scan.notes
    );
    // overrides 里的三条（example.jar / known.toml / broken.toml）在空目标上不冲突。
    assert!(paths(&scan).is_empty(), "{:?}", paths(&scan));
}

#[test]
fn a_remote_entry_with_a_path_but_no_sha1_is_counted_as_unjudgeable() {
    // 复审 r1 P3-4：`unknown_remote` 的**计数口径**与注记文案要同一件事。
    // 这一条只放 `hashes` 里的 sha512、**不写 sha1**：有路径但没有声明摘要 ⇒ 预检没法比对磁盘，
    // 也就不该假装它"没冲突"，而是要计入 unknown_remote 并如实注记。
    let root = TempRoot::new("dec-no-sha1");
    let dir = root.join("src");
    let index = format!(
        r#"{{"game":"minecraft","formatVersion":1,"versionId":"1.0.0","name":"缺 sha1",
             "files":[{{"path":"mods/no-sha1.jar",
                        "hashes":{{"sha512":"{}"}},
                        "downloads":["stub://mods/no-sha1.jar"],"fileSize":16}}],
             "dependencies":{{"minecraft":"1.20.1"}}}}"#,
        "0".repeat(128)
    );
    write_file(&dir.join("modrinth.index.json"), index.as_bytes());
    let pack = root.join("pack.mrpack");
    zip_dir(&dir, &pack);
    let target = root.join("instance");

    let scan = scan_pack(&ImportRequest::new(&pack, &target));
    assert!(scan.is_ok(), "{:?}", scan.error);
    assert_eq!(scan.unknown_remote, 1, "有路径但缺 sha1 ⇒ 判不了，要计数");
    let note = scan
        .notes
        .iter()
        .find(|note| note.message_key == crate::addon::import::keys::NOTE_REMOTE_PATH_UNKNOWN)
        .expect("要如实注记");
    assert!(
        note.message.contains("sha1"),
        "文案要把「缺 sha1 声明」这条来路说清楚（计数口径与文案一致）：{}",
        note.message
    );
}

#[test]
fn a_broken_pack_reports_the_error_instead_of_panicking() {
    let root = TempRoot::new("dec-broken");
    let bad = root.join("bad.zip");
    write_file(&bad, b"not a zip at all");
    let scan = scan_pack(&ImportRequest::new(&bad, &root.join("instance")));
    assert!(!scan.is_ok(), "坏包必须给 error");
    assert!(scan.conflicts.is_empty());
    assert!(!scan.needs_decision(), "判不了就没有可决策的项");
}

#[test]
fn the_scan_does_not_touch_the_target_directory() {
    // 预检是只读的：连目标目录都不该建出来。
    let (pack, _) = layered_pack("dec-readonly");
    let root = TempRoot::new("dec-readonly");
    let target = root.join("instance");
    let scan = scan_pack(&ImportRequest::new(&pack, &target));
    assert!(scan.is_ok(), "{:?}", scan.error);
    assert!(!target.exists(), "预检不许创建目标目录");
}

#[test]
fn mrpack_stub_keeps_its_remote_entries_out_of_the_conflict_list() {
    // 对照夹具（M111 的 mrpack_stub）：远端条目自带真实摘要，目标里没有 ⇒ 一条冲突都没有。
    let (fixture, fetcher) = mrpack_stub("dec-stub");
    let root = TempRoot::new("dec-stub");
    let target = root.join("instance");
    let scan = scan_pack(&ImportRequest::new(&fixture.zip, &target));
    assert!(scan.is_ok(), "{:?}", scan.error);
    assert!(scan.conflicts.is_empty(), "{:?}", paths(&scan));
    let report = block_on(import_pack(
        &ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher),
    ));
    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
}
