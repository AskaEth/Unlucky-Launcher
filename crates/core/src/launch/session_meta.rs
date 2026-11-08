//! 会话元信息（`session_start` 的 payload 来源 + 索引摘要字段）。
//!
//! 从 `session.rs` 拆出以守住“单文件 ≤400 行”的硬约束；公开路径仍是
//! `launch::session::SessionMeta`（在 `session.rs` 里 re-export）。

use crate::launch::platform::PlatformCtx;

/// 会话元信息（`session_start` 的 payload 来源）。
#[derive(Debug, Clone, Default)]
pub struct SessionMeta {
    /// 实例 id。
    pub instance_id: String,
    /// 实例名。
    pub instance_name: String,
    /// 游戏版本。
    pub game_version: String,
    /// 启动器名。
    pub launcher_name: String,
    /// 启动器版本。
    pub launcher_version: String,
    /// 系统名。
    pub os_name: String,
    /// 架构。
    pub os_arch: String,
    /// 位数。
    pub os_bits: u32,
}

impl SessionMeta {
    /// 基本构造。
    pub fn new(
        instance_id: impl Into<String>,
        instance_name: impl Into<String>,
        game_version: impl Into<String>,
    ) -> Self {
        Self {
            instance_id: instance_id.into(),
            instance_name: instance_name.into(),
            game_version: game_version.into(),
            ..Self::default()
        }
    }

    /// 补启动器信息。
    pub fn with_launcher(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.launcher_name = name.into();
        self.launcher_version = version.into();
        self
    }

    /// 补平台信息。
    pub fn with_platform(mut self, ctx: &PlatformCtx) -> Self {
        self.os_name = ctx.os.manifest_name().to_string();
        self.os_arch = ctx.arch.clone();
        self.os_bits = if cfg!(target_pointer_width = "64") {
            64
        } else {
            32
        };
        self
    }
}
