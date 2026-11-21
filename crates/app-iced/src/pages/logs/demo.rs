//! 日志页演示数据：**只给 `pages_demo` 出图与单测**，产品运行时的数据来自 presenter 快照
//! （`app/view/logs.rs::logs_model`）。页面任何路径都不读盘：日志文本要么来自快照，要么来自
//! 本文件 `include_str!` 内嵌的 [`SAMPLE`]（编译期进二进制，运行时零 I/O）。
//!
//! ## 为什么用环境变量挑状态
//!
//! 三种状态（空态 / 有日志 / 有诊断）要分别出图，而 `pages_demo` 的 `--screen` 只有一个
//! `logs`。环境变量是这个 crate 已有的 harness 口径（`ULL_GALLERY_SHOT` / `ULL_FEEDBACK_PART`
//! 都是这么挑的，见 `components/gallery/mod.rs` 的说明）：既能被测试直接调纯函数覆盖，
//! 又不用跟别人的 CLI 解析抢同一个文件。
//!
//! ```text
//! ULL_LOGS_FIXTURE=empty pages_demo --screen=logs --screenshot=logs-empty-light.png
//! ```
//!
//! 取值：`empty`（空态）/ `live`（有日志、无诊断）/ `running`（正在跑，会话索引里还没有它）
//! / `builtin`（10 条内置结论，M395）/ `history`（多场历史，M453）/ `history-live`（多场历史 +
//! 实时一场在跑，M462 的「回到实时」入口）/ 其它或未设 = `crash`（日志 + 四条诊断）。默认取
//! `crash`：一屏就能看见页面的全部结构，reviewer 不必先猜环境变量。
//!
//! ## 样例内容的出处
//!
//! [`SAMPLE`] 是按 `core/diagnose/rules.d/samples/graphics/graphics_driver.log` 与
//! `integrity/unsatisfied_link.log` 的形状写的（含中文 + 堆栈 + 200 字符以上的长行）。
//!
//! 诊断结论**只抄规则 id 与捕获组**（`cause_key` / `groups` 抄自规则表 `rules.d/integrity.yaml`
//! 与 `graphics.yaml`）：标题与提示的文字由 i18n 目录按 `cause_key` 出，**夹具里没有任何中文
//! 结论文案**（M392）。于是 `--lang=en` 的截图看到的是产品路径的真结果，而不是夹具的措辞。
//! 界面文案（标题 / 按钮 / 分组名 / 空态）本来就全走 `unlucky_i18n` 的 key，见 `logs.rs`。

use unlucky_core::diagnose::collectors::{
    ExitSignal, EXIT_ACCESS_VIOLATION, EXIT_DLL_NOT_FOUND, EXIT_SIGKILL, EXIT_STACK_BUFFER_OVERRUN,
};
use unlucky_core::diagnose::engine::{synthetic_findings, DiagnosisInput, Finding as CoreFinding};
use unlucky_core::diagnose::hs_err::{
    FailedFrame, FrameKind, HsErrKind, HsErrReport, MemorySummary,
};
use unlucky_core::diagnose::rules::Severity as CoreSeverity;
use unlucky_i18n::Lang;

use super::model::{Confidence, Finding, Level, Line, Logs, Outcome, Severity};
use super::sessions::Session;

/// 挑演示状态的环境变量。
pub const STATE_ENV: &str = "ULL_LOGS_FIXTURE";

/// 内嵌的崩溃日志片段（见模块文档的出处）。
const SAMPLE: &str = include_str!("sample_crash.log");

/// 内嵌的**正常跑完**一场的日志片段（`live` 状态用；见 `sample_live.log` 的文件头）。
const LIVE_SAMPLE: &str = include_str!("sample_live.log");

/// 演示状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// 空态：没有输出，也没有诊断。
    Empty,
    /// 有日志、无诊断（正常跑着的一场）。
    Live,
    /// 崩溃现场：日志 + 诊断结论。
    Crash,
    /// **正在跑**的一场（页头走「正在运行」那条文案；session 索引里还没有它）。
    Running,
    /// 10 条内置结论（`signal.*` / `hs_err.*`）：M395 的验收屏，见 [`builtin`]。
    Builtin,
    /// **多场历史**（M453 的验收屏）：5 场，含一场崩溃——选择器要在这里滚起来。
    History,
    /// **多场历史 + 实时一场在跑**（M462 的验收屏）：标题行右侧多一颗「回到实时」。
    HistoryLive,
}

impl State {
    /// 从环境变量取值解析（未设 / 不认识一律 `Crash`；取值清单见模块文档）。
    #[must_use]
    pub fn of_env_value(raw: Option<&str>) -> Self {
        match raw {
            Some("empty") => Self::Empty,
            Some("live") => Self::Live,
            Some("running") => Self::Running,
            Some("builtin") => Self::Builtin,
            Some("history") => Self::History,
            Some("history-live") => Self::HistoryLive,
            _ => Self::Crash,
        }
    }

    /// 造出这个状态的视图模型（`lang` 用于页头副标题的 i18n）。
    ///
    /// 会话快照（`sessions` / `selected`）只在有**历史**的状态里造：`empty` 是真的一场都没跑过，
    /// `running` 是「正在跑、还没结束」——后者的会话索引里没有它，正是选择器空态那句话要出现的地方。
    #[must_use]
    pub fn build(self, lang: Lang) -> Logs {
        match self {
            Self::Empty => Logs::default().with_status(lang),
            // 正常跑完的一场用**自己的**日志样例：页头写「正常结束」却配崩溃日志尾行是自己打自己
            // （视觉复审 r2 的 p3）。
            Self::Live => with_sessions(
                Logs {
                    instance: Some("开发测试".to_owned()),
                    when: Some("11-02 19:22".to_owned()),
                    outcome: Some(Outcome::Ok),
                    lines: parse(LIVE_SAMPLE),
                    ..Logs::default()
                },
                single("s-2", "开发测试", "11-02 19:22", Outcome::Ok),
                lang,
            ),
            // 运行中：还没有结局、也不在 session 索引里（`host::logs::sessions` 只在结束时追加），
            // 所以既没有实例名也没有 outcome —— 页头只能走「正在运行」那条中性文案。
            Self::Running => Logs {
                live: true,
                lines: parse(LIVE_SAMPLE),
                ..Logs::default()
            }
            .with_status(lang),
            Self::Crash => with_sessions(
                crash(),
                single("s-3", "开发测试", "11-02 19:40", Outcome::Crashed),
                lang,
            ),
            Self::History => history_state(lang, None),
            // 历史视图 + 实时一场在跑（M462）：标题行右侧那颗「回到实时」的验收屏。
            Self::HistoryLive => history_state(lang, Some(LIVE_SESSION)),
            Self::Builtin => with_sessions(
                builtin(),
                single("s-4", "开发测试", "11-02 20:05", Outcome::Crashed),
                lang,
            ),
        }
    }
}

/// 挂上**一场**历史并选中它（单场档：选择器里只有一行，不该出现滚动区）。
fn single(id: &str, instance: &str, when: &str, outcome: Outcome) -> Vec<Session> {
    vec![Session::new(id, instance, when, outcome)]
}

/// 多场历史（M453「多场」那一档）：5 场、结局各不相同、含一场崩溃；第一场就是页头那一场。
fn history() -> Vec<Session> {
    vec![
        Session::new("s-7", "开发测试", "11-02 19:40", Outcome::Crashed),
        Session::new("s-6", "1.20.4 原版", "11-02 18:12", Outcome::Ok),
        Session::new("s-5", "整合包实例", "11-01 22:40", Outcome::SilentFail),
        Session::new("s-4", "开发测试", "11-01 20:02", Outcome::Ok),
        Session::new("s-3", "1.20.1 原版", "10-31 19:30", Outcome::Ok),
    ]
}

/// 正在跑那一场（M462）：**故意不在 [`history`] 的五场里**——会话索引只在收尾时追加，所以
/// 运行中那一场在选择器里没有行，这正是「回实时」要有独立入口的原因。
pub(super) const LIVE_SESSION: &str = "s-1";

/// 历史视图那一屏：`back_to_live` = 被历史视图挡住那一场（`None` = 没有会话在跑 ⇒ 页面上
/// 一个「回到实时」控件都不该出现）。
fn history_state(lang: Lang, back_to_live: Option<&str>) -> Logs {
    let mut logs = with_sessions(crash(), history(), lang);
    logs.back_to_live = back_to_live.map(str::to_owned);
    logs
}

/// 装上会话快照并把第一场标成选中（`label` 由 `with_status` 按语言拼）。
fn with_sessions(mut logs: Logs, sessions: Vec<Session>, lang: Lang) -> Logs {
    logs.selected = sessions.first().map(|session| session.id.clone());
    logs.sessions = sessions;
    logs.with_status(lang)
}

/// harness 入口：按 [`STATE_ENV`] 挑状态。
///
/// 出图时打一行 `[logs] fixture=… lang=…`：**每张图的证据行**（`DevDocs/artifacts/m453/README.md`
/// 逐图抄的就是这一行——没有它，六张图里哪张是「多场」只能靠人眼看）。只有 `pages_demo` /
/// `pages::demo_model` 走这里，产品路径（`App`）用的是 presenter 快照。
#[must_use]
pub fn demo(lang: Lang) -> Logs {
    let state = State::of_env_value(std::env::var(STATE_ENV).ok().as_deref());
    eprintln!("[logs] fixture={state:?} lang={lang:?}");
    state.build(lang)
}

/// 崩溃现场（日志 + 四条诊断，**三档严重度全都出现**）。
///
/// 第四条刻意是「置信度 95% 但严重度 info」：按置信度分组时它会冒到第一档（GAPS §2.2 点名的
/// 两个错档之一），按严重度分组它落在最后一档。出图时这一屏自己就是 M394 的判据。
///
/// 不带 `status`（副标题要 i18n，见 [`Logs::with_status`]）：[`State::build`] 会补上。
#[must_use]
pub fn crash() -> Logs {
    Logs {
        instance: Some("开发测试".to_owned()),
        when: Some("11-02 19:40".to_owned()),
        outcome: Some(Outcome::Crashed),
        lines: parse(SAMPLE),
        findings: findings(),
        ..Logs::default()
    }
}

/// 10 条**引擎内置结论**（M395 `BUILTIN10` 的验收屏）。
///
/// ## 为什么这里跑真引擎，而不是手抄夹具
///
/// 这 10 条（`signal.*` ×6 + `hs_err.*` ×4）的模板参数是 M395 才由 core 写进 `Finding::groups`
/// 的。手抄一份参数就等于再造一个会漂的真相源：参数名一改，图还是绿的，产品路径却已经坏了。
/// 所以这里直接调 `unlucky_core` 的 `synthetic_findings`——每种触发条件各造一个输入
/// （6 个退出码 + 4 份 hs_err 报告），把 core 的产出投影成页面模型。
///
/// 于是 `--lang=en` 那张图是**产品路径**按目录渲染的结果：目录里缺条目 / 参数没代入 / 模板没接线，
/// 图上立刻能看出来（英文句子里冒中文、或者冒 `{exit_code}` 字面量）。
#[must_use]
pub fn builtin() -> Logs {
    Logs {
        instance: Some("开发测试".to_owned()),
        when: Some("11-02 20:05".to_owned()),
        outcome: Some(Outcome::Crashed),
        lines: parse(SAMPLE),
        findings: builtin_findings(),
        ..Logs::default()
    }
}

/// 解析内嵌样例：`#` 注释与空行跳过，首字符是等级前缀。
pub(super) fn parse(text: &str) -> Vec<Line> {
    text.lines()
        .filter_map(|raw| {
            let raw = raw.trim_end();
            if raw.is_empty() || raw.starts_with('#') {
                return None;
            }

            // 按**字符**取等级前缀，不 `split_at(1)`：后者在非 ASCII 首字符上会切到
            // 多字节序列中间直接 panic（视觉复审 r2 的 p3）。
            let mut chars = raw.chars();
            let level = match chars.next() {
                Some('D') => Level::Debug,
                Some('I') => Level::Info,
                Some('W') => Level::Warn,
                Some('E') => Level::Error,
                _ => return None,
            };

            Some(Line {
                text: chars.as_str().trim_start().to_owned(),
                level,
            })
        })
        .collect()
}

/// 四条诊断（`cause_key` / `groups` 抄自规则表；置信度档走 [`Confidence::of`]）。
///
/// 严重度不是这里编的，是规则表 `rules.d/*.yaml` 的原值（`severity:`）：`integrity.unsatisfied_link`
/// critical、`graphics.gl_operation_failure` / `integrity.no_class_def_found` warning、
/// `other.debug_crash` info（置信度 0.95——**高置信度 + 低严重度**，分组换成严重度后它掉到最后一档，
/// 这正是 M394 要展示的那件事）。
///
/// M392 起这里**没有中文结论**了：标题与提示从 i18n 目录按 `cause_key` 取，`groups` 负责代入
/// 模板里的 `{name}`（第一条的 `lwjgl_opengl.dll`）。这样英文界面的截图也是英文——
/// 出图验证的是产品路径，不是夹具的措辞。
fn findings() -> Vec<Finding> {
    vec![
        finding(
            "integrity.unsatisfied_link",
            Severity::Critical,
            [("name", "lwjgl_opengl.dll")],
            85,
            Some("component.reinstall"),
        ),
        finding(
            "graphics.gl_operation_failure",
            Severity::Warning,
            [],
            70,
            Some("graphics.set"),
        ),
        finding(
            "integrity.no_class_def_found",
            Severity::Warning,
            [],
            60,
            Some("component.reinstall"),
        ),
        finding("other.debug_crash", Severity::Info, [], 95, None),
    ]
}

fn finding<const N: usize>(
    rule: &str,
    severity: Severity,
    groups: [(&str, &str); N],
    confidence: u8,
    action: Option<&str>,
) -> Finding {
    Finding {
        rule: rule.to_owned(),
        cause_key: format!("diagnose.cause.{rule}"),
        severity,
        confidence_band: Confidence::of(confidence),
        confidence,
        groups: groups
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect(),
        // 规则结论走目录；`system_text` 只在模板参数缺了时兜底（见 `diagnostic::texts_of`）。
        system_text: None,
        action: action.map(str::to_owned),
    }
}

/// 10 条内置结论：**跑真引擎**取 `synthetic_findings` 的产出（理由见 [`builtin`]）。
fn builtin_findings() -> Vec<Finding> {
    let mut out = Vec::new();
    for exit in [
        ExitSignal::classify(Some(EXIT_ACCESS_VIOLATION), Some(12_345), 0, ""),
        ExitSignal::classify(Some(EXIT_DLL_NOT_FOUND), Some(8_200), 0, ""),
        ExitSignal::classify(Some(EXIT_STACK_BUFFER_OVERRUN), Some(9_800), 0, ""),
        ExitSignal::classify(Some(EXIT_SIGKILL), Some(45_000), 0, ""),
        ExitSignal::classify(
            Some(1),
            Some(1_200),
            2,
            "Error occurred during initialization of VM",
        ),
        ExitSignal::classify(Some(0), Some(4_200), 3, ""),
    ] {
        out.extend(synthetic_findings(&DiagnosisInput::new().with_exit(exit)));
    }

    for report in [
        hs_err(
            HsErrKind::AccessViolation,
            Some(("C  [ig8d64.dll+0x9b3c4d]", "ig8d64.dll")),
            None,
        ),
        hs_err(
            HsErrKind::OutOfMemory,
            None,
            Some([16_777_216, 3_276_800, 8_388_608, 5_347_328]),
        ),
        hs_err(HsErrKind::StackOverflow, None, None),
        hs_err(HsErrKind::InternalError, None, None),
    ] {
        let mut input = DiagnosisInput::new();
        input.push_hs_err(report);
        out.extend(synthetic_findings(&input));
    }

    out.iter().map(core_finding).collect()
}

/// 造一份 hs_err 报告（只填内置结论要读的那几个字段）。
fn hs_err(kind: HsErrKind, frame: Option<(&str, &str)>, memory: Option<[u64; 4]>) -> HsErrReport {
    HsErrReport {
        header: Some("A fatal error has been detected by the Java Runtime Environment".to_owned()),
        kind,
        failed_frame: frame.map(|(raw, module)| FailedFrame {
            raw: raw.to_owned(),
            kind: FrameKind::Native,
            module: Some(module.to_owned()),
            offset: None,
        }),
        memory: memory.map(|[physical, physical_free, swap, swap_free]| MemorySummary {
            page_size: Some("4k".to_owned()),
            physical_kb: Some(physical),
            physical_free_kb: Some(physical_free),
            swap_kb: Some(swap),
            swap_free_kb: Some(swap_free),
            raw: "Memory: 4k page, physical 16777216k(3276800k free)".to_owned(),
        }),
        ..HsErrReport::default()
    }
}

/// core 的结论 → 页面模型：**逐字段照抄，不生成任何文案**。
///
/// `pub(super)`：`diagnostic_tests` 用它把「core 的真产出」喂给 `texts_of`（M395）。
pub(super) fn core_finding(finding: &CoreFinding) -> Finding {
    let percent = (finding.confidence.clamp(0.0, 1.0) * 100.0).round();
    let confidence = u8::try_from(percent as u32).unwrap_or(100);
    Finding {
        rule: finding.rule_id.clone(),
        cause_key: finding.message_key.clone(),
        severity: match finding.severity {
            CoreSeverity::Info => Severity::Info,
            CoreSeverity::Warning => Severity::Warning,
            CoreSeverity::Critical => Severity::Critical,
        },
        confidence_band: Confidence::of(confidence),
        confidence,
        groups: finding.groups.clone(),
        // 真产品路径（presenter）也会带上 core 的原话；界面在模板参数齐时**不画**它。
        system_text: Some(finding.message.clone()),
        action: finding
            .materialized_actions()
            .first()
            .map(|action| action.tool.to_string()),
    }
}
