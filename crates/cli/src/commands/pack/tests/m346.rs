//! M346 r2 P2-1：`pack import` 登记行的**实参映射**（名称 / 实例 id / 版本 id / `created`）。
//!
//! 为什么单开这一条：`pack/wording.rs` 的用例钉的是那句模板本身，钉不住**调用方传对了字段**
//! ——把 `pack.rs::outcome` 里那三个实参换个位，全仓 20 行 `test result` 照样全绿（r2 复审原话）。

use std::path::PathBuf;

use unlucky_core::addon::import::{ImportReport, RegisteredInstance};

use super::super::outcome;
use super::{ctx_at, TempDir};

use crate::cli::PackImportArgs;

/// 一份不落盘、不联网的 `pack import` 参数。
fn args() -> PackImportArgs {
    PackImportArgs {
        path: PathBuf::from("pack.mrpack"),
        target: None,
        name: None,
        overwrite: false,
        no_register: false,
    }
}

/// 走真的 `outcome()`（人读出口），把登记行钉成字面量。
///
/// 抓得住什么（变异判据，实测过）：三个实参换位 ⇒ 整行对不上 ⇒ 红；`registered.created` 取反
/// ⇒ 结尾那份 `·复用已有档案` 消失 ⇒ 红；把 `id` 那一位换成 `name`（同一个实例的另一个字段）
/// ⇒ 红——用例里三个值互不相同，就是为了让"换错字段"立刻看得见。
///
/// 抓不住：`--json` 里 `registered` 的形状与 `instance_bound`（本用例只看人读行）。
#[test]
fn the_registered_line_names_the_pack_instance_it_registered() {
    let dir = TempDir::new("m346-registered");
    let ctx = ctx_at(dir.path());
    let mut report = ImportReport::empty(PathBuf::from("C:/tmp/instance"));
    report.registered = Some(RegisteredInstance {
        id: "3f2a1c".to_string(),
        name: "Survival".to_string(),
        version_id: "1.20.1-forge-47.2.0".to_string(),
        created: false,
    });
    let human = outcome(
        &ctx,
        &report,
        &PathBuf::from("C:/tmp/instance"),
        &[],
        &args(),
        None,
    )
    .report
    .human;
    let line = human
        .iter()
        .find(|line| line.starts_with("已登记实例"))
        .map(String::as_str)
        .expect("没有登记行");
    assert_eq!(
        line, "已登记实例 Survival（id 3f2a1c）→ 版本 1.20.1-forge-47.2.0·复用已有档案",
        "名称/实例 id/版本 id 三位各就各位，且 created == false 才有复用那半句"
    );
}
