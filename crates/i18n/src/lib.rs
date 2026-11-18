//! # unlucky-i18n · 国际化地基（D28）
//!
//! 本 crate 只做**地基**，不接任何页面：语言探测、两套消息目录、缺译诊断、中文字体策略、
//! 以及「core 的中文状态串怎么进 UI」的契约。页面侧接线的 mission 用它，而不是各自硬编码英文
//! （先硬编码再全量重写就是本 crate 要避免的那条路）。
//!
//! | 模块 | 职责 | 不负责 |
//! |---|---|---|
//! | [`lang`] | `Lang`（en / zh-CN）+ locale 探测（源可注入）+ **四级优先级** [`lang::resolve`] | 读注册表 / 读配置 / spawn 进程 |
//! | [`choice`] | `LangChoice`（跟随系统 / 手选）：设置页那一档 + 它到生效语言的映射 | 落盘（那是 `presenter::settings_bridge`） |
//! | [`catalog`] | `Key` 枚举 + 中英两套常量表 + 取词 / 占位符渲染 + 缺译诊断 | 排版、字号、断行 |
//! | [`font`] | 中文环境扫系统 CJK 字体的**判据** + 缺字体时的「需下载」信号 + 注入式下载器接口 | 联网、写文件、装字体 |
//! | [`bridge`] | core / presenter 的状态串 → `Key` 的**示例契约**（文案边界规则全文在这里） | 改 core 的产出 |
//!
//! 语言从哪来的优先级写在 [`lang::resolve`]：**`ui.language` 显式值 > 平台源 > POSIX 环境变量 > 英文**。
//! 平台源（Windows 上 `reg.exe` 读注册表，零 FFI）在 UI 壳层实现并注入——本 crate 不认平台。
//!
//! ## 三条硬规矩（细则见 [`bridge`]）
//!
//! 1. **一句话 = 一个 key**。UI 侧不得把片段拼成一句话，尤其不得拼中文：
//!    `"Launch failed · exit0 静默失败"` 这种混排就是拼接的后果。带变量的句子用 `{name}` 占位符，
//!    由 [`catalog::Catalog::render`] 统一替换。
//! 2. **core 的中文串不是文案**。core / presenter 里的中文状态串（`原版` / `静默失败（…）`）只用于
//!    日志与 CLI；UI 要么走结构化出口（枚举）映射到 key，要么走 [`bridge::key_of_legacy_status`] 的
//!    过渡表。**任何情况下都不许把未知中文串直接画到界面上。**
//! 3. **英文环境零中文负担**（D28 原话）：`Lang::En` 下不要求任何中文字体、不触发任何下载；
//!    判据在 [`font::decide`]，是纯函数，可脱机单测。
//!
//! ## 与别处的关系
//!
//! - **不依赖 iced / slint / presenter / core**：`cargo tree -p unlucky-i18n --edges normal` 只应打印它自己。
//!   系统字体扫描由 UI 壳层做（iced 版在 `crates/app-iced/src/fonts/cjk.rs`），扫出来的族名喂给
//!   [`font::decide`]；
//! - 配置里的 `ui.language` 允许值 `zh_cn` / `en_us`（见 `core::setting::keys::LANGUAGES`）与
//!   [`lang::Lang::setting_value`] 必须是同一份词表，由 [`lang`] 的 pin 测试守门。

pub mod bridge;
pub mod catalog;
pub mod choice;
pub mod font;
pub mod lang;

pub use catalog::{Catalog, Key};
pub use choice::LangChoice;
pub use lang::Lang;
