//! 账户轨道（三轨统一抽象，`09 §G6` / `D9`）。

use serde::{Deserialize, Serialize};

/// 账户轨道（三轨统一抽象，`09 §G6` / `D9`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountKind {
    /// 离线账户（HMCL 兼容 UUID）
    Offline,
    /// 微软正版
    Microsoft,
    /// authlib-injector（第三方皮肤站）
    Authlib,
}

impl AccountKind {
    /// 徽标文字（CLI/终端用；GUI 走 i18n 词表，不读这里的中文）。
    pub fn label(self) -> &'static str {
        match self {
            Self::Offline => "离线",
            Self::Microsoft => "微软",
            Self::Authlib => "第三方",
        }
    }

    /// 稳定 id（事件/持久化用；与 core 的 `kind.as_str()` 同词表）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Microsoft => "microsoft",
            Self::Authlib => "authlib_injector",
        }
    }

    /// 从 core 的稳定 id 解析（未知轨道按离线处理，不让一个坏条目毁掉整个列表）。
    pub fn parse(raw: &str) -> Self {
        match raw {
            "microsoft" => Self::Microsoft,
            "authlib_injector" => Self::Authlib,
            _ => Self::Offline,
        }
    }
}
