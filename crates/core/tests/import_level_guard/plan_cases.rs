//! 计划层（`plan.rs` / `plan/layer.rs` / `plan/paths.rs`）那几处构造点的级别。
//!
//! 全部走 `import_pack`（产品路径）：清单 → 计划 → 落盘 → 报告。没有一处是把
//! `ImportIssue` 直接拼出来的——那样断言的是"我拼得对"，不是"产品判得对"。

use unlucky_core::addon::import::{import_pack, keys, report::ImportIssueLevel, ImportRequest};
use unlucky_core::download::blocking::block_on;

use crate::fixtures;
use crate::{assert_issue, issues};

/// 一句话读懂这张表：**跳过的都是格式语义规定的、要用户知道的都是"东西可能不齐"**。
///
/// | 构造点 | 事实 | 用户要做什么 | 级别 |
/// |---|---|---|---|
/// | `plan.rs` `NOTE_SERVER_OVERRIDES_SKIPPED` | 服务端覆盖按 Modrinth 官方语义不解压 | 什么都不用做（客户端本来就不需要） | 注记 |
/// | `plan.rs` `NOTE_ENV_UNSUPPORTED` | `env.client=unsupported` 的条目被跳过 | 什么都不用做（客户端不需要这个文件） | 注记 |
/// | `plan.rs` `NOTE_OPTIONAL_FILE` | `env.client=optional` 照装（"可选"≠"禁用"） | 什么都不用做 | 注记 |
/// | `plan.rs` `NOTE_UNKNOWN_DEPENDENCY` | 清单里有本启动器不装的依赖键 | 要自己装/确认少不少东西 | 告警 |
/// | `plan.rs` `NOTE_LOADER_MULTIPLE` | 多个可识别加载器只按第一条组计划 | 要确认装的是不是想要的那个 | 告警 |
/// | `plan/layer.rs` `NOTE_OVERRIDE_SHADOWED` | 同包两条声明命中同一路径，一条被遮蔽 | 什么都不用做（谁赢是规则定的，两份都在包里） | 注记 |
/// | `plan/paths.rs` `NOTE_OLD_SUFFIX` / `.disabled.old` | `.old` 只识别保留 | 什么都不用做（认得出这个名字而已） | 注记 |
///
/// 变异（每条都实跑过，报文见 mission 报告）：把对应构造点的 `note` ↔ `warning` 对调 ⇒ 这条红。
#[test]
fn plan_notes_are_leveled_by_whether_the_user_must_act() {
    let pack = fixtures::mrpack_with_plan_notes("plan-notes");
    let out = fixtures::TempDir::new("plan-notes-out");
    let target = out.join("instance");

    let report = block_on(import_pack(&ImportRequest::new(pack.path(), &target)));
    let all = issues(&report);

    assert_issue(
        &all,
        keys::NOTE_SERVER_OVERRIDES_SKIPPED,
        ImportIssueLevel::Note,
        "服务端内容",
        "服务端覆盖不解压是 Modrinth 官方语义，客户端用户没有可做的动作",
    );
    assert_issue(
        &all,
        keys::NOTE_ENV_UNSUPPORTED,
        ImportIssueLevel::Note,
        "客户端不需要这个文件",
        "env.client=unsupported 是清单自己说「客户端不需要」，跳过不是损失",
    );
    assert_issue(
        &all,
        keys::NOTE_OPTIONAL_FILE,
        ImportIssueLevel::Note,
        "「可选」不等于「禁用」",
        "可选条目照装（装回来默认启用），用户没有待办",
    );
    assert_issue(
        &all,
        keys::NOTE_UNKNOWN_DEPENDENCY,
        ImportIssueLevel::Warning,
        "parchment",
        "本启动器不装这个依赖 ⇒ 照报告「导入成功」去启动可能缺一层补丁/库，用户得自己确认",
    );
    assert_issue(
        &all,
        keys::NOTE_LOADER_MULTIPLE,
        ImportIssueLevel::Warning,
        "只按 primary/第一条组计划",
        "清单声明了多个加载器而只装一个 ⇒ 用户以为装了两个、实际只用一个，得自己核对",
    );
    assert_issue(
        &all,
        keys::NOTE_OVERRIDE_SHADOWED,
        ImportIssueLevel::Note,
        "声明同一实例路径",
        "谁赢由叠层规则定、两份内容都在包里，用户没有可做的动作",
    );
    assert_issue(
        &all,
        keys::NOTE_OLD_SUFFIX,
        ImportIssueLevel::Note,
        "识别并原样保留",
        ".old 只表示「认得出这个名字、不动它」，不是要用户处理的问题",
    );
    assert_issue(
        &all,
        keys::NOTE_DISABLED_SUFFIX,
        ImportIssueLevel::Note,
        "禁用态的旧文件",
        "禁用态按原后缀保留是格式语义，用户没有待办（要启用自己去改后缀）",
    );
}

/// `instance.cfg` 的两条分支各是一条构造点（`plan.rs` 里两个 `ImportIssue::note`）。
///
/// 依据：这段内存上限只在 `OverrideMemory=true` 时生效（Prism 的规矩）。两种情形下用户都
/// 不需要动手——**取到了**是事实确认，**没取到**是因为开关没开、启动器**按规矩不采用**，
/// 都不是"导入出问题"。判成告警的后果：Prism 包里十有八九 `OverrideMemory=false`，
/// 于是几乎每次导入都顶一条假告警。
///
/// 变异：把 `instance_memory` 里任一处 `note` 改成 `warning` ⇒ 对应那一段红。
#[test]
fn instance_cfg_memory_notes_are_notes_on_both_branches() {
    // ① OverrideMemory=true：取到了（`plan.rs` 第二个构造点）。
    let pack = fixtures::multimc("multimc-memory-on", true);
    let out = fixtures::TempDir::new("multimc-memory-on-out");
    let report = block_on(import_pack(&ImportRequest::new(
        pack.path(),
        &out.join("instance"),
    )));
    assert_eq!(report.max_memory_mb, Some(4096), "{:?}", report.error);
    assert_issue(
        &issues(&report),
        keys::NOTE_INSTANCE_CFG_MEMORY,
        ImportIssueLevel::Note,
        "取到内存上限",
        "取到了就是事实确认，用户没有待办",
    );

    // ② OverrideMemory=false：写了但不采用（`plan.rs` 第一个构造点）。
    let pack = fixtures::multimc("multimc-memory-off", false);
    let out = fixtures::TempDir::new("multimc-memory-off-out");
    let report = block_on(import_pack(&ImportRequest::new(
        pack.path(),
        &out.join("instance"),
    )));
    assert_eq!(report.max_memory_mb, None, "{:?}", report.error);
    assert_issue(
        &issues(&report),
        keys::NOTE_INSTANCE_CFG_MEMORY,
        ImportIssueLevel::Note,
        "OverrideMemory 未开",
        "不采用是 Prism 的规矩（不看开关会误判），用户没有待办",
    );
}
