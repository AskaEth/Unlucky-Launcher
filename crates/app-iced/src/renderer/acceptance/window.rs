//! `[window]`：**本进程给窗口的那些设置**（不是我们从别处猜的）。
//!
//! 唯一产地是 `app/boot/diagnostics.rs::window_settings`（`pages_demo` 通道自己也有一份，
//! 它没有最小尺寸与自定标题）。这里只做「从 `window::Settings` 抽字段 → 排版」。
//!
//! **别再把这些字段写进格式化串**：M98 r1 p2-1 就是这么错的——日志说 `decorations=false`，
//! 而真实 `settings` 可以改成 `true` 而不惊动任何断言（踩坑记录 U1）。

/// 窗口尺寸覆盖（`ULL_WINDOW_SIZE=1520x900`）。
///
/// 为什么是环境变量而不是 `--width`：`cli.rs` 不在 M98 的 scope，而验收矩阵必须有「最小尺寸 /
/// 宽屏」两档，且必须是**真外壳**（自绘三键 + 标题栏）——`pages_demo` example 的标题栏是留白带，
/// 出不了「三键位置」那张图。语义与 `ULL_START_ROUTE` 一致：只影响这一次进程。
pub const WINDOW_SIZE_ENV: &str = "ULL_WINDOW_SIZE";

// ---------------------------------------------------------------- [window]

/// 窗口事实：**从真实的 `window::Settings` 抽出来**的那几个字段（唯一产地见
/// `app/boot/diagnostics.rs` 的 `window_settings`）。
///
/// 为什么要有这个结构：M98 r1 p2-1 —— 第一版把 `resizable=true decorations=false transparent=false`
/// 写在格式化串里，与真实 `settings` 无绑定；定向变异把 `settings.decorations` 改成 `true`，
/// 371 个测试全绿，而日志会与 `[caps]` 互相矛盾（踩坑记录 U1）。现在这些字段只能从
/// [`WindowFacts::from_settings`] 来，没有手抄的入口。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowFacts {
    /// 窗口尺寸（逻辑像素）。
    pub size: (f32, f32),
    /// 最小尺寸（`None` = 没设）。
    pub min: Option<(f32, f32)>,
    /// 可缩放。
    pub resizable: bool,
    /// 系统装饰（`false` ⇒ 自绘三键）。
    pub decorations: bool,
    /// 整窗透明。
    pub transparent: bool,
}

impl WindowFacts {
    /// 从**真正交给 iced/winit 的那个** `window::Settings` 抽字段。
    #[must_use]
    pub fn from_settings(settings: &iced::window::Settings) -> Self {
        Self {
            size: (settings.size.width, settings.size.height),
            min: settings.min_size.map(|size| (size.width, size.height)),
            resizable: settings.resizable,
            decorations: settings.decorations,
            transparent: settings.transparent,
        }
    }
}

/// `[window]` 的**请求值**一行（实际值见模块文档的表：脚本从 `[shot] … scale=` 与 PNG 换算）。
///
/// `title` 是 `Option`：`pages_demo` 那条通道没有自定标题，打 `<默认>` —— 编一个空串会让
/// 「标题是什么」变成猜的。
pub fn report_window(
    facts: &WindowFacts,
    scale: Option<f32>,
    size_source: &str,
    title: Option<&str>,
) {
    eprintln!("{}", window_text(facts, scale, size_source, title));
}

/// 见 [`report_window`]。
#[must_use]
pub fn window_text(
    facts: &WindowFacts,
    scale: Option<f32>,
    size_source: &str,
    title: Option<&str>,
) -> String {
    let min = facts
        .min
        .map_or_else(|| "none".to_owned(), |(w, h)| format!("{w}x{h}"));
    let title = title.map_or_else(|| "<默认>".to_owned(), |text| format!("\"{text}\""));
    format!(
        "[window] requested size={}x{} min={min} resizable={} decorations={} transparent={} scale_requested={} size_source={size_source} title={title}",
        facts.size.0,
        facts.size.1,
        facts.resizable,
        facts.decorations,
        facts.transparent,
        scale.map_or_else(|| "system".to_owned(), |value| value.to_string()),
    )
}

/// 解析 `ULL_WINDOW_SIZE`（`1520x900` / `1520X900` / `1520×900`）。
///
/// 返回 `None` = 不是这个格式（调用方保留默认尺寸并打一行警告，**不**静默忽略）。
#[must_use]
pub fn parse_window_size(raw: &str) -> Option<(f32, f32)> {
    let trimmed = raw.trim();
    let (width, height) = trimmed
        .split_once(['x', 'X', '×', '*'])
        .map(|(w, h)| (w.trim(), h.trim()))?;
    let width: f32 = width.parse().ok()?;
    let height: f32 = height.parse().ok()?;
    (width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0)
        .then_some((width, height))
}

/// 尺寸覆盖 → 实际要开多大 + **尺寸来源**（进 `[window] … size_source=`）。
///
/// 三种来源：`default`（没设 / 设了但解析不了）与 `ULL_WINDOW_SIZE`（生效）。
/// 小于最小尺寸时夹到最小并标 `:clamped-to-min`——iced 的 `min_size` 本来也会拦住，
/// 但那样日志会说「要求 640x480」而实际是 880x560，是典型的「日志说请求、报告当结果」。
#[must_use]
pub fn resolve_window_size(
    raw: Option<&str>,
    default: (f32, f32),
    min: (f32, f32),
) -> ((f32, f32), &'static str) {
    let Some(raw) = raw else {
        return (default, "default");
    };
    let Some((width, height)) = parse_window_size(raw) else {
        return (default, "ULL_WINDOW_SIZE:invalid");
    };

    let clamped = (width.max(min.0), height.max(min.1));
    if clamped == (width, height) {
        (clamped, "ULL_WINDOW_SIZE")
    } else {
        (clamped, "ULL_WINDOW_SIZE:clamped-to-min")
    }
}

/// 窗尺寸（含来源）从进程环境取一遍——`app::boot` 与测试共用同一条路径。
#[must_use]
pub fn window_size(default: (f32, f32), min: (f32, f32)) -> ((f32, f32), &'static str) {
    let raw = std::env::var(WINDOW_SIZE_ENV).ok();
    let resolved = resolve_window_size(raw.as_deref(), default, min);
    if resolved.1.ends_with("invalid") {
        eprintln!(
            "[window] {WINDOW_SIZE_ENV}={:?} 解析不了（要 1520x900 这种），按默认尺寸跑",
            raw.unwrap_or_default()
        );
    }
    resolved
}
