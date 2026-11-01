//! 截图验收通道的参数解析与流程（`--screenshot*` / `--data-dir` / `--mock`）。
//!
//! 为什么用进程内渲染而不是截屏：验收机没有交互桌面（`CopyFromScreen` 直接失败），
//! 而 Slint 的软件渲染器能把场景渲进内存像素缓冲（`Window::take_snapshot`），
//! 结果与屏幕上逐像素一致且不需要 GPU（M41 落地的通道，这里只加页面/中间态参数）。
//!
//! 中间态（安装进度 / 启动事件流）走真实后端：`--screenshot-install <版本>` 会真的发起
//! 一次安装并停若干毫秒后取图，`--screenshot-launch <实例>` 同理。

use std::path::PathBuf;

use crate::backend::AppOptions;
use crate::dto::AppCommand;
use crate::ui_bind::AppUi;

/// 一次截图请求。
#[derive(Debug, Clone, Default)]
pub struct ShotOptions {
    /// 输出 PNG 路径
    pub path: String,
    /// 页面名（instances / download / java / logs / settings / account / appearance / agent）
    pub page: Option<String>,
    /// 打开新建实例向导
    pub wizard: bool,
    /// 发起一次真实安装并停 `dwell_ms` 后取图（中间态）
    pub install: Option<String>,
    /// 安装走哪个加载器（`--screenshot-loader forge`；缺省 = 原版）
    pub loader: Option<String>,
    /// 发起一次真实启动并停 `dwell_ms` 后取图（中间态）
    pub launch: Option<String>,
    /// 打开一个日志会话（日志页有数据态：日志行 + 诊断卡片）
    pub session: Option<String>,
    /// 中间态停留毫秒（默认 2500）
    pub dwell_ms: u64,
}

impl ShotOptions {
    /// 中间态默认停留时长。
    pub const DEFAULT_DWELL_MS: u64 = 2_500;
}

/// 从命令行读启动选项（数据根 / mock）。
pub fn app_options() -> AppOptions {
    AppOptions {
        data_root: value_of("--data-dir").map(PathBuf::from),
        mock: flag("--mock"),
    }
}

/// 从命令行读截图请求；没有 `--screenshot` 时返回 `None`。
pub fn shot_options() -> Option<ShotOptions> {
    let path = value_of("--screenshot")?;
    let page = value_of("--screenshot-page");
    let mut options = ShotOptions {
        path,
        page: if flag("--screenshot-settings") {
            Some("appearance".to_string())
        } else {
            page
        },
        wizard: flag("--screenshot-wizard"),
        install: value_of("--screenshot-install"),
        loader: value_of("--screenshot-loader"),
        launch: value_of("--screenshot-launch"),
        session: value_of("--screenshot-session"),
        dwell_ms: value_of("--screenshot-dwell")
            .and_then(|value| value.parse().ok())
            .unwrap_or(ShotOptions::DEFAULT_DWELL_MS),
    };
    if options.dwell_ms < 500 {
        options.dwell_ms = 500;
    }
    options.page = options.page.filter(|page| !page.trim().is_empty());
    Some(options)
}

/// 执行截图流程：切页 → （可选）中间态 → 渲染 PNG。
///
/// # Errors
/// 窗口显示/快照/写文件失败时返回错误。
pub fn capture(app: &AppUi, options: &ShotOptions) -> Result<(), slint::PlatformError> {
    if let Some(page) = &options.page {
        app.open_page(page);
        // 先给后端一次全量刷新：版本清单要在后台线程里拉（12h 缓存），
        // 下载页/Java 页/向导需要它才有「有数据态」。
        let warmup = if matches!(page.as_str(), "download" | "java") {
            4_000
        } else {
            1_200
        };
        app.dispatch_and_run(AppCommand::RefreshAll, warmup);
    }
    if options.wizard {
        app.open_wizard();
    }
    if let Some(version) = &options.install {
        app.dispatch_and_run(
            install_command(version, options.loader.as_deref()),
            options.dwell_ms,
        );
    }
    if let Some(session) = &options.session {
        app.open_session(session);
        app.dispatch_and_run(AppCommand::RefreshLogSessions, options.dwell_ms.min(1_000));
    }
    if let Some(instance) = &options.launch {
        app.dispatch_and_run(
            AppCommand::LaunchInstance(instance.clone()),
            options.dwell_ms,
        );
    }
    app.render_screenshot(&options.path)?;
    // 启动中间态截完就把进程收掉：截图进程退出后子进程会变孤儿，
    // 所以在取图之后显式 stop（kill_tree），不留给系统。
    if let Some(instance) = &options.launch {
        app.dispatch_and_run(AppCommand::StopInstance(instance.clone()), 1_500);
    }
    Ok(())
}

/// 截图通道的安装指令：缺省原版，给了 `--screenshot-loader` 就走向导同一条路
/// （`CreateInstance`），因此 Forge/NeoForge 的中间态也能取图。
fn install_command(version: &str, loader: Option<&str>) -> AppCommand {
    match loader.map(crate::dto::LoaderKind::parse) {
        Some(Some(kind)) => AppCommand::CreateInstance(crate::dto::CreateRequest {
            version: version.to_string(),
            loader: kind,
            loader_version: String::new(),
            name: version.to_string(),
        }),
        _ => AppCommand::InstallVersion(version.to_string()),
    }
}

/// `--flag` 是否存在。
pub fn flag(name: &str) -> bool {
    std::env::args().skip(1).any(|arg| arg == name)
}

/// `--key value` / `--key=value` 的取值。
pub fn value_of(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix(&format!("{name}=")) {
            return Some(value.to_string());
        }
        if arg == name {
            return args.next();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dwell_is_a_visible_mid_state() {
        const DEFAULT: u64 = ShotOptions::DEFAULT_DWELL_MS;
        let dwell = DEFAULT;
        assert!(dwell >= 1_000, "中间态至少要停 1 秒才看得见进度");
    }

    #[test]
    fn options_without_screenshot_are_none() {
        // 测试进程没有 --screenshot 参数
        assert!(
            shot_options().is_none() || std::env::args().any(|arg| arg.contains("--screenshot"))
        );
    }

    #[test]
    fn value_of_ignores_absent_flags() {
        assert!(value_of("--definitely-not-a-real-flag").is_none());
    }
}
