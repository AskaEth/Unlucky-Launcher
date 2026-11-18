//! 日志页演示数据：**只给 `pages_demo` 出图与单测**，产品运行时的数据来自 presenter 快照
//! （`app/view.rs::logs_model`）。页面任何路径都不读盘：日志文本要么来自快照，要么来自
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
//! 取值：`empty`（空态）/ `live`（有日志、无诊断）/ 其它或未设 = `crash`（日志 + 三条诊断）。
//! 默认取 `crash`：一屏就能看见页面的全部结构，reviewer 不必先猜环境变量。
//!
//! ## 样例内容的出处
//!
//! [`SAMPLE`] 是按 `core/diagnose/rules.d/samples/graphics/graphics_driver.log` 与
//! `integrity/unsatisfied_link.log` 的形状写的（含中文 + 堆栈 + 200 字符以上的长行），
//! 诊断结论的 `rule` / `title` / `hint` 抄自规则表 `rules.d/integrity.yaml` 与 `graphics.yaml`。
//! **中文在这里是「规则表的输出（数据）」，不是界面文案**：界面文案（标题 / 按钮 / 分组名 /
//! 空态）全部走 `unlucky_i18n` 的 key，见 `logs.rs`。

use unlucky_i18n::Lang;

use super::model::{Confidence, Finding, Level, Line, Logs, Outcome};

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
}

impl State {
    /// 从环境变量取值解析（未设 / 不认识一律 `Crash`）。
    ///
    /// 取值：`empty`（空态）/ `live`（正常跑完的一场）/ `running`（**正在跑**：页头走「正在运行」）
    /// / 其余 = `crash`。
    #[must_use]
    pub fn of_env_value(raw: Option<&str>) -> Self {
        match raw {
            Some("empty") => Self::Empty,
            Some("live") => Self::Live,
            Some("running") => Self::Running,
            _ => Self::Crash,
        }
    }

    /// 造出这个状态的视图模型（`lang` 用于页头副标题的 i18n）。
    #[must_use]
    pub fn build(self, lang: Lang) -> Logs {
        match self {
            Self::Empty => Logs::default().with_status(lang),
            // 正常跑完的一场用**自己的**日志样例：页头写「正常结束」却配崩溃日志尾行是自己打自己
            // （视觉复审 r2 的 p3）。
            Self::Live => Logs {
                instance: Some("开发测试".to_owned()),
                when: Some("11-02 19:22".to_owned()),
                outcome: Some(Outcome::Ok),
                lines: parse(LIVE_SAMPLE),
                ..Logs::default()
            }
            .with_status(lang),
            // 运行中：还没有结局、也不在 session 索引里（`host::logs::sessions` 只在结束时追加），
            // 所以既没有实例名也没有 outcome —— 页头只能走「正在运行」那条中性文案。
            Self::Running => Logs {
                live: true,
                lines: parse(LIVE_SAMPLE),
                ..Logs::default()
            }
            .with_status(lang),
            Self::Crash => crash().with_status(lang),
        }
    }
}

/// harness 入口：按 [`STATE_ENV`] 挑状态。
#[must_use]
pub fn demo(lang: Lang) -> Logs {
    State::of_env_value(std::env::var(STATE_ENV).ok().as_deref()).build(lang)
}

/// 崩溃现场（日志 + 三条诊断，覆盖 critical / warning 两个分组）。
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

/// 三条诊断（`rule` / `title` / `hint` 抄自规则表；置信度档走 [`Confidence::of`]，
/// 也就是 DTO 缺 `severity` 时产品路径用的同一条档位定义）。
fn findings() -> Vec<Finding> {
    vec![
        finding(
            "integrity.unsatisfied_link",
            85,
            "找不到原生库（natives）：lwjgl_opengl.dll 没解压出来或被杀软删了",
            "重装组件（重新解压 natives）；Windows 上检查杀软隔离区",
            Some("reinstall natives"),
        ),
        finding(
            "graphics.gl_operation_failure",
            70,
            "某个 mod/光影包做了非法的 OpenGL 操作（GL_INVALID_OPERATION）",
            "换掉最近装的光影/资源包，或把资源包分辨率降下来",
            Some("change_graphics.renderer = OpenGL 兼容模式"),
        ),
        finding(
            "integrity.no_class_def_found",
            60,
            "NoClassDefFoundError：缺少类文件（库没下全 / jar 被解压 / mod 不配套）",
            "先检查 libraries 是否完整，再排查 mods 里的解压目录",
            Some("reinstall libraries"),
        ),
    ]
}

fn finding(rule: &str, confidence: u8, title: &str, hint: &str, action: Option<&str>) -> Finding {
    Finding {
        rule: rule.to_owned(),
        confidence_band: Confidence::of(confidence),
        title: title.to_owned(),
        hint: hint.to_owned(),
        confidence,
        action: action.map(str::to_owned),
    }
}
