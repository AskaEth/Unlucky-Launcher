//! 实例存档 / 截图的宿主实现（M235 · STOREMVP 读路径 + M247 · SAVESOPS 写路径）。
//!
//! ## 四条纪律
//!
//! | 纪律 | 落点 |
//! |---|---|
//! | 磁盘只在后台线程碰 | [`spawn_list`] / [`spawn_open_folder`] / [`spawn_rename`] / [`spawn_delete`] / [`spawn_backup`] 全走 `job::spawn`，UI 只收快照 |
//! | 单根语义（D20） | 运行目录来自 `game::isolation::derive_game_dir`——**与启动同一个答案**，不另派生、不做双根合并 |
//! | 读路径只读 | [`list`] 一次写盘都没有；`saves/` 不存在也不替玩家建（core 的 `saves::scan` 同理） |
//! | 写路径自己重校边界 | 三个写操作**不在 host 这边信任自己**：core 的 `saves::write` 每次重新校验 `run_dir` 与目标路径（越界 / 符号链接 / 撞名都在那里拒） |
//! | 游戏在跑就不写 | [`dispatch`] 的 `running` 为真时三条写命令**不 spawn、不碰盘**，直接回 [`crate::dto::SavesFailureKind::GameRunning`] |
//!
//! ## 三个写操作的回程形状
//!
//! | 操作 | 成功 | 失败 |
//! |---|---|---|
//! | 重命名 | 新快照 + [`SavesOped::Renamed`] | 具名 [`SavesFailureKind`]（撞名 / 名字不可用 / 越界 / 链接 / IO） |
//! | 删除 | 新快照 + [`SavesOped::Deleted`] | 同上 |
//! | 备份 | 新快照 + [`SavesOped::BackedUp`]（落点路径 / 字节数 / 跳过的链接数） | 同上 + `TooLarge` |
//!
//! **写成功后一律重新列一遍**（`snapshot`）：界面画的永远是磁盘上的真相，不是"我以为我改成了什么"。
//! 回执只是"这次动的是哪一条"，与快照一起回去，所以 UI 不会出现「列表变了但不知道谁变的」。
//!
//! ## 「游戏在跑」为什么挡在 host 这一侧（M247）
//!
//! 判据是 `CoreHost::in_flight(&id)`（与 `AppCommand::Mods` 同一个），由 `host::dispatch` 传进来。
//! 为真时 [`dispatch`] **不 spawn 线程**、直接发一条 [`SavesFailureKind::GameRunning`] 的失败回去：
//!
//! | 平台 | 不挡的后果 |
//! |---|---|
//! | Windows | 进程占着世界目录 ⇒ rename/删除撞共享冲突 ⇒ 用户读一句 `os error 32` |
//! | **Linux** | **删一个正在跑的世界会成功** —— 用户在玩到一半时丢了存档 |
//!
//! 所以这不是"说得更清楚"，是**安全性**。也不能只靠界面灰按钮：判据留在 host，UI 那一侧只是
//! 尽力而为的提示（本版没加"灰按钮"的形态，理由是快照里没有 `running` 字段——加它要动列表 DTO，
//! 而这一版已经有那条明确的失败句子兜住）。
//!
//! 读路径（[`SavesCommand::List`]）与「打开文件夹」**不受影响**：它们不改盘。

//! ## 为什么不复用启动路径的 `run_dir()`
//!
//! 与 [`super::mods`] 同一条理由：`host::launch` 的 `run_dir()` 顺带把「这次用哪个档位」写进实例
//! 设置（`observe_mode`）。**列存档不该因为用户点开详情页就写盘**，所以这里只取派生结果。
//!
//! ## 「打开文件夹」为什么由 host 决定打开哪个目录
//!
//! `saves/` 可能还不存在（新实例），而用户点「打开文件夹」想看到的是**能放存档的地方**。
//! 于是退化成：`saves/` → `screenshots/` → 运行目录，取第一个存在的。这个判断是"磁盘上到底
//! 有什么"的事实，放在 host 这一侧；UI 只发一条 [`crate::dto::SavesCommand::OpenFolder`]。
//!
//! 真正拉起文件管理器的是 [`unlucky_core::platform::opener::open_dir`]（`AGENTS.md` §6：
//! 平台差异只许收敛在 `core::platform`，本文件一个 `cfg` 都没有，也没有一次裸 spawn）。
//!
//! ## 备份落点为什么是启动器自己的 `exports/`
//!
//! 本仓没有 `rfd` 这类原生文件选择框依赖（`core::launch::crash_bundle::default_target_dir` 的
//! 文档里记着同一条理由），所以「用户选定目录」先退化成：`<data>/exports/saves/`。
//! 命令带一个可选的 `dest`，将来接了选择框、或 CLI 想指定别处，都不用改 core。
//! 成功的回执里带**完整路径 + 字节数**，用户不用猜落点。
//!
//! ## 失败口径
//!
//! 一律走 [`crate::dto::SavesFailureView`]（结构 + OS 原文），**不拼中文**：文案由 app 层按
//! i18n key 现求（M161 的纪律）。读路径三类失败见 [`crate::dto::SavesFailureKind`]，
//! 写路径八类见同一个枚举的 `Write*` 那一族——每一种在 UI 上都有一句话。

use std::path::PathBuf;

use unlucky_core::game::install::instance::read_instance_config;
use unlucky_core::game::isolation::derive_game_dir;
use unlucky_core::game::saves::{self, SavesError, SavesScan};
use unlucky_core::platform::opener;

use crate::dto::{
    locate, InstanceId, InstanceSummary, SavesArea, SavesCommand, SavesFailureKind, SavesFailureOp,
    SavesFailureView, SavesListView, SavesOped, SavesSkipKind, SavesSkipView, ScreenshotPreview,
    ScreenshotView, WorldView,
};

use super::env::HostEnv;
use super::job::{spawn, JobSender};

/// 截图的缩略图：后台解码 + 进程级 memo（D54 第 3/4/5 条，见模块文档）
pub mod thumbs;

mod write;

/// 后台线程回报的一次结果（`HostTask::Saves` 的载荷）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// 实例 id（迟到结果靠它判「这条还是不是当前实例的」）。
    pub instance: InstanceId,
    /// 列表结果：成功 = 整份快照；失败 = 具名原因。
    pub list: Result<SavesListView, SavesFailureView>,
}

/// 后台列一个实例的 `saves/` 与 `screenshots/`。
pub fn spawn_list(env: HostEnv, tx: JobSender, id: InstanceId) {
    spawn("saves-list", move || {
        let report = Report {
            instance: id.clone(),
            list: list(&env, &id),
        };
        let _ = tx.send(super::HostTask::Saves(Box::new(report)));
    });
}

/// 后台用系统文件管理器打开存档目录，**并把刷新后的快照一起带回来**。
///
/// 带快照回来不是顺手：用户常在文件管理器里删掉旧世界再切回来，页面这一拍就能跟着更新，
/// 不用等下一次进详情页。
pub fn spawn_open_folder(env: HostEnv, tx: JobSender, id: InstanceId) {
    spawn("saves-open", move || {
        let report = Report {
            instance: id.clone(),
            list: open_folder(&env, &id),
        };
        let _ = tx.send(super::HostTask::Saves(Box::new(report)));
    });
}

/// 一条指令属于哪个实例（host 侧算「游戏在不在跑」要用它）。
///
/// 五条命令都带实例 id，所以这里不返回 `Option`：少一条分支处理会让闸门在那条命令上
/// **静默失效**，而这正是 M247 要修的东西。
#[must_use]
pub fn instance_of(command: &SavesCommand) -> &str {
    match command {
        SavesCommand::List(id)
        | SavesCommand::OpenFolder { id }
        | SavesCommand::RenameWorld { id, .. }
        | SavesCommand::DeleteWorld { id, .. }
        | SavesCommand::BackupWorld { id, .. } => id,
    }
}

/// 这条命令是不是**改盘**的写操作（读路径与打开文件夹不是）。
fn write_target(command: &SavesCommand) -> Option<(&str, SavesFailureOp)> {
    match command {
        SavesCommand::RenameWorld { dir, .. } => Some((dir, SavesFailureOp::Rename)),
        SavesCommand::DeleteWorld { dir, .. } => Some((dir, SavesFailureOp::Delete)),
        SavesCommand::BackupWorld { dir, .. } => Some((dir, SavesFailureOp::Backup)),
        SavesCommand::List(_) | SavesCommand::OpenFolder { .. } => None,
    }
}

/// 一条指令 → 后台线程（`CoreHost::dispatch` 的唯一入口）。
///
/// `running` = 这个实例正在启动/运行中（`CoreHost::in_flight`）。为真时**三个写操作不落 core**：
/// 直接回一条 [`SavesFailureKind::GameRunning`] 的具名失败，一个字节都不碰盘。
///
/// 为什么这一层必须挡：Windows 上进程占着世界目录，rename/删除会撞共享冲突（难看但至少失败）；
/// **Linux 上删一个正在跑的世界会成功**——那是在用户玩到一半时把存档删掉。所以判据不能只靠
/// 界面灰按钮（那也是同一条纪律：`ModsCommand::SetEnabled` 同样挡在 host 这一侧）。
///
/// 不 spawn 线程：没有磁盘工作要排队，直接发一条结果回去更诚实（`tx` 是同步通道）。
pub fn dispatch(env: HostEnv, tx: JobSender, command: SavesCommand, running: bool) {
    if running {
        if let Some((dir, op)) = write_target(&command) {
            let id = instance_of(&command);
            let report = Report {
                instance: id.to_string(),
                list: Err(blocked(id, dir, op)),
            };
            let _ = tx.send(super::HostTask::Saves(Box::new(report)));
            return;
        }
    }
    match command {
        SavesCommand::List(id) => spawn_list(env, tx, id),
        SavesCommand::OpenFolder { id } => spawn_open_folder(env, tx, id),
        SavesCommand::RenameWorld { id, dir, to } => write::spawn_rename(env, tx, id, dir, to),
        SavesCommand::DeleteWorld { id, dir } => write::spawn_delete(env, tx, id, dir),
        SavesCommand::BackupWorld { id, dir, dest } => write::spawn_backup(env, tx, id, dir, dest),
    }
}

/// 写操作被「游戏在跑」挡下时的那条失败（**结构化，带世界名与操作**，文案由 app 层现求）。
fn blocked(instance: &str, dir: &str, op: SavesFailureOp) -> SavesFailureView {
    let mut failure = failure(instance, SavesFailureKind::GameRunning, String::new());
    failure.file = dir.to_string();
    failure.op = Some(op);
    failure
}

/// 列目录（后台线程里调）。
fn list(env: &HostEnv, id: &str) -> Result<SavesListView, SavesFailureView> {
    let (instance, run_dir) = target(env, id)?;
    snapshot(&instance, &run_dir, None)
}

/// 重新扫一遍目录 → 快照（可挂一条写操作的回执）。
///
/// 写成功后**一律**走这里：界面画的是磁盘上的真相，不是"我以为我改成了什么"。
pub(super) fn snapshot(
    instance: &InstanceSummary,
    run_dir: &std::path::Path,
    last_op: Option<SavesOped>,
) -> Result<SavesListView, SavesFailureView> {
    let scan = saves::scan(run_dir).map_err(|error| unusable(&instance.id, &error))?;
    Ok(view(&instance.id, &scan, last_op))
}

/// 打开存档目录 → 再列一遍（失败只说"打不开"，原文进括号）。
fn open_folder(env: &HostEnv, id: &str) -> Result<SavesListView, SavesFailureView> {
    let (instance, run_dir) = target(env, id)?;
    let scan = saves::scan(&run_dir).map_err(|error| unusable(&instance.id, &error))?;
    let target = open_target(&scan, &run_dir);
    opener::open_dir(&target)
        .map_err(|detail| failure(&instance.id, SavesFailureKind::OpenFailed, detail))?;
    Ok(view(&instance.id, &scan, None))
}

/// 打开哪一个目录：`saves/` → `screenshots/` → 运行目录（取第一个存在的）。
fn open_target(scan: &SavesScan, run_dir: &std::path::Path) -> PathBuf {
    if scan.exists {
        return scan.dir.clone();
    }
    if scan.screenshots_exists {
        return scan.screenshots_dir.clone();
    }
    run_dir.to_path_buf()
}

/// 实例 + 它的运行目录（找不到实例 / 派生不出目录都给具名失败）。
pub(super) fn target(
    env: &HostEnv,
    id: &str,
) -> Result<(InstanceSummary, PathBuf), SavesFailureView> {
    let instances = super::game::instances(env);
    let Some(instance) = locate(&instances, id) else {
        return Err(failure(
            id,
            SavesFailureKind::InstanceUnavailable,
            format!("实例 {id} 不在快照里（可能刚被删除）"),
        ));
    };
    let dir = run_dir(env, instance)
        .map_err(|detail| failure(&instance.id, SavesFailureKind::InstanceUnavailable, detail))?;
    Ok((instance.clone(), dir))
}

/// 实例的运行目录（D20 覆盖链，与启动同一份派生；**不写盘**）。
pub(super) fn run_dir(env: &HostEnv, instance: &InstanceSummary) -> Result<PathBuf, String> {
    let global = super::game::global_mode(env);
    let config = read_instance_config(&env.layout, &instance.id)
        .map_err(|error| format!("读实例设置失败：{error}"))?;
    let derivation = derive_game_dir(
        &env.game,
        &instance.version_id,
        global,
        config.as_ref(),
        None,
    )
    .map_err(|error| format!("派生运行目录失败：{error}"))?;
    Ok(derivation.game_dir)
}

/// 快照映射：core 的扫描结果 → DTO（路径串在 host 这一侧才成形）。
///
/// **截图带缩略图**（M251）：像素在这里（后台线程里）解好、随快照一起交出去，UI 只画。
/// D54 明写「不许在渲染回调里解码」——这条纪律的落点就是「`view` 里 attach，`view()` 只在
/// 后台线程被调」。解码失败/超限/超预算的每一张都带自己的状态，不留白（D54 第 6 条）。
fn view(instance: &str, scan: &SavesScan, last_op: Option<SavesOped>) -> SavesListView {
    let mut screenshots: Vec<ScreenshotView> = scan
        .screenshots
        .iter()
        .map(|shot| ScreenshotView {
            file: shot.file.clone(),
            bytes: shot.bytes,
            modified_ms: shot.modified_ms,
            // 先摆占位，再让 thumbs 按 mtime 倒序挑前 60 张去解（顺序与上限都在那一个模块里）。
            preview: ScreenshotPreview::NotGenerated,
        })
        .collect();
    thumbs::attach(&scan.screenshots_dir, &mut screenshots);
    SavesListView {
        instance: instance.to_string(),
        dir: scan.dir.display().to_string(),
        exists: scan.exists,
        worlds: scan
            .worlds
            .iter()
            .map(|world| WorldView {
                dir: world.dir.clone(),
                icon: world.icon,
                modified_ms: world.modified_ms,
            })
            .collect(),
        screenshots_dir: scan.screenshots_dir.display().to_string(),
        screenshots_exists: scan.screenshots_exists,
        screenshots,
        skipped: scan
            .skipped
            .iter()
            .map(|skip| SavesSkipView {
                area: match skip.area {
                    saves::Area::Saves => SavesArea::Saves,
                    saves::Area::Screenshots => SavesArea::Screenshots,
                },
                file: skip.file.clone(),
                reason: match skip.reason {
                    saves::SkipKind::SymbolicLink => SavesSkipKind::SymbolicLink,
                    saves::SkipKind::NotADirectoryExpected => SavesSkipKind::NotADirectory,
                    saves::SkipKind::NotAFile => SavesSkipKind::NotAFile,
                    saves::SkipKind::UnsafeName => SavesSkipKind::UnsafeName,
                    saves::SkipKind::Unreadable => SavesSkipKind::Unreadable,
                    saves::SkipKind::NotAnImage => SavesSkipKind::NotAnImage,
                },
            })
            .collect(),
        last_op,
    }
}

pub(super) fn failure(
    instance: &str,
    kind: SavesFailureKind,
    detail: impl Into<String>,
) -> SavesFailureView {
    SavesFailureView {
        instance: instance.to_string(),
        file: String::new(),
        op: None,
        kind,
        detail: detail.into(),
    }
}

/// core 的错误 → 「目录不可用」+ 原文（**不翻译**，翻译在 app 层）。
fn unusable(instance: &str, error: &SavesError) -> SavesFailureView {
    failure(
        instance,
        SavesFailureKind::DirUnusable,
        format!("{}：{}", error.dir.display(), error.detail),
    )
}

#[cfg(test)]
mod tests;
