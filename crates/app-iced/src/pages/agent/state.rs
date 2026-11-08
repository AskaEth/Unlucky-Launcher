//! 右栗的**纯 UI 状态**：三档状态机 + 展开宽度 + 草稿 + 键盘焦点环 + [`AgentUi`]。
//!
//! 布局数学（宽度 / 磁铁吸附 / 中栗下限）在 [`super::metrics`]；这里只管「UI 状态怎么变」。
//!
//! ## 会话内容为什么不在这个文件里（M119）
//!
//! M104 的 `AgentUi` 自带 `transcript` / `busy` / `unread` 三个字段，数据来自内置样本
//! （[`super::demo`]）——那是「壳先搭好、等 M103 接」的过渡态。M119 把这三个字段**删掉**：
//! 会话的唯一来源是 presenter 的 `AgentRail`（`app/state.rs` 的 `agent_host`），右栗与独立页
//! 直接读它。于是在这个文件里再放一份就是第二份真相，而两份真相必然漂。
//!
//! ## 三档状态（用户定案 D16）
//!
//! | 档位 | 右栗宽度 | 会话视图在哪 | 切法 |
//! |---|---|---|---|
//! | [`RailState::Collapsed`] | 48（图标条 + 未读点） | 不画 | `⌄` 收起 / `Esc` |
//! | [`RailState::Expanded`] | 280~360（默认 320） | 右栗面板内 | 点图标条 / 收回 |
//! | [`RailState::Detached`] | 0 | `Route::Agent` 独立页 | `⇥` 拆出 |
//!
//! 三档是**同一个会话**的三种呈现：切档只改 [`RailState`] 与路由栈，草稿与焦点一条都不丢
//! （会话本身在 presenter 上，三档共享得比原来更彻底）。
//!
//! ## 未读 / busy 从哪来
//!
//! 都在 `AgentRail` 上：未读由 presenter 按「栏可见吗」累加、`agent_mark_read()` 清零；
//! `busy` 由「有流式消息 || 有写操作待确认」算出来。壳里**不再自己数**未读。

use iced::widget::text_editor;

use super::metrics::{
    content_width, effective_state, magnet_width, rail_width, DEFAULT_EXPANDED_WIDTH,
};

/// 三档状态（顺序 = 展开程度，测试与文件名按它遍历）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailState {
    /// 48px 图标条。
    Collapsed,
    /// 280~360 的会话面板。
    Expanded,
    /// 收进 `Route::Agent` 独立页（右栗消失）。
    Detached,
}

impl RailState {
    /// 三档全序（截图矩阵按它铺）。
    pub const ALL: [RailState; 3] = [
        RailState::Collapsed,
        RailState::Expanded,
        RailState::Detached,
    ];

    /// 稳定名字（文件名 / `ULL_AGENT_RAIL` 的取值 / 失败信息）。
    pub const fn name(self) -> &'static str {
        match self {
            RailState::Collapsed => "collapsed",
            RailState::Expanded => "expanded",
            RailState::Detached => "detached",
        }
    }

    /// 反向解析（取证开关用）。
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.name() == name)
    }
}

/// 键盘软焦点顺序（Tab 环）。顺序 = 屏幕上的阅读顺序：头部动作 → 输入区动作 → 输入框。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// `⌄` 收起。
    Collapse,
    /// `⇥` 拆出（独立页上则是 `⇤` 收回，同一个焦点位）。
    Detach,
    /// 确认条的「允许」（只在有待确认写操作时存在于环上）。
    ///
    /// M119 r1 / 复审 P2-2：确认条原本只有鼠标路径——键盘用户在写操作确认这一步**卡死**
    /// （`busy` 锁住发送，`Stop` 走的是主后端、清不掉 presenter 的待确认）。这两格就是那条键盘通路。
    ConfirmAllow,
    /// 确认条的「拒绝」。
    ConfirmDeny,
    /// 发送。
    Send,
    /// 中断（只在流式中出现）。
    Stop,
    /// 输入框（真实焦点由 iced 的 `text_editor` 持有，这里只记「轮到它了」）。
    Composer,
}

impl Focus {
    /// 稳定名字（日志 / 测试）。
    pub const fn name(self) -> &'static str {
        match self {
            Focus::Collapse => "collapse",
            Focus::Detach => "detach",
            Focus::ConfirmAllow => "confirm-allow",
            Focus::ConfirmDeny => "confirm-deny",
            Focus::Send => "send",
            Focus::Stop => "stop",
            Focus::Composer => "composer",
        }
    }

    /// Tab 环上「这一档真正存在的控件」。
    ///
    /// `confirm_pending` = 有待确认的写操作（那时条上多两格：允许 / 拒绝）。`busy` 在等确认时
    /// 也是真（presenter 的 `busy = streaming || confirm.is_some()`），所以环上还会多一格中断。
    fn order(state: RailState, busy: bool, confirm_pending: bool) -> Vec<Focus> {
        let mut order = Vec::new();
        match state {
            RailState::Collapsed => order.push(Focus::Collapse),
            RailState::Expanded => {
                order.push(Focus::Collapse);
                order.push(Focus::Detach);
            }
            RailState::Detached => order.push(Focus::Detach),
        }
        if confirm_pending {
            order.push(Focus::ConfirmAllow);
            order.push(Focus::ConfirmDeny);
        }
        if state != RailState::Collapsed {
            order.push(Focus::Send);
            if busy {
                order.push(Focus::Stop);
            }
            order.push(Focus::Composer);
        }
        order
    }

    /// 下一个焦点（`shift` = 反向）。当前焦点不在环上时从头/尾进。
    ///
    /// `busy` 由 presenter 的 rail 给（壳里不自己数）：流式中环上多一格 `Stop`。
    pub fn step(self, shift: bool, state: RailState, busy: bool, confirm_pending: bool) -> Focus {
        let order = Self::order(state, busy, confirm_pending);
        let Some(at) = order.iter().position(|focus| *focus == self) else {
            return if shift { order.last() } else { order.first() }
                .copied()
                .unwrap_or(Focus::Collapse);
        };
        let next = if shift {
            (at + order.len() - 1) % order.len()
        } else {
            (at + 1) % order.len()
        };
        order[next]
    }
}

/// 助手栏的 UI 状态（挂在 `App` 上；**唯一**的写入口是 `app/update/agent.rs`）。
pub struct AgentUi {
    /// 三档状态。
    pub rail: RailState,
    /// 展开档的吸附宽度（三档共享一个值：收起再展开回到同一宽度）。
    expanded: f32,
    /// 输入框（多行；iced 的 `text_editor` 自己持有编辑状态，这里只放内容）。
    pub draft: text_editor::Content,
    /// 键盘软焦点。
    pub focus: Focus,
    /// 焦点环是否「被键盘走过」（`Tab` 置 true，鼠标点输入框 / 换档置 false）。
    ///
    /// 为什么需要它：`Enter` 全局订阅与输入框自己的 `key_binding` 共用同一个键。若不加这道闸，
    /// 用户在**别的页面**用鼠标点过发送键之后，随手按 Enter 也会把草稿发出去。只有真的按过
    /// `Tab`（= 用户在用键盘巡航助手栏）才让 `Activate` 生效。
    pub keyboard_mode: bool,
    /// 拖拽中的指针 x（`Some` = 正在拖）。
    pointer: Option<f32>,
}

impl AgentUi {
    /// 出厂态：**折叠** + 空草稿 + 无未读（未读由 presenter 按事件累加，M119）。
    ///
    /// 为什么默认折叠：1180 下展开 320 会把实例页挤成单列（中栗 852 < 816 的两列门槛之外还够，
    /// 但 1024 就掉档了），用户第一次打开启动器不该先被一个助手栏改掉主页面版面。
    pub fn new() -> Self {
        Self {
            rail: RailState::Collapsed,
            expanded: DEFAULT_EXPANDED_WIDTH,
            draft: text_editor::Content::new(),
            focus: Focus::Composer,
            keyboard_mode: false,
            pointer: None,
        }
    }

    /// 当前展开宽度（吸附后的值）。
    pub fn expanded(&self) -> f32 {
        self.expanded
    }

    /// 直接指定展开宽度（只给取证开关用：三档预设值之一）。
    pub fn set_expanded(&mut self, width: f32) {
        self.expanded = width;
    }

    /// 是否正在拖拽（shell 据此铺一层取指针的透明层）。
    pub fn is_dragging(&self) -> bool {
        self.pointer.is_some()
    }

    /// 这一窗宽下**实际**生效的档位（窄窗里 `Expanded` 会退化成 `Collapsed`）。
    pub fn effective_rail(&self, window_width: f32) -> RailState {
        effective_state(window_width, self.rail, self.expanded)
    }

    /// 右栗当前宽度（窗口宽度是判据之一：窗口变窄时展开档会被夹住）。
    ///
    /// 名字故意不叫 `width`：`tests/no_hardcoded_tokens.rs` 会把任何 `.width(数字)` 当成
    /// 「布局尺寸写死」拦下来，而测试里 `agent.width(1180.0)` 会被误判（那条门禁扫的是控件调用点，
    /// 分不清是谁的方法）。
    pub fn rail_width(&self, window_width: f32) -> f32 {
        rail_width(window_width, self.rail, self.expanded)
    }

    /// **交给页面布局系统的宽度**（窗口 − 右栗 − 拖拽缝）。
    ///
    /// `refresh_view` 用它算视图模型：面板展开后页面按更窄的宽度排版，内容才会「收缩」而不是
    /// 顶到面板底下。
    pub fn content_width(&self, window_width: f32) -> f32 {
        content_width(window_width, self.rail, self.expanded)
    }

    /// 收起（`⌄` / `Esc`）。未读**不**清零：收起后新消息仍要能显示成点（清零由 app 层在
    /// 「打开栏」时调 `agent_mark_read` 做——那是 presenter 的语义，不是 UI 的）。
    pub fn collapse(&mut self) {
        self.keyboard_mode = false;
        if self.rail == RailState::Expanded {
            self.rail = RailState::Collapsed;
            self.focus = Focus::Collapse;
        } else if self.rail == RailState::Detached {
            self.dock();
        }
    }

    /// 展开（点图标条）。展开即视为读过——**清零动作在 app 层**（`mark_read`），这里只改档位。
    pub fn expand(&mut self) {
        self.keyboard_mode = false;
        self.rail = RailState::Expanded;
        self.focus = Focus::Collapse;
    }

    /// 拆成独立页（`⇥`）。UI 状态一条不动（会话在 presenter 上，本来就不归这里管）。
    pub fn detach(&mut self) {
        self.keyboard_mode = false;
        self.rail = RailState::Detached;
        self.focus = Focus::Detach;
    }

    /// 收回右栗（`⇤` / 独立页上的 `Esc`）。
    pub fn dock(&mut self) {
        self.keyboard_mode = false;
        self.rail = RailState::Expanded;
        self.focus = Focus::Collapse;
    }

    /// 输入框内容变化（`text_editor::Action` 由 app 层转过来）。
    ///
    /// 这里**不**分 Enter / Shift+Enter：分流在 `conversation::composer::binding_for`（非 shift 的
    /// Enter 根本不会变成 `Action`，它直接发 `Message::Submit`）。所以走到这里的 `Edit(Enter)`
    /// 一定是「Shift+Enter 换行」那一支，`perform` 就够。
    pub fn edit(&mut self, action: text_editor::Action) {
        use iced::widget::text_editor::Action;

        if matches!(action, Action::Click(_)) {
            // 鼠标点了输入框 = 用户切回鼠标，键盘巡航结束（见 `keyboard_mode` 的理由）
            self.focus = Focus::Composer;
            self.keyboard_mode = false;
        }
        self.draft.perform(action);
    }

    /// 清空输入框（发送成功之后调：草稿已经交给 presenter 了）。
    pub fn clear_draft(&mut self) {
        self.draft = text_editor::Content::new();
    }

    /// 输入框是否为空（发送键的禁用判据）。
    pub fn draft_is_blank(&self) -> bool {
        self.draft.text().trim().is_empty()
    }

    /// 拖拽开始（按下分隔条）。`0.0` 只是占位：第一次 `drag_to` 就会换成真实指针位置。
    pub fn drag_start(&mut self) {
        self.pointer = Some(0.0);
    }

    /// 拖拽中：`x` = 指针在**窗口**里的横坐标（0 = 窗口左缘）。
    pub fn drag_to(&mut self, window_width: f32, x: f32) {
        if self.pointer.is_none() {
            return;
        }
        self.pointer = Some(x);
        self.expanded = magnet_width(window_width, window_width - x);
    }

    /// 拖拽结束（松手）。
    pub fn drag_end(&mut self) {
        self.pointer = None;
    }

    /// Tab 走一格（走一次就把键盘巡航打开）。`busy` / `confirm_pending` 都由 presenter 的 rail 给。
    ///
    /// **待确认的那一次 Tab 只点亮焦点环、不移动焦点**：待确认出现时 app 层已经把焦点放到
    /// 「允许」上（阻塞态，见 `App::sync_confirm_focus`），但**不会**替用户把键盘巡航打开
    /// （M119 r2 / 复审 P2-2：自动 arm 会让用户随手敲的空格变成「批准」）。所以第一次 `Tab`
    /// 是「我要用键盘」的显式表态，此时焦点原地不动——否则第一次 Tab 会把「允许」挪到「拒绝」，
    /// 用户按 `Enter` 就是反着裁决。
    pub fn step_focus(&mut self, shift: bool, busy: bool, confirm_pending: bool) {
        let arming = !self.keyboard_mode
            && confirm_pending
            && matches!(self.focus, Focus::ConfirmAllow | Focus::ConfirmDeny);
        self.keyboard_mode = true;
        if !arming {
            self.focus = self.focus.step(shift, self.rail, busy, confirm_pending);
        }
    }
}

impl Default for AgentUi {
    fn default() -> Self {
        Self::new()
    }
}
