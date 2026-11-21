//! 「停止」与「结论」两件事（M388）：`AgentHost` 上的一条很短的通道，但顺序是承重的。
//!
//! 从 `agent.rs` 拆出来守 400 行硬线：那边已经背着契约表、后端选择与自愈重试，再塞这两块
//! 就顶线了。拆法照 `real/` 的样子——同一个模块的子文件，`impl AgentHost` 接着写。

use super::AgentHost;
use crate::app::state::presenter::transport::Conclusion;

impl AgentHost {
    /// 后端报上来的**收场事实**（M388；目前只有「超时 N 秒」）。
    ///
    /// 取走即清。UI 拿到它之后按当前语言渲染一条结论（[`Self::conclude`]）——句子不能在 worker
    /// 里写死：那条线程没有 catalog，写死的中文会出现在英文界面上。
    pub(crate) fn take_conclusion(&mut self) -> Option<Conclusion> {
        self.conclusion.lock().ok().and_then(|mut slot| slot.take())
    }

    /// 本轮以**一条结论**收尾（清流式标记 + 落一条系统条目；句子由调用方按语言给）。
    pub(crate) fn conclude(&mut self, notice: String) {
        self.presenter.agent_conclude(notice);
    }

    /// 用户按了「停止」（M388）：**当场**收尾，不等后端回声。
    ///
    /// 三步的顺序是承重的（复审 r1 次级观察 2：v1 把「清队列」放在了「静音」前面，中间有一个
    /// 几微秒的窗口能让一条 delta 在结论之后被 apply）：
    /// 1. `presenter.agent_stop(..)` → **先静音映射器**（此后 worker 再吐什么都进不了会话）
    ///    + 置取消旗标（能打断就打断）+ 收尾（清 `streaming`、落一条「已取消」条目）；
    /// 2. **再清队列**：静音之前已经挤进队列的事件（含 `ConfirmBridge` 直推、绕过 `Mapper` 的那条
    ///    确认请求）都不许活到结论之后；
    /// 3. 输入区立刻可用（第 1 步已经做掉）。
    ///
    /// 返回值：`true` = 信号已交给**活的内核**（它在 await 处析构请求，这是真打断）；
    /// `false` = 没有可打断的调用 ⇒ 本轮**只丢弃**（不是中断）。这句区别写进日志，不糊在界面上。
    pub(crate) fn stop(&mut self, notice: String) -> bool {
        let interrupted = self.presenter.agent_stop(notice);
        let mut discarded = 0usize;
        if let Ok(mut queue) = self.queue.lock() {
            discarded = queue.len();
            queue.clear();
        }
        self.last_failure = None;
        eprintln!(
            "[agent] 停止：丢弃队列中 {discarded} 条迟到事件；{}",
            if interrupted {
                "已置取消旗标（内核在 await 处析构请求 future）"
            } else {
                "没有可打断的内核调用 ⇒ 本轮只丢弃，不是中断"
            }
        );
        interrupted
    }
}
