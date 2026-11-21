//! 回滚**搬不动**的两条（M309）：第二步被拒 ⇒ 停车位挪回；两步都被拒 ⇒ 需要人工恢复。
//!
//! # 为什么要注入
//!
//! `roll_back_with` 里那条「第二步失败就把停车位挪回去」的恢复路径在真盘上没法确定性复现
//! （要么让盘满，要么让杀软恰好在那一刻咬住）。这里用 [`ReplaceFs`] 的注入器把「这一次改名成不
//! 成功」变成输入——与 `install_io_cases` 的 `FakeFs` 同一条纪律：注入器**改不了**判据与顺序，
//! 生产路径与用例跑的是同一份 `roll_back_with`（`&dyn` 后面那个实现不同而已）。
//!
//! 抓得住：恢复路径是否真的跑了（`dest` 空不空）、搬不动是否被如实报成 `RollbackFailed`、
//! 旧字节有没有被顺手删掉。
//! 抓不住：真实文件系统的占用/权限形态，以及「整卷消失」这类极端现场。

use std::cell::Cell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::update::install::InstallReceipt;
use crate::update::replace_io::ReplaceFs;
use crate::update::rollback::{
    arm, attempt_path, reconcile, reconcile_with, Action, NoticeKind, TRIAL_DEADLINE_MS,
};
use crate::update::ReplacePolicy;

use super::support::{names_in, TempDir};

const OLD: &[u8] = b"old build bytes";
const NEW: &[u8] = b"new build bytes";

/// 一次「已经替换完成」的现场：`dest` 上是新版字节，同目录留着一份旧版备份。
fn installed(tmp: &TempDir) -> (PathBuf, PathBuf) {
    let dest = tmp.write("unlucky-launcher.exe", NEW);
    let backup = tmp.write(".unlucky-launcher.exe.ullold-1-0", OLD);
    (dest, backup)
}

fn receipt(dest: &Path, backup: &Path) -> InstallReceipt {
    InstallReceipt {
        dest: dest.to_path_buf(),
        backup: Some(backup.to_path_buf()),
        bytes: NEW.len() as u64,
    }
}

fn bytes(path: &Path) -> Vec<u8> {
    fs::read(path).expect("读文件")
}

/// 回滚那两步改名的注入器：按**第几次 rename** 决定失败（`0` = 第一次改名 = `dest → 停车位`，
/// `1` = 第二次 = `备份 → dest`，`2` = 恢复 = `停车位 → dest`）。
///
/// 与 `install_io_cases` 的 `FakeFs` 同一条纪律：注入器只能改「这一次改名成不成功」，改不了
/// 判据与顺序——生产路径与用例跑的是同一份 `roll_back_with`。
struct FlakyFs {
    fail_at: Vec<usize>,
    renames: Cell<usize>,
}

impl FlakyFs {
    fn failing_at(fail_at: &[usize]) -> Self {
        Self {
            fail_at: fail_at.to_vec(),
            renames: Cell::new(0),
        }
    }
}

impl ReplaceFs for FlakyFs {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let index = self.renames.get();
        self.renames.set(index + 1);
        if self.fail_at.contains(&index) {
            return Err(io::Error::other(format!("注入：第 {index} 次改名被拒")));
        }
        fs::rename(from, to)
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        fs::copy(from, to)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
}

/// 不重试（注入的失败每次都一样，重试只会把用例拖成 5 × 80ms）。
fn one_shot() -> ReplacePolicy {
    ReplacePolicy::new(1, std::time::Duration::from_millis(0))
}

/// 把现场推到「超窗口、该回滚」的那一刻，返回注入器与（超窗口的）时刻。
fn poised(cache: &Path) -> u64 {
    let start = 3_000_u64;
    assert_eq!(
        reconcile(cache, "0.2.3", start).action,
        Action::TrialRecorded
    );
    start + TRIAL_DEADLINE_MS
}

/// 回滚**第二步**搬不动（`备份 → dest` 被拒）⇒ 停车位必须被挪回目标路径，`RollbackFailed` 带具名 detail。
///
/// 抓得住：去掉恢复路径——`dest` 会空着，`bytes(&dest)` 立刻红；或者把搬不动当成成功
/// （`action` 红在 `RolledBack`）。
/// 抓不住：真盘上杀软/占用造成的失败形态（注入器只造「这一次被拒」，不造原因）。
#[test]
fn a_failed_second_rename_puts_the_new_build_back() {
    let tmp = TempDir::new("rb-restore");
    let cache = tmp.join("cache");
    let (dest, backup) = installed(&tmp);
    arm(receipt(&dest, &backup), &cache, "0.2.2", "0.2.3", 1_000).expect("落标记");
    let expired = poised(&cache);

    // 第 1 次改名（dest → 停车位）成功，第 2 次（备份 → dest）失败，第 3 次（停车位 → dest）成功。
    let fs = FlakyFs::failing_at(&[1]);
    let outcome = reconcile_with(&fs, one_shot(), &cache, "0.2.3", expired);
    assert_eq!(outcome.action, Action::RollbackFailed);
    let notice = outcome.notice.expect("搬不动也要说一句");
    assert_eq!(notice.kind, NoticeKind::RollbackFailed);
    assert!(
        notice.detail.contains("第 1 次改名被拒"),
        "detail 要带上真正的原因：{}",
        notice.detail
    );
    assert_eq!(
        bytes(&dest),
        NEW,
        "停车位必须被挪回目标路径（不许留个空 dest）"
    );
    assert_eq!(bytes(&backup), OLD, "旧版本仍在备份名上，可以再试");
    assert!(
        !attempt_path(&cache).exists(),
        "搬不动也要清标记，否则每次启动都重演一次失败"
    );
}

/// 两步都失败（第二次与恢复都被拒）⇒ 目标路径空着、旧版留在 `.ullold-*` 上——这是
/// **需要人工恢复**的那一档（与 `install.rs` 的既有手工恢复语义同源），不能报成「已回滚」。
///
/// 抓得住：把「恢复也失败」当成无事发生（`dest` 断言红），或者谎称回滚成功（`action` 红）。
#[test]
fn a_double_rename_failure_leaves_the_old_build_in_its_backup() {
    let tmp = TempDir::new("rb-unrecoverable");
    let cache = tmp.join("cache");
    let (dest, backup) = installed(&tmp);
    arm(receipt(&dest, &backup), &cache, "0.2.2", "0.2.3", 1_000).expect("落标记");
    let expired = poised(&cache);

    let fs = FlakyFs::failing_at(&[1, 2]);
    let outcome = reconcile_with(&fs, one_shot(), &cache, "0.2.3", expired);
    assert_eq!(outcome.action, Action::RollbackFailed);
    let notice = outcome.notice.expect("这一档更要说话");
    assert!(!notice.detail.is_empty(), "要给出为什么没搬动");
    assert!(!dest.exists(), "两步都失败：目标路径会空着");
    assert_eq!(bytes(&backup), OLD, "旧版本还在备份名上（人工恢复的材料）");
    let leftovers = names_in(tmp.path())
        .into_iter()
        .filter(|name| name.contains(".ullold-"))
        .count();
    assert_eq!(
        leftovers, 2,
        "备份 + 停车位都应留在盘上（别把唯一一份旧字节删了）"
    );
    assert!(!attempt_path(&cache).exists(), "清标记：不许反复重演");
}
