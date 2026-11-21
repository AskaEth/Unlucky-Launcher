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
//! **M458 修掉了原先登记在这里的「已知残留」**：`open_session(A)` 曾经不移动 `selected_session`，
//! 于是「selected 停在 B 时点 A」会被「有选中就只认选中」那一条当成串场挡掉——页头与高亮写 B，
//! 正文与诊断却是 A。病根在 presenter 的**历史路径缺身份**：实时路径的 `LogAppended` 自带
//! `session`，历史路径的 `LogLinesChanged` 载荷里没有（也不该加：那是全仓构造点最多的契约）。
//! 现在 host 打开会话时先发一条只带身份的 `CoreEvent::LogSessionOpened`，`selected_session`
//! 随之移动——**本文件一行判据都没动**，它吃到的 selected 已经是 A 了。
//!
//! **M459 补的第四条（历史视图专属）**：历史视图期间（`state::LogView::in_history()`），
//! **被挡住的那一场**（实时）的报告一律不认——它是「实时串进历史」的另一半。此时
//! `live_session` 是空的，第二条判据会落到「报告不属于任何已知会话」那条兜底上（那是给
//! `ImportCrashLog` 的 `file:` 开的），于是实时那一场刚收尾的结论会被当成历史那一场的结论画出来。
//! 正文那一半由 presenter 挡（`state/live.rs` 的 `LogView::holds`），本文件只管「结论算不算」。
//!
//! 「没有命中规则」的**占位报告**也不画：host 的 `open_session` 在规则引擎无结论时会发一份
//! `rule: ""` / `cause_key: ""` / `confidence: 0` 的假报告（`CoreEvent::DiagnosisReady`
//! 是非 Option，host 无法表达「没有结论」）——它是标记不是结论，画出来等于给用户看一条「严重度提示」。
//! （M392 之前这份占位还塞着一句中文 `title`；现在它连 `system_text` 都是 `None`：没有结论就没有
//! 原话可透传。M395 之后 `system_text` 只在内置结论的模板参数缺了时才被画——见
//! `pages::logs::diagnostic::texts_of`。）
//!
//! 已知缺口（已报 finding，不在本 mission 修）：`clear_logs` 之后 `selected_session` 不清，
//! 身份会继续指向那一场；导入崩溃文件只发 `DiagnosisReady`（不带日志行），所以页面上会出现
//! 「结论 + 上一场的日志正文」——行由 `open_session` 负责，那是 presenter 的缺口。

use unlucky_presenter::dto::{Diagnosis, DiagnosisSeverity, LogLevel, LogOutcome};
use unlucky_presenter::state::AppState;

use crate::pages;

/// 日志页视图模型：presenter 快照 → 要画的东西。
///
/// DTO 侧当年的两处**信息丢失**（M392 已补）：`severity` 与 `cause.key`。
/// 现在 DTO 带 `cause_key` / `severity` / `groups`（模板参数）与「core 原话」`system_text`，
/// 页面据此去 i18n 目录取文案（见 `pages::logs::diagnostic` 的 `texts_of`）。M395 起内置结论
/// 也走同一条路：`groups` 由 core 填好，页面不再有任何「这条没法翻译」的分支。
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
    // 3. 报告的会话**不在会话索引里** —— `ImportCrashLog` 用 `file:<文件名>` 当会话 id
    //    （它不发日志行、也不发身份，只有这一条能认出它）；这类路径与报告是同一次调用发出的，
    //    把它关掉就是把「导入崩溃日志」这条功能画成空（r3 的实测回归）。
    let known = |id: &str| state.log_sessions.iter().any(|session| session.id == id);
    // M459：历史视图期间，**被挡住的那一场**（实时）刚收到/刚收尾的结论不许挂到这一页上——
    // 正文是打开的那一场历史，页头也是它。此时 `live_session` 是空的（被 `state::LogView` 挡着），
    // 下面第二条判据会落到「报告不属于任何已知会话」那条兜底（它是给 `ImportCrashLog` 的
    // `file:<文件名>` 开的），正好把实时那一场的结论放进来。
    let held = state.log_view.held();
    let report = state.diagnosis.as_ref().filter(|report| {
        let id = report.session.as_str();
        if state.log_view.in_history() {
            return held != Some(id) && (selected == Some(id) || !known(id));
        }
        match (live, selected) {
            // 有会话在跑：日志行是它自己在追加，别的会话的报告挂上来就是串场（r3 修的 P1）。
            (Some(live), _) => id == live,
            // 没有在跑、但有选中项：报告要么是它的，要么是「会话索引里没有的那一份」
            // （M458 之后 `open_session` 会把索引里的真 id 写进 `selected_session`，
            //  所以第一支覆盖「打开历史会话」，第二支只剩 `ImportCrashLog` 那条路）。
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
        // M453：会话选择器的行。顺序 = presenter 给的顺序（页面不重排）；`label` 由
        // `with_status(lang)` 统一填（i18n 在模型层，渲染期不拼句子）。
        sessions: state
            .log_sessions
            .iter()
            .map(|session| {
                pages::Session::new(
                    &session.id,
                    &session.instance,
                    &session.when,
                    log_outcome(session.outcome),
                )
            })
            .collect(),
        // 选中项与页头**同源**（`described`）：页头写着哪一场，选择器里就是哪一行亮着。
        // 不另算一套「当前项」——两套就一定会漂（见模块文档的跨会话串场）。
        selected: described.map(str::to_owned),
        // M462：历史视图下「回到实时那一场」的入口。判据在 presenter（`LogView::live_return`：
        // 历史视图 + 被挡住那一场**仍在跑**），这里只转达——`None` 时页面一个控件都不画。
        back_to_live: state.log_view.live_return().map(str::to_owned),
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

/// 一条诊断结论 → 页面结论（文案 = 目录键 + 捕获组，见 [`pages::Finding`]）。
fn diagnosis_of(diagnosis: &Diagnosis) -> pages::Finding {
    pages::Finding {
        rule: diagnosis.rule.clone(),
        cause_key: diagnosis.cause_key.clone(),
        severity: severity_of(diagnosis.severity),
        confidence_band: pages::Confidence::of(diagnosis.confidence),
        confidence: diagnosis.confidence,
        groups: diagnosis.groups.clone(),
        system_text: diagnosis.system_text.clone(),
        action: diagnosis.fix.clone(),
    }
}

/// 严重度：presenter 枚举 → 页面枚举（**分组依据**，见 [`pages::Severity`] 的文档）。
fn severity_of(severity: DiagnosisSeverity) -> pages::Severity {
    match severity {
        DiagnosisSeverity::Info => pages::Severity::Info,
        DiagnosisSeverity::Warning => pages::Severity::Warning,
        DiagnosisSeverity::Critical => pages::Severity::Critical,
    }
}

#[cfg(test)]
mod tests;
