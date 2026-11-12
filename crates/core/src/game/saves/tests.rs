//! 只读判据的用例：**真的建盘上目录**再读回来（不 mock 文件系统）。
//!
//! 每条用例都写在「抓得住什么 / 抓不住什么」的口径里（M224 的写法）：
//!
//! | 用例 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`scan_lists_worlds_and_screenshots_read_only`] | 收纳口径、排序、`icon.png` 只判存在、**调用前后盘上指纹不变** | 大世界的耗时（不测性能） |
//! | [`missing_dirs_are_empty_and_not_created`] | 「不存在 ⇒ 空列表」且**不建目录** | 权限不足导致的 `NotFound`（本机跑不出来） |
//! | [`unusable_dirs_are_errors`] | 目录被占成文件 ⇒ 具名错误 | Windows 上建符号链接要开发者模式，见下一条 |
//! | [`symlinks_are_refused`] | 符号链接不被跟随（目录级与条目级） | 非 unix 平台（`cfg(unix)`；Windows 建链接要提权） |

use std::path::Path;

use super::*;
use crate::addon::temp::TempRoot;

/// 建一个运行目录（`<tmp>/run`）。
///
/// `create_dir_all` 这一句不是冗余：`addon::temp::TempRoot::under` 返回时它内部的 `base`
/// 已经析构、把那棵树删掉了（M235 已作为 finding 上报，`addon/` 不在本 mission 的改动范围）。
/// 修好之后这一句会退化成 no-op。
fn run_dir(tag: &str) -> TempRoot {
    let root = TempRoot::under(tag, "run");
    std::fs::create_dir_all(&root).expect("建运行目录");
    root
}

/// 盘上指纹：`(相对路径, 是否目录, 字节数, mtime)` 逐条排序后的列表。
///
/// 这就是「调用前后版本树快照不变」的判据本体——只要列举路径里多了一次 `create_dir`、
/// 一次 `write`、一次 `remove`，或者顺手动了 mtime，这个列表就会不同。
fn fingerprint(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = read.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |since| since.as_millis());
            out.push(format!(
                "{rel}|dir={}|len={}|mtime={mtime}",
                meta.is_dir(),
                meta.len()
            ));
            if meta.is_dir() {
                stack.push(path);
            }
        }
    }
    out.sort();
    out
}

/// 收纳口径 + 排序 + `icon.png` + **只读**（M235 的核心判据）。
#[test]
fn scan_lists_worlds_and_screenshots_read_only() {
    let root = run_dir("m235-scan");
    let saves = saves_dir(&root);
    std::fs::create_dir_all(saves.join("World A")).expect("建世界 A");
    std::fs::write(saves.join("World A/level.dat"), b"nbt").expect("写 level.dat");
    std::fs::write(saves.join("World A/icon.png"), b"png").expect("写 icon");
    std::fs::create_dir_all(saves.join("world b")).expect("建世界 B");
    // `saves/` 下的散落文件：不是世界，登记原因。
    std::fs::write(saves.join("readme.txt"), b"hi").expect("写散落文件");

    let shots = screenshots_dir(&root);
    std::fs::create_dir_all(&shots).expect("建截图目录");
    std::fs::write(shots.join("2024-05-01_10.00.00.png"), b"aa").expect("写截图 1");
    std::fs::write(shots.join("2024-06-02_11.00.00.PNG"), b"bbb").expect("写截图 2");
    std::fs::write(shots.join("notes.txt"), b"x").expect("写非图片");

    let before = fingerprint(&root);
    let scan = scan(&root).expect("列存档");
    let after = fingerprint(&root);
    assert_eq!(before, after, "列举改动了磁盘：{before:?} → {after:?}");

    assert!(scan.exists);
    assert!(scan.screenshots_exists);
    assert_eq!(scan.dir, save_path(&root));
    assert_eq!(scan.screenshots_dir, shot_path(&root));

    // 世界：名字排序大小写不敏感；`icon.png` 只判存在。
    let worlds: Vec<(&str, bool)> = scan
        .worlds
        .iter()
        .map(|world| (world.dir.as_str(), world.icon))
        .collect();
    assert_eq!(worlds, [("World A", true), ("world b", false)]);
    assert!(
        scan.worlds.iter().all(|world| world.modified_ms.is_some()),
        "世界的 mtime 应当取得到：{:?}",
        scan.worlds
    );

    // 截图：新的在前（文件名倒序），扩展名大小写不敏感，非图片登记原因。
    let shots_listed: Vec<&str> = scan
        .screenshots
        .iter()
        .map(|shot| shot.file.as_str())
        .collect();
    assert_eq!(
        shots_listed,
        ["2024-06-02_11.00.00.PNG", "2024-05-01_10.00.00.png"]
    );
    assert_eq!(scan.screenshots[0].bytes, 3, "字节数来自 metadata");
    assert_eq!(scan.screenshots[1].bytes, 2);

    let skipped: Vec<(Area, &str, SkipKind)> = scan
        .skipped
        .iter()
        .map(|skip| (skip.area, skip.file.as_str(), skip.reason))
        .collect();
    assert_eq!(
        skipped,
        [
            (Area::Saves, "readme.txt", SkipKind::NotADirectoryExpected),
            (Area::Screenshots, "notes.txt", SkipKind::NotAnImage),
        ],
        "跳过必须逐条登记原因（不静默丢）"
    );
}

/// 「目录不存在 ⇒ 空列表」**且不替玩家建目录**。
#[test]
fn missing_dirs_are_empty_and_not_created() {
    let root = run_dir("m235-missing");
    let before = fingerprint(&root);

    let scan = scan(&root).expect("两个目录都不存在不是错误");
    assert!(!scan.exists);
    assert!(!scan.screenshots_exists);
    assert!(scan.worlds.is_empty());
    assert!(scan.screenshots.is_empty());
    assert!(scan.skipped.is_empty());
    assert!(scan.is_empty());

    assert_eq!(fingerprint(&root), before, "列举顺手建了目录");
    assert!(!saves_dir(&root).exists(), "`saves/` 被建出来了");
    assert!(
        !screenshots_dir(&root).exists(),
        "`screenshots/` 被建出来了"
    );
    // 路径本身照样是"拼得出来的"（UI 的「打开文件夹」需要它）。
    assert_eq!(scan.dir, saves_dir(&root));
    assert_eq!(scan.screenshots_dir, screenshots_dir(&root));
}

/// 「存在但不是目录」⇒ 具名错误（不折叠成空列表：那不是"没存档"，是"读不了"）。
#[test]
fn unusable_dirs_are_errors() {
    let root = run_dir("m235-unusable");
    std::fs::write(saves_dir(&root), b"not a dir").expect("用文件占住 saves 这个名字");

    let error = scan(&root).expect_err("saves 是个文件，居然列成功了");
    assert_eq!(error.dir, saves_dir(&root));
    assert!(error.detail.contains("不是目录"), "原因：{}", error.detail);

    // `screenshots/` 那一侧单独判（saves 正常、shot 被占）。
    let other = run_dir("m235-unusable-shots");
    std::fs::create_dir_all(saves_dir(&other)).expect("建 saves");
    std::fs::write(screenshots_dir(&other), b"x").expect("用文件占住 screenshots");
    let error = scan(&other).expect_err("screenshots 是个文件，居然列成功了");
    assert_eq!(error.dir, screenshots_dir(&other));
}

/// 符号链接不跟随（目录级 = 错误；条目级 = 跳过并登记原因）。
#[cfg(unix)]
#[test]
fn symlinks_are_refused() {
    use std::os::unix::fs::symlink;

    // 目录级：`saves/` 指到别处 ⇒ 拒（列下去会把实例外的东西算成存档）。
    let root = run_dir("m235-symlink-dir");
    let elsewhere = root.join("elsewhere");
    std::fs::create_dir_all(elsewhere.join("Outside")).expect("建实例外目录");
    symlink(&elsewhere, saves_dir(&root)).expect("建符号链接");
    let error = scan(&root).expect_err("符号链接的 saves 居然列成功了");
    assert!(error.detail.contains("符号链接"), "原因：{}", error.detail);

    // 条目级：世界目录 / 截图文件各一个链接 ⇒ 跳过，不跟随。
    let root = run_dir("m235-symlink-entry");
    let saves = saves_dir(&root);
    std::fs::create_dir_all(&saves).expect("建 saves");
    std::fs::create_dir_all(elsewhere.join("Real")).expect("建真实世界");
    symlink(elsewhere.join("Real"), saves.join("Linked")).expect("建世界链接");
    let shots = screenshots_dir(&root);
    std::fs::create_dir_all(&shots).expect("建 screenshots");
    std::fs::write(elsewhere.join("shot.png"), b"png").expect("写真实截图");
    symlink(elsewhere.join("shot.png"), shots.join("linked.png")).expect("建截图链接");

    let scan = scan(&root).expect("列存档");
    assert!(scan.worlds.is_empty(), "跟随了世界符号链接");
    assert!(scan.screenshots.is_empty(), "跟随了截图符号链接");
    let reasons: Vec<(Area, SkipKind)> = scan
        .skipped
        .iter()
        .map(|skip| (skip.area, skip.reason))
        .collect();
    assert_eq!(
        reasons,
        [
            (Area::Saves, SkipKind::SymbolicLink),
            (Area::Screenshots, SkipKind::SymbolicLink),
        ]
    );
}

/// 名字过不了路径规则的条目：跳过并登记（不静默丢）。
///
/// `:` 在 unix 上是合法文件名字符，在 Windows 上建不出来——所以这条只在 unix 跑得起来。
#[cfg(unix)]
#[test]
fn unsafe_names_are_skipped() {
    let root = run_dir("m235-unsafe");
    let scenes = saves_dir(&root);
    std::fs::create_dir_all(&scenes).expect("建 saves");
    std::fs::create_dir_all(scenes.join("bad:name")).expect("建带冒号的世界");
    std::fs::create_dir_all(scenes.join("good")).expect("建正常世界");

    let scan = scan(&root).expect("列存档");
    let worlds: Vec<&str> = scan.worlds.iter().map(|world| world.dir.as_str()).collect();
    assert_eq!(worlds, ["good"]);
    assert_eq!(scan.skipped.len(), 1);
    assert_eq!(scan.skipped[0].file, "bad:name");
    assert_eq!(scan.skipped[0].reason, SkipKind::UnsafeName);
}

/// 路径拼接口径：两个目录都挂在**运行目录**下面（不是游戏根、不是 `.minecraft`）。
#[test]
fn paths_hang_off_the_run_dir() {
    let run = Path::new("D:/ULL/.ULL/data/instances/better-mc");
    assert_eq!(saves_dir(run).file_name().unwrap(), SAVES_DIR);
    assert_eq!(screenshots_dir(run).file_name().unwrap(), SCREENSHOTS_DIR);
    assert!(saves_dir(run).starts_with(run));
    assert!(screenshots_dir(run).starts_with(run));
}

fn save_path(root: &Path) -> std::path::PathBuf {
    root.join(SAVES_DIR)
}

fn shot_path(root: &Path) -> std::path::PathBuf {
    root.join(SCREENSHOTS_DIR)
}
