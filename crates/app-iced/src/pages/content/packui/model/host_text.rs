//! 宿主侧失败的**稳定码** → 目录句子（M336 / 台账 P31 finding②）。
//!
//! ## 为什么这一层存在
//!
//! 宿主层（`presenter/.../pack/import.rs::TargetProblem`）在「目标实例不可用」时**不写散文**：
//! 它只给一个稳定码（`import_host_occupied` 那一族）与一格细节（哪个目录 / 系统 IO 原文），
//! 两个字段都进 `--json` 与日志。人话在这里取，而且只取一次：码 → [`Key`]，再按当前语言渲染。
//!
//! 这一层之前，那两格是 `"Target instance is not usable"`（硬编码英文）+ `"import_host"`（内部码），
//! 界面照 `format!("{} · {}", label, code)` 画成 `Target instance is not usable · import_host`——
//! 中文界面里混英文，尾巴上挂着一串用户看不懂、也照它做不了任何事的内部 key。
//!
//! ## 为什么码在界面侧翻、而不是宿主层翻（D56 明明允许宿主依赖文案库）
//!
//! 宿主拿不到界面当前的语言档位（`ImportRequestView` 里没有 `Lang`，D56 又禁止它在内部自己探测
//! 系统语言），要翻就得把语言字段一路送进后台线程——那要动 DTO 与 `HostEnv`。而界面这一侧本来
//! 就有 `Catalog`（`report_of` 的参数），码 → [`Key`] 又正是 [`unlucky_i18n::bridge`] R2 第一条
//! 推荐的通道（结构化出口 → key）。于是：宿主给码，界面取词，宿主零 i18n 依赖。
//!
//! ## 诚实边界
//!
//! - **抓得住**：这四个码的句子与语言档位（`model/tests/host.rs` 逐码 × 中英断言）；
//! - **抓不住**：core 的失败（`pack_traversal` / `import_integrity` …）仍是 core 的中文散文，
//!   见 [`text`] 的 `None` 那一支——那是另一笔债（core 没有结构化出口），不在本 mission 的 scope。

use unlucky_i18n::{Catalog, Key};

/// 宿主侧那四个码各自的（大类名, 细节句）。
///
/// | 码 | 大类名 | 细节句 |
/// |---|---|---|
/// | `import_host_occupied` | [`Key::PackHostTargetUnusable`] | [`Key::PackHostOccupied`] |
/// | `import_host_unreadable` | 同上 | [`Key::PackHostUnreadable`] |
/// | `import_host_create_failed` | 同上 | [`Key::PackHostCreateFailed`] |
/// | `import_host_instance_gone` | 同上 | [`Key::PackHostInstanceGone`] |
///
/// 最后一档的模板**不带 `{detail}`**：那一格的细节是上游（`pack.rs::instance_root`）拼的英文散文，
/// 只进 `--json`，界面一个英文词都不画。
///
/// `None` = 不是这一族：调用方照旧画 DTO 的 `label` / `message`（原文照画，不猜、不吞）。
pub(super) fn text(code: &str, detail: &str, catalog: Catalog) -> Option<(String, String)> {
    let (label, message) = match code {
        "import_host_occupied" => (Key::PackHostTargetUnusable, Key::PackHostOccupied),
        "import_host_unreadable" => (Key::PackHostTargetUnusable, Key::PackHostUnreadable),
        "import_host_create_failed" => (Key::PackHostTargetUnusable, Key::PackHostCreateFailed),
        "import_host_instance_gone" => (Key::PackHostTargetUnusable, Key::PackHostInstanceGone),
        _ => return None,
    };
    Some((
        catalog.text(label).to_owned(),
        catalog.render(message, &[("detail", detail)]),
    ))
}
