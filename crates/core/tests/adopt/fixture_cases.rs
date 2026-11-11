//! 快照工具自身的回归：它必须抓得住「凭空多出一个空目录」（复审 r1 的 R5）。
//!
//! 早先的快照只记文件：`scan` 或 `--dry-run` 若在外部树里建了一个空目录（例如
//! `<game>/versions/<v>/mods`），快照完全不变，于是「零写入」这条承诺可以是假绿。
//! 这里用**同一份** `snapshot.rs` 证明：目录节点、链接节点都进快照，读盘失败不吞。

use std::path::Path;

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
/// 抓得住：目录节点只记路径不记 mtime（那样 `Node::Dir{mtime_ns}` 相同，第二条断言红）。
/// 抓不住：某些文件系统对 mtime 的粗粒度（本机 NTFS 是 100ns 级；真粗到同一 tick 时这条会软过）。
#[test]
fn snapshot_notices_a_directory_mtime_change() {
    let root = TempRoot::new("snap-dirmtime");
    let dir = root.path().join("watched");
    std::fs::create_dir_all(&dir).expect("建目录");
    let before = snapshot(root.path());
    std::fs::write(dir.join("new.txt"), b"x").expect("写文件");
    let after = snapshot(root.path());
    let before_node = before.get("watched").expect("有该目录节点");
    let after_node = after.get("watched").expect("有该目录节点");
    assert_ne!(
        before_node, after_node,
        "目录里加了文件，目录节点（含 mtime）必须变"
    );
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
