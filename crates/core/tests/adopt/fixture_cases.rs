//! 快照工具自身的回归：它必须抓得住「凭空多出一个空目录」（复审 r1 的 R5）。
//!
//! 早先的快照只记文件：`scan` 或 `--dry-run` 若在外部树里建了一个空目录（例如
//! `<game>/versions/<v>/mods`），快照完全不变，于是「零写入」这条承诺可以是假绿。
//! 这里用**同一份** `snapshot.rs` 证明：目录节点、链接节点都进快照，读盘失败不吞。

use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::fixture::{
    link_guard, snapshot, snapshot_opt, try_link_dir, try_symlink_dir, Node, TempRoot,
};

/// 目录 / 文件 / 链接三类节点都记，且链接**不跟随**（目标内容不会挂到链接路径下）。
///
/// 抓得住：
/// - 目录节点缺失（`dir` 断言红）；
/// - 跟随链接（`link/inside.txt` 会同时出现在链接路径下，`is_none` 断言红）；
/// - 链接被当成普通目录（`Node::Link` 断言红）。
///
/// 抓不住：链接目标本身的内容变化（不跟随就看不见——这正是本工具要的语义）。
#[test]
fn snapshot_records_dirs_files_and_links_without_following() {
    let root = TempRoot::new("snap-shapes");
    let real = root.path().join("real");
    std::fs::create_dir_all(&real).expect("建真实目录");
    std::fs::write(real.join("inside.txt"), b"x").expect("写文件");
    let link = root.path().join("link");
    if !link_guard(try_symlink_dir(&real, &link), "快照链接夹具") {
        // 链接建不出来时仍要验目录/文件两类（guard 已显式记录跳过原因）。
        let snap = snapshot(root.path());
        assert!(matches!(snap.get("real"), Some(Node::Dir { .. })));
        assert!(matches!(
            snap.get("real/inside.txt"),
            Some(Node::File { .. })
        ));
        return;
    }

    let snap = snapshot(root.path());
    assert!(
        matches!(snap.get("."), Some(Node::Dir { .. })),
        "根自身也要记"
    );
    assert!(matches!(snap.get("real"), Some(Node::Dir { .. })));
    assert!(matches!(
        snap.get("real/inside.txt"),
        Some(Node::File { .. })
    ));
    assert!(
        matches!(snap.get("link"), Some(Node::Link { .. })),
        "链接要记成 Link：{:?}",
        snap.get("link")
    );
    assert!(
        !snap.contains_key("link/inside.txt"),
        "不许跟随链接（否则根外/别处内容会被算进这棵树）"
    );
}

/// **新增一个空目录**必须让快照变化（R5 的核心判据）。
///
/// 抓得住：只记文件的旧快照（`before == after` 会让第一条断言红）。
/// 抓不住：目录 mtime 的精度（这条只比路径集合与节点形状；mtime 由 `assert_unchanged` 另外守）。
#[test]
fn snapshot_catches_a_new_empty_directory() {
    let root = TempRoot::new("snap-newdir");
    let game = root.game_root().join("versions").join("1.20.1");
    std::fs::create_dir_all(&game).expect("建版本目录");
    let before = snapshot(&root.game_root());

    // 「登记过程凭空建了一个空目录」——正是旧快照看不见的那种改动。
    let planted = game.join("mods");
    std::fs::create_dir_all(&planted).expect("建空目录");
    let after = snapshot(&root.game_root());

    assert_ne!(before, after, "新增空目录必须改变快照");
    assert!(
        !before.contains_key("versions/1.20.1/mods"),
        "改动前不该有这个节点"
    );
    assert!(
        matches!(after.get("versions/1.20.1/mods"), Some(Node::Dir { .. })),
        "新增的空目录要作为目录节点出现：{:?}",
        after.get("versions/1.20.1/mods")
    );

    // 反向：删掉之后路径集合回到原样（目录 mtime 会变，所以只比键集合）。
    std::fs::remove_dir(&planted).expect("删空目录");
    let again = snapshot(&root.game_root());
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        again.keys().collect::<Vec<_>>(),
        "删掉空目录后路径集合应回到原样"
    );
}

/// 目录的 mtime 变化也要能看见（条目增删会改父目录 mtime）。
///
/// ## 为什么不再「写个文件然后赌 mtime 变了」（M262）
///
/// 上一版只做「建目录 → 拍快照 → 写文件 → 再拍快照 → `assert_ne!(node)`」，靠的是**文件系统
/// 恰好把两次操作记成不同的 mtime**。这一条在 windows CI 上真红（写文件与建目录落在同一 tick），
/// 而它自己的注释还写着「真粗到同一 tick 时会软过」——既不可靠也不软过，是个真红。
///
/// 现在两个 mtime 都由本用例**显式给定**（[`set_dir_mtime`]），不来自时钟：t0 = 1_000_000_000s，
/// t1 = t0 + 3600s，两者都是整秒、都对任何常见粒度（NTFS 100ns / ext4 1ns / HFS+ 1s / FAT 2s）
/// 无余数，于是「快照读到的目录 mtime」与「我们钉的那个值」可以逐 ns 比对。
///
/// 抓得住：
/// - `Node::Dir` 不再携带 mtime（记 0 / 记常量）—— 逐 ns 断言当场红；
/// - 快照不再重新读元数据（缓存 / 只记路径）—— 同上；
/// - 目录节点整个丢失（`expect` 红）。
///
/// 抓不住：
/// - **文件系统会不会自己在条目增删时抬高目录 mtime**：本用例把这件事绕开了（两次 mtime 都是我们
///   钉的），所以「NTFS 真机上目录 mtime 会不会被延时更新」这件事依然**没有**用例守——它有自己
///   的用例（`snapshot_catches_a_new_empty_directory` 比的是路径集合，不靠 mtime）；
/// - 粒度比 3600s 还粗的文件系统（常见 FS 里没有；真遇到会红在「逐 ns 相等」那句，不会软过）；
/// - 「目录里多了一个文件」这件事本身（那由 [`snapshot_catches_a_new_empty_directory`] 与
///   下面 `watched/new.txt` 那一条单独守，不靠 mtime）。
#[test]
fn snapshot_notices_a_directory_mtime_change() {
    let root = TempRoot::new("snap-dirmtime");
    let dir = root.path().join("watched");
    std::fs::create_dir_all(&dir).expect("建目录");

    // 两个明确的时刻（整秒，任何常见 filetime 粒度都能原样存下）。
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    let t1 = t0 + Duration::from_secs(3_600);

    set_dir_mtime(&dir, t0);
    let before = snapshot(root.path());

    // 场景照旧是真的：往目录里加了一个文件（条目增删是 mtime 变的**真实**来源）。
    std::fs::write(dir.join("new.txt"), b"x").expect("写文件");
    // 但差异由本用例给出，而不是「期望这次写恰好落在另一个 tick」。
    set_dir_mtime(&dir, t1);

    let after = snapshot(root.path());

    // 场景本身要成立（否则下面比的是一个没变的世界）。
    assert!(
        !before.contains_key("watched/new.txt")
            && matches!(after.get("watched/new.txt"), Some(Node::File { .. })),
        "先把「目录里真多了一个文件」这条落实：before={:?} after={:?}",
        before.get("watched/new.txt"),
        after.get("watched/new.txt")
    );

    let before_node = before.get("watched").expect("有该目录节点");
    let after_node = after.get("watched").expect("有该目录节点");
    assert_ne!(
        before_node, after_node,
        "目录 mtime 变了，目录节点必须变（mtime 是我们钉的，不看时钟）"
    );
    assert_eq!(
        dir_mtime_ns(after_node),
        t1.duration_since(SystemTime::UNIX_EPOCH)
            .expect("t1 在纪元之后")
            .as_nanos(),
        "快照里的目录 mtime 必须就是我们钉的那个值（逐 ns）"
    );
    assert_eq!(
        dir_mtime_ns(before_node),
        t0.duration_since(SystemTime::UNIX_EPOCH)
            .expect("t0 在纪元之后")
            .as_nanos()
    );
}

/// 给**目录**显式钉一个 mtime（std-only；两个平台的句柄打开方式不同）。
#[cfg(windows)]
fn set_dir_mtime(dir: &Path, at: SystemTime) {
    use std::os::windows::fs::OpenOptionsExt;
    // 0x0100 = FILE_WRITE_ATTRIBUTES：`SetFileTime` 要的就是这个权限，而 GENERIC_WRITE
    // （`OpenOptions::write(true)`）在**目录**上会被拒（os error 5）。
    // 0x0200_0000 = FILE_FLAG_BACKUP_SEMANTICS：不开它 `CreateFile` 根本打不开目录句柄。
    let handle = std::fs::OpenOptions::new()
        .access_mode(0x0100)
        .custom_flags(0x0200_0000)
        .open(dir)
        .unwrap_or_else(|error| panic!("打不开目录句柄 {}：{error}", dir.display()));
    handle
        .set_modified(at)
        .unwrap_or_else(|error| panic!("给目录设 mtime 失败 {}：{error}", dir.display()));
}

/// 同 [`set_dir_mtime`]（`futimens` 走普通目录句柄，不需要额外权限）。
#[cfg(not(windows))]
fn set_dir_mtime(dir: &Path, at: SystemTime) {
    let handle = std::fs::File::open(dir)
        .unwrap_or_else(|error| panic!("打不开目录句柄 {}：{error}", dir.display()));
    handle
        .set_modified(at)
        .unwrap_or_else(|error| panic!("给目录设 mtime 失败 {}：{error}", dir.display()));
}

/// 目录节点的 mtime（纳秒）。节点不是目录就直接红：本用例只谈目录。
fn dir_mtime_ns(node: &Node) -> u128 {
    match node {
        Node::Dir { mtime_ns } => *mtime_ns,
        other => panic!("watched 应当是目录节点：{other:?}"),
    }
}

/// 目标根本不存在 ⇒ `snapshot_opt` 给 `None`（与「空树」区分开）。
///
/// 抓得住：把「不存在」当成空快照（那样「dry-run 建了 .ULL」就看不出来）。
/// 抓不住：存在但权限不足（那一支直接 panic，见模块文档）。
#[test]
fn a_missing_target_is_none_not_an_empty_snapshot() {
    let root = TempRoot::new("snap-missing");
    let absent: &Path = &root.path().join("nope");
    assert!(snapshot_opt(absent).is_none());
    std::fs::create_dir_all(absent).expect("建空目录");
    assert!(snapshot_opt(absent).is_some(), "空目录也算存在");
}

/// 链接建不出来时 `try_link_dir` 会退回联接（Windows）；这条只验它不 panic。
#[test]
fn link_fixtures_are_total() {
    let root = TempRoot::new("snap-link-total");
    let target = root.path().join("target");
    std::fs::create_dir_all(&target).expect("建目标");
    let link = root.path().join("link");
    let created = try_link_dir(&target, &link);
    if created {
        assert!(link.is_dir());
    }
}
