//! 设置页「助手（AI）」分组（M174）：左列第 6 组的**挂载点**与本线的消息口。
//!
//! 本文件只留三样东西，正文/状态/读写都在 `pages/settings/ai/` 里：
//!
//! | 这里 | 内容 |
//! |---|---|
//! | [`GROUP`] | 左列名 / 图标 / 演示行（产品路径走 [`super::ai::view::body`]） |
//! | [`AI_GROUP`] | 本组在 `GROUPS` 里的下标（与 General / Update 同一条**按下标分发**：本组尚未迁到 `Group.body` 槽位，见下节） |
//! | [`Message`] | 本线的消息口（落点 `app/update/settings/ai_about.rs`） |
//! | [`State`] | 转发 `ai::State`（`Groups.ai_about` 的载荷类型不变，`settings/state.rs` 一行不用改） |
//!
//! ## 为什么 `body` 是 `None` 而不是 `Some(..)`
//!
//! `Group.body` 的槽位（M179 起）拿的是 [`States`](super::States) 与**生效语言** `Lang`——本组要的
//! 两件它其实都装得下。本组仍走按下标分发，是因为**还没迁过去**：槽位要求返回 `Option<Element>`，
//! 而 `ai::view::body` 返回 `Element`，把本组（以及 Java 组）统一迁到槽位上是一次独立重构。
//! M106 的语言行与 M159 的更新组当初也是同一个理由走上这条路，代价是 `settings/mod.rs` 多一行 `else if`。
//!
//! ## 密钥纪律
//!
//! [`Message::EditKey`] 的载荷是用户刚敲的明文（输入框必须有它），它**只**活在这个进程里：
//! 落盘走 `ai::store::save`（不经 `AppCommand::SetSetting` 那条可序列化通道），
//! 上屏默认掩码（`State::reveal` 才明文），`Debug` 由 `ai::state::Secret` 掩码。

pub use super::ai::State;

use unlucky_design::icons::IconKind;

use super::Group;

/// 本组在 [`super::GROUPS`] 里的下标（**必须是最后一个**：加组只追加不重排）。
pub(crate) const AI_GROUP: usize = 5;

/// 「助手」分组（左列名固定英文：左列是导航，与导航轨同源，不进 catalog）。
pub(super) const GROUP: Group = Group {
    name: "Assistant",
    icon: IconKind::Ai,
    // 只给 harness / 窄窗「按组堆叠」路径用的演示行：产品路径走 `ai::view::body`。
    rows: &[
        ("Provider", "OpenAI compatible"),
        ("Endpoint", "https://api.deepseek.com/v1"),
        ("Model", "deepseek-chat"),
    ],
    body: None,
};

/// 本线的消息（**只归本线**）：加变体只改本文件 + `app/update/settings/ai_about.rs`。
#[derive(Debug, Clone)]
pub enum Message {
    /// 启用开关（`ai.enabled`）
    ToggleEnabled(bool),
    /// 选提供商（`ai.provider`）
    PickProvider(super::ai::ProviderId),
    /// 端点草稿（`ai.base_url`）
    EditBaseUrl(String),
    /// 模型名草稿（`ai.model`）
    EditModel(String),
    /// 新密钥草稿（明文只在内存里；空 = 保留已存的那把）。
    ///
    /// 载荷是 [`super::ai::Secret`] 而不是 `String`：**`Debug` 会打掩码**。这条消息会被 harness
    /// 的 `{:?}` 打进 stderr（r1 复审 P2-2 的现场），也会出现在任何 `assert_eq!` 失败输出里，
    /// 用裸 `String` 等于给密钥开了一条到日志的路。
    EditKey(super::ai::Secret),
    /// 密钥输入框明文/掩码
    Reveal(bool),
    /// 保存（校验 + 落盘）
    Save,
    /// 测试连接（异步；结果走 [`Message::ProbeDone`]）
    Test,
    /// 测试连接的结果（分类 + 数值 + 脱敏详情）
    ProbeDone(Box<super::ai::ProbeResult>),
}

impl Message {
    /// 一条本组消息 → 设置页的投递口（`pages::settings::GroupMessage::AiAbout`）。
    #[must_use]
    pub fn into_group(self) -> super::GroupMessage {
        super::GroupMessage::AiAbout(self)
    }
}
