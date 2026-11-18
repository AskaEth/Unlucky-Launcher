//! `instances scan` / `instances adopt` 共用的 JSON 与错误投影。
//!
//! 两条命令输出同一种候选形状（`candidate` 对象），错误也走同一份映射——分两份写迟早漂。

use serde_json::{json, Value};
use unlucky_core::game::adopt::{AdoptCandidate, AdoptError, AdoptProblem};

use crate::error::CliError;

/// 候选 → `--json` 里的 `candidate` 对象。
pub(super) fn candidate_json(candidate: &AdoptCandidate) -> Value {
    json!({
        "version_id": candidate.version_id,
        "version_dir": candidate.version_dir.display().to_string(),
        "manifest_path": candidate.manifest_path.display().to_string(),
        "jar_present": candidate.jar_present,
        "manifest_id": candidate.manifest_id,
        "inherits": candidate.inherits,
        "registered": candidate.registered.iter().map(|item| json!({
            "id": item.id,
            "name": item.name,
        })).collect::<Vec<Value>>(),
        "adoptable": candidate.adoptable(),
        "problems": problems_json(&candidate.problems),
    })
}

/// 问题清单 → 结构化数组（`code` 稳定、`message` 人读）。
pub(super) fn problems_json(problems: &[AdoptProblem]) -> Value {
    Value::Array(
        problems
            .iter()
            .map(|problem| {
                json!({
                    "code": problem.code(),
                    "message": problem.to_string(),
                })
            })
            .collect(),
    )
}

/// `AdoptError` → CLI 错误：稳定 code + 逐条问题进 `detail`（脚本不必解析散文）。
pub(super) fn map_error(error: AdoptError) -> CliError {
    let code = error.code();
    let message = error.to_string();
    let cli = CliError::user(code, message);
    let hint = match &error {
        AdoptError::DuplicateName { .. } => {
            Some("换个展示名：%prog% instances adopt <version-id> --name <新名字>")
        }
        AdoptError::NotAdoptable { .. } => Some(
            "这些版本要么清单坏了、要么缺父版本、要么越界：先在原来的启动器里把它修好，\
             或 `%prog% install <version>` 让本启动器自己装一份",
        ),
        AdoptError::AlreadyRegistered { .. } => {
            Some("用 `%prog% instances list` 看已有实例；要第二个实例请换 --name")
        }
        AdoptError::TargetNotEmpty { .. } => {
            Some("换一个名字（缺省目录按名字派生），或用 --isolation custom --game-dir <空目录>")
        }
        AdoptError::UnknownVersion { .. } => Some("先看有哪些可用：%prog% instances scan"),
        AdoptError::NoFreeRunDir { .. } => Some(
            "换一个 --name（缺省目录按名字派生），或用 --isolation custom --game-dir <空目录>",
        ),
        AdoptError::WriteSurfaceInGameTree { .. } => Some(
            "数据目录（.ULL / --data-dir）和游戏树重叠了：把它指到游戏树之外的目录再登记             （整体迁移到别处是允许的，默认登记不会把实例数据写进别人的 .minecraft）",
        ),
        AdoptError::ConfigSurfaceUnsafe { .. } => Some(
            "数据根里的 config/ 或 config/instances/ 被链到别处了：先把它恢复成真实目录再登记             （整体迁移请把整个 .ULL 目录一起搬走，别只链其中一层）",
        ),
        AdoptError::VersionsDirUnreadable { .. } => Some(
            "游戏根/versions 入口存在但读不出来：检查它是不是断链的符号链接/目录联接，             或把它指回真实目录后再扫",
        ),
        _ => None,
    };
    let cli = match hint {
        Some(hint) => cli.with_hint(hint),
        None => cli,
    };
    cli.with_detail(json!({
        "problems": problems_json(error.problems()),
    }))
}
