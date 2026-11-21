//! 日志页的视图模型：**纯展示字段**（沿用 `pages::model` 的分层——不含 iced 类型，也不认识 presenter）。
//!
//! ## 为什么单独一层
//!
//! 渲染函数只吃 [`Logs`]，于是同一棵树能被三份数据喂：
//!
//! | 数据源 | 谁造 | 用途 |
//! |---|---|---|
//! | presenter 快照（`AppState`） | `app/view/logs.rs::logs_model` | 产品运行时 |
//! | [`super::demo`](mod@super::demo) 的内嵌崩溃日志片段 | `pages::demo_model` | `pages_demo --screen=logs` 出图 |
//! | [`Logs::default`] | 空态 | 真的没有输出（未接线时也是这个兜底） |
//!
//! 页面**不读盘**：日志文本只有两条来路——presenter 快照里的 `LogLine`，或 `demo` 里
//! `include_str!` 内嵌的样例。日志文件的路径从头到尾没进过页面。
//!
//! ## 分组依据搬了两次家（M392 `DIAGI18N` → M394 `SEVGROUP`）
//!
//! 1. **分组 = core 的真严重度**（M394）。presenter 的 `dto::Diagnosis` 从前没有 core 的
//!    `Severity`（info / warning / critical），拿置信度硬凑严重度实测 **64 条规则里 12 条会凑错档、
//!    两个方向都有**（`mod.mixin_failure` critical/0.70 沉到第二档、`other.debug_crash` info/0.95
//!    冒到第一档）。M95 r4 的临时解法是**如实**按置信度分组（标题写「高/中/低置信度」——它描述的是
//!    「规则引擎有多确定」）；M392 把 `severity` 补进 DTO 与 [`Finding::severity`]，M394 才把分组
//!    换回来，见 [`Severity`] 与 `GROUPS`。
//!
//!    **换分组不等于删旧维度**：置信度（[`Confidence`] 档 + [`Finding::confidence`] 百分比）仍逐条
//!    画在卡片末行。「引擎有多确定」与「这毛病有多严重」是两件事，用户两件都要看。
//! 2. **`title` / `hint` 不再是中文字段**——它们被**删掉**了。结论文案改成
//!    「[`Finding::cause_key`] + [`Finding::groups`] → 目录取模板并代入占位符」，见
//!    [`super::diagnostic`] 的 `texts_of`。这一步是 i18n 到 100% 的最后一公里：此前英文界面里
//!    会冒出整句中文（GAPS §2.6 的 N3）。
//!
//! 页面自己的界面文案（标题 / 按钮 / 分组名 / 空态）一直全部走 i18n key，这条没变。

use std::collections::BTreeMap;

use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_i18n::Lang;

use super::sessions::Session;

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

/// 一条诊断结论的**严重度**（presenter `DiagnosisSeverity` 的展示投影）。
///
/// ## 它现在是**分组依据**（M394 `SEVGROUP`）
///
/// 用户看崩溃日志要问的是「这毛病有多严重」，所以档位取自 core 规则表的 `severity`，
/// **不是**置信度：拿置信度折算严重度实测 64 条规则里 12 条会进错档、两个方向都有
/// （`mod.mixin_failure` critical/0.70 沉到第二档、`other.debug_crash` info/0.95 冒到第一档）。
///
/// 分组**不删**旧维度：置信度仍逐条画在卡片末行（[`Confidence`] + [`Finding::confidence`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// 提示性：能跑但不最佳。
    Info,
    /// 警告：可疑但结论不确定。
    Warning,
    /// 严重：基本可以确定崩溃原因。
    Critical,
}

/// 分区顺序（**严重度从高到低**：崩溃现场第一段就是最该看的那一档）。
///
/// M394 换分组时**一次对齐**的三样——只换组名、不换另两样，组名是对的，读起来的顺序还是错的：
///
/// | 项 | 口径 |
/// |---|---|
/// | 分组顺序 | 本常量：`Critical` → `Warning` → `Info` |
/// | 组内排序 | [`Logs::group`] 按置信度降序（稳定排序：同分保持 presenter 给的顺序） |
/// | 空组 | **不画组头**：一条都没有的档位不占位置（画出来只是噪音，还让人以为自己漏看了） |
pub const GROUPS: [Severity; 3] = [Severity::Critical, Severity::Warning, Severity::Info];

impl Severity {
    /// 稳定 slug（摘要行、测试与排障用；不是文案——分组标题走 i18n key，见
    /// `pages::logs::diagnostic::key_of`）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

/// 诊断结论的**置信度档**（规则引擎有多确定）。
///
/// ## 它**不再是**分组依据（M394），但也不是死字段
///
/// M95 r4 → M394 之间它是分组轴：那时 DTO 没有严重度，如实说「引擎有多确定」比编一个严重度好
/// （视觉复审 r3 的 P2-2 + 塔裁决）。M394 把严重度补到位、分组换走之后，这一档仍然有用：
///
/// | 用处 | 落点 |
/// |---|---|
/// | 卡片末行的百分比（＋这一档） | `diagnostic::finding_block` 的 `LogsConfidence` |
/// | 一行摘要里的 `rule:band(pct)` | [`Logs::summary`] |
///
/// 留着它是**故意的**：换分组不等于把旧维度删掉。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// 低：规则只是「有点像」。
    Low,
    /// 中：可疑。
    Medium,
    /// 高：基本可以确定。
    High,
}

impl Confidence {
    /// 稳定 slug（摘要行用；不是文案）。
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
    /// 阈值是**档位定义**，不是从严重度反推的代理：这一档说的就是置信度本身。
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
///
/// ## 标题与提示**不在这里**（M392）
///
/// 以前这里有 `title` / `hint` 两个 `String`，装的是 core 规则表 `rules.d/*.yaml` 的中文原文，
/// 页面直接画 ⇒ 英文界面里冒出整句中文。现在页面只拿 [`cause_key`](Self::cause_key) 与
/// [`groups`](Self::groups)，渲染时去 `unlucky_i18n` 目录取文案（见
/// `super::diagnostic` 的 `texts_of`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// 命中的规则 id（`graphics.opengl_not_supported`）：技术标识，原样显示。
    pub rule: String,
    /// 结论文案的目录键（`diagnose.cause.<rule id>`）：取标题与提示的唯一入口。
    pub cause_key: String,
    /// 严重度（**分组依据**，见 [`Severity`]）。
    pub severity: Severity,
    /// 置信度档（**不再分组**：它只在卡片与摘要上作为「引擎有多确定」的标记）。
    pub confidence_band: Confidence,
    /// 0~100 置信度。
    pub confidence: u8,
    /// 模板参数：目录模板里的 `{sourcemod}` / `{exit_code}` 这类占位符要代入的值。
    ///
    /// 规则捕获组（M392）与内置结论的具名参数（M395）共用这一个字段，见
    /// `unlucky_presenter::dto::logs::Diagnosis::groups`。
    pub groups: BTreeMap<String, String>,
    /// core 拼好的结论原文（中文）。**默认不画**：只在模板参数缺了时兜底
    /// （M395 之前写的会话 JSONL 没有 [`groups`](Self::groups)），见 `super::diagnostic` 的 `texts_of`。
    pub system_text: Option<String>,
    /// 建议动作（规则表的 `fix_action[].tool`，技术标识 / 设置项名）。
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
    /// 历史会话（顺序 = presenter 给的顺序，页面不重排；M453 起画成页顶的选择器）。
    ///
    /// 只在会话**结束**时由 host 追加，所以「正在跑的那一场」不在里面——见 [`Session`] 与
    /// `pages::logs::sessions` 的模块文档。
    pub sessions: Vec<Session>,
    /// 当前**显示**的那一场的 id（页头副标题描述的就是它；`None` = 不知道是哪一场）。
    ///
    /// 与页头**同一个来源**（`app/view/logs.rs` 的 `described`）：用户看到「页头写着 A」和
    /// 「选择器里 A 是选中的」必须同时成立，所以这里不许另算一套「当前项」。
    pub selected: Option<String>,
    /// 「回到实时那一场」的落点（M462）：`Some(id)` = 画这个入口，`None` = 一个控件都不画。
    /// 判据在 presenter（`AppState::log_view.live_return()`：历史视图 + 被挡住那一场仍在跑）；
    /// 入口长什么样、点到哪，见 [`super::sessions`] 的模块文档。
    pub back_to_live: Option<String>,
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

        // 选择器每行的文案（M453）：**同一个时机、同一个模板**（`status_line`）——语言换了，
        // 页头与行文案一起换；页面渲染期不再拼句子（`ListItem` 只借 `&'a str`）。
        super::sessions::fill_labels(&mut self.sessions, lang);

        self
    }

    /// 没有任何可显示的东西（空态判据：行与诊断都空）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.findings.is_empty()
    }

    /// 某一**严重度**档下的结论（空组返回空 `Vec`）。
    ///
    /// ## 组内排序（M394 换分组时定下的第二样）
    ///
    /// **置信度降序**。置信度分组时代页面刻意「不重排 presenter 给的顺序」（那时顺序没有第二个轴
    /// 可比，重排只是抖动）；现在最外层是严重度，组内唯一还有信息量的轴就是置信度——同一档里最
    /// 可能的那条排最前，用户从上往下读即可。
    ///
    /// `sort_by_key` 是**稳定排序**：置信度同分时保持 presenter 给的相对顺序（同分就是同分，
    /// 页面不编第二判据）。
    #[must_use]
    pub fn group(&self, severity: Severity) -> Vec<&Finding> {
        let mut findings: Vec<&Finding> = self
            .findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .collect();
        findings.sort_by_key(|finding| std::cmp::Reverse(finding.confidence));
        findings
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
