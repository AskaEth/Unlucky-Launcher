//! # unlucky-app-iced · iced 版桌面壳（P4 / D26）
//!
//! 本 crate 是「UI 从头写」的第一步：**只做骨架**（能开窗、颜色/字体对、层级在），
//! 业务页面、组件库、i18n 全文案、玻璃 shader 都是后续 mission。
//!
//! | 模块 | 职责 | 不负责 |
//! |---|---|---|
//! | [`theme`] | 令牌（`unlucky-design`）→ iced 类型：`Color` / `Font` / `Radius` / `Theme` | 发明令牌值 |
//! | `cli` | `--renderer` / `--software-renderer` / `--theme` / `--screenshot*` 参数 | 业务开关 |
//! | `probe` | 渲染后端探测（D27）+ 写 `ICED_BACKEND`，失败自动降级 CPU | 绘制 |
//! | `fonts` | 字体栈求值（D28 前半）：平台族名 → 系统字体库校验 → 启动日志 | 中文字体下载 |
//! | `renderer` | **渲染后端判据表**（M97）+ wgpu 预检 + `log` 桥：什么条件选 wgpu / 什么时候降级 CPU / 日志该长什么样 | 实际绘帧 |
//! | `app` / `shell` | Elm 三件套 + 自绘标题栏 / 导航轨 / 内容骨架 | 业务逻辑 |
//! | `shot` | `window::screenshot` → PNG（验收通道） | 图像比对 |
//! | `icons` | 令牌里的 SVG path → iced `svg` 控件 | 画新图标 |
//! | [`ui`] | 界面语言的入口层（M106）：`LangState` = 用户档位 + 探测结论，目录由它选 | 翻译（那是 `unlucky-i18n`） |
//! | `platform` | 平台窗口能力（D36）：整窗 α 的纯判据 + win32 FFI（补分层样式 + 下发 α）—— **全仓唯一** `unsafe` 落点 | 窗口创建 / 业务开关 |
//!
//! ## 消费纪律
//!
//! **组件层零硬编码色值/字号/半径/间距**：一律走 [`theme::Ui`]。守门测试在
//! `tests/no_hardcoded_tokens.rs`（扫 `src/**`，出现 `#RRGGBB` / `Color::from_rgb(` 即红）。
//!
//! ## 与 Slint 版的关系
//!
//! 并存期：`crates/app` 仍出 `unlucky-launcher`，本 crate 出 `unlucky-launcher-iced`。
//! 等价验收后翻牌（bin 改名 + 旧 crate 降级），步骤写在
//! `DevDocs/SubAgentsTalk/app-iced-骨架与翻牌.md`。

pub mod pages;
pub mod platform;
pub mod renderer;
pub mod shot;
pub mod theme;

mod app;
mod cli;
mod fonts;
mod icons;
mod probe;
mod shell;
mod ui;

pub use cli::{Backend, Options};
pub use probe::Probe;

/// 启动 UI（阻塞到窗口关闭）。渲染后端由 `probe` 在 `run()` 之前写进 `ICED_BACKEND`。
pub fn run(options: Options) -> iced::Result {
    app::run(options)
}

/// 与 [`run`] 同义，但把命令行做成最终结论：`Options::from_env()` + 启动日志。
///
/// 二进制入口用 [`Options::from_env`] + [`run`] 自己拼（要额外处理 `--diff`），这个函数留给
/// 集成测试与将来的 CLI 复用。
///
/// # Errors
/// 参数不合法时返回人话（不 panic、不 `exit`）。
pub fn run_from_env() -> Result<iced::Result, String> {
    let options = Options::from_env()?;
    Ok(run(options))
}
