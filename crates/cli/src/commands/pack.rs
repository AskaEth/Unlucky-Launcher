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
//! | `failed` | `1` | 致命失败，**三档**（见下）——不是"一律已整包回滚" |
//! | `cancelled` | `130` | 用户取消（管线在检查点停手并回滚，**不是失败**） |
//!
//! `failed` 的三档（判据在 [`lines`] 的 `rollback::rollback_stage`，出词在 `status_label` 与
//! `rollback_line`）：**一个字节都没写**（预检/装配失败，`rolled_back` 为真却**没有**回滚记录）/
//! **已整包回滚**（写过，回滚完整）/ **回滚没做完**（写过，有没回去的项）。M161 之前这一格写的是
//! "已整包回滚"，对第一档是假话；人读那一行也跟着分三句。
//!
//! `130`（= `128 + SIGINT`）是"被用户中断"的惯例码：CLI 这一轮没有取消入口（Ctrl-C 直接杀进程），
//! 但 `ImportStatus::Cancelled` 是 core 的公开结果，脚本读 `--json` 时要能把它与 `failed` 分开。
//!
//! `--json` 的信封在三种状态下**都是** `{"ok":true,"data":{…}}`（命令跑完了），分流看
//! `data.status` / `data.error.code` 或退出码；`ok:false` 只用于"命令没跑起来"（参数错、数据根不可用）。
//!
//! # CurseForge 的 Key（M318）
//!
//! CLI 与 GUI 用**同一处**判据（`core::addon::curseforge::auth::resolve_key`：设置
//! `online.curseforge.api_key` 优先、`ULL_CF_API_KEY` 兜底、空白算没配）——[`fetcher_for`] 把
//! 结果交给 core 的 fetcher。**没配不是失败**：那条 CF 条目记为「缺文件 + 缺 Key」，其余文件照装，
//! 整包 `partial`（core 的 `import_curseforge_key` 用例钉着这一档）。

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::json;
use unlucky_core::addon::curseforge::auth::{resolve_key, API_KEY_ENV};
use unlucky_core::addon::curseforge::ApiKey;
use unlucky_core::addon::import::{
    import_pack, ConflictPolicy, DownloadEngineFetcher, EngineLoaderInstaller, FileAction,
    ImportReport, ImportRequest, ImportStage,
};
use unlucky_core::addon::loader::processors::{StepOutcome, StepSink};
use unlucky_core::download::blocking::block_on;
use unlucky_core::download::DownloadEngine;
use unlucky_core::game::install::pack_dir;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::cli::PackImportArgs;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{progress_line, Outcome, Report};

use lines::{
    conflict_lines, count_action, missing_lines, processor_lines, rejected_lines, rollback_line,
    stage_ids, stages_line, status_label, warnings_lines,
};

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

    let stages: Arc<Mutex<Vec<ImportStage>>> = Arc::new(Mutex::new(Vec::new()));
    // **M321 r1**：进度回调搬到 `lines::progress_sink`（可测），这里只给它真 stderr 这个出口。
    // 除了阶段名，它还会印装载器段的逐文件行——原版 + assets 那 6 分钟以前是彻底黑屏的。
    let out: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(progress_line);
    let sink = lines::progress_sink(Arc::clone(&stages), out);
    // **M321**：processor 链的逐步骤行（stderr 实时）。以前这条路径把 sink 传成 `None`
    // ——Forge / NeoForge 的六步串行跑了四十秒，`stderr` 一个字节都没有，用户只能干等。
    // 形状照 `ull install --loader`（`install/loader_processors.rs` 的 `step_sink`）。
    let step_sink: StepSink = Arc::new(move |outcome: &StepOutcome| {
        progress_line(&lines::step_line(outcome));
    });

    let engine = build_engine(ctx);
    let game = ctx.game();
    // **M311 第六跳**：清单声明了加载器就真的装（与 GUI 同一套 core 实现）。
    let installer = EngineLoaderInstaller::new(&engine, &layout);
    let fetcher = fetcher_for(&engine, ctx);
    let mut request = ImportRequest::new(&args.path, &target)
        .with_fetcher(&fetcher)
        .with_loader_install(&installer, &game)
        .with_progress(sink)
        .with_steps(step_sink);
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

/// CF Key 的设置键（键名常量只在 core 的 schema 登记处写一次，与 GUI 同一个）。
const CF_KEY: &str = unlucky_core::setting::keys::online::CURSEFORGE_API_KEY;

/// 本次导入用的 CurseForge Key：**设置优先、环境变量兜底**。
///
/// 判据不在本文件：`core::addon::curseforge::auth::resolve_key` 是那条优先级的唯一表达，
/// 本函数只做「从 CLI 自己的 `launcher.json` 取裸值 + 读一次环境变量」。GUI 走同一个函数，
/// 所以同一份配置不会出现「设置里填了、命令行说没配」（M318 之前正是这样：CLI 构造 fetcher
/// 用的是 `new()`，连 Key 都不看，CF 条目一律记缺）。
///
/// 空白 / 空串算「没配」由 [`resolve_key`] 那一档定（`ApiKey::new` 的既有判据），本文件不重复。
fn curseforge_key(ctx: &Ctx) -> Option<ApiKey> {
    let store = SettingStore::load(ctx.launcher_config(), launcher_schema());
    let doc = store.doc();
    resolve_key(
        doc.effective_str(CF_KEY),
        std::env::var(API_KEY_ENV).ok().as_deref(),
    )
}

/// 绑好引擎与 CF Key 的 fetcher（与 presenter 的 `host::addon::pack::import::fetcher_for` 同形）。
///
/// **缺 Key 不是错误**：core 的 `curse_file` 会回具名的 `NotConfigured`，那一条进报告的
/// `missing`（其余条目照装，整包 `Partial`）。Key 只在拼 CF 请求头时取值，本函数不打日志。
fn fetcher_for<'a>(engine: &'a DownloadEngine, ctx: &Ctx) -> DownloadEngineFetcher<'a> {
    let fetcher = DownloadEngineFetcher::new(engine);
    match curseforge_key(ctx) {
        Some(key) => fetcher.with_curse_key(key),
        None => fetcher,
    }
}

/// 组装 `Outcome`（三态 → 退出码 + 两种输出）。
fn outcome(
    ctx: &Ctx,
    report: &ImportReport,
    target: &Path,
    stages: &[ImportStage],
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
        object.insert("stages".to_string(), json!(stage_ids(stages)));
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
    if let Some(line) = stages_line(stages) {
        // **M324**：这行以前直接 `join` 核心的内部 id（`阶段：probe → loader`），现在走目录文案。
        human.push(line);
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
            if !report.loader_installed && !ctx.game().is_version_installed(version_id) {
                human.push(format!(
                    "提示：{version_id} 的加载器还没装，启动前先跑 `%prog% install` 装它"
                ));
            }
        }
    }
    // **M321**：第六跳的 processor 链与告警都要印出来。渲染的是数据层（`report.processors` /
    // `report.warnings`），CLI 只负责排版——GUI 读同一份数据，两边不可能各说一套。
    human.extend(processor_lines(report));
    human.extend(warnings_lines(report));
    if let Some(memory) = report.max_memory_mb {
        human.push(format!("内存：{memory} MB（来自 instance.cfg）"));
    }
    // **M323**：这三段（未支持组件 / 冲突 / 缺文件）以前把 core 的稳定 key 原样印进人读输出
    // （`（pack.import.missing.fetch_failed：…）`）。渲染搬到 `lines`，与告警段同一套取词。
    human.extend(rejected_lines(report));
    human.extend(conflict_lines(report));
    human.extend(missing_lines(report));
    if let Some(registered) = &report.registered {
        // M346：与 `install` / `instances adopt` 同一份实现（`wording::registered_line`），
        // 前半句取目录 `PackReportRegistered`（界面读的同一句）。
        human.push(wording::registered_line(
            &registered.name,
            &registered.id,
            &registered.version_id,
            registered.created,
        ));
    }
    if let Some(error) = &report.error {
        human.push(format!("错误：{}（{}）", error.message(), error.code()));
        if let Some(hint) = error.hint() {
            human.push(format!("提示：{hint}"));
        }
        human.push(rollback_line(report));
    }

    // **必须走 `Report::new`**：它才是「人读行渲染掉 `%prog%` 占位符」的那一步
    // （`cli::program::PLACEHOLDER`）。用结构体字面量绕过它，`pack import` 就会把
    // `%prog% install` 原样打给用户（M318 真包盘查的人读输出里就是这样）。
    Outcome::new(Report::new(data, human)).with_exit_code(report.status.exit_code())
}

/// 与 `install` / `java` 共用同一套引擎装配（镜像 / 地区 / 健康度 / 内容缓存），
/// 参数来自设置：读数口径见 [`crate::download`]。
pub(crate) fn build_engine(ctx: &Ctx) -> Arc<DownloadEngine> {
    crate::download::engine(ctx)
}

#[path = "pack/lines.rs"]
mod lines;

/// 三个命令共用的两句人读话（单步结局 / 实例登记结果）：`pub(crate)` 让 `install` 与
/// `instances` 也走同一份实现。为什么住在 `pack/` 见文件头。
#[path = "pack/wording.rs"]
pub(crate) mod wording;

#[cfg(test)]
#[path = "pack/tests.rs"]
mod tests;
