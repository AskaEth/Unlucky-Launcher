//! 启动期的取证环境变量解析（`ULL_START_ROUTE` / 动效三个开关）。
//!
//! 从 `app/update.rs` 原样搬出（那个文件在 M105/M106/M109/M113 之后贴着 400 行硬线）：
//! **纯函数、无状态**，只是取值表长到了十条。M192 在这里加的是动效那一组
//! （[`motion_scale`] / [`env_motion_pin`] / [`env_motion_fixture`]）：档位策略只有这一处，
//! 帧泵自己不读环境变量。

use unlucky_design::motion::MotionScale;
use unlucky_presenter::router::Route;

/// 取证开关 `ULL_START_ROUTE` 的取值解析（`settings` / `instances` / `download` / `content` / `logs`）。
///
/// 为什么是环境变量：`crates/app-iced/src/cli.rs` 不在 M93 的 scope，而外观矩阵必须在**设置页**上
/// 出图。语义与 `ULL_APPEARANCE_*` / `ULL_TICK_TRACE` 一致：只影响这一次进程。
pub(super) fn env_start_route() -> Option<Route> {
    match std::env::var("ULL_START_ROUTE").ok()?.trim() {
        "instances" => Some(Route::Instances),
        "download" => Some(Route::Download),
        "logs" => Some(Route::Logs),
        "settings" => Some(Route::Settings),
        // M104：AI 独立页也要能直达（三态矩阵的 Detached 那张）
        "agent" => Some(Route::Agent),
        // M109：内容库（截图矩阵要能直达；页面内的四条侧栏不在路由里）
        "content" => Some(Route::Content),
        // M114：账户页的 D30 矩阵要能直达（`cli.rs` 不在本 mission 的 scope）
        "accounts" => Some(Route::Accounts),
        _ => None,
    }
}

/// 动效档位与相位钉住（M192）：帧泵**只在产品的 `on_boot` 里定档**。
///
/// ## 为什么档位在这里算
///
/// 三个输入只有启动这一拍齐全：①是不是截图通道（`--screenshot`）②实际渲染后端
/// （`backend::effective_kind()`，不是 `--renderer` 的请求值）③`ULL_MOTION` 的强制值。
/// 帧泵自己（`components::motion`）不读环境变量，它只接受一个 [`MotionScale`]——这样动效策略
/// 只有一处，测试也不必摆环境变量。
///
/// | 条件 | 档位 | 为什么 |
/// |---|---|---|
/// | `ULL_MOTION=off` | `NONE` | 一键关（排障 / 想量基线时用） |
/// | 截图通道，且没给 `ULL_MOTION_AT` | `NONE` | D30：出图不许有动画（否则第 3 帧抓到中间态，图就不可复现） |
/// | CPU 渲染（`tiny-skia`） | `HALF` | 时长减半 = 帧数减半。位移是**布局量**，CPU 档最贵的就是逐帧重排 |
/// | 其余（`wgpu`） | `STANDARD` | —— |
/// | `ULL_MOTION=half` / `full` | 强制 | 覆盖上表（出图矩阵要在 GPU 上量同一段动效） |
///
/// 「减少动效」的用户开关**还不存在**（`core/src/setting` 里没有 animation key，见报告 follow-up），
/// 所以本函数只认上面这些来源。
pub(super) fn motion_scale(screenshot: bool, gpu: bool, pinned: bool) -> MotionScale {
    match std::env::var("ULL_MOTION").ok().as_deref().map(str::trim) {
        Some("off") | Some("none") => return MotionScale::NONE,
        Some("half") => return MotionScale::HALF,
        Some("full") => return MotionScale::STANDARD,
        _ => {}
    }
    if pinned {
        // 取证相位优先于「截图必须静」，否则多帧序列图一张都出不来。
        return MotionScale::STANDARD;
    }
    if screenshot {
        return MotionScale::NONE;
    }
    if gpu {
        MotionScale::STANDARD
    } else {
        MotionScale::HALF
    }
}

/// `ULL_MOTION_AT=<ms>`：把动画时钟钉在某一毫秒（多帧序列图取证）。
///
/// 语义与 `ULL_APPEARANCE_*` / `ULL_DIALOG_FIXTURE` 一致：只影响这一次进程。没有它，第 3 帧落在
/// 动画的哪个相位取决于这台机器的渲染速度 —— 那样「0% / 50% / 100% 三张图」是抓阄抓出来的。
pub(super) fn env_motion_pin() -> Option<f64> {
    std::env::var("ULL_MOTION_AT")
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|ms| ms.is_finite() && *ms >= 0.0)
}

/// `ULL_MOTION_FIXTURE`：第 1 帧之后**真的**做一次状态变化（多帧序列图靠它）。
///
/// 为什么要制造变化：页面主体的入场只在「内容标识变了」时起跑（`motion::observe`），而截图通道
/// 从第一帧起画的就是同一页——不制造一次真实的路由变化，就没有可拍的动效。
#[derive(Debug, Clone)]
pub(super) enum MotionFixture {
    /// 跳到一个一级页（走「换页」那一档）。
    Route(Route),
    /// 「列表 → 详情」：先回列表（第 1 帧），再点进第一条实例（第 2 帧）。
    Detail,
}

/// `ULL_MOTION_FIXTURE=<instances|download|logs|settings|content|detail>`。
pub(super) fn env_motion_fixture() -> Option<MotionFixture> {
    Some(match std::env::var("ULL_MOTION_FIXTURE").ok()?.trim() {
        "instances" => MotionFixture::Route(Route::Instances),
        "download" => MotionFixture::Route(Route::Download),
        "logs" => MotionFixture::Route(Route::Logs),
        "settings" => MotionFixture::Route(Route::Settings),
        "content" => MotionFixture::Route(Route::Content),
        "detail" => MotionFixture::Detail,
        _ => return None,
    })
}

/// `ULL_MOTION_FIXTURE_AT=<秒>`：启动后第 N 次心跳（≈第 N 秒）**真的**导航一次。
///
/// ## 为什么需要它（真机空闲取证）
///
/// 复审 r1 要求「空闲 ≥10 s 后点一次导航轨，看 `[tick] motion=` 涨多少」。本机工作区所在会话
/// 没有前台窗口（`GetForegroundWindow()==0`），合成鼠标事件（`mouse_event` 与
/// `PostMessage(WM_LBUTTONDOWN)` 都试过）递不进 iced 的输入层，所以这里换一条**同构**的路：
/// 由 1 Hz 心跳（空闲态本来就有的消息）在第 N 秒做一次真实的路由跳转。
///
/// 两条路在 `update` 之后**完全同一段代码**：消息 → `sync_motion`（起跑）→ `subscription()`
/// （同一拍看见轨道）→ `frames()`。差别只在「消息是谁产生的」，而这正是 P1 要验的那一段。
pub(super) fn env_motion_fixture_at() -> Option<u32> {
    std::env::var("ULL_MOTION_FIXTURE_AT")
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|beats| *beats > 0)
}

/// 把启动那一拍的**事实**写进启动器自己的日志（M187 / r1 复审 P2-4）。
///
/// ## 为什么这一拍只能「先记、后落」
///
/// `.ULL/logs` 的落点由 `platform::data_dir` 的三模式决定（要读配置、follow `redirect.json`），
/// 那件事只有 presenter 的 `HostEnv` 做过。app 层拿不到它，于是这里只 `record`
/// （core 侧缓冲，最多 200 行）；presenter **构造 `HostEnv` 的那一刻**就调
/// `launcher_log::set_dir`（M188 补上的那一半），缓冲一次刷进 `<.ULL>/logs/launcher.log`。
///
/// 效果正是用户投诉要的那份东西：他「双击 → 启动 → 崩 → 找日志」时，`launcher.log` 里从
/// **启动这一拍**（主题 / 语言 / 路由 / 窗口 / 后端）一直写到退出，而不是只有 launch 那一段。
///
/// M188 之后边界也变了：`boot.rs` / `boot/fallback.rs` / `state/*` 那些 `eprintln!` 改走
/// [`crate::app::diag::emit`]（stderr + 文件双写），同样靠这条缓冲接住——所以「一局都没开就崩」
/// 的进程也会留下 `launcher.log`。
pub(crate) fn record_boot_log(app: &super::super::state::App) {
    use unlucky_core::launch::launcher_log;

    launcher_log::record("boot", "app boot");
    launcher_log::record(
        "boot",
        &format!(
            "theme={} lang={:?} route={:?} window={:.0}x{:.0} screenshot={} instances={}",
            app.ui().name(),
            app.lang(),
            app.state.route(),
            app.width,
            app.height,
            app.has_screenshot(),
            app.state.instances.len(),
        ),
    );
    launcher_log::record("boot", &unlucky_cli::commands::msa_client_id_log_line());
    // 后端这一拍也要在日志里：`[probe]` / `[backend]` 那几行由 `renderer::acceptance` 打到
    // stderr（那个模块不在 M188 的 scope），事故之后翻日志的人至少要看得出「这次用的是哪个后端」。
    launcher_log::record(
        "boot",
        &format!(
            "backend request={} effective={} renderer_fallback={}",
            app.options.backend.name(),
            super::super::backend::effective_backend(app.options.backend).name(),
            std::env::var(crate::renderer::FALLBACK_ENV).unwrap_or_else(|_| "-".to_string()),
        ),
    );
}
