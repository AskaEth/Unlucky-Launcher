//! 日志页的视图模型：**纯展示字段**（沿用 `pages::model` 的分层——不含 iced 类型，也不认识 presenter）。
//!
//! ## 为什么单独一层
//!
//! 渲染函数只吃 [`Logs`]，于是同一棵树能被三份数据喂：
//!
//! | 数据源 | 谁造 | 用途 |
//! |---|---|---|
//! | presenter 快照（`AppState`） | `app/view.rs::logs_model` | 产品运行时 |
//! | [`super::demo`] 的内嵌崩溃日志片段 | `pages::demo_model` | `pages_demo --screen=logs` 出图 |
//! | [`Logs::default`] | 空态 | 真的没有输出（未接线时也是这个兜底） |
//!
//! 页面**不读盘**：日志文本只有两条来路——presenter 快照里的 `LogLine`，或 `demo` 里
//! `include_str!` 内嵌的样例。日志文件的路径从头到尾没进过页面。
//!
//! ## 缺口（M95 登记，已报 finding）
//!
//! presenter 的 `dto::Diagnosis` 只有 `rule` / `title` / `hint` / `confidence` / `fix`：
//!
//! 1. **core 的 `Severity`（info / warning / critical）在半路丢了**——`core::diagnose::rules`
//!    里每条规则都带它，`presenter::dto::Diagnosis` 没有。拿严重度当分组名就只能拿置信度硬凑，
//!    实测 **64 条规则里 12 条会被凑错档、两个方向都有**（`mod.mixin_failure` critical/0.70 沉到
//!    第二档、`other.debug_crash` info/0.95 冒到第一档）。
//!
//!    所以 M95 r4 起页面**不再借 core 的词汇**：分组改用**置信度档**
//!    [`Confidence::High`] / [`Confidence::Medium`] / [`Confidence::Low`]，标题直接写「高/中/低
//!    置信度」——它描述的是「规则引擎有多确定」，不是「这毛病有多严重」。DTO 补上 `severity`
//!    之后这里应该换回真严重度（finding 已记给 presenter）。
//! 2. `title` / `hint` 只有中文单语，而且规则表里其实已经有结构化出口
//!    （`rules.d/*.yaml` 的 `cause.key: diagnose.cause.…`）。DTO 没带 `key`，页面就把规则原文
//!    当**数据**画出来，页面的界面文案（标题 / 按钮 / 分组名 / 空态）仍然全部走 i18n key。

use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_i18n::Lang;

/// 一行日志的等级（presenter `LogLevel` 的展示投影）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// 调试噪音。
    Debug,
    /// 正常流水。
    Info,
    /// 可疑。
    Warn,
    /// 错误行（崩溃现场）。
    Error,
}

impl Level {
    /// 稳定 slug（摘要行与测试用；不是文案）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// 诊断结论的**置信度档**（规则引擎有多确定，不是「这毛病有多严重」）。
///
/// 为什么不是 `Severity`：`presenter::dto::Diagnosis` 没带 core 规则表的 `severity`，
/// 页面自己按置信度折算严重度会凑错档（64 条规则里 12 条，两个方向都有）。宁可**如实**
/// 说「这条结论置信度多高」，也不要编一个「严重度」（视觉复审 r3 的 P2-2 + 塔裁决）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// 低：规则只是「有点像」。
    Low,
    /// 中：可疑。
    Medium,
    /// 高：基本可以确定。
    High,
}

/// 分区顺序（**高置信度在上**：崩溃日志换 issue 时第一段就是最该看的）。
pub const GROUPS: [Confidence; 3] = [Confidence::High, Confidence::Medium, Confidence::Low];

impl Confidence {
    /// 稳定 slug（摘要行用；不是文案——分组标题走 i18n key）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// 0~100 置信度 → 档位（阈值 `≥80` 高 / `≥50` 中 / 其余低）。
    ///
    /// 阈值是**档位定义**，不是从严重度反推的代理：标题与文案都说的是「置信度」。
    #[must_use]
    pub const fn of(confidence: u8) -> Self {
        if confidence >= 80 {
            Self::High
        } else if confidence >= 50 {
            Self::Medium
        } else {
            Self::Low
        }
    }
}

/// 一行日志。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// 原始文本（**不截断**：折行是渲染的事，数据要留全，否则用户永远拿不回堆栈）。
    pub text: String,
    /// 等级。
    pub level: Level,
}

/// 一条诊断结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// 命中的规则 id（`graphics.opengl_not_supported`）：技术标识，原样显示。
    pub rule: String,
    /// 置信度档（DTO 没有 core 的 severity，见 [`Confidence`]）。
    pub confidence_band: Confidence,
    /// 结论标题（规则表的 `cause.summary`，**数据不是文案**）。
    pub title: String,
    /// 建议说明（规则表的 `cause.detail`）。
    pub hint: String,
    /// 0~100 置信度。
    pub confidence: u8,
    /// 建议动作（规则表的 `fix_action`，技术标识 / 设置项名）。
    pub action: Option<String>,
}

/// 一次运行的结局（`presenter::dto::LogOutcome` 的展示投影；文案走 `LaunchOutcome*` key）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// 正常退出。
    Ok,
    /// 崩溃。
    Crashed,
    /// exit 0 但游戏没起来。
    SilentFail,
}

impl Outcome {
    /// 稳定 slug（摘要行用）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Crashed => "crashed",
            Self::SilentFail => "silent-fail",
        }
    }
}

/// 日志页要画的东西。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Logs {
    /// 会话所属实例名（`None` = 不知道，页头副标题退成通用文案）。
    pub instance: Option<String>,
    /// 会话时间（展示用短串 `10-29 18:40`，presenter 已经格式化好）。
    pub when: Option<String>,
    /// 结局。
    pub outcome: Option<Outcome>,
    /// 页头副标题（`实例 · 时间 · 结局`）：**构造时**拼好（见 [`Logs::with_status`]），
    /// 页面只负责显示——i18n 的「一句话 = 一个 key」不许在渲染里拼句子。
    pub status: Option<String>,
    /// 有没有正在跑的会话（`live_session` 非空）。
    ///
    /// 为什么需要它：session 索引只在会话**结束**时追加（`host::logs::sessions`），所以运行中的
    /// 那一场根本不在 `log_sessions` 里 ⇒ 拿不到实例名与结局。r3 之前这里直接退回
    /// 「当前没有运行中的实例」，于是**每次实时运行页头都在说反话**（视觉复审 r3 的 P2-3）。
    pub live: bool,
    /// 日志行（顺序 = 文件顺序）。
    pub lines: Vec<Line>,
    /// 诊断结论（顺序 = presenter 给的顺序，页面不重排）。
    pub findings: Vec<Finding>,
}

impl Logs {
    /// 补上页头副标题（M95 r2，视觉复审 P2-2；r4 补「运行中」这一档，P2-3）。
    ///
    /// 为什么在这里做而不是在页面里：`Header::subtitle` 要 `&'a str`，而拼出来的是 `String`；
    /// 让模型持有它，页面就永远只借不拼。
    ///
    /// 三级回退：**有实例 + 结局** → `实例 · 时间 · 结局`；**正在跑但还没有结局**（session 索引
    /// 里没有它） → 中性的「正在运行」；都没有 → `None`（页面退回 [`Key::LogsSubtitleIdle`]）。
    #[must_use]
    pub fn with_status(mut self, lang: Lang) -> Self {
        let catalog = Catalog::of(lang);

        self.status = status_line(
            self.instance.as_deref(),
            self.when.as_deref(),
            self.outcome,
            lang,
        )
        .or_else(|| {
            self.live
                .then(|| catalog.text(Key::LogsSubtitleLive).to_owned())
        });

        self
    }

    /// 没有任何可显示的东西（空态判据：行与诊断都空）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.findings.is_empty()
    }

    /// 某一置信度档下的结论（顺序不变；空组返回空 `Vec`）。
    #[must_use]
    pub fn group(&self, band: Confidence) -> Vec<&Finding> {
        self.findings
            .iter()
            .filter(|finding| finding.confidence_band == band)
            .collect()
    }

    /// **可复制的一行摘要**（崩溃日志换 issue 时直接粘）。
    ///
    /// 格式是刻意的：ASCII 骨架 + 稳定字段序 + ` | ` 分隔，人读与机器读都行；
    /// 不含任何界面文案（所以不随语言变），只带数据与控制面的技术标识。
    ///
    /// ```text
    /// ULL-LOGS v1 | instance=开发测试 | when=10-29 18:40 | outcome=crashed | lines=6 \
    ///   | findings=graphics.opengl_not_supported:critical(85)
    /// ```
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts = vec![SUMMARY_HEADER.to_owned()];

        if let Some(instance) = &self.instance {
            parts.push(format!("instance={instance}"));
        }
        if let Some(when) = &self.when {
            parts.push(format!("when={when}"));
        }
        if let Some(outcome) = self.outcome {
            parts.push(format!("outcome={}", outcome.slug()));
        }
        parts.push(format!("lines={}", self.lines.len()));

        if !self.findings.is_empty() {
            let hits: Vec<String> = self
                .findings
                .iter()
                .map(|finding| {
                    format!(
                        "{}:{}({})",
                        finding.rule,
                        finding.confidence_band.slug(),
                        finding.confidence
                    )
                })
                .collect();
            parts.push(format!("findings={}", hits.join(",")));
        }

        parts.join(" | ")
    }
}

/// 摘要行的版本头（改了格式就换代，别让旧 issue 里的摘要含义漂移）。
pub const SUMMARY_HEADER: &str = "ULL-LOGS v1";

/// 页头副标题：`{instance} · {when} · {outcome}`。
///
/// 结局文案走 [`Key::LaunchOutcomeNormal`] / [`Key::LaunchOutcomeCrash`] /
/// [`Key::LaunchOutcomeSilent`] —— 就是 `unlucky_i18n::bridge` 文档里写了很久的那条
/// 「`LogOutcome` 三态 → 三 key 结构化出口」，M95 r2 之前它们在 UI 侧**零调用点**（视觉复审 P2-2）。
///
/// 没有实例名或结局时返回 `None`（不知道是哪一场就别编），页头退回静态副标题。
#[must_use]
pub fn status_line(
    instance: Option<&str>,
    when: Option<&str>,
    outcome: Option<Outcome>,
    lang: Lang,
) -> Option<String> {
    let instance = instance?;
    let outcome = outcome?;
    let catalog = Catalog::of(lang);

    let outcome_key = match outcome {
        Outcome::Ok => Key::LaunchOutcomeNormal,
        Outcome::Crashed => Key::LaunchOutcomeCrash,
        Outcome::SilentFail => Key::LaunchOutcomeSilent,
    };

    Some(catalog.render(
        Key::LogsStatus,
        &[
            ("instance", instance),
            ("when", when.unwrap_or("—")),
            ("outcome", catalog.text(outcome_key)),
        ],
    ))
}
