//! 安装选项区（M105）：**选中版本之后先给选项，点「安装」才开工**。
//!
//! ## 为什么要有这一步
//!
//! 用户实跑反馈的原话是「点击下载之后它就直接下载了，没有选择前置加载器(版本选择)选项」。
//! 这不是画得不好，是**功能没做**：`Message::InstallVersion` 直接派 `InstallVersion`，
//! 而 core 早就会装四家加载器（M32/M35/M45 已合）。于是「能装什么」与「UI 让你装什么」
//! 之间差着一整个加载器维度。
//!
//! ## 文件分工
//!
//! | 文件 | 内容 |
//! |---|---|
//! | [`model`] | **决策**（无 iced）：加载器词表、兼容性静态规则、选项视图、确认键判据 |
//! | [`view`] | 控件树：加载器 chip ×2 行 + 加载器版本列表 + 实例名 + 不兼容提示 |
//! | `tests`（`options/tests.rs`） | 兼容性真值表 + 确认键门（纯函数，不开图形环境） |
//!
//! ## 一条纪律：页面只发意图
//!
//! | 控件 | 消息 | app 层落点 |
//! |---|---|---|
//! | 加载器 chip | [`Message::SetLoader`](super::Message::SetLoader) | 改草稿 + 按需派 `ListLoaderVersions` |
//! | 加载器版本一行 | [`Message::SetLoaderVersion`](super::Message::SetLoaderVersion) | 改草稿 |
//! | 实例名输入 | [`Message::SetInstanceName`](super::Message::SetInstanceName) | 改草稿 |
//! | 「安装」 | [`Message::InstallVersion`](super::Message::InstallVersion) | `AppCommand::CreateInstance(草稿)` |
//!
//! 草稿（选中版本 + 三个选项）住在 `App`，由 `refresh_view` 带进页面模型——与分类筛选同一个做法，
//! 因为 `refresh_view` 每次事件都会把整棵模型重建，页面自己存不住任何东西。

mod fade;
mod model;
#[cfg(test)]
mod s1_tests;
#[cfg(test)]
mod tests;
mod view;

pub use model::{InstallDraft, Loader, OptionsView};
pub(crate) use view::{section, version_menu_height};
