//! 整合包**导入 / 导出**（M113 `PACKUI`）：把 M111/M112 的管线接到内容库页上。
//!
//! ## 这里只做「解析目标 + 翻译 + 转发」
//!
//! 真正的活全在 core：`addon::import::import_pack`（三种格式 → 实例目录）与
//! `addon::export::{scan_instance, export_instance}`（实例目录 → `.mrpack` / CF 清单）。
//! 本模块不重新判定任何东西——三态、逐项原因、三分类全部**逐字段照搬** core 的报告
//! （[`convert`]），于是「UI 说的」与「管线干的」是同一份事实。
//!
//! ## 文件分工（守 400 行）
//!
//! | 文件 | 职责 |
//! |---|---|
//! | 本文件 | `CoreHost::pack` 落点 + 取消标记 + 三条 spawn 共用的目标解析 |
//! | [`import`] | 后台导入（探测 → 目标目录 → 管线 → 报告） |
//! | [`export`] | 后台预检（不写盘）与后台写盘 |
//! | [`convert`] | core 报告 → DTO（逐字段照搬）+ 小工具 |
//! | [`diagnose`] | 探测诊断：「看起来像 X，但缺 Y」 |
//!
//! ## 导出预检与写盘用同一套判据
//!
//! core 的 `export_instance` 没有 dry-run 入口（已报 finding）。预检用 `scan_instance` +
//! 「离线目录 ⇒ 一律内联」这条事实拼出来：`RemoteCatalog` 目前只有 `OfflineCatalog`，
//! 所以每个可扫文件都证明不了可被远程替代 ⇒ 内联；`SkippedLocal` 就是「无法导出」。
//! `tests::preview_counts_match_the_written_report` 用真管线钉住这个等式。
//!
//! ## 取消是**界面级**的
//!
//! [`cancel_pack`] 置模块级标记 + `dispatch` 里的 `CancelInstall` 把代次 +1：
//!
//! 1. 面板立刻收回（不是等后台回一句话）；
//! 2. 这一轮之后回来的事件因代次不符被 `handle` 丢掉；
//! 3. **后台那次导入会继续跑到结束**——core 的 `ImportRequest` 没有取消令牌，进度回调也拿不到
//!    控制流。所以「取消」的真实含义是「不再显示、结果作废」，不是「把它停手」。要真停必须给
//!    core 加 cancel token（已登记 finding，报告里同样划出来）；
//! 4. **提示语不能借用安装那条**（复审 r1 P1-2）：下载页的取消 toast 写着「实例未创建」，
//!    而导入会把实例**登记上**。所以 `cancel_pack()` 回一个「刚才有没有导入在跑」，
//!    `dispatch` 据此改发 [`crate::dto::PackEvent::Cancelled`]，界面上换成
//!    「导入会在后台继续跑完 —— 本次结果作废」这句。

mod convert;
mod diagnose;
mod export;
mod import;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use unlucky_core::addon::export::{read_instance_snapshot, InstanceSnapshot};
use unlucky_core::addon::pack::PackFormat;
use unlucky_core::game::install::instance::read_instance_config;
use unlucky_core::game::isolation::derive_game_dir;

use crate::dto::{
    CoreEvent, ExportRequestView, ImportProgressView, InstanceId, PackCommand, PackEvent,
    PackFormatView,
};

use super::super::env::HostEnv;
use super::super::job::{HostTask, JobSender};

/// pack 的**每 host** 状态：代次 + 在跑计数 + 本次运行的取消标记。
///
/// ## 为什么不是裸静态量（复审 r2 P3-6 要的那句回答）
///
/// 裸静态量是**跨 host 共享**的。生产路径只有一个 host 所以看不出来，但测试里两个 host 会互相
/// 干扰 —— 本轮实测撞到过：`host/cancel_tests` 派一次 `CancelInstall` 就把我这条用例在飞的导入
/// 事件按代次丢了。键取 `LauncherLayout::home()`（每个测试的临时根都不同），于是各自独立：
///
/// - **代次**：每个 host 一条，`handle` 只跟**自己这个 host** 的当前值比；
/// - **在跑计数**：一张计数表（每个 host 一格），由 [`InFlight`] 的 `Drop` 维护；
/// - **取消标记**：每个 host 一份 `Arc<AtomicBool>`，导入线程拿克隆，`cancel_pack` 置位。
///
/// 为什么不挪进 `CoreHost`：那要给 `host/mod.rs` 加字段（不在本 mission 的 scope）。这张表等价，
/// 只是把「每 host 的字段」放在模块里；`host/mod.rs` 一旦进 scope，搬过去只是把这张表删掉。
#[derive(Default, Clone, Copy)]
struct HostPackState {
    epoch: u64,
    in_flight: usize,
}

static PACK_STATE: Mutex<Vec<(PathBuf, HostPackState)>> = Mutex::new(Vec::new());
static PACK_CANCEL: Mutex<Vec<(PathBuf, Arc<AtomicBool>)>> = Mutex::new(Vec::new());

/// 这张表的键（数据根；`HostEnv::at_root` 的每个测试根都不同）。
fn host_key(env: &HostEnv) -> PathBuf {
    env.layout.home().to_path_buf()
}

fn with_state<T>(env: &HostEnv, body: impl FnOnce(&mut HostPackState) -> T) -> T {
    let key = host_key(env);
    let mut table = PACK_STATE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if let Some(index) = table.iter().position(|(name, _)| *name == key) {
        return body(&mut table[index].1);
    }
    table.push((key, HostPackState::default()));
    body(&mut table.last_mut().expect("刚插入").1)
}

/// 这个 host 当前的 pack 代次（`handle` 用它判断事件是否已被取消/关面板作废）。
pub(in crate::host) fn pack_epoch(env: &HostEnv) -> u64 {
    with_state(env, |state| state.epoch)
}

/// 作废这个 host 在飞的一切 pack 事件（取消 / 关面板都走它）。
pub(in crate::host) fn bump_pack_epoch(env: &HostEnv) -> u64 {
    with_state(env, |state| {
        state.epoch = state.epoch.wrapping_add(1);
        state.epoch
    })
}

/// 这个 host 现在有几条导入在跑。
pub(in crate::host) fn in_flight(env: &HostEnv) -> usize {
    with_state(env, |state| state.in_flight)
}

/// 「这次导入还在跑」的 **RAII 句柄**。
///
/// 复审 r2 P2-1 的教训：手工 `finished()` 只在 `block_on(import_pack(..))` 之后调用，
/// 而失败早退（探测失败 / 目标目录不可用）都在它之前 ⇒ 计数永远留在「在跑」，于是下一次
/// **下载页**的「取消安装」不再发 `InstallCancelled`，还给内容库画一句「导入还在后台跑」的假话。
///
/// 现在计数由句柄的 `Drop` 负责：**任何**退出路径（早退、报错、`?`、panic 展开）都会减一。
pub(super) struct InFlight {
    env: HostEnv,
}

impl InFlight {
    /// 计一次「在跑」。句柄活多久，这次导入就算跑多久。
    pub(super) fn begin(env: &HostEnv) -> Self {
        with_state(env, |state| state.in_flight += 1);
        Self { env: env.clone() }
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        with_state(&self.env, |state| {
            state.in_flight = state.in_flight.saturating_sub(1);
        });
    }
}

/// 取消这个 host 正在跑的导入（`CancelInstall` 与「关面板」都走它；没在跑时是 no-op）。
///
/// 返回值 = 「刚才确实有一次导入在跑」。调用方据此决定提示语：**导入的取消不是 install 的取消**
/// （后台会跑完并登记实例，见模块文档），所以不能复用下载页那条「安装已取消、实例未创建」。
pub(in crate::host) fn cancel_pack(env: &HostEnv) -> bool {
    let key = host_key(env);
    {
        let table = PACK_CANCEL
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some((_, cancel)) = table.iter().find(|(name, _)| *name == key) {
            cancel.store(true, Ordering::SeqCst);
        }
    }
    bump_pack_epoch(env);
    in_flight(env) > 0
}

/// 这个 host 新一次导入的取消标记（挂进表里供 [`cancel_pack`] 用）。
fn fresh_cancel(env: &HostEnv) -> Arc<AtomicBool> {
    let cancel = Arc::new(AtomicBool::new(false));
    let key = host_key(env);
    let mut table = PACK_CANCEL
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    match table.iter_mut().find(|(name, _)| *name == key) {
        Some((_, slot)) => *slot = Arc::clone(&cancel),
        None => table.push((key, Arc::clone(&cancel))),
    }
    cancel
}

/// `AppCommand::Pack` 的落点（M113）。
impl super::super::CoreHost {
    pub(crate) fn pack(&mut self, command: PackCommand) {
        let epoch = pack_epoch(&self.env);
        match command {
            PackCommand::Scan(request) => {
                // 预检只读：不点亮导入面板（那会让用户以为已经在装了），也不计"在跑导入"。
                let cancel = fresh_cancel(&self.env);
                import::spawn_scan(self.env.clone(), self.tx.clone(), *request, epoch, cancel);
            }
            PackCommand::Import(request) => {
                let cancel = fresh_cancel(&self.env);
                // 句柄在**派发点**就计上，随线程一起结束（含早退与 panic）
                let guard = InFlight::begin(&self.env);
                // 先把面板点亮：用户点了就有反应，不等后台回第一条进度
                let name = request
                    .instance_name
                    .clone()
                    .or_else(|| request.instance.clone())
                    .unwrap_or_default();
                self.send(CoreEvent::Pack(Box::new(PackEvent::Progress(Some(
                    Box::new(ImportProgressView::new(name)),
                )))));
                import::spawn_import(
                    self.env.clone(),
                    self.tx.clone(),
                    *request,
                    epoch,
                    cancel,
                    guard,
                );
            }
            PackCommand::Export(request) => {
                export::spawn_preview(self.env.clone(), self.tx.clone(), *request, epoch);
            }
            PackCommand::Confirm(request) => {
                export::spawn_write(self.env.clone(), self.tx.clone(), *request, epoch);
            }
            PackCommand::Dismiss => {
                // 只作废 pack 自己的代次：**不碰** `install_epoch`（那是下载页安装的命门）
                cancel_pack(&self.env);
                self.send(CoreEvent::Pack(Box::new(PackEvent::Cleared)));
            }
        }
    }
}

/// 后台任务 → host 的一跳（三个 spawn 共用）。
pub(super) fn send(tx: &JobSender, epoch: u64, event: PackEvent) {
    let _ = tx.send(HostTask::Pack {
        event: Box::new(event),
        epoch,
    });
}

/// 实例 → 运行目录（与 `addon/install.rs` 同一条派生：实例隔离档 > 全局档）。
pub(super) fn instance_root(env: &HostEnv, id: &InstanceId) -> Result<PathBuf, String> {
    let instance = super::super::game::instances(env)
        .into_iter()
        .find(|instance| &instance.id == id)
        .ok_or_else(|| format!("instance not found: {id}"))?;
    let global = super::super::game::global_mode(env);
    let config = read_instance_config(&env.layout, &instance.id)
        .map_err(|error| format!("read instance config failed: {error}"))?;
    derive_game_dir(
        &env.game,
        &instance.version_id,
        global,
        config.as_ref(),
        None,
    )
    .map(|derivation| derivation.game_dir)
    .map_err(|error| format!("derive game dir failed: {error}"))
}

/// 实例快照（清单的 `game` + `dependencies` 从它来）。读不出来就返回 `None`，
/// 由 core 自己按 `root/versions/` 再试一次（失败会给具名错误）。
pub(super) fn snapshot_of(env: &HostEnv, id: &InstanceId) -> Option<InstanceSnapshot> {
    let instance = super::super::game::instances(env)
        .into_iter()
        .find(|instance| &instance.id == id)?;
    // 隔离实例的 `versions/` 在全局根下，所以试全局根
    read_instance_snapshot(env.game.root(), Some(&instance.version_id)).ok()
}

/// 格式视图 → core 的静态格式（MultiMC 只读，不能当导出目标）。
pub(super) fn format_of(format: PackFormatView) -> Result<&'static dyn PackFormat, String> {
    use unlucky_core::addon::pack::{curseforge, modrinth};
    match format {
        PackFormatView::Modrinth => Ok(&modrinth::MODRINTH),
        PackFormatView::Curseforge => Ok(&curseforge::CURSEFORGE),
        PackFormatView::Multimc => {
            Err("MultiMC / Prism can only be imported, not exported".to_string())
        }
    }
}

/// 缺省落盘路径：`<data>/exports/<name>-<version>.<ext>`。
pub(super) fn default_export_path(env: &HostEnv, request: &ExportRequestView) -> PathBuf {
    let ext = match request.format {
        PackFormatView::Curseforge => "zip",
        _ => "mrpack",
    };
    let name = format!(
        "{}-{}",
        convert::slug(&request.name),
        convert::slug(&request.version)
    );
    env.layout
        .data()
        .join("exports")
        .join(format!("{name}.{ext}"))
}
