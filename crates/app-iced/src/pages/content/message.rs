//! 内容库页的**自有**消息（M109）。
//!
//! 与设置页同一个范式（`pages/settings/message.rs` + `pages::Message::Settings`）：页面自己的一套
//! 意图在这个文件里声明，`pages::Message` 只留一个 `Content(..)` 投递口，于是「加一条内容库消息」
//! 不碰共享枚举的正文，也不碰 `app/update/page_messages.rs` 那条不许 `_ =>` 的 match。
//!
//! **一条纪律**：这里的每个变体都是**意图**，不是状态变更。改了条件 ≠ 已经发请求
//! （`QueryChanged` 只改输入框里的字，`Search` 才发指令）；点了「装入」≠ 已装上
//! （装上的证据是 `InstallProgress` 事件回流到面板）。

use unlucky_presenter::dto::{ContentKind, ContentSort, ContentSource, LoaderKind};

/// 内容库页的消息。
#[derive(Debug, Clone)]
pub enum Message {
    /// 换内容类型（左侧四条一级侧栏）
    SetKind(ContentKind),
    /// 关键词输入框变了（**只改草稿**，不 dispatch）
    QueryChanged(String),
    /// 提交搜索（零偏移重来）
    Search,
    /// 换来源（Modrinth / CurseForge）
    SetSource(ContentSource),
    /// 换排序
    SetSort(ContentSort),
    /// 换游戏版本过滤（`None` = 不限）
    SetVersion(Option<String>),
    /// 换加载器过滤（`None` = 不限）
    SetLoader(Option<LoaderKind>),
    /// 选中一个项目（**页面内选中**，不 dispatch）
    Select(String),
    /// 选目标实例（一键装入的对象）
    SetTarget(String),
    /// 加载下一页
    LoadMore,
    /// 同条件重试（失败态的出口）
    Retry,
    /// 装进目标实例
    Install,
    /// 导入/导出区（M113：整合包导入与导出；一切细节在 `packui::Message`）
    Pack(super::packui::PackMessage),
}
