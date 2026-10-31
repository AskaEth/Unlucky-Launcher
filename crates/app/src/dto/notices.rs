//! 横幅通知 DTO：实例页/下载页顶部的「需要注意」提示条。
//!
//! 与 Toast 的分工：Toast 是「刚才发生了什么」（3 秒消失），通知条是
//! 「现在需要你处理」（用户点了才收）。隔离模式切换提示（M30 `ModeChangeWarning`）
//! 与启动失败都用它。

use serde::{Deserialize, Serialize};

/// 通知级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoticeLevel {
    /// 一般信息
    Info,
    /// 需要留意（黄条）
    Warn,
    /// 失败（红条）
    Error,
}

impl NoticeLevel {
    /// 级别文案（无障碍/测试用）。
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "信息",
            Self::Warn => "注意",
            Self::Error => "错误",
        }
    }
}

/// 一条通知。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoticeView {
    /// 稳定 id（关闭时按它移除）
    pub id: String,
    /// 级别
    pub level: NoticeLevel,
    /// 标题
    pub title: String,
    /// 正文
    pub body: String,
}

impl NoticeView {
    /// 建一条。
    pub fn new(id: &str, level: NoticeLevel, title: &str, body: &str) -> Self {
        Self {
            id: id.to_string(),
            level,
            title: title.to_string(),
            body: body.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_are_serde_round_trip_safe() {
        let notice = NoticeView::new(
            "isolation:demo",
            NoticeLevel::Warn,
            "隔离模式已切换",
            "旧目录里还有 mods/saves",
        );
        let text = serde_json::to_string(&notice).expect("序列化");
        let back: NoticeView = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(back, notice);
        assert_eq!(back.level.label(), "注意");
    }
}
