//! 替换与回滚：成功后备份还在、失败后旧版本仍在原位、目标目录写不进去要报错。
//!
//! Windows 的「文件被占用」用例是**真造出来**的：用 `share_mode(FILE_SHARE_READ)` 打开目标
//! 文件（不带 `FILE_SHARE_DELETE`，与 Windows 加载器打开运行中映像的口径一致），让 `rename`
//! 撞共享冲突。它不是「运行中的 exe」（那需要真跑一个子进程，属真机验收），这一点在报告里写明。

use std::fs;
use std::path::PathBuf;

use crate::update::manifest::ReleaseManifest;
use crate::update::{
    install_staged, stage, DirSource, InstallFailure, ReplacePolicy, StagedPackage,
};

use super::support::{artifact, manifest, names_in, release, TempDir};

const UPDATE: &[u8] = b"new build bytes";
const OLD: &[u8] = b"old build bytes";

fn policy() -> ReplacePolicy {
    ReplacePolicy::new(2, std::time::Duration::from_millis(1))
}

/// 发布一个包体、跑完取包+校验，拿到可直接安装的 `StagedPackage`。
fn staged(tmp: &TempDir) -> StagedPackage {
    tmp.write("ull-test.zip", UPDATE);
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact("win-x64", "portable-zip", "ull-test.zip", UPDATE)],
        )],
    );
    let parsed = ReleaseManifest::from_json(&json).expect("清单应可解析");
    stage(
        &DirSource::new(tmp.path()),
        &parsed.releases[0].artifacts[0],
        &tmp.join("staging"),
    )
    .expect("取包与校验应通过")
}

#[test]
fn installs_over_an_existing_file_and_keeps_the_backup() {
    let tmp = TempDir::new("install-ok");
    let dest = tmp.write("app/ull.exe", OLD);
    let staged = staged(&tmp);
    let receipt = install_staged(&staged, &dest, policy()).expect("替换应成功");

    assert_eq!(fs::read(&dest).expect("目标应存在"), UPDATE);
    assert_eq!(receipt.bytes, UPDATE.len() as u64);
    assert_eq!(receipt.dest, dest);
    let backup = receipt.backup.clone().expect("旧版本应被留在备份路径");
    assert_eq!(fs::read(&backup).expect("备份应存在"), OLD);
    assert_eq!(
        backup.parent(),
        dest.parent(),
        "备份必须与目标同目录，跨卷 rename 会失去原子性"
    );

    receipt.remove_backup().expect("删备份");
    assert!(!backup.exists(), "备份应被删掉");
    assert_eq!(names_in(dest.parent().expect("父目录")), ["ull.exe"]);
}

#[test]
fn installing_into_a_fresh_path_has_no_backup() {
    let tmp = TempDir::new("install-fresh");
    let staged = staged(&tmp);
    let dest = tmp.join("dist/ull.exe");
    let receipt = install_staged(&staged, &dest, policy()).expect("新装应成功");
    assert_eq!(receipt.backup, None);
    assert_eq!(fs::read(&dest).expect("目标应存在"), UPDATE);
}

#[test]
fn failed_move_rolls_the_old_version_back() {
    let tmp = TempDir::new("install-rollback");
    let dest = tmp.write("app/ull.exe", OLD);
    let staged = staged(&tmp);
    // 让第二步（暂存包 → 目标）注定失败：把暂存文件删掉，等价于"搬到一半源没了"。
    fs::remove_file(staged.path()).expect("删掉暂存文件");

    let failure: InstallFailure =
        install_staged(&staged, &dest, policy()).expect_err("搬不进去必须失败");
    assert!(failure.rolled_back, "旧版本应回到原位：{failure}");
    assert!(!failure.needs_manual_repair());
    assert_eq!(failure.backup_left, None);
    assert_eq!(fs::read(&dest).expect("旧版本应仍在"), OLD);
    assert_eq!(
        names_in(dest.parent().expect("父目录")),
        ["ull.exe"],
        "回滚后不该留下备份文件"
    );
}

#[test]
fn manual_rollback_restores_the_previous_build() {
    let tmp = TempDir::new("install-manual-rollback");
    let dest = tmp.write("app/ull.exe", OLD);
    let staged = staged(&tmp);
    let mut receipt = install_staged(&staged, &dest, policy()).expect("替换应成功");
    assert_eq!(fs::read(&dest).expect("新版本"), UPDATE);
    let backup = receipt.backup.clone().expect("旧版本备份");

    receipt.rollback(policy()).expect("回滚应成功");
    assert_eq!(fs::read(&dest).expect("旧版本应回来"), OLD);
    assert!(!backup.exists(), "回滚会把备份搬回去，不留副本");
    assert_eq!(receipt.backup, None, "回滚成功后不该再指着不存在的备份");
    assert!(receipt.remove_backup().is_ok(), "没有备份时删备份是空操作");
}

#[test]
fn unwritable_target_directory_fails_without_touching_anything() {
    let tmp = TempDir::new("install-unwritable");
    let staged = staged(&tmp);
    // 父路径是个普通文件：建目录必然失败，等价于"目标目录不可写"
    // （比 chmod 555 更可移植：CI 里常以 root 跑，chmod 对 root 无效）。
    let blocked = tmp.write("blocked", b"not a directory");
    let dest = blocked.join("app/ull.exe");

    let failure = install_staged(&staged, &dest, policy()).expect_err("写不进目标目录必须失败");
    assert!(failure.rolled_back, "没写任何东西，旧状态未变：{failure}");
    assert_eq!(
        fs::read(&blocked).expect("占位文件应原样"),
        b"not a directory"
    );
    assert!(!dest.exists());
}

#[cfg(windows)]
#[test]
fn windows_locked_target_fails_and_leaves_the_old_file_intact() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    /// `FILE_SHARE_READ`：只允许别人读，不允许改名/删除。
    const FILE_SHARE_READ: u32 = 0x1;

    let tmp = TempDir::new("install-locked");
    let dest = tmp.write("app/ull.exe", OLD);
    let staged = staged(&tmp);
    // 刻意**不**共享删除：`std::fs::File::open` 默认带 FILE_SHARE_DELETE（改名会被放行），
    // 而 Windows 加载器打开正在运行的映像时不带它——这才是我们要复现的占用形态。
    let held = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&dest)
        .expect("打开目标文件");
    let failure = install_staged(&staged, &dest, policy()).expect_err("占用中的文件改不了名");
    assert!(failure.rolled_back, "旧版本必须完好：{failure}");
    assert_eq!(fs::read(&dest).expect("旧版本应仍在"), OLD);
    drop(held);
    assert_eq!(
        names_in(dest.parent().expect("父目录")),
        ["ull.exe"],
        "失败的尝试不该留下备份或暂存垃圾"
    );
}

#[test]
fn backup_path_stays_in_the_same_directory() {
    let dest = PathBuf::from("some/dir/ull.exe");
    let backup = crate::update::backup_path(&dest);
    assert_eq!(backup.parent(), dest.parent());
    let name = backup
        .file_name()
        .expect("备份名")
        .to_string_lossy()
        .into_owned();
    assert!(name.starts_with(".ull.exe.ullold-"), "实际：{name}");
    assert_ne!(
        backup,
        crate::update::backup_path(&dest),
        "同一进程两次调用不能撞名"
    );
}

/// 备份只留最近一份：连装两次不会在安装目录里堆两个几十 MB 的旧 exe。
///
/// 旧行为是「每次 apply 留一个、永不删」（全仓没有任何调用方调 `remove_backup`），这条用例把
/// 上界钉在「一次安装一份」上。它抓得住「prune 被删掉/被改坏」；抓不住**并发**：两个进程同时
/// 替换同一路径本来就不成立（后者的回滚会把前者的新版本换回旧字节），这一点写在 `install` 的
/// 文档里，不是这里能验的。
#[test]
fn only_the_latest_backup_is_kept() {
    let tmp = TempDir::new("install-retention");
    let dest = tmp.write("app/ull.exe", OLD);
    // 上一次也没人删的备份（名字形态与 `backup_path` 一致，只是 pid/seq 不同）。
    tmp.write("app/.ull.exe.ullold-1-1", b"older old bytes");
    tmp.write("app/.ull.exe.ullold-1-2", b"even older bytes");
    // 别的程序的备份：不是我们的名字，一个都不许动。
    tmp.write("app/.other.exe.ullold-1-1", b"someone else's backup");
    // 上一次失败遗留的中转文件：垃圾，要在这次成功之后清掉。
    tmp.write(
        "app/.ull.exe.ullnew-1-1",
        b"half a build from a dead attempt",
    );

    let staged = staged(&tmp);
    let receipt = install_staged(&staged, &dest, policy()).expect("替换应成功");
    let backup = receipt.backup.clone().expect("本次备份");

    assert_eq!(fs::read(&backup).expect("本次备份"), OLD);
    assert_eq!(
        names_in(dest.parent().expect("父目录")),
        [
            ".other.exe.ullold-1-1".to_owned(),
            backup
                .file_name()
                .expect("备份名")
                .to_string_lossy()
                .into_owned(),
            "ull.exe".to_owned(),
        ],
        "只该留本次备份，且不许碰别的程序的备份"
    );
}

/// 崩溃现场：上一次装到「dest → 备份」之后被杀，`dest` 空着、旧字节只剩备份那一份。
///
/// 这种情况下本次安装的 `keep = None`（dest 原本不存在 ⇒ 不建新备份），清理**必须放过**
/// `.ullold-*`——它是用户旧版本唯一幸存的副本，删了就没法手工恢复了。`.ullnew-*` 仍是垃圾，
/// 该清。变异实测：把 `prune_stale_artifacts` 里 `keep.is_some() &&` 这一半去掉 ⇒ 本条红。
#[test]
fn a_crashed_install_keeps_the_only_surviving_old_copy() {
    let tmp = TempDir::new("install-crash-scene");
    let app = tmp.join("app");
    fs::create_dir_all(&app).expect("建安装目录");
    // 崩现场：没有 ulls.exe，旧字节只在备份里。
    let survivor = tmp.write("app/.ull.exe.ullold-1-1", OLD);
    let transit = tmp.write(
        "app/.ull.exe.ullnew-1-1",
        b"half a build from a dead attempt",
    );

    let staged = staged(&tmp);
    let dest = app.join("ull.exe");
    let receipt = install_staged(&staged, &dest, policy()).expect("新装应成功");

    assert_eq!(receipt.backup, None, "dest 原本不存在 ⇒ 本次不建备份");
    assert_eq!(fs::read(&dest).expect("目标"), UPDATE);
    assert!(
        survivor.exists(),
        "崩溃现场唯一幸存的旧 exe 不许被删（用户靠它手工恢复）：{:?}",
        names_in(&app)
    );
    assert_eq!(
        fs::read(&survivor).expect("幸存副本"),
        OLD,
        "旧字节要原样留着"
    );
    assert!(
        !transit.exists(),
        "残留的中转名仍是垃圾，该清：{:?}",
        names_in(&app)
    );
}
