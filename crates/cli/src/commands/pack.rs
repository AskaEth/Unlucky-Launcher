//! `pack import <file>`：整合包导入（M111）。
//!
//! 逻辑**全在** `core::addon::import`：本文件只做三件事——参数落地、下载引擎接线、两种输出。
//! 之所以坚持这一层很薄，是因为 M113 的导入页要复用的是 core 的 `import_pack` + `ImportReport`，
//! 不是这里的打印逻辑。
//!
//! # 三态退出码（脚本契约）
//!
//! | `data.status` | 退出码 | 含义 |
//! |---------------|--------|------|
//! | `success` | `0` | 全部落地、无冲突 |
//! | `partial` | `3` | 有缺文件或有保留用户文件的冲突（实例可用但不完整） |
//! | `failed` | `1` | 致命失败，**已整包回滚** |
//!
//! `--json` 的信封在三种状态下**都是** `{"ok":true,"data":{…}}`（命令跑完了），分流看
//! `data.status` / `data.error.code` 或退出码；`ok:false` 只用于"命令没跑起来"（参数错、
//! 数据根不可用），与其它命令一致。

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::json;
use unlucky_core::addon::import::{
    import_pack, ConflictChoice, ConflictPolicy, DownloadEngineFetcher, FileAction, ImportProgress,
    ImportReport, ImportRequest, ImportStatus,
};
use unlucky_core::download::blocking::block_on;
use unlucky_core::download::cache::now_ms;
use unlucky_core::download::region::{decide_from, load_cached, store_cached};
use unlucky_core::download::region_system::detect_signals;
use unlucky_core::download::{
    ContentCache, DownloadEngine, EngineConfig, HealthRegistry, MirrorTable,
};

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
    let target = args
        .target
        .clone()
        .unwrap_or_else(|| ctx.game().root().to_path_buf());

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
    let layout = ctx.layout();
    let mut request = ImportRequest::new(&args.path, &target)
        .with_fetcher(&fetcher)
        .with_progress(sink);
    if args.overwrite {
        request = request.with_conflict_policy(ConflictPolicy::Overwrite);
    }
    if !args.no_register {
        request = request.with_registration(&layout, args.name.as_deref());
    }

    let report = block_on(import_pack(&request));
    let seen = stages.lock().map(|seen| seen.clone()).unwrap_or_default();
    Ok(outcome(ctx, &report, &target, &seen, args))
}

/// 组装 `Outcome`（三态 → 退出码 + 两种输出）。
fn outcome(
    ctx: &Ctx,
    report: &ImportReport,
    target: &Path,
    stages: &[&'static str],
    args: &PackImportArgs,
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
            "（缺省 = 数据根的共享游戏目录）"
        } else {
            ""
        }
    ));
    if !stages.is_empty() {
        human.push(format!("阶段：{}", stages.join(" → ")));
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
        if report.rolled_back {
            human.push("已整包回滚：目标目录与导入前一致".to_string());
        } else if let Some(rollback) = &report.rollback {
            human.push(format!(
                "回滚未完整：{} 项没回去（详见 --json 的 rollback.failed）",
                rollback.failure_count()
            ));
        }
    }

    Outcome::new(Report { data, human }).with_exit_code(report.status.exit_code())
}

/// 状态的人读名。**失败要按回滚是否干净分叉**：不完整时不能说"已回滚"。
fn status_label(report: &ImportReport) -> &'static str {
    match report.status {
        ImportStatus::Success => "成功",
        ImportStatus::Partial => "部分成功",
        ImportStatus::Failed if report.rolled_back => "失败（已回滚）",
        ImportStatus::Failed => "失败（回滚不完整）",
    }
}

fn count_action(report: &ImportReport, action: FileAction) -> usize {
    report
        .files
        .iter()
        .filter(|file| file.action == action)
        .count()
}

/// 与 `install` 共用同一套引擎构造（镜像 / 地区 / 健康度 / 内容缓存）。
fn build_engine(ctx: &Ctx) -> DownloadEngine {
    let layout = ctx.layout();
    let cache_root = layout.cache();
    let now = now_ms();
    let region_path = cache_root.join("region.json");
    let region = load_cached(&region_path, now).unwrap_or_else(|| {
        let profile = decide_from(&detect_signals(), now);
        let _ = store_cached(&region_path, &profile);
        profile
    });
    DownloadEngine::new(
        EngineConfig::default(),
        Arc::new(MirrorTable::builtin()),
        Arc::new(region),
        Arc::new(HealthRegistry::open_in(&cache_root, now)),
        Arc::new(ContentCache::open(&cache_root)),
    )
}
