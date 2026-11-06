//! 日志页的展示转换：presenter 快照 → [`pages::Logs`]（从 `app/view.rs` 拆出来，M95 r3）。
//!
//! ## 跨会话串场（M95 r3 的 p2，代码复审抓的）
//!
//! r2 的写法先按 `state.diagnosis` 定「这是哪一场」：
//!
//! ```ignore
//! let session = report.and_then(|r| log_sessions.find(|s| s.id == r.session)).or_else(|| …selected…);
//! ```
//!
//! 而 `state.diagnosis` **全仓只写不清**（`presenter::state::apply_core` 里
//! `CoreEvent::DiagnosisReady` 是唯一赋值点，没有任何清零点），host 也只在崩溃 / 打开会话 /
//! 导入崩溃文件时发它。于是「崩溃一场 → 再启动一场正常跑」这个最普通的路径下：
//! 页头把**新**会话标成上一场的 `The game crashed`、诊断卡挂着上一场的结论，而用户复制去贴
//! issue 的摘要里 `lines=` 是新的那一场的行数——**结论与行数不同源**，用户会把错诊断递出去。
//!
//! 现在的口径（不动 presenter 的写入口，只在页面侧判别）：
//!
//! | 概念 | 取值 |
//! |---|---|
//! | **身份**（这一场是谁） | `live_session`（正在跑）→ `selected_session`（打开/刚结束的），**不从 `diagnosis` 反推** |
//! | 诊断结论 | 见下表的三个分支（r4 修正了 r3 的一条**回归**：r3 只认身份，把「导入崩溃文件」与「打开会话」两条路径的诊断一并关掉了） |
//! | 实例/时间/结局（给页头描述） | 身份优先；身份为空才退到报告的 session |
//!
//! 归属判据（`report.session` 与身份的关系）：
//!
//! | `live_session` | `selected_session` | 报告算数的条件 |
//! |---|---|---|
//! | `Some(live)` | 任意 | `report.session == live` —— 只有它在线时日志行是它自己在追加，别人的报告挂上来就是串场 |
//! | `None` | `Some(sel)` | `report.session == sel` **或** 报告不属于任何已知会话（`ImportCrashLog` 用 `file:<文件名>` 当会话 id，永远不在索引里） |
//! | `None` | `None` | 全部算数 —— 此时日志行只可能来自 `open_session` 或导入，两者都与报告同一次调用发出 |
//!
//! 已知残留（今天 iced 侧不可达，登记在报告里）：`open_session(A)` 在 `selected_session` 仍停在上
//! 一场 B 时会被第三行判据挡掉（A 是已知会话且 ≠ B）——真正要修得让 presenter 在
//! `LogLinesChanged` 时把「日志行属于哪一场」记下来（`AppState` 今天没有这个字段）。
//!
//! 「没有命中规则」的**占位报告**也不画：host 的 `open_session` 在规则引擎无结论时会发一份
//! `rule: ""` / `confidence: 0` / `title: "该会话没有命中已知规则"` 的假报告（`CoreEvent::DiagnosisReady`
//! 是非 Option，host 无法表达「没有结论」）——它是标记不是结论，画出来等于给用户看一条「严重度提示」。
//!
//! 已知缺口（已报 finding，不在本 mission 修）：`clear_logs` 之后 `selected_session` 不清，
//! 身份会继续指向那一场；导入崩溃文件只发 `DiagnosisReady`（不带日志行），所以页面上会出现
//! 「结论 + 上一场的日志正文」——行由 `open_session` 负责，那是 presenter 的缺口。

use unlucky_presenter::dto::{Diagnosis, LogLevel, LogOutcome};
use unlucky_presenter::state::AppState;

use crate::pages;

/// 日志页视图模型：presenter 快照 → 要画的东西。
///
/// 两处 DTO 侧的**信息丢失**（已报 finding）：`dto::Diagnosis` 没有 core 的 `severity`
/// （所以页面按置信度档分组，不冒称严重度）与 `cause.key`（`title`/`hint` 只能当数据画）。
pub(super) fn logs_model(state: &AppState) -> pages::Logs {
    let live = state.live_session.as_deref();
    let selected = state.selected_session.as_deref();
    let identity = live.or(selected);

    // 归属判据（r4 修正 r3 的**回归**：r3 的「有选中就只认选中」把导入/打开这两条路径的诊断
    // 一起关掉了）。三条命中任一即认：
    //
    // 1. 报告属于**正在跑**的那一场 —— 这一条必须单独成立：只有它在线时日志行是它自己在追加，
    //    别的会话的报告挂上来就是串场（r3 修的正是这个）；
    // 2. 报告属于**当前选中**的那一场（没有 live 会话时）；
    // 3. 报告的会话**不在会话索引里** —— `ImportCrashLog` 用 `file:<文件名>` 当会话 id，
    //    `open_session` 也不一定把 id 写进 `selected_session`；这两条路径与报告是同一次调用发出的，
    //    把它们一并关掉就是把「导入崩溃日志」这条功能画成空（r3 的实测回归）。
    let known = |id: &str| state.log_sessions.iter().any(|session| session.id == id);
    let report = state.diagnosis.as_ref().filter(|report| {
        let id = report.session.as_str();
        match (live, selected) {
            // 有会话在跑：日志行是它自己在追加，别的会话的报告挂上来就是串场（r3 修的 P1）。
            (Some(live), _) => id == live,
            // 没有在跑、但有选中项：报告要么是它的，要么是「会话索引里没有的那一份」
            // （`ImportCrashLog` 用 `file:<文件名>`；`open_session` 不写 `selected_session`，
            // 所以只能靠「不在索引里」认它）。
            (None, Some(selected)) => id == selected || !known(id),
            // 什么身份都没有：日志行只可能来自 `open_session` 或导入，两者与报告同一次发出。
            (None, None) => true,
        }
    });

    // 身份为空时，描述的来源只能退到报告（见模块文档的表格）。
    let described = identity.or_else(|| report.map(|report| report.session.as_str()));
    let session =
        described.and_then(|id| state.log_sessions.iter().find(|session| session.id == id));

    pages::Logs {
        instance: session.map(|session| session.instance.clone()),
        when: session.map(|session| session.when.clone()),
        outcome: session.map(|session| log_outcome(session.outcome)),
        // 运行中的会话不在 `log_sessions` 里（索引只在结束时追加），所以页头要靠这个标记
        // 走「正在运行」那条文案，而不是「当前没有运行中的实例」。
        live: live.is_some(),
        lines: state
            .log_lines
            .iter()
            .map(|line| pages::Line {
                text: line.text.clone(),
                level: log_level(line.level),
            })
            .collect(),
        findings: report
            .filter(|report| !report.diagnosis.rule.is_empty())
            .map(|report| diagnosis_of(&report.diagnosis))
            .into_iter()
            .collect(),
        ..pages::Logs::default()
    }
}

/// 日志等级：presenter 枚举 → 页面等级（**结构化出口**，不是认字符串）。
fn log_level(level: LogLevel) -> pages::Level {
    match level {
        LogLevel::Debug => pages::Level::Debug,
        LogLevel::Info => pages::Level::Info,
        LogLevel::Warn => pages::Level::Warn,
        LogLevel::Error => pages::Level::Error,
    }
}

/// 会话结局：presenter 枚举 → 页面结局（文案走 `LaunchOutcome*` key，页面不画 core 的中文串）。
fn log_outcome(outcome: LogOutcome) -> pages::Outcome {
    match outcome {
        LogOutcome::Ok => pages::Outcome::Ok,
        LogOutcome::Crashed => pages::Outcome::Crashed,
        LogOutcome::SilentFail => pages::Outcome::SilentFail,
    }
}

/// 一条诊断结论 → 页面结论（置信度档见 [`pages::Confidence`]）。
fn diagnosis_of(diagnosis: &Diagnosis) -> pages::Finding {
    pages::Finding {
        rule: diagnosis.rule.clone(),
        confidence_band: pages::Confidence::of(diagnosis.confidence),
        title: diagnosis.title.clone(),
        hint: diagnosis.hint.clone(),
        confidence: diagnosis.confidence,
        action: diagnosis.fix.clone(),
    }
}

#[cfg(test)]
mod tests;
