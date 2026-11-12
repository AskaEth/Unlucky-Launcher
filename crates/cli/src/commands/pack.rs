//! `pack import <file>`：整合包导入（M111）。
//!
//! 逻辑**全在** `core::addon::import`：本文件只做参数落地、引擎接线、两种输出。坚持这一层很薄，
//! 是因为 M113 的导入页要复用的是 core 的 `import_pack` + `ImportReport`，不是这里的打印逻辑。
//!
//! # 三态退出码（脚本契约）
//!
//! | `data.status` | 退出码 | 含义 |
//! |---------------|--------|------|
//! | `success` | `0` | 全部落地、无冲突 |
//! | `partial` | `3` | 有缺文件或有保留用户文件的冲突（实例可用但不完整） |
//! | `failed` | `1` | 致命失败，**已整包回滚** |
//! | `cancelled` | `130` | 用户取消（管线在检查点停手并回滚，**不是失败**） |
//!
//! `130`（= `128 + SIGINT`）是"被用户中断"的惯例码：CLI 这一轮没有取消入口（Ctrl-C 直接杀进程），
//! 但 `ImportStatus::Cancelled` 是 core 的公开结果，脚本读 `--json` 时要能把它与 `failed` 分开。
//!
//! `--json` 的信封在三种状态下**都是** `{"ok":true,"data":{…}}`（命令跑完了），分流看
//! `data.status` / `data.error.code` 或退出码；`ok:false` 只用于"命令没跑起来"（参数错、数据根不可用）。

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::json;
use unlucky_core::addon::import::{
    import_pack, ConflictChoice, ConflictPolicy, DownloadEngineFetcher, FileAction, ImportProgress,
    ImportReport, ImportRequest, ImportStatus,
};
use unlucky_core::download::blocking::block_on;
use unlucky_core::download::DownloadEngine;
use unlucky_core::game::install::pack_dir;

use crate::cli::PackImportArgs;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{Outcome, Report};

/// `ull pack import`。
///
/// # Errors
///
/// 整合包路径不存在（用户错误）。导入本身的失败**不是** `Err`：它走三态报告 + 退出码。
pub fn run(ctx: &Ctx, args: &PackImportArgs) -> Result<Outcome, CliError> {
    if !args.path.exists() {
        return Err(CliError::user(
            "not_found",
            format!("整合包不存在：{}", args.path.display()),
        )
        .with_hint("给 .zip / .mrpack 文件，或 MultiMC·Prism 的实例目录"));
    }
    let layout = ctx.layout();
    // **M216 ②**：缺省落点 = `<data>/instances/<实例名>`（与 GUI 同源，判据在 core 的
    // `pack_dir::import_target`），不再是共享的 `.minecraft` 根。
    let (target, instance_name) = pack_dir::import_target(
        &layout,
        &args.path,
        args.target.as_deref(),
        args.name.as_deref(),
    )
    .map_err(|reason| {
        CliError::user("target_exists", reason).with_hint(
            "加 --name 换个实例名，或显式给 --target（--overwrite 只管文件冲突，绕过不了这条预检）",
        )
    })?;

    let stages: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&stages);
    let sink: unlucky_core::addon::ImportProgressSink =
        Arc::new(move |progress: &ImportProgress| {
            if let Ok(mut seen) = recorder.lock() {
                if seen.last().copied() != Some(progress.stage.as_str()) {
                    seen.push(progress.stage.as_str());
                }
            }
        });

    let engine = build_engine(ctx);
    let fetcher = DownloadEngineFetcher::new(&engine);
    let mut request = ImportRequest::new(&args.path, &target)
        .with_fetcher(&fetcher)
        .with_progress(sink);
    if args.overwrite {
        request = request.with_conflict_policy(ConflictPolicy::Overwrite);
    }
    if !args.no_register {
        // 登记名与目录名同源（GUI 也是这样：`instance_name` 缺省用包名）。
        request = request.with_registration(&layout, Some(&instance_name));
    }

    let report = block_on(import_pack(&request));
    // **§5**：缺省建出来的实例要绑到它的内容目录（判据/文案在 core，与 GUI 同一份），否则
    // 「导入成功」而那批 mods/config 永远不会被那条实例的启动用到。
    let auto = args.target.is_none() && !args.no_register;
    let binding = pack_dir::bind_created_instance(&layout, &target, auto, &report);
    let seen = stages.lock().map(|seen| seen.clone()).unwrap_or_default();
    Ok(outcome(
        ctx,
        &report,
        &target,
        &seen,
        args,
        binding.as_ref(),
    ))
}

/// 组装 `Outcome`（三态 → 退出码 + 两种输出）。
fn outcome(
    ctx: &Ctx,
    report: &ImportReport,
    target: &Path,
    stages: &[&'static str],
    args: &PackImportArgs,
    binding: Option<&Result<String, String>>,
) -> Outcome {
    let mut data = serde_json::to_value(report).unwrap_or_else(|_| {
        json!({
            "status": report.status.as_str(),
            "error": {"code": "serialize", "message": "报告序列化失败"},
        })
    });
    if let Some(object) = data.as_object_mut() {
        object.insert("target".to_string(), json!(target.display().to_string()));
        object.insert("stages".to_string(), json!(stages));
        object.insert("status_key".to_string(), json!(report.status.message_key()));
        object.insert("installed_files".to_string(), json!(report.installed()));
        object.insert(
            "target_is_default".to_string(),
            json!(args.target.is_none()),
        );
    }

    let mut human: Vec<String> = Vec::new();
    human.push(format!(
        "整合包 {}（{}）→ {}{}",
        report.pack_name.as_deref().unwrap_or("<未读成>"),
        report.format.as_deref().unwrap_or("-"),
        target.display(),
        if args.target.is_none() {
            "（缺省 = 新实例目录 <数据根>/instances/）"
        } else {
            ""
        }
    ));
    if !stages.is_empty() {
        human.push(format!("阶段：{}", stages.join(" → ")));
    }
    if let Some(binding) = binding {
        // **M216 r2**：`--json` 里给**结构化成败**（r1 只印人读行，脚本看不到成败）。
        if let Some(object) = data.as_object_mut() {
            object.insert("instance_bound".to_string(), json!(binding.is_ok()));
        }
        human.push(match binding {
            Ok(note) | Err(note) => note.clone(),
        });
    }
    human.push(format!(
        "结果：{}  文件 新{} / 覆盖{} / 跳过{} / 保留{}  冲突 {}  缺文件 {}",
        status_label(report),
        count_action(report, FileAction::Installed),
        count_action(report, FileAction::Replaced),
        count_action(report, FileAction::SkippedIdentical),
        count_action(report, FileAction::KeptUser),
        report.conflicts.len(),
        report.missing.len(),
    ));
    if let Some(loader) = &report.loader {
        human.push(format!(
            "加载器：{} {}（版本 id {}）",
            loader.kind.label(),
            loader.loader_version,
            loader.version_id
        ));
        if let Some(version_id) = &report.version_id {
            if !ctx.game().is_version_installed(version_id) {
                human.push(format!(
                    "提示：{version_id} 还没安装，启动前先跑 `%prog% install` 装它"
                ));
            }
        }
    }
    if let Some(memory) = report.max_memory_mb {
        human.push(format!("内存：{memory} MB（来自 instance.cfg）"));
    }
    for component in &report.rejected_components {
        human.push(format!(
            "未支持的 component：{} {}（{}）",
            component.uid,
            component.version.as_deref().unwrap_or("-"),
            component.message_key
        ));
    }
    for conflict in &report.conflicts {
        human.push(format!(
            "冲突：{}（{}，已{}）",
            conflict.path,
            conflict.message_key,
            match conflict.choice {
                ConflictChoice::KeepUser => "保留你的文件",
                ConflictChoice::Overwrite => "按包覆盖",
            }
        ));
    }
    for missing in &report.missing {
        human.push(format!(
            "缺文件：{}（{}：{}）",
            missing.display, missing.message_key, missing.detail
        ));
    }
    if let Some(registered) = &report.registered {
        human.push(format!(
            "已登记实例 {}（{}）{}",
            registered.name,
            registered.version_id,
            if registered.created {
                ""
            } else {
                "·复用已有档案"
            }
        ));
    }
    if let Some(error) = &report.error {
        human.push(format!("错误：{}（{}）", error.message(), error.code()));
        if let Some(hint) = error.hint() {
            human.push(format!("提示：{hint}"));
        }
        human.push(rollback_line(report));
    }

    Outcome::new(Report { data, human }).with_exit_code(report.status.exit_code())
}

/// 状态的人读名。**失败要按「动没动过目标目录」分叉**（见 [`rollback_line`]）。
fn status_label(report: &ImportReport) -> &'static str {
    match rollback_stage(report) {
        Some(RollbackStage::NotStarted) => "失败（未写入：目标目录没被动过）",
        Some(RollbackStage::RolledBack) => "失败（已回滚）",
        Some(RollbackStage::Incomplete) => "失败（回滚不完整）",
        None => match report.status {
            ImportStatus::Success => "成功",
            // 取消**不是失败**：不说"失败"，也不说"部分成功"（那会让人以为装了一半）。
            // 但回滚不完整时也不许说"与导入前一致"——与失败路径同一套诚实口径。
            ImportStatus::Cancelled => {
                if report.rolled_back {
                    "已取消（目标目录与导入前一致）"
                } else {
                    "已取消（回滚不完整，目标目录可能与导入前不同）"
                }
            }
            _ => "部分成功",
        },
    }
}

/// 「这次失败到底动没动过目标目录」（三档）。
///
/// 与 `presenter` 的 `host::addon::pack::convert::failure_stage` **同义**——CLI 不依赖 presenter
/// 层（`unlucky-cli` 只依赖 core + agent），所以这套判据在这里各写一份。已登记 finding：
/// 更干净的做法是把三档挪进 `core::addon::import::ImportReport`（那儿有 `rollback` /
/// `rolled_back` 两个机器位），两处都改成读它。
///
/// 为什么要三档而不是一个 `rolled_back` 布尔：core 的 `rolled_back` 在「回滚完整完成」与
/// **「从未写过目标目录」**两种情形下都是 `true`。只按它出词，预检失败会被说成「已整包回滚：目标
/// 目录与导入前一致」——那句话在**一个字节都没写**时是假的（M161 复审的 P1 同族问题）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RollbackStage {
    /// 预检/装配阶段就失败：目标目录一个字节都没写
    NotStarted,
    /// 写过，回滚**完整**完成
    RolledBack,
    /// 写过，回滚有失败项（目标目录可能与导入前不同）
    Incomplete,
}

/// 三档判据（与 presenter 那份逐条对应）。
fn rollback_stage(report: &ImportReport) -> Option<RollbackStage> {
    if report.status != ImportStatus::Failed {
        return None;
    }
    Some(match &report.rollback {
        Some(outcome) if outcome.is_complete() => RollbackStage::RolledBack,
        Some(_) => RollbackStage::Incomplete,
        None if report.rolled_back => RollbackStage::NotStarted,
        None => RollbackStage::Incomplete,
    })
}

/// 失败收尾那一段人读文案（**三档三句**，不许共用）。
fn rollback_line(report: &ImportReport) -> String {
    match rollback_stage(report) {
        Some(RollbackStage::NotStarted) => {
            "预检未通过：目标目录一个字节都没写（不需要回滚）".to_string()
        }
        Some(RollbackStage::RolledBack) => "已整包回滚：目标目录与导入前一致".to_string(),
        Some(RollbackStage::Incomplete) => match &report.rollback {
            Some(rollback) => format!(
                "回滚未完整：{} 项没回去（详见 --json 的 rollback.failed）",
                rollback.failure_count()
            ),
            None => "回滚状态不明：目标目录可能与导入前不同（详见 --json）".to_string(),
        },
        None => String::new(),
    }
}

fn count_action(report: &ImportReport, action: FileAction) -> usize {
    report
        .files
        .iter()
        .filter(|file| file.action == action)
        .count()
}

/// 与 `install` / `java` 共用同一套引擎装配（镜像 / 地区 / 健康度 / 内容缓存），
/// 参数来自设置：读数口径见 [`crate::download`]。
pub(crate) fn build_engine(ctx: &Ctx) -> Arc<DownloadEngine> {
    crate::download::engine(ctx)
}

#[cfg(test)]
mod tests {
    //! `pack import` 失败收尾的三档文案（M161 复审「不计入但该记」第一条：CLI 也曾把
    //! 预检失败说成「已整包回滚」）。
    //!
    //! 抓得住：三档塌成两句（预检 → 已回滚）；`is_complete()` 被丢掉；`status != Failed` 也出话。

    use std::path::PathBuf;

    use unlucky_core::addon::import::{ImportError, ImportReport, ImportStatus, RollbackOutcome};

    use super::*;

    fn failed() -> ImportReport {
        let mut report = ImportReport::empty(PathBuf::from("C:/tmp/.minecraft"));
        report.fail(ImportError::integrity("sha1 mismatch"));
        report
    }

    /// 三档三句，**互不相同**（同族的谎就是这个 mission 要消灭的那类）。
    #[test]
    fn the_three_rollback_stages_get_three_different_sentences() {
        // ① 预检没过：一个字节都没写。
        let mut preflight = failed();
        preflight.rolled_back = true;
        assert_eq!(rollback_stage(&preflight), Some(RollbackStage::NotStarted));
        let not_started = rollback_line(&preflight);
        assert!(not_started.contains("没写"), "{not_started}");
        assert!(
            !not_started.contains("回滚：目标目录"),
            "预检失败不许说「已回滚」：{not_started}"
        );
        assert_eq!(status_label(&preflight), "失败（未写入：目标目录没被动过）");

        // ② 写过又完整回滚。
        let mut rolled_back = failed();
        rolled_back.rolled_back = true;
        rolled_back.rollback = Some(RollbackOutcome {
            attempted: 2,
            failed: Vec::new(),
        });
        assert_eq!(
            rollback_stage(&rolled_back),
            Some(RollbackStage::RolledBack)
        );
        assert!(rollback_line(&rolled_back).contains("已整包回滚"));
        assert_eq!(status_label(&rolled_back), "失败（已回滚）");

        // ③ 回滚没做完。
        let mut incomplete = failed();
        incomplete.rolled_back = false;
        incomplete.rollback = Some(RollbackOutcome {
            attempted: 2,
            failed: vec!["mods/a.jar".to_string()],
        });
        assert_eq!(rollback_stage(&incomplete), Some(RollbackStage::Incomplete));
        assert!(rollback_line(&incomplete).contains("1 项没回去"));
        assert_eq!(status_label(&incomplete), "失败（回滚不完整）");

        // 三句必须互不相同（把任何两档并成一句，这里就红）。
        let lines = [
            rollback_line(&preflight),
            rollback_line(&rolled_back),
            rollback_line(&incomplete),
        ];
        assert_eq!(
            lines.iter().collect::<std::collections::HashSet<_>>().len(),
            3,
            "{lines:?}"
        );
    }

    /// 不是致命失败 ⇒ 没有回滚那一段话（成功/部分成功不该出现「已回滚」）。
    #[test]
    fn a_non_fatal_report_says_nothing_about_rollback() {
        let mut report = ImportReport::empty(PathBuf::from("C:/tmp"));
        report.status = ImportStatus::Partial;
        assert_eq!(rollback_stage(&report), None);
        assert_eq!(rollback_line(&report), "");
        assert_eq!(status_label(&report), "部分成功");
    }
}
