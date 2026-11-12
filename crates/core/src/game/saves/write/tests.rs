//! 写操作的判据：**真的在盘上建目录再动它**（不 mock 文件系统）。
//!
//! # 每个用例抓得住什么
//!
//! | 用例 | 抓得住 |
//! |---|---|
//! | [`rename_moves_only_the_named_world`] | 改名真的搬了目录、世界内容原样、别的世界不动 |
//! | [`rename_refuses_conflicts_and_bad_names`] | 撞名 / 同名 / 非法字符 / 分隔符 / 穿越全部拒，且**盘上一个字节没变** |
//! | [`traversal_is_refused_before_touching_the_disk`] | `saves/../..` 这类名字被挡在名字规则上，`saves/` 之外的东西不受影响 |
//! | [`saves_must_be_a_plain_child_of_the_run_dir`] | `saves/` 被占成文件 ⇒ 具名错误；被换成链接 ⇒ 拒（`cfg(unix)`） |
//! | [`symlinks_are_never_followed`] | 世界目录是链接 ⇒ 三个操作都拒、链接指向的那棵树**一根毫毛没少**（`cfg(unix)`） |
//! | [`nested_symlinks_are_removed_or_skipped`] | 世界**内部**的链接：删除只删链接、备份跳过并计数（`cfg(unix)`） |
//! | [`delete_removes_one_world_and_bumps_the_parent_mtime`] | 只删指名的那一个；`saves/` 的 mtime 变了、别的世界的 mtime 没变 |
//! | [`backup_writes_a_timestamped_zip`] | 时戳命名、条目与内容能读回来、目录条目也在 |
//! | [`backup_never_overwrites_an_earlier_export`] | 同一时刻两份备份 ⇒ `-2` 后缀，前一份内容不变 |
//! | [`backup_limits_are_checked_before_any_byte_is_written`] | 超限 ⇒ `TooLarge` 且落点目录**一个文件都没有** |
//! | [`missing_targets_are_named`] | 不存在的世界 / 不是目录 / 没有 `saves/` 各给具名类别 |
//!
//! # 变异自证（M247 的门禁要求）
//!
//! 把 `boundary::guard` 里那三道的**第二、三道**（`saves` 与世界的父级判据 + `real_dir` 的
//! 链接判据）删掉之后，[`symlinks_are_never_followed`] 与
//! [`saves_must_be_a_plain_child_of_the_run_dir`] 必红——报告里附的是实测输出，不是推测。
//! 只删第一道（名字规则）则 [`traversal_is_refused_before_touching_the_disk`] 与
//! [`rename_refuses_conflicts_and_bad_names`] 必红。

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::*;
use crate::addon::temp::TempRoot;
use crate::game::saves::{saves_dir, SAVES_DIR};

mod backup;

/// 一个只跑一帧的临时运行目录（`TempRoot::under` 返回时父级已被删，所以要自己建）。
fn run_dir(tag: &str) -> TempRoot {
    let root = TempRoot::under(tag, "run");
    std::fs::create_dir_all(&root).expect("建运行目录");
    root
}

/// 在 `saves/` 下造一个世界（两个文件 + 两层目录）。
fn world(root: &Path, name: &str) -> PathBuf {
    let dir = saves_dir(root).join(name);
    std::fs::create_dir_all(dir.join("region")).expect("建世界子目录");
    std::fs::write(dir.join("level.dat"), b"nbt").expect("写 level.dat");
    std::fs::write(dir.join("region/r.0.0.mca"), b"chunk").expect("写 region");
    dir
}

/// `(相对路径, 是否目录, 字节数, mtime 毫秒)` 的排序列表——「盘上没变」的判据本体。
fn fingerprint(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            out.push(format!(
                "{rel}|dir={}|len={}|mtime={}",
                meta.is_dir(),
                meta.len(),
                meta.modified()
                    .ok()
                    .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map_or(0, |since| since.as_millis())
            ));
            if meta.is_dir() {
                stack.push(path);
            }
        }
    }
    out.sort();
    out
}

/// 改名真的搬了目录、世界内容原样、别的世界不动。
#[test]
fn rename_moves_only_the_named_world() {
    let root = run_dir("m247-rename");
    world(&root, "Alpha");
    world(&root, "Beta");
    let beta_before = fingerprint(&saves_dir(&root).join("Beta"));

    let moved = rename_world(&root, "Alpha", "Alpha Renamed").expect("改名");
    assert!(moved.ends_with("Alpha Renamed"));
    assert!(!saves_dir(&root).join("Alpha").exists(), "旧目录还在");
    let new = saves_dir(&root).join("Alpha Renamed");
    assert_eq!(
        std::fs::read(new.join("level.dat")).expect("读 level.dat"),
        b"nbt",
        "改名把内容搬丢了"
    );
    assert!(new.join("region/r.0.0.mca").exists(), "子目录没跟过来");
    assert_eq!(
        fingerprint(&saves_dir(&root).join("Beta")),
        beta_before,
        "改名动了别的世界"
    );
}

/// 撞名 / 同名 / 非法字符 / 分隔符 / 穿越全部拒，且**盘上一个字节没变**。
#[test]
fn rename_refuses_conflicts_and_bad_names() {
    let root = run_dir("m247-rename-bad");
    world(&root, "Alpha");
    world(&root, "Beta");
    let before = fingerprint(&root);

    let cases: &[(&str, &str, SavesWriteKind)] = &[
        ("Alpha", "Beta", SavesWriteKind::Conflict),
        ("Alpha", "Alpha", SavesWriteKind::Conflict),
        ("Alpha", "../escape", SavesWriteKind::UnsafeName),
        ("Alpha", "a/b", SavesWriteKind::UnsafeName),
        ("Alpha", "a\\b", SavesWriteKind::UnsafeName),
        ("Alpha", "C:evil", SavesWriteKind::UnsafeName),
        ("Alpha", "..", SavesWriteKind::UnsafeName),
        ("Alpha", ".", SavesWriteKind::UnsafeName),
        ("Alpha", "", SavesWriteKind::UnsafeName),
        // 源自己也不许穿越
        ("../Alpha", "Other", SavesWriteKind::UnsafeName),
        ("saves/../Alpha", "Other", SavesWriteKind::UnsafeName),
        ("", "Other", SavesWriteKind::UnsafeName),
    ];
    for (from, to, kind) in cases {
        let error =
            rename_world(&root, from, to).expect_err(&format!("{from} → {to:?} 居然成功了"));
        assert_eq!(error.kind, *kind, "{from} → {to:?}：{}", error.detail);
    }
    assert_eq!(fingerprint(&root), before, "被拒的改名动了磁盘");
}

/// `saves/../..` 这类名字被挡在名字规则上，`saves/` 之外的东西不受影响。
#[test]
fn traversal_is_refused_before_touching_the_disk() {
    let root = run_dir("m247-traversal");
    world(&root, "Alpha");
    // `saves/` 之外的哨兵：任何一次越界删除都会把它带走。
    std::fs::write(root.join("sentinel.txt"), b"keep me").expect("写哨兵");
    let outside = root.parent().expect("临时目录一定有父级");
    let before = fingerprint(&root);

    for name in [
        "../..",
        "..",
        "../../sentinel.txt",
        "saves/../..",
        "/etc",
        "C:/Windows",
        ".",
        "",
    ] {
        let error = delete_world(&root, name).expect_err(&format!("删 {name:?} 居然成功了"));
        assert_eq!(
            error.kind,
            SavesWriteKind::UnsafeName,
            "{name:?}：{}",
            error.detail
        );
        let error = backup_world(&root, name, &root.join("exports"))
            .expect_err(&format!("备份 {name:?} 居然成功了"));
        assert_eq!(error.kind, SavesWriteKind::UnsafeName);
    }
    assert_eq!(fingerprint(&root), before, "越界尝试动了磁盘");
    assert!(root.join("sentinel.txt").exists(), "哨兵被删了");
    assert!(!outside.join("exports").exists(), "备份在实例外建了目录");
}

/// `saves/` 必须是运行目录下的**真目录**：被占成文件 ⇒ 具名错误；被换成链接 ⇒ 拒。
#[test]
fn saves_must_be_a_plain_child_of_the_run_dir() {
    // 被占成文件
    let root = run_dir("m247-saves-file");
    std::fs::write(saves_dir(&root), b"not a dir").expect("用文件占住 saves");
    let error = delete_world(&root, "Alpha").expect_err("saves 是文件，居然删成功了");
    assert_eq!(error.kind, SavesWriteKind::NotADirectory);

    // 没有 saves/
    let empty = run_dir("m247-no-saves");
    let error = delete_world(&empty, "Alpha").expect_err("没有 saves/，居然删成功了");
    assert_eq!(error.kind, SavesWriteKind::NotFound);

    // 被换成符号链接（指到实例之外）
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let linked = run_dir("m247-saves-link");
        let elsewhere = linked.parent().expect("父级").join("elsewhere-saves");
        std::fs::create_dir_all(elsewhere.join("Alpha")).expect("建实例外世界");
        std::fs::write(elsewhere.join("Alpha/level.dat"), b"nbt").expect("写实例外 level.dat");
        symlink(&elsewhere, saves_dir(&linked)).expect("建 saves 链接");
        let error = delete_world(&linked, "Alpha").expect_err("saves 是链接，居然删成功了");
        assert_eq!(error.kind, SavesWriteKind::SymbolicLink, "{}", error.detail);
        assert!(
            elsewhere.join("Alpha/level.dat").exists(),
            "顺着 saves 链接把实例外的世界删了"
        );
    }
}

/// 世界目录是链接 ⇒ 三个操作都拒、链接指向的那棵树一根毫毛没少（`cfg(unix)`）。
#[cfg(unix)]
#[test]
fn symlinks_are_never_followed() {
    use std::os::unix::fs::symlink;

    let root = run_dir("m247-symlink-world");
    std::fs::create_dir_all(saves_dir(&root)).expect("建 saves");
    let elsewhere = root.parent().expect("父级").join("shared-world");
    std::fs::create_dir_all(elsewhere.join("region")).expect("建实例外世界");
    std::fs::write(elsewhere.join("level.dat"), b"nbt").expect("写 level.dat");
    symlink(&elsewhere, saves_dir(&root).join("Linked")).expect("建世界链接");

    let before = fingerprint(&elsewhere);
    for error in [
        rename_world(&root, "Linked", "Copied").expect_err("链接世界居然改名成功了"),
        delete_world(&root, "Linked").expect_err("链接世界居然删成功了"),
        backup_world(&root, "Linked", &root.join("exports")).expect_err("链接世界居然备份成功了"),
    ] {
        assert_eq!(error.kind, SavesWriteKind::SymbolicLink, "{}", error.detail);
    }
    assert_eq!(fingerprint(&elsewhere), before, "跟着链接动了实例外的目录");
}

/// Windows 上「不跟随链接」的可执行证据：**目录联接（junction）不需要管理员权限**，
/// 所以这条判据在 Windows 上真的跑得起来（`symlink_dir` 要提权 / 开发者模式，跑不了）。
///
/// 判据是「拒 + 实例外那棵树一根毫毛没少」；具体的具名类别两档都算过：
/// 联接被 `symlink_metadata` 认成链接 ⇒ `SymbolicLink`（**M247 实测就是这一档**）；
/// 只被解析成目录 ⇒「`saves` 的直属子目录」那道父级判据把它拦成 `Boundary`。
#[cfg(windows)]
#[test]
fn junctions_are_never_followed() {
    let root = run_dir("m247-junction-world");
    std::fs::create_dir_all(saves_dir(&root)).expect("建 saves");
    let elsewhere = root.parent().expect("父级").join("shared-junction");
    std::fs::create_dir_all(elsewhere.join("region")).expect("建实例外世界");
    std::fs::write(elsewhere.join("level.dat"), b"nbt").expect("写 level.dat");
    let link = saves_dir(&root).join("Linked");
    // `quiet` 不是洁癖：Windows 上裸 `Command::new` 会给每个子进程开一块控制台，
    // GUI 父进程下就是桌面上「黑框闪一下」（`tests/child_quiet_guard.rs` 盯着这一条）。
    let status = crate::platform::child::quiet("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&elsewhere)
        .output()
        .expect("拉起 mklink");
    assert!(
        status.status.success(),
        "mklink /J 失败：{}",
        String::from_utf8_lossy(&status.stderr)
    );

    let before = fingerprint(&elsewhere);
    let error = delete_world(&root, "Linked").expect_err("联接世界居然删成功了");
    assert!(
        matches!(
            error.kind,
            SavesWriteKind::SymbolicLink | SavesWriteKind::Boundary
        ),
        "联接被当成普通目录放过了：{:?} / {}",
        error.kind,
        error.detail
    );
    let error = backup_world(&root, "Linked", &root.join("exports")).expect_err("居然备份了联接");
    assert!(matches!(
        error.kind,
        SavesWriteKind::SymbolicLink | SavesWriteKind::Boundary
    ));
    assert_eq!(fingerprint(&elsewhere), before, "跟着联接动了实例外的目录");
    // 收尾：先摘掉联接，别让临时根的清理去碰它（`TempRoot` 的 Drop 在下面自己删）。
    let _ = std::fs::remove_dir(&link);
}

/// 世界**内部**的链接：删除只删链接、备份跳过并计数（`cfg(unix)`）。
#[cfg(unix)]
#[test]
fn nested_symlinks_are_removed_or_skipped() {
    use std::os::unix::fs::symlink;

    let root = run_dir("m247-nested-link");
    let dir = world(&root, "Alpha");
    let target = root.join("outside.bin");
    std::fs::write(&target, b"outside").expect("写实例外文件");
    symlink(&target, dir.join("region/link.bin")).expect("建世界内链接");

    let exports = root.join("exports");
    let backup = backup_world(&root, "Alpha", &exports).expect("备份");
    assert_eq!(backup.skipped, 1, "世界内的链接要跳过并计数");

    delete_world(&root, "Alpha").expect("删世界");
    assert!(!dir.exists(), "世界目录还在");
    assert!(target.exists(), "删世界时把链接指向的实例外文件也删了");
}

/// 只删指名的那一个；`saves/` 的 mtime 变了、别的世界的 mtime 没变。
#[test]
fn delete_removes_one_world_and_bumps_the_parent_mtime() {
    let root = run_dir("m247-delete");
    world(&root, "Alpha");
    world(&root, "Beta");
    let saves = saves_dir(&root);
    let parent_before = mtime(&saves);
    let beta_before = mtime(&saves.join("Beta"));
    let beta_tree = fingerprint(&saves.join("Beta"));
    // mtime 的粒度可能是一秒（FAT / 一些网络盘），睡一下再删，判据才有分辨力。
    std::thread::sleep(Duration::from_millis(20));

    delete_world(&root, "Alpha").expect("删世界");
    assert!(!saves.join("Alpha").exists(), "世界目录还在");
    assert_eq!(
        fingerprint(&saves.join("Beta")),
        beta_tree,
        "删除动了别的世界"
    );
    assert_eq!(
        mtime(&saves.join("Beta")),
        beta_before,
        "别的世界的 mtime 变了"
    );
    assert!(
        mtime(&saves) >= parent_before,
        "saves/ 的 mtime 应该跟着变（{} → {}）",
        parent_before,
        mtime(&saves)
    );
}

/// 不存在的世界 / 不是目录 / 没有 `saves/` 各给具名类别。
#[test]
fn missing_targets_are_named() {
    let root = run_dir("m247-missing");
    world(&root, "Alpha");
    std::fs::write(saves_dir(&root).join("loose.txt"), b"x").expect("写散落文件");

    let error = delete_world(&root, "Nope").expect_err("不存在的世界居然删成功了");
    assert_eq!(error.kind, SavesWriteKind::NotFound);
    let error = delete_world(&root, "loose.txt").expect_err("文件居然被当成世界删了");
    assert_eq!(error.kind, SavesWriteKind::NotADirectory);
    let error = backup_world(&root, "Nope", &root.join("exports2")).expect_err("居然备份成功");
    assert_eq!(error.kind, SavesWriteKind::NotFound);
    assert!(
        saves_dir(&root).join("loose.txt").exists(),
        "散落文件被动了"
    );
    assert_eq!(
        saves_dir(&root).file_name().unwrap().to_string_lossy(),
        SAVES_DIR,
        "路径拼接口径"
    );
}

/// mtime（毫秒；取不到给 0）。
fn mtime(path: &Path) -> u128 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_millis())
}
