//! AI 助手栏 / AI 独立页的文案（M104 `AGENTRAIL`）。
//!
//! 这两处**共用同一套会话视图**（`crates/app-iced/src/pages/agent/`），所以文案也只在这里出现一次：
//! 栏上的折叠/拆出、页上的收回、两处都要的发送/中断/空会话引导。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节：本宏把条目**追加**到 `$acc` 之后，
//! 再把「还没装配的域」原样交给 `$next`。条目顺序 = `Key` 的声明顺序（金表 `catalog::golden` 守门）。
//!
//! **新增 key 时按顺序追加在最后**：金表是「拆分前那份表的逐字快照」，重排既有行会让它红。

macro_rules! agent_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── AI 助手（右栗 + 独立页共用）
            AgentTitle => ("Assistant", "助手"),
            // M119 r1 复审 P2-1：这条一度被改成中性的「会话」，理由是「跑什么后端由端点标签说」——
            // 但端点标签在壳里根本不存在，于是默认配置下用户看到的是一个像是接上了模型的助手。
            // 现在把诚实的话写回来，而且它是**常驻**的（标题下就一行，不点也能看到）。
            AgentSubtitle => ("Simulated assistant · no real model is connected", "模拟助手 · 还没接真实模型"),
            AgentCollapse => ("Collapse", "收起"),
            AgentDetach => ("Pop out", "拆出"),
            AgentDock => ("Dock back", "收回"),
            AgentOpen => ("Open assistant", "打开助手"),
            AgentStripLabel => ("AI", "助手"),
            AgentSend => ("Send", "发送"),
            AgentStop => ("Stop", "中断"),
            // 输入框上再说一次：用户真要动手的位置就在这儿
            AgentPlaceholder => ("Message the simulated assistant…", "给模拟助手发消息…"),
            AgentEmptyTitle => ("Nothing in this session yet", "这个会话还没有内容"),
            // 空会话是最容易被误读的一屏（没有人说过话，很容易以为是「连上了但还没问」）
            AgentEmptyBody => ("Ask about instances, downloads or logs. This assistant is simulated: no real model is connected.", "可以问实例、下载或日志。当前是模拟助手，没有接真实模型。"),
            AgentNotePending => ("Placeholder shell: your message is kept here, nothing was sent to a model.", "占位外壳：消息留在这里，没有发给任何模型。"),
            AgentNoteStopped => ("Stopped by you.", "你中断了这次回复。"),
            AgentUnread => ("{count} unread", "{count} 条未读"),
            AgentResize => ("Drag to resize the assistant rail", "拖动调整助手栏宽度"),
            AgentWidthLabel => ("Width {width}", "宽度 {width}"),
            AgentRoleYou => ("You", "你"),
            AgentRoleAssistant => ("Assistant", "助手"),
            AgentRoleNotice => ("Notice", "提示"),
            // ── M119：失败态与写操作确认（接线后才有；`AgentRail::error` / `rail.confirm`）
            //
            // 为什么必须成 key：任务要求「presenter 出错时 UI 显示**具名**错误」，
            // 而 M106 立的规矩是界面文案一律走 catalog——没有键就只能硬编码中英。
            AgentErrorTitle => ("Assistant error", "助手出错"),
            AgentErrorUnavailable => ("Couldn't reach the assistant backend. Check the endpoint, then try again.", "连不上助手后端：检查端点后重试。"),
            AgentErrorNotImplemented => ("This assistant request isn't wired up yet.", "这个助手请求还没接线。"),
            AgentConfirmRequest => ("The assistant wants to change something: {summary}", "助手想改一项设置：{summary}"),
            AgentConfirmAllow => ("Allow", "允许"),
            AgentConfirmDeny => ("Deny", "拒绝"),
            // 头部常驻小标记（P2-1）：与副标题/占位符互为冗余，三个位置都能看到「这是模拟」
            AgentSimulated => ("Simulated", "模拟"),
            // ── M119 r2：三处文案的**真实态**配对（模拟态用上面那三条已有的 key）
            //
            // 复审 r2 的 P2-1：上一轮只有标题胶囊由 `simulated` 驱动，副标题/占位符/空态正文是
            // 无条件取词，而那三条译文里写着「模拟 / 没接真实模型」——接上真 transport 之后
            // 这三句会**反方向**地留着。现在配成对：`simulated == true` 走模拟态，
            // `false` 走这三条中性话（`pages/agent/tests/disclaimer.rs` 钉住四处一起变）。
            AgentSubtitleReal => ("Conversation", "会话"),
            AgentPlaceholderReal => ("Message the assistant…", "给助手发消息…"),
            AgentEmptyBodyReal => ("Ask about instances, downloads or logs.", "可以问实例、下载或日志。"),
        } }
    };
}

pub(crate) use agent_entries;
