//! 对话框队列（HMCL 队列语义 + Metro 化，09 §3）。
//!
//! 语义：**一次只显示一条**。新请求按来源插队（用户操作 > 后台任务 > 提示），同来源 FIFO；
//! 当前条被确认/取消后由 [`DialogQueue::advance`] 推进到下一条，UI 只渲染 `current`。
//! 非阻塞提示走 [`crate::toast`]，不占模态队列。

/// 对话框标识。
pub type DialogId = u64;

/// 对话框来源：决定插队优先级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DialogSource {
    /// 提示类（教程、首次使用说明）
    Hint = 0,
    /// 后台任务（下载完成、需要选择镜像源）
    BackgroundTask = 1,
    /// 用户操作触发（AI 敏感写操作的二次确认）
    UserAction = 2,
}

impl DialogSource {
    /// 数值越大越优先。
    pub fn priority(self) -> u8 {
        self as u8
    }
}

/// 对话框类型（09 §3：确认 / 进度 / 输入 / 错误）。
#[derive(Debug, Clone, PartialEq)]
pub enum DialogKind {
    /// 确认：主按钮 + 次按钮
    Confirm {
        /// 主按钮文字（如「允许」）
        primary: String,
        /// 次按钮文字（如「拒绝」）
        secondary: String,
    },
    /// 输入：单行文本
    Input {
        /// 输入框标签
        label: String,
        /// 初始值
        value: String,
        /// 主按钮文字
        primary: String,
    },
    /// 进度：不可点主按钮（任务结束后由 [`crate::dto::AppEvent::DialogClosed`] 收起）
    Progress {
        /// 阶段文案
        label: String,
        /// 0.0~1.0
        progress: f32,
    },
    /// 错误：含诊断入口提示
    Error {
        /// 主按钮文字
        primary: String,
        /// 诊断/修复提示
        hint: Option<String>,
    },
}

impl DialogKind {
    /// 主按钮文字。
    pub fn primary_label(&self) -> &str {
        match self {
            Self::Confirm { primary, .. }
            | Self::Input { primary, .. }
            | Self::Error { primary, .. } => primary,
            Self::Progress { .. } => "取消",
        }
    }

    /// 次按钮文字（None 表示不显示次按钮）。
    pub fn secondary_label(&self) -> Option<&str> {
        match self {
            Self::Confirm { secondary, .. } => Some(secondary),
            Self::Error { .. } => Some("忽略"),
            Self::Input { .. } | Self::Progress { .. } => None,
        }
    }

    /// 是否可交互（进度条不可点主按钮）。
    pub fn is_blocking_progress(&self) -> bool {
        matches!(self, Self::Progress { .. })
    }
}

/// 一条对话框请求。
#[derive(Debug, Clone, PartialEq)]
pub struct DialogRequest {
    /// 队列内唯一 id（由 [`DialogQueue::push_request`] 重新编号）
    pub id: DialogId,
    /// 来源（决定插队优先级）
    pub source: DialogSource,
    /// 标题
    pub title: String,
    /// 正文
    pub body: String,
    /// 类型与按钮
    pub kind: DialogKind,
}

/// 模态队列：一次一条 + 来源插队。
#[derive(Debug, Default)]
pub struct DialogQueue {
    current: Option<DialogRequest>,
    pending: Vec<DialogRequest>,
    next_id: DialogId,
}

impl DialogQueue {
    /// 新建空队列。
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前应显示的对话框。
    pub fn current(&self) -> Option<&DialogRequest> {
        self.current.as_ref()
    }

    /// 排队中（不含当前）条数。
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// 是否完全没有对话框（当前 + 排队）。
    pub fn is_empty(&self) -> bool {
        self.current.is_none() && self.pending.is_empty()
    }

    /// 入队一条请求（UI 内部调用方用，id 由队列自增分配）。没有当前条时立即成为当前条。
    ///
    /// 返回值用于后续 [`Self::close`] / 进度更新。
    pub fn push(
        &mut self,
        source: DialogSource,
        title: impl Into<String>,
        body: impl Into<String>,
        kind: DialogKind,
    ) -> DialogId {
        self.next_id += 1;
        self.push_request(DialogRequest {
            id: self.next_id,
            source,
            title: title.into(),
            body: body.into(),
            kind,
        })
    }

    /// 入队一条已经组装好的请求（事件通道用）。
    ///
    /// **id 归生产者**：这里原样保留 `request.id`（不重编号），因为关闭事件
    /// [`crate::dto::AppEvent::DialogClosed`] 携带的是生产者 id；重编号会让
    /// 「任务完成 → 自动收起进度框」在多来源弹框时静默失配。生产者负责自身 id 唯一。
    pub fn push_request(&mut self, request: DialogRequest) -> DialogId {
        let id = request.id;
        let source = request.source;
        if self.current.is_none() {
            self.current = Some(request);
        } else {
            let at = self
                .pending
                .iter()
                .position(|queued| queued.source < source)
                .unwrap_or(self.pending.len());
            self.pending.insert(at, request);
        }
        id
    }

    /// 关闭指定对话框：若是当前条则推进到下一条，否则从队列移除。
    ///
    /// 返回 `true` 表示确实移除了。
    pub fn close(&mut self, id: DialogId) -> bool {
        if self
            .current
            .as_ref()
            .is_some_and(|current| current.id == id)
        {
            self.advance();
            return true;
        }
        if let Some(at) = self.pending.iter().position(|queued| queued.id == id) {
            self.pending.remove(at);
            return true;
        }
        false
    }

    /// 推进到下一条排队请求（当前条已被处理）。
    pub fn advance(&mut self) {
        self.current = if self.pending.is_empty() {
            None
        } else {
            Some(self.pending.remove(0))
        };
    }

    /// 关闭当前条并推进到下一条（确认 / 取消 / 点遮罩都走这里）。
    ///
    /// 返回 `false` 表示本来就没有当前条（幂等，双击不会误推进队列）。
    pub fn close_current(&mut self) -> bool {
        if self.current.is_none() {
            return false;
        }
        self.advance();
        true
    }

    /// 更新进度对话框的进度（任务回调直接喂进来）。
    pub fn update_progress(&mut self, id: DialogId, progress: f32) {
        let Some(current) = &mut self.current else {
            return;
        };
        if current.id != id {
            return;
        }
        if let DialogKind::Progress {
            progress: value, ..
        } = &mut current.kind
        {
            *value = progress.clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DialogKind, DialogQueue, DialogSource};

    fn confirm() -> DialogKind {
        DialogKind::Confirm {
            primary: "确定".into(),
            secondary: "取消".into(),
        }
    }

    #[test]
    fn shows_one_dialog_at_a_time() {
        let mut queue = DialogQueue::new();
        let first = queue.push(DialogSource::BackgroundTask, "A", "", confirm());
        let second = queue.push(DialogSource::BackgroundTask, "B", "", confirm());

        assert_eq!(queue.current().map(|d| d.id), Some(first));
        assert_eq!(queue.pending_len(), 1);

        queue.advance();
        assert_eq!(queue.current().map(|d| d.id), Some(second));
        assert_eq!(queue.pending_len(), 0);

        queue.advance();
        assert!(queue.is_empty());
    }

    #[test]
    fn user_action_jumps_the_queue() {
        let mut queue = DialogQueue::new();
        queue.push(DialogSource::Hint, "提示", "", confirm());
        queue.push(DialogSource::Hint, "提示2", "", confirm());
        queue.push(DialogSource::BackgroundTask, "后台", "", confirm());
        let urgent = queue.push(DialogSource::UserAction, "确认删除", "", confirm());

        // 当前仍是队首（一次一条），但排队里用户操作插到最前 → 关掉当前条后立刻轮到它
        queue.advance();
        assert_eq!(queue.current().map(|d| d.id), Some(urgent));
    }

    #[test]
    fn closing_non_current_dialog_removes_it_from_pending() {
        let mut queue = DialogQueue::new();
        let first = queue.push(DialogSource::Hint, "A", "", confirm());
        let second = queue.push(DialogSource::Hint, "B", "", confirm());

        assert!(queue.close(second));
        assert_eq!(queue.pending_len(), 0);
        assert_eq!(queue.current().map(|d| d.id), Some(first));
        assert!(!queue.close(999));
    }

    #[test]
    fn progress_dialog_can_be_updated() {
        let mut queue = DialogQueue::new();
        let id = queue.push(
            DialogSource::BackgroundTask,
            "下载中",
            "",
            DialogKind::Progress {
                label: "资源".into(),
                progress: 0.0,
            },
        );
        queue.update_progress(id, 1.7);
        match queue.current().map(|d| &d.kind) {
            Some(DialogKind::Progress { progress, .. }) => {
                assert!((*progress - 1.0).abs() < f32::EPSILON);
            }
            other => panic!("unexpected dialog kind: {other:?}"),
        }
    }
}
