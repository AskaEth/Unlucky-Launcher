//! `ull instances scan`：列出当前游戏根 `versions/` 里**已经铺好**的版本，逐项说清能不能登记（M221）。
//!
//! 只读：不写任何文件、不改任何 mtime。用途是「我盘上已经有别的启动器装好的版本，看看 ULL 认不认」，
//! 以及 `instances adopt` 的前置检查（同一套判据，见 [`unlucky_core::game::adopt::scan`]）。

use serde_json::json;
use unlucky_core::game::adopt;

use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

use super::adopt_shared::candidate_json;

/// `ull instances scan`。
///
/// # Errors
///
/// `versions/` 或 `profiles.json` 读不了时返回用户错误（**不降级成空列表**）。
pub fn run(ctx: &Ctx) -> Result<Report, CliError> {
    let scan = adopt::scan(&ctx.game(), &ctx.layout()).map_err(super::adopt_shared::map_error)?;
    let ready = scan
        .candidates
        .iter()
        .filter(|candidate| candidate.adoptable())
        .count();
    let registered = scan
        .candidates
        .iter()
        .filter(|candidate| candidate.is_registered())
        .count();

    let candidates: Vec<serde_json::Value> = scan.candidates.iter().map(candidate_json).collect();
    let data = json!({
        "game_root": ctx.data.game_home.display().to_string(),
        "versions_dir": scan.versions_dir.display().to_string(),
        "counts": {
            "total": scan.candidates.len(),
            "adoptable": ready,
            "registered": registered,
        },
        "candidates": candidates,
    });

    let mut human = vec![format!(
        "游戏根 {}：versions/ 里 {} 个版本，{} 个可登记",
        scan.versions_dir.display(),
        scan.candidates.len(),
        ready
    )];
    if scan.candidates.is_empty() {
        human.push(
            "  （空）这个游戏根下还没有任何版本；`%prog% install <version>` 可以自己装一份"
                .to_string(),
        );
    }
    for candidate in &scan.candidates {
        if candidate.adoptable() {
            let state = if candidate.is_registered() {
                "已登记"
            } else {
                "可登记"
            };
            human.push(format!(
                "  ✓ {}  [{state}]{}",
                candidate.version_id,
                chain_suffix(candidate)
            ));
        } else {
            human.push(format!("  ✗ {}", candidate.version_id));
            for problem in &candidate.problems {
                human.push(format!("      原因[{0}] {problem}", problem.code()));
            }
        }
    }
    if ready > 0 {
        human.push(
            "登记一个：%prog% instances adopt <version-id>（缺省落 ULL 数据区的独立运行目录）"
                .to_string(),
        );
    }
    Ok(Report::new(data, human))
}

/// 人读行尾的父链提示（`1.12.2-forge-…` 后面标出它继承的是谁）。
fn chain_suffix(candidate: &unlucky_core::game::AdoptCandidate) -> String {
    match candidate.inherits.len() {
        0 | 1 => String::new(),
        _ => format!("  继承 {}", candidate.inherits[1..].join(" -> ")),
    }
}
