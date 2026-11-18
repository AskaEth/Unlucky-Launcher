//! 设置页的**分组消息投递口**（M101 `SETTINGSMSG`）：四条填页线各写各的变体，不再抢
//! `pages/mod.rs` 的枚举尾部与 `app/update/page_messages.rs` 那条不许兜底的 match。
//!
//! ## 这一层为什么存在
//!
//! M100 把设置页的**文件**按分区拆开了，但四条线真正的交汇处仍是同一处追加锚点：
//! `pages::Message` 的枚举尾部 + `page_messages.rs` 的 match 臂（该 match 有
//! `the_page_match_has_no_wildcard_fallback` 守门，不许 `_ =>`）。四个变体、四条线、两个共享
//! 文件 ⇒ 四路并行必然四路冲突。M101 只做一件事：把「加消息」从共享文件搬进各组自己的文件——
//! `pages::Message` 从此只有**这一个**设置页变体。
//!
//! ```text
//! pages::Message::Settings(GroupMessage)
//!   ├─ Java(java::Message)                       → settings/java.rs
//!   ├─ Download(download::Message)               → settings/download.rs
//!   ├─ LanguageAppearance(language_appearance::Message) → settings/language_appearance.rs
//!   └─ AiAbout(ai_about::Message)                → settings/ai_about.rs
//! ```
//!
//! 四个载荷**必须**是四个不同类型、分别定义在四个文件里；否则「两条线往同一个枚举里加变体」的
//! 冲突只是从 `pages/mod.rs` 搬到了这里。`each_group_owns_its_own_message_type` 用 `type_name`
//! 里的模块路径守这一条。
//!
//! ## 加一支设置页消息要改几个文件（M101 r2 之后）
//!
//! **2 个**：你自己那条线的消息口文件（`settings/<线>.rs`）加一个变体，加上你那条落点
//! （`app/update/settings/<线>.rs`）接一条臂——两处都只归你。`pages/mod.rs`、
//! `page_messages.rs`、`settings/message.rs`、`settings/state.rs`、`app/update/settings.rs` 都不用动。
//!
//! ## 穷尽性：抓得住什么 / 抓不住什么
//!
//! - **抓得住**：某条线加了变体却忘了接线 → 编译器在 `app/update/settings.rs` 的 match 上报
//!   **E0004**（那里对 [`GroupMessage`] 做穷尽匹配，没有 `_ =>`）。注意 `page_messages.rs`
//!   那条守卫只认 `pages::Message`，**它抓不住组内变体**——把守卫下放到 `settings.rs` 正是本层
//!   的第二个目的，两个 dispatch 各守一层。
//! - **抓不住**：handler 忽略消息（`let _ = message; Task::none()`）或两个 handler 的**身体**被
//!   写成一样——类型对得上就编得过。那要人读，或者等真接业务后由「点了没反应」的用例抓。
//!
//! 类型还顺手挡掉了另一种错：把 `Java(m)` 转给 `on_settings_download` 编不过（载荷类型不同），
//! 所以「接错组」这一半是编译期兜住的，「接了个空壳」那一半不是。
//!
//! ## 测试
//!
//! 本文件的 [`tests::each_group_owns_its_own_message_type`]；dispatch 侧的
//! `the_group_dispatch_has_no_catch_all_arm` 在 `app/update/settings/tests.rs`（`app/update/settings.rs`
//! 只是 `#[cfg(test)] mod tests;` 的声明点，别去那里找）。

use super::{ai_about, download, java, language_appearance};

/// 设置页分组消息：**唯一**一个进 `pages::Message` 的投递口。
///
/// 每个变体只归一条线，载荷类型定义在那条线自己的文件里（见模块文档的四个箭头）。
/// 加变体 = 改自己那个文件 + 改 `app/update/settings/<线>.rs`，两处都只归你。
#[derive(Debug, Clone)]
pub enum GroupMessage {
    /// Java 分组（`settings/java.rs`）。
    Java(java::Message),
    /// Downloads 分组（`settings/download.rs`）。
    Download(download::Message),
    /// 语言 + 外观两组的共同消息口（`settings/language_appearance.rs`）。
    LanguageAppearance(language_appearance::Message),
    /// AI / About 分组（`settings/ai_about.rs`）。
    AiAbout(ai_about::Message),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 四条线的载荷**必须**住在四个不同文件里。
    ///
    /// 抓得住：① 为了少建一个文件，把两个组的 `Message` 塞进同一个文件（四条线又会撞同一个
    /// 追加锚点，本 mission 的目的当场作废）；② 谁把某个分组文件改名后忘了同步（`type_name`
    /// 里带模块路径，改名就红）。
    /// 抓不住：变体装得对不对（那是 `app/update/settings.rs` 的 E0004 与其守卫的事），也抓不住
    /// handler 身体写空/写反（类型对得上就编得过）。
    #[test]
    fn each_group_owns_its_own_message_type() {
        let homes = [
            (
                std::any::type_name::<java::Message>(),
                "settings::java::Message",
            ),
            (
                std::any::type_name::<download::Message>(),
                "settings::download::Message",
            ),
            (
                std::any::type_name::<language_appearance::Message>(),
                "settings::language_appearance::Message",
            ),
            (
                std::any::type_name::<ai_about::Message>(),
                "settings::ai_about::Message",
            ),
        ];

        for (actual, expected) in homes {
            assert!(
                actual.ends_with(expected),
                "载荷类型 {actual} 不在它自己的文件里（应为 {expected}）：\
                 两个组共用一个消息类型，四条线又会撞同一个追加锚点"
            );
        }
    }
}
