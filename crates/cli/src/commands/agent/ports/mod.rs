//! 端口装配：把 `unlucky-core` 的实现塞进 agent 的五个端口。
//!
//! 为什么这一层在 CLI 而不在 agent：agent 只知道 trait（`#!` 依赖倒置），
//! `crates/app` 是 GUI 的装配处，而 `ull agent serve` 是**headless 的另一个装配入口**。
//! 放这里让 CLI 自洽，也让 GUI 后续复用同一批适配器（把本目录上移到 app 是一次纯搬家）。
//!
//! 每个文件一个端口，与 `crates/agent/src/port*.rs` 一一对应，读起来不用跳来跳去。

pub mod accounts;
pub mod diagnose;
pub mod events;
pub mod game;
pub mod session;
pub mod setting;

use std::sync::Arc;

use unlucky_agent::port::Ports;
use unlucky_core::platform::data_dir::DataMode;
use unlucky_core::platform::paths::Platform;

use crate::context::Ctx;

pub use accounts::CoreAccounts;
pub use diagnose::CoreDiagnose;
pub use game::CoreGame;
pub use session::CoreSessions;
pub use setting::CoreSetting;

/// 按一次 CLI 上下文装配五个端口。
#[must_use]
pub fn build(ctx: &Ctx) -> Ports {
    Ports {
        // 数据根键（`core.*`）落便携目录那份，与 `Ctx::resolve` 同源——CLI `config set`
        // 与 AI `set_config` 共用 `Ctx` 给出的两个落点（finding …-agent-settingport-core-ai）。
        setting: Arc::new(CoreSetting::split(
            ctx.launcher_config(),
            ctx.portable_launcher_config(),
        )),
        sessions: Arc::new(
            CoreSessions::new(ctx.sessions_root()).with_legacy(ctx.legacy_sessions_root()),
        ),
        diagnose: Arc::new(
            CoreDiagnose::new(ctx.sessions_root()).with_legacy(ctx.legacy_sessions_root()),
        ),
        game: Arc::new(CoreGame::new(
            ctx.layout(),
            ctx.game(),
            data_mode(ctx),
            platform(ctx),
        )),
        accounts: Arc::new(CoreAccounts::new(ctx.auth_data())),
    }
}

pub(crate) fn data_mode(ctx: &Ctx) -> DataMode {
    ctx.data.mode
}

pub(crate) fn platform(ctx: &Ctx) -> Platform {
    ctx.platform
}
