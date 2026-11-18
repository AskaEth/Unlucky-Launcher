//! 「新版起不来就退回上一版」（M309）：四条情形 + guard + 一次性凭证 + 标记写不下来。
//!
//! | 用例 | mission 条目 |
//! |---|---|
//! | [`a_successful_first_start_clears_the_marker_and_the_backup`] | ①升级后首次启动成功 ⇒ 清标记、备份按既定策略收敛 |
//! | [`a_confirmed_version_is_never_rolled_back_later`] | ①之后的「不许无限循环」：确认过的版本永不再回滚 |
//! | [`a_trial_inside_the_window_is_never_rolled_back`] | ②标记在但未超阈值（正常首次启动途中）⇒ 不回滚 |
//! | [`an_expired_trial_rolls_back_exactly_once`] | ③超阈值 ⇒ 回滚到备份且标记清除，回滚后不再二次触发 |
//! | [`a_second_attempt_at_the_same_version_gives_up`] | ④回滚后标记又出现 ⇒ 停止自动回滚，只给具名结论 |
//! | [`arming_without_a_marker_rolls_the_new_build_back`] | 标记落不下去 ⇒ 当场退回旧版 + 具名原因 |
//! | [`arming_without_a_backup_writes_nothing`] | 没有旧版本就没有可回退的东西 ⇒ 不落标记 |
//!
//! 「回滚那两步改名搬不动」的两条在 `rollback_io_cases`（要注入器，与 `install_io_cases` 同族）。
//!
//! **都没跑真进程**：「起不来的新版」是用「标记里 `trial.started_ms` 落在窗口之外」造出来的
//! （`now_ms` 直接喂给 [`reconcile`]），不是真的让它崩。真机验收（装一个必崩的构建、看它自动
//! 退回）见报告「没验的部分」。
//!
//! 每个用例的注释里写清「抓得住什么 / 抓不住什么」——变异证明（改一处 ⇒ 哪条用例红）见报告。

use std::fs;
use std::path::{Path, PathBuf};

use crate::update::install::InstallReceipt;
use crate::update::rollback::{
    arm, attempt_path, confirm, reconcile, Action, NoticeKind, RollbackCause, TRIAL_DEADLINE_MS,
};
use crate::update::UpdateErrorKind;

use super::support::TempDir;

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

/// ①升级后首次启动成功 ⇒ 标记清除、旧备份收敛；**之后再启动多少次都不许回滚**。
///
/// 抓得住：去掉「成功后清标记」（或清完又写回去）——第 4 段那个「远超阈值的一次启动」
/// 会拿同一份标记判成「起不来」并把一个**正常**版本退回旧版（dest 变回 `OLD`）。
/// 抓不住：真机上一次启动到底有没有走到 `confirm`（那由 app-iced 的接线用例守）。
#[test]
fn a_successful_first_start_clears_the_marker_and_the_backup() {
    let tmp = TempDir::new("rb-ok");
    let cache = tmp.join("cache");
    let (dest, backup) = installed(&tmp);
    arm(receipt(&dest, &backup), &cache, "0.2.2", "0.2.3", 1_000).expect("落标记");
    assert!(attempt_path(&cache).is_file(), "装上新版必须留下尝试标记");

    // 新版第一次启动：只登记，不回滚（这一刻还完全可能是在正常启动途中）。
    let first = reconcile(&cache, "0.2.3", 1_100);
    assert_eq!(first.action, Action::TrialRecorded);
    assert!(first.notice.is_none(), "正常启动途中不该有任何提示");
    assert_eq!(bytes(&dest), NEW);

    // 进了主界面 ⇒ 清标记 + 删旧备份（`InstallReceipt::remove_backup` 的生产调用方）。
    let confirmed = confirm(&cache, "0.2.3");
    assert!(confirmed.marker_cleared, "成功后必须清标记");
    assert!(
        confirmed.backup_removed,
        "旧版本备份按既定策略收敛（成功启动后删掉）"
    );
    assert!(!attempt_path(&cache).exists(), "标记文件必须真的不在盘上");
    assert!(!backup.exists(), "备份文件必须真的被删掉");

    // 一周后再启动（远超阈值）：这一版是正常版本，**绝不许**被回滚。
    let much_later = reconcile(&cache, "0.2.3", 1_100 + 7 * 24 * 3600 * 1_000);
    assert_eq!(
        much_later.action,
        Action::Nothing,
        "没有标记就不该有任何动作"
    );
    assert!(much_later.notice.is_none(), "成功路径不弹多余提示");
    assert_eq!(bytes(&dest), NEW, "正常版本不许被退回旧版");
}

/// ①确认过的版本**永远不许**再回滚（这条就是「不许无限循环」的直接判据）。
///
/// 抓得住：去掉 `confirm` 里的清标记——下一次启动会拿同一份标记判成「起不来」，把一个已经
/// 正常跑到主界面的版本退回旧版（`dest` 变回 `OLD`、`action` 变 `RolledBack`）。
/// 抓不住：真机上「进了主界面」那一拍是否真的被调到（由 app-iced 的接线用例守）。
#[test]
fn a_confirmed_version_is_never_rolled_back_later() {
    let tmp = TempDir::new("rb-loop");
    let cache = tmp.join("cache");
    let (dest, backup) = installed(&tmp);
    arm(receipt(&dest, &backup), &cache, "0.2.2", "0.2.3", 1_000).expect("落标记");
    assert_eq!(
        reconcile(&cache, "0.2.3", 2_000).action,
        Action::TrialRecorded
    );
    // 首帧画出来了（确认）。此后无论过多久、启动多少次，都不许再动这一版。
    let _ = confirm(&cache, "0.2.3");
    for step in 1..=3_u64 {
        let later = reconcile(&cache, "0.2.3", 2_000 + step * TRIAL_DEADLINE_MS);
        assert_eq!(
            later.action,
            Action::Nothing,
            "确认过的版本不许再回滚（第 {step} 次启动）"
        );
        assert!(later.notice.is_none(), "成功路径不弹多余提示");
        assert_eq!(bytes(&dest), NEW, "已确认的版本不许被退回旧版");
    }
}

/// ②标记在、但还没超阈值（正常首次启动途中）⇒ **不回滚**。
///
/// 抓得住：把窗口判定去掉/改成恒真——第二段那次（`TRIAL_DEADLINE_MS - 1`）会当场回滚，
/// `dest` 变回 `OLD`、`action` 变 `RolledBack`。
/// 抓不住：窗口长度选得对不对（阈值本身是判断，不是事实）。
#[test]
fn a_trial_inside_the_window_is_never_rolled_back() {
    let tmp = TempDir::new("rb-window");
    let cache = tmp.join("cache");
    let (dest, backup) = installed(&tmp);
    arm(receipt(&dest, &backup), &cache, "0.2.2", "0.2.3", 1_000).expect("落标记");

    let start = 2_000;
    assert_eq!(
        reconcile(&cache, "0.2.3", start).action,
        Action::TrialRecorded
    );
    // 差 1 毫秒到期：仍然按「启动途中」处理。
    let inside = reconcile(&cache, "0.2.3", start + TRIAL_DEADLINE_MS - 1);
    assert_eq!(inside.action, Action::TrialRecorded, "窗口内不许回滚");
    assert!(inside.notice.is_none(), "窗口内不该给用户任何结论");
    assert_eq!(bytes(&dest), NEW, "目标路径上必须还是新版");
    assert!(backup.exists(), "旧版本备份还在（还没到判死刑的时候）");
    assert!(
        attempt_path(&cache).is_file(),
        "标记还在，等下一次启动接着判"
    );
}

/// ③超阈值 ⇒ 回滚到备份、清标记、**只回滚一次**；旧版启动时把结论说一次就收。
///
/// 抓得住：①回滚分支不清标记（紧接着的「又一次启动」会拿同一份标记再判一次）；
/// ②一次性凭证不删（旧版启动时那句永远说得出来）。
/// 抓不住：回滚在真机上搬不搬得动（运行中的 exe 只能改名，这条靠真机验收）。
#[test]
fn an_expired_trial_rolls_back_exactly_once() {
    let tmp = TempDir::new("rb-expired");
    let cache = tmp.join("cache");
    let (dest, backup) = installed(&tmp);
    arm(receipt(&dest, &backup), &cache, "0.2.2", "0.2.3", 1_000).expect("落标记");

    let start = 5_000;
    assert_eq!(
        reconcile(&cache, "0.2.3", start).action,
        Action::TrialRecorded
    );
    // 过了窗口还没进主界面：这一次启动就把它退回去。
    let expired = reconcile(&cache, "0.2.3", start + TRIAL_DEADLINE_MS);
    assert_eq!(expired.action, Action::RolledBack);
    let notice = expired.notice.expect("回滚必须给用户一句解释");
    assert_eq!(notice.kind, NoticeKind::RolledBack);
    assert_eq!(notice.to, "0.2.3");
    assert_eq!(notice.from, "0.2.2");
    assert_eq!(
        notice.cause,
        RollbackCause::Timeout,
        "只启动过一次 ⇒ 启动阶段挂住/被杀（超时）"
    );
    assert_eq!(bytes(&dest), OLD, "旧版本必须回到目标路径上");
    assert!(
        !attempt_path(&cache).exists(),
        "回滚后标记必须清掉（命门第一道）"
    );

    // 回滚之后：再启动同一个 to 也不许再搬任何东西（一次性凭证在这一刻被说掉一次）。
    let again = reconcile(&cache, "0.2.3", start + 10 * TRIAL_DEADLINE_MS);
    assert_eq!(again.action, Action::Nothing, "回滚后不许二次触发");
    assert_eq!(bytes(&dest), OLD);
    assert_eq!(
        again.notice.map(|notice| notice.kind),
        Some(NoticeKind::RolledBack),
        "回滚进程若当场死掉，下一次启动仍要说得出来"
    );
    let silent = reconcile(&cache, "0.2.2", start + 10 * TRIAL_DEADLINE_MS + 1);
    assert!(silent.notice.is_none(), "一次性凭证只能被说一次");
    assert_eq!(silent.action, Action::Nothing);
}

/// ④「回滚后标记又出现」（用户重装同一份坏构建）⇒ 停止自动回滚，只给具名结论。
///
/// 抓得住：去掉 guard 判定——第二次超阈值会把旧备份**再搬一次**（并在备份已被消费掉时变成
/// 一次 `RollbackFailed`），`action` 就不再是 `GaveUp`。
/// 抓不住：用户之后真机上的手工处置。
#[test]
fn a_second_attempt_at_the_same_version_gives_up() {
    let tmp = TempDir::new("rb-guard");
    let cache = tmp.join("cache");
    let (dest, backup) = installed(&tmp);
    arm(receipt(&dest, &backup), &cache, "0.2.2", "0.2.3", 1_000).expect("落标记");
    let start = 9_000;
    assert_eq!(
        reconcile(&cache, "0.2.3", start).action,
        Action::TrialRecorded
    );
    assert_eq!(
        reconcile(&cache, "0.2.3", start + TRIAL_DEADLINE_MS).action,
        Action::RolledBack
    );

    // 用户又把同一份 0.2.3 装上（「标记又出现」）。
    let (dest2, backup2) = installed(&tmp);
    arm(receipt(&dest2, &backup2), &cache, "0.2.2", "0.2.3", 20_000).expect("落标记");
    assert_eq!(
        reconcile(&cache, "0.2.3", 20_000).action,
        Action::TrialRecorded,
        "第一次启动仍然只登记（窗口内）"
    );
    let gave_up = reconcile(&cache, "0.2.3", 20_000 + TRIAL_DEADLINE_MS);
    assert_eq!(gave_up.action, Action::GaveUp, "同一版本不再自动回滚");
    let notice = gave_up.notice.expect("停手也要说清");
    assert_eq!(notice.kind, NoticeKind::GaveUp);
    assert_eq!(bytes(&dest2), NEW, "停手 = 一个文件都不搬");
    assert!(
        !attempt_path(&cache).exists(),
        "停手也要清标记，否则每次启动都喊一遍"
    );

    // 更高的版本不受这一格影响（guard 只钉住那一个 `to`）。
    let (dest3, backup3) = installed(&tmp);
    arm(receipt(&dest3, &backup3), &cache, "0.2.2", "0.2.4", 40_000).expect("落标记");
    assert_eq!(
        reconcile(&cache, "0.2.4", 40_000).action,
        Action::TrialRecorded
    );
    assert_eq!(
        reconcile(&cache, "0.2.4", 40_000 + TRIAL_DEADLINE_MS).action,
        Action::RolledBack,
        "换一个版本照常自动回滚"
    );
}

/// 标记落不下去（只读数据根）⇒ **当场退回旧版**到目标路径上，并给出具名原因。
///
/// 抓得住：把「写标记失败」当成无关紧要（只 `ok()` 掉）——新版会留在目标路径上，
/// `dest` 断言立刻红；或者静默 `Ok`，`unwrap_err` 直接 panic。
/// 抓不住：真只读盘上的权限行为（这里用「cache 路径上是个普通文件」造同样的 `create_dir_all` 失败）。
#[test]
fn arming_without_a_marker_rolls_the_new_build_back() {
    let tmp = TempDir::new("rb-nowrite");
    // cache 的位置上放一个**文件**：`create_dir_all` 必定失败，等于数据根不可写。
    let cache = tmp.write("cache", b"not a directory");
    let (dest, backup) = installed(&tmp);
    let error = arm(receipt(&dest, &backup), &cache, "0.2.2", "0.2.3", 1_000)
        .expect_err("标记写不下去就必须报错");
    assert_eq!(error.kind, UpdateErrorKind::Io);
    let text = error.to_string();
    assert!(text.contains("已退回旧版本"), "{text}");
    assert_eq!(bytes(&dest), OLD, "落不下标记 ⇒ 新版不许留在目标路径上");
    assert!(!backup.exists(), "回滚已经把备份搬回目标路径");
}

/// 安装前目标本来就不存在（`backup == None`）⇒ 不落标记：没有可回退的东西。
///
/// 抓得住：给「没有旧版本」的安装也塞一份标记——下次启动会拿一个空备份去搬，白报一次失败。
#[test]
fn arming_without_a_backup_writes_nothing() {
    let tmp = TempDir::new("rb-nobackup");
    let cache = tmp.join("cache");
    let dest = tmp.write("unlucky-launcher.exe", NEW);
    let receipt = InstallReceipt {
        dest: dest.clone(),
        backup: None,
        bytes: NEW.len() as u64,
    };
    arm(receipt, &cache, "", "0.2.3", 1_000).expect("没有备份不算错");
    assert!(
        !attempt_path(&cache).exists(),
        "没有可回退的东西就不许留标记"
    );
    assert_eq!(bytes(&dest), NEW);
}
