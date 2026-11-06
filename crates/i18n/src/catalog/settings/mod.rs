//! 设置页文案：**一个分组一个文件**，与页面侧的 `pages/settings/**` 同构。
//!
//! 为什么拆到这个粒度：设置页正文接下来由四条并行 mission 分别接（java / download /
//! language+appearance / ai+about），它们都会往设置页加 key。一个分组一个文件之后，每条线只改
//! 自己那个文件，`catalog.rs` 的装配链一行都不用动（链里已经把这五个域接好了）。
//!
//! | 文件 | 分组（`pages/settings` 的 `GROUPS`） | 现状 |
//! |---|---|---|
//! | [`general`] | General（Language / Theme / Startup page / Close button） | 2 条 key（语言） |
//! | [`appearance`] | Appearance（Density / Corner style / Motion / Text scale） | 预留，暂无 key |
//! | [`java`] | Java（Runtime / Default memory / JVM arguments） | 预留，暂无 key |
//! | [`download`] | Downloads（Preferred source / Concurrency / Verify checksums） | 预留，暂无 key |
//! | [`ai`] | AI / About（页面上还没有这个分组） | 预留，暂无 key |
//!
//! 「预留」的文件里是**空转移器**（只把累积表原样交给下一个域）：加 key 时把条目写进它
//! `$($acc)*` 后面即可——签名不用改，`catalog.rs` 不用碰，跨线零冲突。

mod ai;
mod appearance;
mod download;
mod general;
mod java;

pub(crate) use ai::settings_ai_entries;
pub(crate) use appearance::settings_appearance_entries;
pub(crate) use download::settings_download_entries;
pub(crate) use general::settings_general_entries;
pub(crate) use java::settings_java_entries;
