//! `ull instances adopt`：把盘上已有的版本登记成实例（M221）。
//!
//! **只读外部、只写自己**：写盘面只有 `<数据根>/.ULL/config/` 下的两份配置（+ 选中缺省运行目录时
//! 建那个空目录），`versions/` `libraries/` `assets/` 一个字节都不写。判据、落点与话术全在
//! [`unlucky_core::game::adopt`]；本模块只做「参数 → core 调用 → 人读/JSON 两条输出」。
//!
//! `--dry-run` 走 `adopt::plan`（零写盘），所以「先看一眼」不会改任何东西，也不会建目录。

use serde_json::{json, Value};
use unlucky_core::game::adopt::{self, AdoptRequest};
use unlucky_core::game::GameLayout;
use unlucky_core::launch::time::{format_rfc3339_millis, now};
use unlucky_core::platform::layout::LauncherLayout;

use crate::cli::model::InstancesAdoptArgs;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

use super::adopt_shared::{candidate_json, map_error};

/// `ull instances adopt <version-id> [--name N] [--isolation …] [--game-dir DIR] [--dry-run]`。
///
/// # Errors
///
/// 版本不在盘上 / 候选不合格 / 名字冲突 / 运行目录不可用 / 读配置失败，全部由
/// [`unlucky_core::game::adopt::AdoptError`] 映射成稳定错误码（`adopt_*`）。
pub fn run(ctx: &Ctx, args: &InstancesAdoptArgs) -> Result<Report, CliError> {
    let request = AdoptRequest {
        version_id: args.version_id.clone(),
        name: args.name.clone(),
        target: args.target.clone(),
        created_at: Some(format_rfc3339_millis(now())),
    };
    let (game, launcher) = (ctx.game(), ctx.layout());
    if args.dry_run {
        return dry_run(&game, &launcher, &request);
    }
    let outcome = adopt::adopt(&game, &launcher, &request).map_err(map_error)?;
    // 人读行先拼（只借用），再把字段移进 JSON（两条出口的内容同源、顺序不影响事实）。
    let mut human = vec![format!(
        "已登记实例 `{}`（id {}）→ 版本 {}",
        outcome.name, outcome.instance_id, outcome.version_id
    )];
    human.push(format!(
        "  隔离档 {}（运行目录来源 {}），运行目录 {}",
        outcome.isolation.as_str(),
        outcome.target.as_str(),
        outcome.run_dir.display()
    ));
    human.push(format!("  实例设置 {}", outcome.config_path.display()));
    if outcome.dir_created {
        human.push("  已新建该运行目录（空目录；玩家数据由游戏自己往里写）".to_string());
    }
    human.push(
        "  只写了本启动器的配置：versions/ libraries/ assets/ 一个字节都没动（不搬文件）"
            .to_string(),
    );
    human.extend(outcome.notes.iter().map(|note| format!("  提示：{note}")));
    let version_id = outcome.version_id.clone();
    let data = json!({
        "dry_run": false,
        "version_id": version_id,
        "instance": json!({
            "id": outcome.instance_id,
            "name": outcome.name,
            "version_id": version_id,
            "isolation": outcome.isolation.as_str(),
            "run_dir": outcome.run_dir.display().to_string(),
            "run_dir_source": outcome.target.as_str(),
            "config_path": outcome.config_path.display().to_string(),
            "created": outcome.created,
            "dir_created": outcome.dir_created,
        }),
        "notes": outcome.notes,
    });
    Ok(Report::new(data, human))
}

/// `--dry-run`：只算落点与判据，不写盘（连缺省运行目录都不建）。
fn dry_run(
    game: &GameLayout,
    launcher: &LauncherLayout,
    request: &AdoptRequest,
) -> Result<Report, CliError> {
    let plan = adopt::plan(game, launcher, request).map_err(map_error)?;
    let mut human = vec![format!(
        "检查通过：版本 {} 可登记（--dry-run：没有写任何文件）",
        plan.candidate.version_id
    )];
    human.push(format!(
        "  实例名 {}，隔离档 {}（运行目录来源 {}）",
        plan.name,
        plan.isolation.as_str(),
        plan.target.as_str()
    ));
    human.push(format!("  运行目录 {}", plan.run_dir.display()));
    human.extend(plan.notes.iter().map(|note| format!("  提示：{note}")));
    let version_id = plan.candidate.version_id.clone();
    let data = json!({
        "dry_run": true,
        "version_id": version_id,
        "candidate": candidate_json(&plan.candidate),
        "instance": json!({
            "id": Value::Null,
            "name": plan.name,
            "version_id": version_id,
            "isolation": plan.isolation.as_str(),
            "run_dir": plan.run_dir.display().to_string(),
            "run_dir_source": plan.target.as_str(),
            "config_path": Value::Null,
            "created": false,
            "dir_created": false,
        }),
        "notes": plan.notes,
    });
    Ok(Report::new(data, human))
}
