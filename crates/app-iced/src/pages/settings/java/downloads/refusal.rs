//! 「下载按钮为什么没开始」的五档原因（M197）。
//!
//! 从 `downloads.rs` 拆出来守 400 行硬线（与 `downloads/tests.rs` 同一种拆法）。
//! 点了没反应是禁止的：每一种拒绝都要在下载区顶上写清是哪一档。

/// 「没开始」的四种原因（每一种都要在界面上说清，不许点了没反应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// 目录里没有这个大版本
    UnknownMajor,
    /// 这一档在这台机器上没有发行版承接
    NoVendor,
    /// 已经有一个作业在跑
    Busy,
    /// 没有可写的设置文件
    NoStorage,
    /// 有设置文件，但安装根解析不出来（standard 模式拿不到 %APPDATA% 之类的系统目录）
    RootUnresolved,
}

impl Refusal {
    /// 文案 key（页面自己的拒绝原因，与 `state::Refusal` 同一套渲染路径）。
    #[must_use]
    pub const fn label(self) -> unlucky_i18n::Key {
        use unlucky_i18n::Key;
        match self {
            Self::UnknownMajor => Key::SettingsJavaReasonDownloadUnknownMajor,
            Self::NoVendor => Key::SettingsJavaReasonDownloadNoVendor,
            Self::Busy => Key::SettingsJavaReasonDownloadBusy,
            Self::NoStorage => Key::SettingsJavaNoStorage,
            Self::RootUnresolved => Key::SettingsJavaReasonRootUnresolved,
        }
    }
}
