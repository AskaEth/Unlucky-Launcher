//! `rollback` 的文件层：落点、原子写、以及「把旧版本搬回目标路径」那两次改名。
//!
//! # 为什么单独一层
//!
//! 判据（阈值、guard、一次性凭证的语义）在 [`super::rollback`]，这里只放「怎么碰盘」。拆开的
//! 理由只是可读性与本仓 400 行硬线：那边回答「什么时候该退」，这边回答「退的时候文件怎么动」。
//!
//! # 回滚怎么搬文件（物理约束）
//!
//! 与 [`super::install`] 同一条：**Windows 上正在运行的 exe 只能改名，不能覆盖/删除**
//! （加载器打开映像时带 `FILE_SHARE_DELETE`，所以改名可以、`MoveFileEx(REPLACE_EXISTING)` 不行）。
//!
//! ```text
//! 1. rename(dest → .<名>.ullold-<pid>-<seq>)   此刻跑着的新版挪进一个**新**停车位
//! 2. rename(backup → dest)                    旧版本回到目标路径（同卷，原子）
//! 3. 尽力删停车位                              Unix 上当场消失；Windows 上还被映像占着 ⇒ 留给
//!                                             下一次安装的 prune_stale_artifacts 收
//!                                             （它本来就删多余的 .ullold-*）
//! ```
//!
//! 第 2 步失败就把停车位挪回去：目标路径上永远不会出现「什么都没有」的状态。停车位名由
//! [`backup_path`] 现生成——它每次调用都带新的序号，与真正的备份名不会撞。
//!
//! **恢复也失败**（三步全挂）时目标路径会空着、旧版本留在 `.ullold-*` 上——这是
//! [`super::install`] 里「回滚失败 ⇒ 需要人工处理」的同一套既有语义（那里把旧文件的落点报给
//! 用户）。本模块只做「清标记 + 具名说明」，不再自动重试：旧字节一个都不删，用户还有材料。
//!
//! # 三个文件
//!
//! | 文件 | 寿命 | 谁写 / 谁删 |
//! |---|---|---|
//! | `upgrade-attempt.json` | 一次升级尝试（装上新版 → 进主界面 / 回滚） | apply 写；`confirm` 或 `reconcile` 删 |
//! | `rollback-guard.json` | 永久（每个 `to` 一格） | 回滚时写；没人删（它就是「别再退一次」的记忆） |
//! | `rollback-notice.json` | 一次性 | 回滚/停手时写；下一次启动读一次就删 |

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::install::{backup_path, ReplacePolicy};
use super::replace_io::{rename_with_retry, ReplaceFs};
use super::rollback::{Attempt, Notice, SCHEMA};

const ATTEMPT_FILE: &str = "upgrade-attempt.json";
const GUARD_FILE: &str = "rollback-guard.json";
const NOTICE_FILE: &str = "rollback-notice.json";

/// 「升级尝试」标记的路径（`<cache>/update/upgrade-attempt.json`）。
#[must_use]
pub fn attempt_path(cache: &Path) -> PathBuf {
    update_dir(cache).join(ATTEMPT_FILE)
}

fn update_dir(cache: &Path) -> PathBuf {
    cache.join("update")
}

fn guard_path(cache: &Path) -> PathBuf {
    update_dir(cache).join(GUARD_FILE)
}

fn notice_path(cache: &Path) -> PathBuf {
    update_dir(cache).join(NOTICE_FILE)
}

/// 读标记；读不懂 / 格式版本不认识都当没有（**绝不**按猜的字段去搬用户的 exe）。
pub(crate) fn read_attempt(cache: &Path) -> Option<Attempt> {
    let bytes = fs::read(attempt_path(cache)).ok()?;
    let attempt: Attempt = serde_json::from_slice(&bytes).ok()?;
    (attempt.schema == SCHEMA).then_some(attempt)
}

pub(crate) fn write_attempt(cache: &Path, attempt: &Attempt) -> io::Result<()> {
    write_json(&attempt_path(cache), attempt)
}

pub(crate) fn clear_attempt(cache: &Path) {
    let _ = fs::remove_file(attempt_path(cache));
}

/// guard 里记的是「上一次自动回退掉的那一个 `to`」。
pub(crate) fn read_guard(cache: &Path) -> Option<String> {
    serde_json::from_slice(&fs::read(guard_path(cache)).ok()?).ok()
}

pub(crate) fn write_guard(cache: &Path, to: &str) {
    write_json(&guard_path(cache), &to).ok();
}

/// 取走一次性凭证（读到就删；读不懂也删——它只是一句提示，卡住没有意义）。
pub(crate) fn take_notice(cache: &Path) -> Option<Notice> {
    let path = notice_path(cache);
    let bytes = fs::read(&path).ok()?;
    let _ = fs::remove_file(&path);
    serde_json::from_slice(&bytes).ok()
}

pub(crate) fn write_notice(cache: &Path, notice: &Notice) {
    write_json(&notice_path(cache), notice).ok();
}

/// 把旧版本备份搬回 `dest`（顺序与理由见本文件模块文档）：两步改名都走调用方给的 [`ReplaceFs`]
/// （与 `install::install_with` 同一条形状；生产路径由 [`super::rollback::reconcile`] 传 `&RealFs`）。
///
/// 注入器能改的只有「这一次改名成不成功」，改不了判据与顺序——生产路径与单测跑的是同一份代码，
/// `&dyn` 后面那个实现不同而已。`rollback_io` 里那条「第二步失败就把停车位挪回去」的恢复路径
/// 只有靠它才可能被用例打到（真盘上没法确定性复现）。
pub(crate) fn roll_back_with(
    fs: &dyn ReplaceFs,
    attempt: &Attempt,
    policy: ReplacePolicy,
) -> io::Result<()> {
    let parked = match fs::symlink_metadata(&attempt.dest) {
        Ok(_) => {
            let parked = backup_path(&attempt.dest);
            rename_with_retry(fs, &attempt.dest, &parked, policy)?;
            Some(parked)
        }
        // dest 已经不在了（用户手工删过）：没有要挪开的东西，直接搬备份。
        Err(_) => None,
    };
    match rename_with_retry(fs, &attempt.backup, &attempt.dest, policy) {
        Ok(()) => {
            if let Some(parked) = parked {
                let _ = fs::remove_file(parked);
            }
            Ok(())
        }
        Err(error) => {
            if let Some(parked) = parked {
                let _ = rename_with_retry(fs, &parked, &attempt.dest, policy);
            }
            Err(error)
        }
    }
}

/// 原子写一份 JSON（先写同目录临时名，再同卷改名）：中途掉电不会留下半截标记。
fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let Some(dir) = path.parent() else {
        return Err(io::Error::other("标记文件没有父目录"));
    };
    fs::create_dir_all(dir)?;
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let tmp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    fs::write(&tmp, &bytes)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&tmp);
            Err(error)
        }
    }
}
