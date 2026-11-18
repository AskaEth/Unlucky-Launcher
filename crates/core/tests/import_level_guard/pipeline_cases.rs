//! 管线层（`pipeline.rs` 的基线 / 登记、`before_commit.rs` 的残片清理、`scan.rs` 的预检计数）
//! 那几处构造点的级别。
//!
//! `record_rollback` / `abandoned` 那三处在 `pipeline/tests.rs`（它们是同模块的私有函数），
//! 这里都是**能走产品入口**的几条：`scan_pack`（只读预检）与 `import_pack`（整条管线）。

use unlucky_core::addon::import::{
    import_pack, keys, report::ImportIssueLevel, scan_pack, ImportRequest,
};
use unlucky_core::download::blocking::block_on;
use unlucky_core::platform::layout::LauncherLayout;

use crate::fixtures;
use crate::{assert_issue, issues};

/// 预检对**基线**的两种「读不出 / 不是我的」都要顶到告警：降级成"没有基线"意味着
/// 磁盘上同名文件一律算用户已有内容（⇒ 冲突清单变长、覆盖判断更保守）。
///
/// 依据：用户看到的是「有 N 条冲突等你点头」还是「可以装了」，取决于基线在不在。基线坏了
/// 却只说一句注记，用户会以为那是老生常谈而漏掉"这次判定的前提变了"。
///
/// 变异：把 `pipeline.rs` `load_baseline` 里任一处 `ImportIssue::warning` 改成 `note` ⇒ 对应段红。
#[test]
fn baseline_problems_are_warnings() {
    // ① 基线读不出来（`pipeline.rs` 的 `NOTE_BASELINE_UNREADABLE`）。
    let pack = fixtures::mrpack_with_plan_notes("baseline-broken");
    let out = fixtures::TempDir::new("baseline-broken-out");
    let target = out.join("instance");
    fixtures::write_file(
        &target.join(fixtures::baseline_relative()),
        b"not json at all",
    );

    let scan = scan_pack(&ImportRequest::new(pack.path(), &target));
    assert!(scan.is_ok(), "{:?}", scan.error);
    let all: Vec<_> = scan.notes.iter().collect();
    assert_issue(
        &all,
        keys::NOTE_BASELINE_UNREADABLE,
        ImportIssueLevel::Warning,
        "基线读不出来",
        "判定前提变了（无基线 ⇒ 同名文件一律算用户内容），用户必须知道冲突清单为什么变长",
    );

    // ② 基线属于别的包（`pipeline.rs` 的 `NOTE_BASELINE_PACK_MISMATCH`）。
    let out = fixtures::TempDir::new("baseline-foreign-out");
    let target = out.join("instance");
    fixtures::write_foreign_baseline(&target);

    let scan = scan_pack(&ImportRequest::new(pack.path(), &target));
    assert!(scan.is_ok(), "{:?}", scan.error);
    let all: Vec<_> = scan.notes.iter().collect();
    assert_issue(
        &all,
        keys::NOTE_BASELINE_PACK_MISMATCH,
        ImportIssueLevel::Warning,
        "基线属于另一个整合包",
        "拿别人的基线当 base 会把用户改过的文件当升级而静默覆盖 ⇒ 必须让用户看见这次降级",
    );
}

/// 预检里「有远端条目判不了」是**事实陈述**：判不了不等于出事，导入时照「缺省保留」处置。
///
/// 依据：CurseForge 清单的 `files[]` 补全前就没有路径，这是**常态**（不是异常）。判成告警的
/// 后果：每个 CF 包一进来就顶一条告警，用户很快学会无视告警区——那时真告警也不响了。
///
/// 变异：把 `scan.rs` 里 `NOTE_REMOTE_PATH_UNKNOWN` 的 `ImportIssue::note` 改成 `warning` ⇒ 本条红。
#[test]
fn unknown_remote_entries_are_a_note_in_the_scan() {
    let pack = fixtures::curseforge("scan-cf");
    let out = fixtures::TempDir::new("scan-cf-out");
    let target = out.join("instance");

    let scan = scan_pack(&ImportRequest::new(pack.path(), &target));
    assert!(scan.is_ok(), "{:?}", scan.error);
    assert_eq!(scan.unknown_remote, 1, "CF 的 files[] 补全前没有路径");
    let all: Vec<_> = scan.notes.iter().collect();
    assert_issue(
        &all,
        keys::NOTE_REMOTE_PATH_UNKNOWN,
        ImportIssueLevel::Note,
        "判不了",
        "「补全前判不了」是 CF 常态，导入时按缺省保留；没有要用户动手的",
    );
}

/// 登记成功是**事实确认** ⇒ 注记。
///
/// 依据：界面上的实例列表已经多了一条（用户点得开、看得见），这条注记只是把"登记了哪个
/// 版本"说清楚。判成告警的后果：每次导入都顶一条"要你处理"的假告警。
///
/// 变异：把 `pipeline.rs` 里 `NOTE_REGISTERED` 的 `ImportIssue::note` 改成 `warning` ⇒ 本条红。
#[test]
fn registration_success_is_a_note() {
    let pack = fixtures::multimc("register", false);
    let out = fixtures::TempDir::new("register-out");
    let target = out.join("instance");
    let layout = LauncherLayout::new(out.join("data"));

    let report = block_on(import_pack(
        &ImportRequest::new(pack.path(), &target).with_registration(&layout, Some("levelguard")),
    ));
    assert!(report.registered.is_some(), "{:?}", report.error);
    assert_issue(
        &issues(&report),
        keys::NOTE_REGISTERED,
        ImportIssueLevel::Note,
        "已登记实例",
        "实例已经出现在列表里，这句话只是确认；用户没有待办",
    );
}

/// 「提交前剔掉了下载引擎的中间产物」是**事实陈述**（它们本来就不该进实例）。
///
/// 依据：用户看到的是"实例里没有 `.ullpart-*` 垃圾"，而那正是他期望的结果——没有待办。
/// 判成告警的后果：**每次下载成功的一包都会**顶一条假告警（引擎续传元数据几乎总会留下）。
///
/// 触发方式：假远端像真引擎那样在落点旁边留下 `<名>.ullpart-<pid>-0` 与 `<名>.ullmeta.json`
/// （真实现的同名产物由 `core::download` 的原子落盘写出，见 `stage::is_engine_artifact`）。
///
/// 变异：把 `before_commit.rs` 里 `NOTE_ENGINE_ARTIFACTS_DROPPED` 的 `import` 改成 `warning` ⇒ 本条红。
#[test]
fn purged_engine_artifacts_are_a_note() {
    let pack = fixtures::mrpack_with_remote("engine-artifacts");
    let out = fixtures::TempDir::new("engine-artifacts-out");
    let target = out.join("instance");
    let fetcher = fixtures::StubFetcher::new().with_engine_artifacts();

    let report = block_on(import_pack(
        &ImportRequest::new(pack.path(), &target).with_fetcher(&fetcher),
    ));
    assert_eq!(
        report.status,
        unlucky_core::addon::import::ImportStatus::Success,
        "{:?} {:?}",
        report.error,
        report.missing
    );
    assert!(
        !target.join("mods/jei.jar.ullmeta.json").exists(),
        "中间产物不许留在实例里"
    );
    assert_issue(
        &issues(&report),
        keys::NOTE_ENGINE_ARTIFACTS_DROPPED,
        ImportIssueLevel::Note,
        "中间产物",
        "剔掉它们是用户期望的结果（实例里不该有续传垃圾），没有待办",
    );
}
