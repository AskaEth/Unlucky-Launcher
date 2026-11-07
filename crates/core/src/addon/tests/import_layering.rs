//! 逐文件冲突策略的**边界样本**（research/18 §3.3 / §6 表 B #1–#2 的那几条）。
//!
//! 三条样本各自钉一个事实，缺一条就会复现不同的 bug：
//!
//! | 样本 | 必须成立的事实 | 写错的后果 |
//! |------|----------------|-----------|
//! | 同路径不同哈希 | 叠层顺序里**最后一条**胜；被遮蔽的那条不落地、只留一条具名注记 | 报告里两条 `Installed`，前一条的哈希根本没落地 |
//! | 同哈希不同路径 | **两个文件都落地**（按路径判重，不许按哈希判重） | 静默丢一个文件（内容一样不等于同一个文件） |
//! | overrides 叠层顺序 | `overrides/` 先、`client-overrides/` 后（= 后者胜，Modrinth 官方语义） | 与 HMCL 的相反语义混进来，同一包在不同启动器装出不同内容 |
//!
//! 判据的纯函数版本（`plan::layer::resolve`）在那边有单元测试；这里是**接在管线上的行为**：
//! 真打 zip、真导入、真读回磁盘内容与报告字段。

use std::sync::{Arc, Mutex};

use crate::addon::import::{
    import_pack, keys, ConflictReason, FileAction, ImportProgress, ImportRequest, ImportStage,
    ImportStatus,
};
use crate::download::blocking::block_on;

use super::support::sha1;
use super::{read_text, temp_root, write_file, zip_dir};

/// 打一个只有 `files: []` 的 mrpack（覆盖内容全在 overrides 系列目录里）。
fn index_json(name: &str) -> String {
    format!(
        r#"{{"game": "minecraft", "formatVersion": 1, "versionId": "1.0.0",
            "name": "{name}", "files": [], "dependencies": {{"minecraft": "1.20.1"}}}}"#
    )
}

/// 报告里所有 `NOTE_OVERRIDE_SHADOWED` 注记的消息。
fn shadow_notes(report: &crate::addon::import::ImportReport) -> Vec<&str> {
    report
        .warnings
        .iter()
        .filter(|note| note.message_key == keys::NOTE_OVERRIDE_SHADOWED)
        .map(|note| note.message.as_str())
        .collect()
}

#[test]
fn same_path_with_different_bytes_lands_the_later_side_only() {
    let root = temp_root("layer-same-path");
    let dir = root.join("src");
    write_file(
        &dir.join("modrinth.index.json"),
        index_json("dup conflict").as_bytes(),
    );
    write_file(&dir.join("overrides/config/dup.toml"), b"from-overrides\n");
    write_file(
        &dir.join("client-overrides/config/dup.toml"),
        b"from-client\n",
    );
    let pack = root.join("pack.mrpack");
    zip_dir(&dir, &pack);
    let target = root.join("instance");

    // Extract 阶段的总条数就是"要落地的条目数"：两点声明 ⇒ 只算 1 条（被遮蔽的不读字节）。
    let totals: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = {
        let totals = Arc::clone(&totals);
        Arc::new(move |progress: &ImportProgress| {
            if progress.stage == ImportStage::Extract {
                totals.lock().expect("锁").push(progress.total);
            }
        })
    };
    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target).with_progress(sink),
    ));

    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    // Modrinth 官方语义：client-overrides/ 后写 = 胜。
    assert_eq!(read_text(&target.join("config/dup.toml")), "from-client\n");
    // 一路径一行，且那一行的哈希必须是**真落地**的那份内容。
    let rows: Vec<_> = report
        .files
        .iter()
        .filter(|file| file.path == "config/dup.toml")
        .collect();
    assert_eq!(rows.len(), 1, "同一路径只许一行：{:?}", report.files);
    assert_eq!(
        rows[0].sha1.as_deref(),
        Some(sha1(b"from-client\n").as_str())
    );
    assert_eq!(rows[0].action, FileAction::Installed);
    // 遮蔽要留下具名注记：谁赢了、两份内容不同。
    let notes = shadow_notes(&report);
    assert_eq!(notes.len(), 1, "{:?}", report.warnings);
    assert!(notes[0].contains("内容不同"), "{}", notes[0]);
    assert!(notes[0].contains("client-overrides"), "{}", notes[0]);
    assert!(
        report.conflicts.is_empty(),
        "包内重复声明不是用户冲突：{:?}",
        report.conflicts
    );
    // 被遮蔽的条目不再进 Extract 的总数（1 条真落地）。
    assert_eq!(totals.lock().expect("锁").last().copied(), Some(1));
}

#[test]
fn the_same_bytes_at_two_paths_are_two_files() {
    let root = temp_root("layer-same-bytes");
    let dir = root.join("src");
    write_file(
        &dir.join("modrinth.index.json"),
        index_json("same bytes").as_bytes(),
    );
    let content = b"identical payload\n";
    write_file(&dir.join("overrides/mods/a.jar"), content);
    write_file(&dir.join("overrides/mods/b.jar"), content);
    let pack = root.join("pack.mrpack");
    zip_dir(&dir, &pack);
    let target = root.join("instance");

    let report = block_on(import_pack(&ImportRequest::new(&pack, &target)));

    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    for path in ["mods/a.jar", "mods/b.jar"] {
        assert_eq!(
            read_text(&target.join(path)),
            "identical payload\n",
            "{path}"
        );
    }
    let rows: Vec<_> = report
        .files
        .iter()
        .filter(|file| file.path.ends_with(".jar"))
        .collect();
    assert_eq!(rows.len(), 2, "同哈希不同路径不许折叠：{:?}", report.files);
    assert!(rows
        .iter()
        .all(|row| row.sha1.as_deref() == Some(sha1(content).as_str())));
    assert!(
        shadow_notes(&report).is_empty(),
        "不同路径不是重复声明：{:?}",
        report.warnings
    );
}

#[test]
fn identical_bytes_on_both_sides_shadow_without_claiming_a_conflict() {
    let root = temp_root("layer-same-content");
    let dir = root.join("src");
    write_file(
        &dir.join("modrinth.index.json"),
        index_json("dup identical").as_bytes(),
    );
    let content = b"shared = true\n";
    write_file(&dir.join("overrides/config/shared.toml"), content);
    write_file(&dir.join("client-overrides/config/shared.toml"), content);
    let pack = root.join("pack.mrpack");
    zip_dir(&dir, &pack);
    let target = root.join("instance");

    let report = block_on(import_pack(&ImportRequest::new(&pack, &target)));

    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    assert_eq!(
        read_text(&target.join("config/shared.toml")),
        "shared = true\n"
    );
    let notes = shadow_notes(&report);
    assert_eq!(notes.len(), 1, "{:?}", report.warnings);
    assert!(
        notes[0].contains("逐字节相同"),
        "内容一致必须说清楚，别让人以为丢了东西：{}",
        notes[0]
    );
}

#[test]
fn the_baseline_remembers_the_winning_bytes() {
    // 叠层顺序的**后果**：基线要记胜者那份。记错的话，下一次升级会把"用户没动过"判成
    // "用户改过"（或反过来静默覆盖用户改动）。
    let root = temp_root("layer-baseline");
    let dir = root.join("src");
    write_file(
        &dir.join("modrinth.index.json"),
        index_json("baseline dup").as_bytes(),
    );
    write_file(&dir.join("overrides/config/dup.toml"), b"from-overrides\n");
    write_file(
        &dir.join("client-overrides/config/dup.toml"),
        b"from-client\n",
    );
    let pack = root.join("pack.mrpack");
    zip_dir(&dir, &pack);
    let target = root.join("instance");
    let first = block_on(import_pack(&ImportRequest::new(&pack, &target)));
    assert_eq!(first.status, ImportStatus::Success, "{:?}", first.error);

    // 同一份包再导一次：磁盘内容 == 胜者内容 == 基线 ⇒ 既不该报冲突，也不该重写盘。
    let second = block_on(import_pack(&ImportRequest::new(&pack, &target)));
    assert_eq!(second.status, ImportStatus::Success, "{:?}", second.error);
    assert!(
        second.conflicts.is_empty(),
        "基线记的必须是胜者那份，否则这里会冒出一条假的 user_modified：{:?}",
        second.conflicts
    );
    assert_eq!(read_text(&target.join("config/dup.toml")), "from-client\n");
    // 反向确认：冲突清单里不该出现这条路径的**任何**成因。
    assert!(!second
        .conflicts
        .iter()
        .any(|conflict| conflict.path == "config/dup.toml"
            && conflict.reason != ConflictReason::PreExisting));
}
