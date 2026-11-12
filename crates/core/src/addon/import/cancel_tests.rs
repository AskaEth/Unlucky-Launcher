//! 导入取消（M195）的守门用例：**取消不是失败**、**取消不留半包 / 不覆盖已有实例**、
//! **取消后不登记实例**。
//!
//! # 每条用例挡哪种回归（变异证明见 mission 报告）
//!
//! | 用例 | 删掉哪一处就红 |
//! |------|----------------|
//! | `a_cancel_before_the_first_write_touches_nothing` | 入口检查点 |
//! | `a_cancel_inside_the_extract_loop_leaves_no_half_unpacked_pack` | 解压循环检查点（判据 = 解压进度条数：有 = 1、删掉 = 3） |
//! | `a_cancel_in_the_middle_of_the_merge_restores_the_users_file` | 取消时的 `rollback_merge` |
//! | `a_cancel_right_before_registering_rolls_the_whole_instance_back` | 登记前的检查点 |
//! | `cancel_and_failure_are_two_different_result_types` | 把取消并进 `fail()` 那条路 |
//!
//! # 解压那条必须数事件，其余没有真守卫的检查点
//!
//! 解压只写**暂存目录**，所以"status == Cancelled / 目标目录不存在 / `files` 为空 / 无残留暂存"
//! 在删掉循环检查点后**依然成立**（提交前检查点会替它停一次）——只有"解了几条"能区分两者。
//!
//! 有变异证明的只有上表五处。**没有**：检查点②解析后 / ③补全后 / ④开暂存目录前 / ⑤提交前（删掉
//! 任一处，别的检查点会接住同一次取消，`pipeline.rs` 的断言仍绿）、全新导入 `commit` 的取消分支、
//! [`super::phase`] 远端下载逐条检查点、[`super::complete`] 补全循环的 `break`——补它们要两段式
//! 夹具或更多计数探针，本轮没做。
//!
//! 触发点全部走**进度回调**（置位 = 用户此刻点了取消，无 sleep、无竞态）；全部离线。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::cancel::ImportCancel;
use super::report::{ImportProgress, ImportProgressSink, ImportStage, ImportStatus};
use super::{import_pack, ImportRequest};
use crate::addon::temp::TempRoot;
use crate::download::blocking::block_on;

/// 一份只含覆盖内容的 mrpack 清单（`files` 为空 ⇒ 全部内容来自 `overrides/`，无远端条目）。
const INDEX: &str = r#"{
  "game": "minecraft",
  "formatVersion": 1,
  "versionId": "1.0.0",
  "name": "Cancelable",
  "files": [],
  "dependencies": {"minecraft": "1.20.1", "fabric-loader": "0.15.11"}
}
"#;

/// 手造 mrpack（`entries` 是 `overrides/` 下的实例内路径与字节）。
fn mrpack(tag: &str, entries: &[(&str, &[u8])]) -> (TempRoot, PathBuf) {
    let root = TempRoot::new(tag);
    let source = root.join("src");
    write_file(&source.join("modrinth.index.json"), INDEX.as_bytes());
    for (path, bytes) in entries {
        write_file(&source.join("overrides").join(path), bytes);
    }
    let pack = root.join("pack.mrpack");
    zip_dir(&source, &pack);
    (root, pack)
}

/// 三个文件的包。
fn three_files(tag: &str) -> (TempRoot, PathBuf) {
    mrpack(
        tag,
        &[
            ("mods/a.jar", b"pack a"),
            ("mods/b.jar", b"pack b"),
            ("mods/c.jar", b"pack c"),
        ],
    )
}

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建目录");
    }
    std::fs::write(path, bytes).expect("写文件");
}

fn zip_dir(source: &Path, target: &Path) {
    let file = std::fs::File::create(target).expect("建 zip");
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let dir = source.join(&relative);
        let mut children: Vec<_> = std::fs::read_dir(&dir)
            .expect("读目录")
            .collect::<Result<Vec<_>, _>>()
            .expect("目录项");
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let relative = relative.join(child.file_name());
            if child.file_type().expect("类型").is_dir() {
                pending.push(relative);
                continue;
            }
            let bytes = std::fs::read(child.path()).expect("读夹具文件");
            let entry = relative.to_string_lossy().replace('\\', "/");
            writer.start_file(entry, options).expect("写 zip 条目");
            writer.write_all(&bytes).expect("写 zip 内容");
        }
    }
    writer.finish().expect("收尾 zip");
}

/// 在**指定阶段的第一条进度事件**上置位取消（= 用户此刻点了取消）。
fn cancel_on(token: &ImportCancel, stage: ImportStage) -> ImportProgressSink {
    let token = token.clone();
    Arc::new(move |progress: &ImportProgress| {
        if progress.stage == stage {
            token.cancel();
        }
    })
}

/// 数**解压阶段**的进度条数，并在第一条上置位取消（这是解压循环检查点唯一的可观测判据：
/// 有它 ⇒ 第一条之后立刻停 = 1；删掉 ⇒ 一路解完 = 文件条数）。
fn cancel_on_first_extract_counting(
    token: &ImportCancel,
) -> (ImportProgressSink, Arc<AtomicUsize>) {
    let token = token.clone();
    let count = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&count);
    let sink: ImportProgressSink = Arc::new(move |progress: &ImportProgress| {
        if progress.stage != ImportStage::Extract {
            return;
        }
        if seen.fetch_add(1, Ordering::SeqCst) == 0 {
            token.cancel();
        }
    });
    (sink, count)
}

/// 在**合并态第二条逐文件进度**上置位取消。合并是排序后逐文件搬：第 0 条是基线
/// （`.ull/pack-baseline.json`，`.` 排最前），第 1 条才是 `mods/a.jar`——`done == 1` 正好是
/// "用户那个被覆盖的文件已经搬进去、后面还有"的窗口，journal 回滚要接住的正是这一刻。
fn cancel_on_second_merged_file(token: &ImportCancel) -> ImportProgressSink {
    let token = token.clone();
    Arc::new(move |progress: &ImportProgress| {
        if progress.stage == ImportStage::Commit && progress.done == 1 {
            token.cancel();
        }
    })
}

/// 目标目录里全部文件的实例内相对路径（排序后）：钉"取消后与导入前逐文件相同"。
fn files_under(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                pending.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .map(|tail| tail.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            out.push(relative);
        }
    }
    out.sort();
    out
}

/// 父目录里有没有遗留的暂存目录（`.<name>.ull-import-<pid>-<seq>`）。
fn stray_staging(parent: &Path) -> Vec<String> {
    std::fs::read_dir(parent)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .filter(|name| name.contains(".ull-import-"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_cancel_before_the_first_write_touches_nothing() {
    // 入口检查点：包还没读，目标目录还没建。
    let (root, pack) = three_files("cancel-entry");
    let target = root.join("instance");
    let token = ImportCancel::new();
    token.cancel();

    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target).with_cancel(&token),
    ));

    assert_eq!(report.status, ImportStatus::Cancelled);
    assert!(
        report.error.is_none(),
        "取消不是失败：{}",
        report.status.as_str()
    );
    assert!(report.pack_name.is_none(), "停在入口 = 包都没读");
    assert!(report.files.is_empty());
    assert!(report.rolled_back, "一个字节都没写，目标目录与导入前一致");
    assert!(!target.exists(), "目标目录不许出现");
    assert!(
        stray_staging(root.path()).is_empty(),
        "{:?}",
        stray_staging(root.path())
    );
}

#[test]
fn a_cancel_inside_the_extract_loop_leaves_no_half_unpacked_pack() {
    // 解压循环里的检查点：第一条进度事件上取消 ⇒ 第一条写进临时目录，第二条之前停下，
    // 临时目录整个删掉 ⇒ 目标目录**不出现半成品**，报告里也不许有"已落地 N 条"的假账。
    let (root, pack) = three_files("cancel-extract");
    let target = root.join("instance");
    let token = ImportCancel::new();
    let (sink, extract_events) = cancel_on_first_extract_counting(&token);

    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target)
            .with_cancel(&token)
            .with_progress(sink),
    ));

    assert_eq!(report.status, ImportStatus::Cancelled, "{:?}", report.error);
    // **这一条才是解压循环检查点的判据**：有它 ⇒ 第一条之后立刻停（3 个文件的包里只报了 1 条
    // 解压进度）；删掉它 ⇒ 一路解完（3 条）。其余断言在删掉检查点后仍然成立，挡不住回归。
    assert_eq!(
        extract_events.load(Ordering::SeqCst),
        1,
        "取消必须在第一条解压之后立刻生效（包里有 3 个文件，解完再停 = 解压段没有检查点）"
    );
    assert!(report.error.is_none(), "取消不许带错误");
    assert!(!target.exists(), "不许留下半包：{}", target.display());
    assert!(
        report.files.is_empty(),
        "取消前的处理记录一条都没落地，报告不许冒充已装：{:?}",
        report.files
    );
    assert_eq!(report.bytes_written, 0, "写入量要归零");
    assert!(report.registered.is_none());
    assert!(report.rolled_back);
    assert!(
        stray_staging(root.path()).is_empty(),
        "{:?}",
        stray_staging(root.path())
    );
}

#[test]
fn a_cancel_in_the_middle_of_the_merge_restores_the_users_file() {
    // 合并态（目标目录已有内容）：提交是逐文件 rename + journal。取消落在两条搬动之间 ⇒
    // **逆序回滚**必须把用户被覆盖的文件改回来，且不留备份、不留暂存目录。
    let (root, pack) = three_files("cancel-merge");
    let target = root.join("instance");
    write_file(&target.join("mods/a.jar"), b"user a");
    write_file(&target.join("mods/user.jar"), b"user only");
    let token = ImportCancel::new();

    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target)
                .with_cancel(&token)
                // 覆盖档：让 a.jar 真的走"备份 + 覆盖"那条路，取消后必须恢复用户内容。
                .with_conflict_policy(super::ConflictPolicy::Overwrite)
                .with_progress(cancel_on_second_merged_file(&token)),
    ));

    assert_eq!(report.status, ImportStatus::Cancelled, "{:?}", report.error);
    assert!(report.error.is_none());
    assert_eq!(
        std::fs::read(target.join("mods/a.jar")).expect("用户文件要还在"),
        b"user a",
        "取消后必须恢复被覆盖的用户文件（回滚被删掉就会读到包内字节）"
    );
    assert_eq!(
        std::fs::read(target.join("mods/user.jar")).expect("用户文件要还在"),
        b"user only"
    );
    assert!(!target.join("mods/b.jar").exists(), "取消后不许有半包");
    assert!(!target.join("mods/c.jar").exists(), "取消后不许有半包");
    assert_eq!(
        files_under(&target),
        vec!["mods/a.jar".to_string(), "mods/user.jar".to_string()],
        "目标目录必须与导入前**逐文件相同**（多出基线 / 少掉用户文件都是错）"
    );
    assert!(
        std::fs::read_dir(target.join("mods"))
            .expect("读 mods")
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains(".ullbak-")),
        "备份必须被清掉，不许在实例里留 .ullbak-*"
    );
    assert!(
        stray_staging(root.path()).is_empty(),
        "{:?}",
        stray_staging(root.path())
    );
    // 回滚做过，且完整 ⇒ 机器位与文案位都说"与导入前一致"。
    assert!(report.rolled_back, "{:?}", report.rollback);
    assert_eq!(
        report
            .rollback
            .as_ref()
            .map(super::stage::RollbackOutcome::failure_count),
        Some(0)
    );
}

#[test]
fn a_cancel_right_before_registering_rolls_the_whole_instance_back() {
    // 登记前的检查点（**取消后不得登记实例**）：取消在"内容已落盘、档案还没建"的窗口里落下
    // ⇒ 连内容一起回滚。删掉这条检查，实例就会被登记上（本用例立刻红）。
    let (root, pack) = three_files("cancel-register");
    let target = root.join("instance");
    let home = root.join("data");
    let layout = crate::platform::layout::LauncherLayout::new(&home);
    let token = ImportCancel::new();

    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target)
            .with_cancel(&token)
            .with_registration(&layout, Some("cancelled pack"))
            .with_progress(cancel_on(&token, ImportStage::Register)),
    ));

    assert_eq!(report.status, ImportStatus::Cancelled, "{:?}", report.error);
    assert!(report.error.is_none());
    assert!(report.registered.is_none(), "取消后不许登记实例");
    let profiles = crate::game::install::instance::read_profiles(&layout).expect("读档案");
    assert!(
        profiles.profiles.is_empty(),
        "profiles.json 里不许出现被取消的实例：{:?}",
        profiles.profiles
    );
    assert!(!target.exists(), "内容已提交也要连内容一起回滚");
    assert!(report.rolled_back);
    assert!(
        stray_staging(root.path()).is_empty(),
        "{:?}",
        stray_staging(root.path())
    );
}

#[test]
fn cancel_and_failure_are_two_different_result_types() {
    // 条款③：取消与真失败必须产生**不同的结果类型**（状态变体、错误位、退出码、文案 key、
    // JSON 的 status 全不同）。把两条路合并成同一个错误，这里就红。
    let (root, pack) = three_files("cancel-vs-fail");
    let token = ImportCancel::new();
    token.cancel();
    let cancelled = block_on(import_pack(
        &ImportRequest::new(&pack, &root.join("cancelled")).with_cancel(&token),
    ));

    // 真失败：清单坏掉的包（开包成功、读清单失败）。
    let broken = root.join("broken.zip");
    {
        let file = std::fs::File::create(&broken).expect("建 zip");
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("manifest.json", options).expect("写条目");
        writer.write_all(b"{\"manifestType\":").expect("写内容");
        writer.finish().expect("收尾");
    }
    let failed = block_on(import_pack(&ImportRequest::new(
        &broken,
        &root.join("failed"),
    )));

    assert_eq!(cancelled.status, ImportStatus::Cancelled);
    assert_eq!(failed.status, ImportStatus::Failed);
    assert!(cancelled.error.is_none(), "取消不是错误");
    assert!(failed.error.is_some(), "失败必须带错误");
    assert_ne!(
        cancelled.status.exit_code(),
        failed.status.exit_code(),
        "退出码必须能区分「用户取消」与「装坏了」"
    );
    assert_eq!(cancelled.status.exit_code(), 130);
    assert_eq!(failed.status.exit_code(), 1);
    assert_ne!(cancelled.status.message_key(), failed.status.message_key());
    let cancelled_json = serde_json::to_value(&cancelled).expect("序列化");
    let failed_json = serde_json::to_value(&failed).expect("序列化");
    assert_eq!(cancelled_json["status"], serde_json::json!("cancelled"));
    assert_eq!(failed_json["status"], serde_json::json!("failed"));
    assert!(
        cancelled_json["error"].is_null(),
        "取消的 JSON 里不许有 error：{}",
        cancelled_json["error"]
    );
}

mod report_shaping;
