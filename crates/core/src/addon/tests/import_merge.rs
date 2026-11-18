//! overrides 三方合并的**行为**测试：用户改过 / 用户删过 / 没动过 / 没有基线 / 侧优先级。
//!
//! 判据本身（纯函数）在 `import::merge` 的单元测试里；这里验证的是"判据真的接在管线上了"：
//! 真写盘、真重导、真读回内容。

use crate::addon::import::{
    import_pack, ConflictChoice, ConflictPolicy, ConflictReason, FileAction, ImportRequest,
    ImportStatus,
};
use crate::download::blocking::block_on;

use super::support::mrpack_stub;
use super::{list_tree, read_text, write_file, zip_dir, TempRoot};

use crate::addon::import::{PackBaseline, BASELINE_FILE};

#[test]
fn user_modified_file_is_kept_and_reported() {
    let (fixture, fetcher) = mrpack_stub("merge-modified");
    let root = TempRoot::new("merge-modified-out");
    let target = root.join("instance");
    let first = block_on(import_pack(
        &ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher),
    ));
    assert_eq!(first.status, ImportStatus::Success, "{:?}", first.error);

    // 用户动手改了一个包内文件。
    write_file(&target.join("config/known.toml"), b"shared = false\n");
    let second = block_on(import_pack(
        &ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher),
    ));

    assert_eq!(
        second.status,
        ImportStatus::Partial,
        "保留用户文件 ⇒ 部分成功"
    );
    assert_eq!(second.conflicts.len(), 1, "{:?}", second.conflicts);
    let conflict = &second.conflicts[0];
    assert_eq!(conflict.path, "config/known.toml");
    assert_eq!(conflict.reason, ConflictReason::UserModified);
    assert_eq!(conflict.choice, ConflictChoice::KeepUser);
    assert_eq!(
        conflict.message_key,
        crate::addon::import::keys::CONFLICT_USER_MODIFIED
    );
    assert!(conflict.base_sha1.is_some(), "基线里应有上次的哈希");
    assert!(conflict.disk_sha1.is_some());
    // 磁盘内容没被盖掉。
    assert_eq!(
        read_text(&target.join("config/known.toml")),
        "shared = false\n"
    );
    // 报告里这条的结局是"保留用户"。
    assert!(second
        .files
        .iter()
        .any(|file| file.path == "config/known.toml" && file.action == FileAction::KeptUser));
    assert_eq!(second.status.exit_code(), 3);
}

#[test]
fn user_deleted_file_is_not_restored() {
    let (fixture, fetcher) = mrpack_stub("merge-deleted");
    let root = TempRoot::new("merge-deleted-out");
    let target = root.join("instance");
    let first = block_on(import_pack(
        &ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher),
    ));
    assert_eq!(first.status, ImportStatus::Success, "{:?}", first.error);

    // 用户删了一个包内文件。
    std::fs::remove_file(target.join("mods/half.jar")).expect("删夹具文件");
    let second = block_on(import_pack(
        &ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher),
    ));

    assert_eq!(second.status, ImportStatus::Partial);
    let conflict = second
        .conflicts
        .iter()
        .find(|conflict| conflict.path == "mods/half.jar")
        .expect("删除也要进冲突清单");
    assert_eq!(conflict.reason, ConflictReason::UserDeleted);
    assert_eq!(conflict.choice, ConflictChoice::KeepUser);
    assert_eq!(conflict.disk_sha1, None);
    assert!(
        !target.join("mods/half.jar").exists(),
        "用户删掉的文件不许偷偷装回来"
    );
}

#[test]
fn untouched_file_is_upgraded_by_a_newer_pack() {
    let (fixture, fetcher) = mrpack_stub("merge-upgrade");
    let root = TempRoot::new("merge-upgrade-out");
    let target = root.join("instance");
    let first = block_on(import_pack(
        &ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher),
    ));
    assert_eq!(first.status, ImportStatus::Success, "{:?}", first.error);

    // 新版本包：同一个路径，内容变了；用户没动过。
    write_file(
        &fixture.dir.join("overrides/config/known.toml"),
        b"shared = 2\n",
    );
    zip_dir(&fixture.dir, &fixture.zip);
    let second = block_on(import_pack(
        &ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher),
    ));

    assert_eq!(
        second.status,
        ImportStatus::Success,
        "未改过的文件升级不算冲突"
    );
    assert!(second.conflicts.is_empty(), "{:?}", second.conflicts);
    assert_eq!(read_text(&target.join("config/known.toml")), "shared = 2\n");
    assert!(second
        .files
        .iter()
        .any(|file| file.path == "config/known.toml" && file.action == FileAction::Replaced));
}

#[test]
fn pre_existing_file_without_baseline_is_kept() {
    let (fixture, fetcher) = mrpack_stub("merge-existing");
    let root = TempRoot::new("merge-existing-out");
    let target = root.join("instance");
    // 首次导入前磁盘上就有同名文件：没有基线可判 ⇒ 最安全的一侧（不覆盖）。
    write_file(&target.join("config/known.toml"), b"mine\n");
    let report = block_on(import_pack(
        &ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher),
    ));

    assert_eq!(report.status, ImportStatus::Partial);
    let conflict = report
        .conflicts
        .iter()
        .find(|conflict| conflict.path == "config/known.toml")
        .expect("要有冲突清单");
    assert_eq!(conflict.reason, ConflictReason::PreExisting);
    assert!(conflict.base_sha1.is_none());
    assert_eq!(read_text(&target.join("config/known.toml")), "mine\n");
}

#[test]
fn overwrite_policy_replaces_user_file_and_reports_the_choice() {
    let (fixture, fetcher) = mrpack_stub("merge-overwrite");
    let root = TempRoot::new("merge-overwrite-out");
    let target = root.join("instance");
    let first = block_on(import_pack(
        &ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher),
    ));
    assert_eq!(first.status, ImportStatus::Success, "{:?}", first.error);
    write_file(&target.join("config/known.toml"), b"shared = false\n");

    let report = block_on(import_pack(
        &ImportRequest::new(&fixture.zip, &target)
            .with_fetcher(&fetcher)
            .with_conflict_policy(ConflictPolicy::Overwrite),
    ));

    assert_eq!(report.status, ImportStatus::Success, "覆盖后结果是干净的");
    let conflict = report
        .conflicts
        .iter()
        .find(|conflict| conflict.path == "config/known.toml")
        .expect("冲突仍要记账");
    assert_eq!(conflict.reason, ConflictReason::UserModified);
    assert_eq!(conflict.choice, ConflictChoice::Overwrite);
    assert_eq!(
        read_text(&target.join("config/known.toml")),
        "shared = true\n"
    );
}

#[test]
fn client_overrides_wins_over_overrides() {
    // Modrinth 官方客户端语义：`client-overrides/` 后写 = 胜（HMCL 恰好相反）。
    let root = TempRoot::new("merge-priority");
    let dir = root.join("src");
    write_file(
        &dir.join("modrinth.index.json"),
        br#"{"game": "minecraft", "formatVersion": 1, "versionId": "1.0.0",
             "name": "dup", "files": [], "dependencies": {"minecraft": "1.20.1"}}"#,
    );
    write_file(&dir.join("overrides/config/dup.toml"), b"from-overrides\n");
    write_file(
        &dir.join("client-overrides/config/dup.toml"),
        b"from-client\n",
    );
    write_file(
        &dir.join("server-overrides/config/dup.toml"),
        b"from-server\n",
    );
    let pack = root.join("pack.mrpack");
    zip_dir(&dir, &pack);

    let target = root.join("instance");
    let report = block_on(import_pack(&ImportRequest::new(&pack, &target)));

    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    assert_eq!(read_text(&target.join("config/dup.toml")), "from-client\n");
    assert!(
        report.conflicts.is_empty(),
        "侧优先级不是冲突：{:?}",
        report.conflicts
    );
}

#[test]
fn baseline_read_and_write_paths_are_the_same_file() {
    // 复审 r5 P1-1 的回归钉：读侧（`PackBaseline::path`）与写侧（`relative_path` 拼 root）
    // **必须解析到同一个路径**。只测"读得到"在大小写不敏感的系统上会假绿。
    let root = TempRoot::under("baseline-path", "instance");
    let read = PackBaseline::path(&root);
    let write = root.join(PackBaseline::relative_path());
    assert_eq!(read, write, "读写必须是同一个文件：{read:?} vs {write:?}");
    // 相对路径的唯一来源是 `GameLayout::ull_root()`（游戏根内的 `.ull/`），不是启动器数据根。
    assert_eq!(
        PackBaseline::relative_path(),
        std::path::PathBuf::from(".ull").join(BASELINE_FILE)
    );
    assert_eq!(
        read.parent(),
        Some(
            crate::game::layout::GameLayout::new(root.to_path_buf())
                .ull_root()
                .as_path()
        )
    );
    // 写侧（pipeline 提交时用的相对路径）必须同源；`.ULL` 是**启动器数据根**的名字，
    // 出现在实例内路径里就是 P1-1 那个 bug（大小写敏感的系统上会变成两个文件）。
    let write_side = crate::addon::import::baseline_relative();
    assert_eq!(
        write_side,
        PackBaseline::relative_path()
            .to_string_lossy()
            .replace('\\', "/"),
        "写侧必须从同一个来源派生"
    );
    assert!(
        !write_side.contains(".ULL"),
        "实例内路径不许用启动器数据根的 `.ULL`：{write_side}"
    );
}

#[test]
fn baseline_of_another_pack_is_not_used_as_base() {
    // 同一目录先后导入两个整合包：别人的基线当 base 会把"用户改过"误判成"整合包升级"而静默覆盖。
    let root = TempRoot::new("merge-identity");
    let source = super::fixtures::multimc("merge-identity-a");
    let target = root.join("instance");
    let first = block_on(import_pack(&ImportRequest::new(&source, &target)));
    assert_eq!(first.status, ImportStatus::Success, "{:?}", first.error);
    // 用户改了一个包内文件。
    write_file(&target.join("mods/example.jar"), b"mine");

    // 另一个包（名字不同 + 同名文件内容不同）导进同一目录。
    let other = root.join("other");
    write_file(
        &other.join("mmc-pack.json"),
        br#"{"formatVersion": 1, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"},
            {"uid": "net.fabricmc.fabric-loader", "version": "0.15.11"}]}"#,
    );
    write_file(
        &other.join("instance.cfg"),
        "InstanceType=OneSix\nname=另一个包\n".as_bytes(),
    );
    write_file(&other.join(".minecraft/mods/example.jar"), b"pack B jar");
    let report = block_on(import_pack(&ImportRequest::new(&other, &target)));

    assert!(
        report
            .warnings
            .iter()
            .any(|note| note.message_key == crate::addon::import::keys::NOTE_BASELINE_PACK_MISMATCH),
        "跨包基线要具名告警：{:?}",
        report.warnings
    );
    let conflict = report
        .conflicts
        .iter()
        .find(|conflict| conflict.path == "mods/example.jar")
        .expect("要有冲突");
    assert_eq!(
        conflict.reason,
        ConflictReason::PreExisting,
        "别人的基线不能拿来判 user_modified"
    );
    assert!(conflict.base_sha1.is_none(), "身份不符时基线应被忽略");
    assert_eq!(read_text(&target.join("mods/example.jar")), "mine");
    assert_eq!(report.status, ImportStatus::Partial);
    assert!(list_tree(&target).contains(&"mods/example.jar".to_string()));
}
