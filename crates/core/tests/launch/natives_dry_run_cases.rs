//! `--dry-run` 的零落盘契约（M231 / DRYRUNPURE）：预览只读版本树，不建目录、不改 mtime。
//!
//! 判据是「版本树的节点类型 + mtime 快照逐字相等」而不是「natives 目录不存在」：
//! 后者抓不住 `create_dir_all` 之后立刻删掉的情形（父目录 mtime 已经变了），
//! 也抓不住「目录本来就在、但被重写/清空」的情形。

use std::path::{Path, PathBuf};
use std::time::Duration;

use unlucky_core::launch::natives::{self, NativeJar, NativesPlan};

use crate::prelude::*;

/// 版本树的「节点类型 + mtime（纳秒）+ 相对路径」快照，用来逐字比较 dry-run 前后。
///
/// 抓得住：新增/删除任何节点（含 `natives/` 目录本身）、任何已有节点被重写或 mtime 被
/// 更新；根节点（= 父目录 `versions/<id>/`）也在快照里，所以「建目录」会同时体现在
/// 节点集合与父目录 mtime 上。
/// 抓不住：①内容变了但 mtime 被回填成原值（本仓没有这种写法）；②**建了立刻删**这类
/// 瞬时写入只留父目录 mtime 一条线索，而 NTFS 会把该更新合并掉 —— 本仓实测：全量并行跑
/// 测试时这条偶发看不见（单跑又看得见），所以瞬时建删只剩「`natives` 目录不存在」兜底。
///
/// 时间戳口径：**精度 ≠ 可靠性**。NTFS / ext4 的 mtime 字段精度是 100ns 级，但「两个操作能不能
/// 分辨」取决于时间戳**何时被采样、以多大粒度更新**（同内容立即重写在 NTFS 上可能落在同一系统
/// 时钟 tick 内、句柄关闭时才按粗粒度落盘），所以快照只在「有人显式改过时间戳」时才是判据。
/// 判据自检见 `tree_snapshot_sees_new_nodes_and_mtime_changes`（防假绿）。
fn tree_snapshot(root: &Path) -> Vec<String> {
    fn line(meta: &std::fs::Metadata, relative: &str) -> String {
        let nanos = meta
            .modified()
            .expect("mtime")
            .duration_since(std::time::UNIX_EPOCH)
            .expect("epoch 之后")
            .as_nanos();
        let kind = if meta.is_dir() { "d" } else { "f" };
        format!("{kind} {nanos} {relative}")
    }
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .expect("读目录")
            .flatten()
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for path in entries {
            let meta = std::fs::symlink_metadata(&path).expect("元数据");
            let name = path
                .file_name()
                .expect("文件名")
                .to_string_lossy()
                .to_string();
            let relative = format!("{prefix}{name}");
            out.push(line(&meta, &relative));
            if meta.is_dir() {
                walk(&path, &format!("{relative}/"), out);
            }
        }
    }
    let mut out = vec![line(
        &std::fs::symlink_metadata(root).expect("根元数据"),
        "<root>",
    )];
    walk(root, "", &mut out);
    out
}

/// `--dry-run` 零落盘（有 jar）：只报「将解压几个」，版本树里一个节点都不新增、mtime 不动。
#[test]
fn dry_run_previews_natives_without_touching_disk() {
    let root = temp_dir("natives-dry-run-pure");
    let jar = root.join("libraries/lwjgl-natives-windows.jar");
    make_jar(&jar, &lwjgl_jar_entries());
    let version_dir = root.join("versions/1.16.5");
    std::fs::create_dir_all(&version_dir).expect("建版本目录");
    std::fs::write(version_dir.join("1.16.5.json"), b"{}").expect("放清单占位");
    let natives_dir = version_dir.join("natives");
    let before = tree_snapshot(&version_dir);

    let (mut request, _) = legacy_forge_request();
    request.dirs.natives_dir = natives_dir.clone();
    request.natives = NativesPlan::new(vec![NativeJar::new(&jar, "lwjgl")]);
    request.manifest.legacy_arguments = Some("--username ${auth_player_name}".to_string());
    let sessions = temp_sessions_root("natives-dry-run-pure-sessions");
    let launcher = Launcher::new(LauncherConfig::new(&sessions));

    let dry = launcher.dry_run(&request).expect("dry-run 应成功");
    assert_eq!(
        before,
        tree_snapshot(&version_dir),
        "dry-run 不许在版本树里加节点 / 改 mtime"
    );
    let report = dry.natives.expect("应有 natives 预览");
    assert!(report.planned, "预览报告要自报家门：{report:?}");
    assert!(!report.dir_exists, "natives 目录原本不在盘上：{report:?}");
    assert_eq!(report.files, 2, "要算出「将写下」的数量：{report:?}");
    assert_eq!(report.ignored, 4, "忽略口径要与真解压一致：{report:?}");
    assert!(!report.up_to_date, "{report:?}");
    assert!(
        report.summary().contains("未落盘"),
        "人读行得说清没写盘：{}",
        report.summary()
    );

    assert!(!natives_dir.exists(), "natives 目录不该被建出来");
}

/// `--dry-run` 零落盘（无 jar，1.19+ 常态）：不建 natives 目录，人读行说清「尚不存在」。
#[test]
fn dry_run_with_no_natives_jar_creates_no_directory() {
    let root = temp_dir("natives-dry-run-nojar");
    let version_dir = root.join("versions/1.20.1");
    std::fs::create_dir_all(&version_dir).expect("建版本目录");
    let natives_dir = version_dir.join("natives");
    let before = tree_snapshot(&version_dir);

    let (mut request, _) = legacy_forge_request();
    request.dirs.natives_dir = natives_dir.clone();
    request.natives = NativesPlan::default();
    request.manifest.legacy_arguments = Some("--username ${auth_player_name}".to_string());
    let sessions = temp_sessions_root("natives-dry-run-nojar-sessions");
    let launcher = Launcher::new(LauncherConfig::new(&sessions));

    let dry = launcher.dry_run(&request).expect("dry-run 应成功");
    assert_eq!(before, tree_snapshot(&version_dir), "无 jar 也不许建目录");
    let report = dry.natives.expect("应有 natives 预览");
    assert_eq!(report.jars, 0, "{report:?}");
    assert_eq!(report.files, 0, "{report:?}");
    assert!(!report.dir_exists, "盘上没有这个目录：{report:?}");
    assert!(report.up_to_date, "无 jar = 无需解压：{report:?}");
    assert!(
        report.summary().contains("尚不存在"),
        "{}",
        report.summary()
    );

    assert!(!natives_dir.exists(), "natives 目录不该被建出来");
}

/// `--dry-run` 零落盘（空 jar：只有 `META-INF/` 与噪音）：数得出 0 个原生库，仍不建目录。
#[test]
fn dry_run_with_empty_native_jar_creates_no_directory() {
    let root = temp_dir("natives-dry-run-emptyjar");
    let jar = root.join("natives-windows.jar");
    make_jar(
        &jar,
        &[
            ("META-INF/MANIFEST.MF", b"manifest".as_slice()),
            ("README.txt", b"noise".as_slice()),
        ],
    );
    let version_dir = root.join("versions/1.16.5");
    std::fs::create_dir_all(&version_dir).expect("建版本目录");
    let natives_dir = version_dir.join("natives");
    let before = tree_snapshot(&version_dir);

    let (mut request, _) = legacy_forge_request();
    request.dirs.natives_dir = natives_dir.clone();
    request.natives = NativesPlan::new(vec![NativeJar::new(&jar, "lwjgl")]);
    request.manifest.legacy_arguments = Some("--username ${auth_player_name}".to_string());
    let sessions = temp_sessions_root("natives-dry-run-emptyjar-sessions");
    let launcher = Launcher::new(LauncherConfig::new(&sessions));

    let dry = launcher.dry_run(&request).expect("dry-run 应成功");
    assert_eq!(before, tree_snapshot(&version_dir), "空 jar 也不许建目录");
    let report = dry.natives.expect("应有 natives 预览");
    assert_eq!(report.files, 0, "空 jar 解不出原生库：{report:?}");
    assert_eq!(report.ignored, 2, "两条噪音都要数进忽略：{report:?}");
    assert!(!report.up_to_date, "{report:?}");
    assert!(!report.dir_exists, "{report:?}");

    assert!(!natives_dir.exists(), "natives 目录不该被建出来");
}

/// 目录已经真解压过：预览读戳记报「已就绪」，同样零写盘（幂等路径也不许碰盘）。
#[test]
fn dry_run_after_real_extract_reports_ready_without_writing() {
    let root = temp_dir("natives-dry-run-ready");
    let jar = root.join("natives-windows.jar");
    make_jar(&jar, &[("lwjgl.dll", b"DLL".as_slice())]);
    let version_dir = root.join("versions/1.16.5");
    let natives_dir = version_dir.join("natives");
    std::fs::create_dir_all(&version_dir).expect("建版本目录");
    natives::extract(
        &natives_dir,
        &NativesPlan::new(vec![NativeJar::new(&jar, "lwjgl")]),
    )
    .expect("先真解压一次");
    let before = tree_snapshot(&version_dir);

    let (mut request, _) = legacy_forge_request();
    request.dirs.natives_dir = natives_dir.clone();
    request.natives = NativesPlan::new(vec![NativeJar::new(&jar, "lwjgl")]);
    request.manifest.legacy_arguments = Some("--username ${auth_player_name}".to_string());
    let sessions = temp_sessions_root("natives-dry-run-ready-sessions");
    let launcher = Launcher::new(LauncherConfig::new(&sessions));

    let dry = launcher.dry_run(&request).expect("dry-run 应成功");
    assert_eq!(
        before,
        tree_snapshot(&version_dir),
        "预览不该重写已有的原生库或戳记"
    );
    let report = dry.natives.expect("应有 natives 预览");
    assert!(report.planned, "{report:?}");
    assert!(report.dir_exists, "{report:?}");
    assert!(report.up_to_date, "戳记命中应报已就绪：{report:?}");
    assert_eq!(report.files, 0, "预览不该再数一遍：{report:?}");
    assert_eq!(report.skipped, 1, "{report:?}");
}

/// 判据自检（防假绿）：快照必须看得见「新增节点」与「已有节点的 mtime 变化」，
/// 否则上面几条「快照逐字相等」等于没断言。
///
/// 这里**不**用「建了立刻删」来探父目录 mtime：NTFS 会把瞬时建删对父目录 mtime 的影响
/// 合并掉（本仓实测：全量并行跑时偶发看不见、单跑看得见），拿它当门禁就是引入 flaky。
///
/// 时间维度也**不**靠「同内容立刻重写」：NTFS 上 mtime 不保证「每次写都变」——同一个系统
/// 时钟 tick 内的两次写、或时间戳只在句柄关闭时按粗粒度落盘，都会让重写后的 mtime 与写之前
/// **逐字节相同**。M275 的实际失败正是这一条：main 上 `test (windows-latest)` 偶发红在这行
/// `assert_ne!`（`crates\core\tests\launch\natives_dry_run_cases.rs:241`），本机永远复现不出来。
/// `sleep` + 重试轮询只是把概率压低（把「不可靠观察」换成「不可靠观察 + 更长的墙钟」），
/// 不是消除。确定性判据是**显式时间戳**：`File::set_modified` 把 mtime 直接写成
/// 「基线 + 5s」——值由我们指定，与时钟 tick / 落盘粒度无关；快照若看不见它，说明快照真的
/// 丢了时间维度，而不是运气不好。
#[test]
fn tree_snapshot_sees_new_nodes_and_mtime_changes() {
    let root = temp_dir("natives-snapshot-probe");
    let version_dir = root.join("versions/1.16.5");
    std::fs::create_dir_all(&version_dir).expect("建版本目录");
    let manifest = version_dir.join("1.16.5.json");
    std::fs::write(&manifest, b"{}").expect("放清单占位");
    let baseline = tree_snapshot(&version_dir);

    // 1) 结构维度：多一个目录节点（正是 `create_dir_all(natives)` 留下的痕迹）。
    std::fs::create_dir(version_dir.join("natives")).expect("建目录");
    let with_dir = tree_snapshot(&version_dir);
    assert!(
        with_dir.len() > baseline.len(),
        "快照必须看得见新目录：{baseline:?} vs {with_dir:?}"
    );
    assert_ne!(baseline, with_dir);

    // 2) 时间维度：显式把清单文件的 mtime 推到基线之后（节点集合不变 ⇒ 差异只可能来自时间戳）。
    //    不改内容长度（那动的是 size，不是 mtime）、不 `sleep`、不重试。
    let before = std::fs::metadata(&manifest)
        .expect("清单元数据")
        .modified()
        .expect("基线 mtime");
    let pushed = before + Duration::from_secs(5);
    let file = std::fs::File::options()
        .write(true)
        .open(&manifest)
        .expect("打开清单占位");
    file.set_modified(pushed).expect("把 mtime 推到基线之后");
    drop(file);

    let rewritten = tree_snapshot(&version_dir);
    assert_eq!(
        with_dir.len(),
        rewritten.len(),
        "节点集合没变，差异只该在 mtime 上"
    );
    assert_ne!(
        with_dir, rewritten,
        "快照必须看得见已有文件的 mtime 变化，否则零落盘断言是假绿"
    );
    // 差异必须是我们推的那一下，而不是两次 `stat` 的偶然抖动（粗粒度文件系统会把 5s 取整到
    // 4s 或 6s，所以判据是「至少前进 4s」而不是「恰好 5s」）。
    let after = std::fs::metadata(&manifest)
        .expect("清单元数据")
        .modified()
        .expect("推进后的 mtime");
    assert!(
        after.duration_since(before).expect("mtime 应当前进了") >= Duration::from_secs(4),
        "mtime 应当被推到基线之后：{before:?} → {after:?}"
    );
}
