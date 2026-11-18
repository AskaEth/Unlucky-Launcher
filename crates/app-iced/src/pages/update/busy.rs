//! 「这一条更新动作此刻能不能点」（M161：把「按钮亮着、点了没反应」收掉）。
//!
//! 判据只有一条：有没有检查/下载正在跑。三处必须同源——
//!
//! | 处 | 读的是 |
//! |---|---|
//! | presenter `Runtime::inflight()` | `begin_check` / `begin_apply` 置位，`adopt` 清位 |
//! | 设置页两颗按钮 | [`Update::can_check`] / [`Update::can_apply`]（本文件） |
//! | 设置页状态行 | 「正在检查…」/「正在安装…」（同一份 `busy()` 判据） |
//! | app 的消息入口 | `App::update_busy`（`app/update/update_action.rs`）——最后一道闸 |
//!
//! 为什么拆分到本文件：`pages/update.rs` 贴着 400 行硬线（AGENTS §3-1），
//! 而这一组判据是「独立的一块问题」（能不能点），独立成文件后更好找。

use super::{Update, UpdateStatus};

impl Update {
    /// 有一条检查/下载正在跑。
    ///
    /// 两个字段都要看：`status == Checking` 是「查版本」，`applying` 是「下载/校验/替换」。
    /// presenter 侧同一个判据是 `Runtime::inflight()`（`begin_check` / `begin_apply` 置位，
    /// `adopt` 清位），页面这层是它的**只读投影**——按钮的禁用态必须与它同源，
    /// 否则会出现「按钮亮着、点了没反应」。
    #[must_use]
    pub fn busy(&self) -> bool {
        self.applying || matches!(self.status, UpdateStatus::Checking)
    }

    /// 「立即检查」此刻能点吗。
    ///
    /// 忙碌时**禁用而不是消失**：入口消失会让人以为功能没了，禁用 + 状态行那句
    /// 「正在检查…」才说得清「它在干活，等一下」。
    #[must_use]
    pub fn can_check(&self) -> bool {
        !self.busy()
    }

    /// 「下载并安装」此刻能点吗（要有候选，且不忙）。
    #[must_use]
    pub fn can_apply(&self) -> bool {
        self.available().is_some() && !self.busy()
    }
}
