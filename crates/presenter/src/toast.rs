//! Toast 中心：右下角非阻塞提示（下载完成 / 复制成功 / 后台任务结论）。
//!
//! 与模态对话框队列（[`crate::dialogs`]）分开：Toast 不阻塞、不排序、可叠加，
//! 只做「先进先出 + 上限裁剪 + TTL 到期」。UI 侧由 Rust 定时器每帧调用 [`ToastCenter::expire`]。

use serde::{Deserialize, Serialize};

/// Toast 标识。
pub type ToastId = u64;

/// Toast 语义色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToastKind {
    /// 一般信息
    Info,
    /// 成功
    Success,
    /// 警告
    Warn,
    /// 错误
    Error,
}

/// 一条 Toast；`remaining_ms` 由 [`ToastCenter::expire`] 递减。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toast {
    /// 队列内唯一 id（由 [`ToastCenter::push_request`] 重新编号）
    pub id: ToastId,
    /// 语义色
    pub kind: ToastKind,
    /// 提示文案
    pub text: String,
    /// 剩余存活毫秒（0 = 用默认 TTL，入队时补）
    pub remaining_ms: u64,
}

/// Toast 中心：先进先出，超出上限丢最旧的。
#[derive(Debug)]
pub struct ToastCenter {
    items: Vec<Toast>,
    next_id: ToastId,
    ttl_ms: u64,
}

impl Default for ToastCenter {
    fn default() -> Self {
        Self::new()
    }
}

impl ToastCenter {
    /// 默认 4s 后自动消失。
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            next_id: 0,
            ttl_ms: 4_000,
        }
    }

    /// 同时最多显示的条数。
    pub const MAX_VISIBLE: usize = 4;

    /// 入队一条 Toast。
    pub fn push(&mut self, kind: ToastKind, text: impl Into<String>) -> ToastId {
        self.push_request(Toast {
            id: 0,
            kind,
            text: text.into(),
            remaining_ms: 0,
        })
    }

    /// 入队一条已组装好的 Toast（事件通道用）；id 重新编号，`remaining_ms == 0` 视为用默认 TTL。
    pub fn push_request(&mut self, mut toast: Toast) -> ToastId {
        self.next_id += 1;
        toast.id = self.next_id;
        if toast.remaining_ms == 0 {
            toast.remaining_ms = self.ttl_ms;
        }
        let id = toast.id;
        self.items.push(toast);
        while self.items.len() > Self::MAX_VISIBLE {
            self.items.remove(0);
        }
        id
    }

    /// 当前显示列表（UI 模型来源）。
    pub fn items(&self) -> &[Toast] {
        &self.items
    }

    /// 手动关闭（点了关闭按钮）。
    pub fn dismiss(&mut self, id: ToastId) -> bool {
        if let Some(at) = self.items.iter().position(|toast| toast.id == id) {
            self.items.remove(at);
            true
        } else {
            false
        }
    }

    /// 时间推进：移除到期 Toast。
    pub fn expire(&mut self, elapsed_ms: u64) {
        for toast in &mut self.items {
            toast.remaining_ms = toast.remaining_ms.saturating_sub(elapsed_ms);
        }
        self.items.retain(|toast| toast.remaining_ms > 0);
    }

    /// 当前条数。
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// 是否没有 Toast。
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{ToastCenter, ToastKind};

    #[test]
    fn toasts_expire_and_cap_the_stack() {
        let mut center = ToastCenter::new();
        center.push(ToastKind::Info, "a");
        center.push(ToastKind::Info, "b");
        assert_eq!(center.len(), 2);

        for i in 0..10 {
            center.push(ToastKind::Success, format!("t{i}"));
        }
        assert_eq!(center.len(), ToastCenter::MAX_VISIBLE);

        center.expire(3_000);
        assert_eq!(center.len(), ToastCenter::MAX_VISIBLE);
        center.expire(1_001);
        assert!(center.is_empty());
    }

    #[test]
    fn dismiss_removes_one_toast() {
        let mut center = ToastCenter::new();
        let first = center.push(ToastKind::Info, "a");
        center.push(ToastKind::Warn, "b");
        assert!(center.dismiss(first));
        assert!(!center.dismiss(first));
        assert_eq!(center.len(), 1);
    }
}
