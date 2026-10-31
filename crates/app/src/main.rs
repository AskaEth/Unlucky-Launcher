//! Unlucky-Launcher 桌面壳（`unlucky_app`）：Slint UI + Metro 视觉。
//!
//! 边界（决策 D3 / D16）：本 crate 是**唯一**允许依赖 UI 框架的 crate；
//! 它只经「快照 + 事件流 + 注入的显示服务」消费 core/agent，不直接触碰其内部类型
//! （P0 的 host 是 `mock` 假后端，`crate::dto` 是唯一跨界类型集合）。
//!
//! 渲染策略（`DevDocs/PlanDoc/01c-native-gui-options.md` §6）：
//! GL 优先（femtovg），启动参数 `--software-renderer` 或 Win7 老机探测失败时切 CPU 软渲染。
//!
//! 主题与外观（D17 / 09 §101-113）：
//! - 主题三态 `appearance.theme_mode ∈ {auto, light, dark}`（出厂 auto）；
//! - 「外观（高级）」四件（背景图 / 面板半透明度 / 玻璃拟态 / 强调色）出厂全关；
//! - 全部走 `core::setting` 的 schema（`crate::settings_bridge`），AI 可写并且**热切**
//!   （tick 里 1s 一次 mtime 比对）；系统主题由 `core::platform::theme_probe` 对齐 + 兜底轮询。
//!
//! 架构分层（本文件只有一个职责：把窗口拉起来）：
//! - `state.rs` / `prefs.rs` / `router.rs` / `dialogs.rs` / `toast.rs`：纯 Rust 状态（可单测，无 Slint）
//! - `dto/`：跨边界 DTO 与事件/指令
//! - `conv.rs`：状态 → Slint 视图模型
//! - `ui_bind.rs` + `callbacks.rs`：窗口/状态/后端缝合 + 回调接线
//! - `settings_bridge.rs`：`config/launcher.json` 的读写与热切（core::setting 的 schema API）
//! - `bg_image.rs` + `png.rs`：背景图/模糊层加工与截图编码（09 §101/§103 的预生成手法）
//! - `mock.rs` + `mock_data.rs`：P0 假后端（脚本化事件）
//!
//! 关于 unsafe：本 crate 不写 unsafe，workspace lint 已设 `unsafe_code = "deny"`。
//! 这里不能升级成 `#![forbid(unsafe_code)]`——Slint 生成的代码自带 `#[allow(unsafe_code)]`，
//! 而 forbid 无法被内层 allow 覆盖（会直接编译失败）。同理 Slint 生成类型不带文档注释，
//! 只能在 crate 级放开 `missing_docs`（库 crate 仍由 workspace lint 保持 warn）。
// 关于 dead_code：P0 骨架里有一批「已定义、等接线」的公共 API——事件通道 / 偏好键登记 /
// 诊断文案 / mock 数据构造器等，它们被单测覆盖但业务侧要 P1 才用到。逐条 `#[allow]`
// 会把噪音藏进实现细节，这里统一放开并在 P1 接线时逐个转正（届时删掉本行即可暴露真死码）。
#![allow(missing_docs, dead_code)]

mod appearance_bind;
mod backend;
mod bg_image;
mod callbacks;
mod callbacks_pages;
mod conv;
mod dialogs;
mod dto;
mod host;
mod mock;
mod mock_data;
mod page_sync;
mod png;
mod prefs;
mod router;
mod screenshot;
mod settings_bridge;
mod state;
mod theme_runtime;
mod toast;
mod ui_bind;
mod wizard;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    apply_renderer_flag();

    let app = ui_bind::AppUi::with_options(MainWindow::new()?, screenshot::app_options());

    // 截图验收通道（M41 视觉验收；无交互桌面/无 GPU 环境下的唯一可靠取图方式：
    // 不截屏幕，而是让 Slint 把当前场景渲染进内存像素缓冲再自己编码 PNG）。
    // P3 追加页面选择与中间态（安装进度 / 启动事件流），见 `crate::screenshot`。
    if let Some(options) = screenshot::shot_options() {
        screenshot::capture(&app, &options)?;
        return Ok(());
    }

    app.start_timer();
    app.run()
}

/// Win7 老机 / 无可用 OpenGL 驱动时的软渲染兜底开关（01c §6 的 Slint 核心卖点）。
fn apply_renderer_flag() {
    let software = std::env::args()
        .skip(1)
        .any(|arg| arg == "--software-renderer");
    if software {
        std::env::set_var("SLINT_BACKEND", "winit-software");
    }
}
