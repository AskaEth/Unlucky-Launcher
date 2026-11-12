//! 设置页「Java」分组（M175）：运行时列表 / 默认 JDK / 内存 / JVM 参数。
//!
//! 本文件只剩三样东西，正文 / 状态 / 读写都在 `pages/settings/java/` 里：
//!
//! | 这里 | 内容 |
//! |---|---|
//! | [`GROUP`] | 左列名 / 图标（正文走 [`view::body`]，`body` 槽位保持 `None`） |
//! | [`JAVA_GROUP`] | 本组在 `GROUPS` 里的下标（与 General / Update / Assistant 同一条**按下标分发**：本组要 `Lang`，`Group.body` 的签名里没有它） |
//! | [`Message`] | 本线的消息口（落点 `app/update/settings/java.rs`） |
//!
//! ## M101 的演示行去哪了
//!
//! M101 留的 `rows: &[("Runtime","Temurin 21.0.4"), …]` 与绊线测试
//! `the_java_group_is_still_demo_rows`（断言 `body.is_none()`）在 M175 一起删掉了：
//! 那三个值是**写死的演示字符串**，用户实测把它们当成了真检测结果
//! （「它默认显示 Temurin 21.0.4 我甚至不能确定它是否是一个硬编码的值」）。
//! 现在的真值来自三段真读盘：`core::game::java::staged`（扫描）、`core::game::java::manual`
//! （手动路径严格探测）、`config/launcher.json`（`java.*` 三键）。
//!
//! `rows` 仍留成空切片：它是 `Group` 的字段（另外五组在用），而本组的 `body` 分发永远命中
//! [`view::body`]，空切片不会被画出来——宁可空着，也不留一句会撒谎的演示值。

mod context;
mod download_view;
pub(crate) mod downloads;
mod fields;
pub(crate) mod prefs;
pub(crate) mod prefs_store;
mod prefs_view;
mod row;
pub(crate) mod scan;
mod seed;
mod state;
pub(crate) mod store;
pub(super) mod view;

#[cfg(test)]
mod tests;

pub use state::{same_path, Context, Layer, Notice, Requirement, State};

/// 测试里的注入作业要写 `Vec<RuntimeRow>`（App 层在生产路径上只用 `State`，不需要这个名字）。
#[cfg(test)]
pub use row::RuntimeRow;

use unlucky_design::icons::IconKind;

use super::Group;

/// 本组在 [`super::GROUPS`] 里的下标（`GROUPS` 的顺序是 general / appearance / **java** / …）。
///
/// 写死 2 与 `groups[2]` 同一个来源：`tests/mod.rs` 的「左列顺序」把这张表钉住了。
pub(crate) const JAVA_GROUP: usize = 2;

/// Java 分组。
pub(super) const GROUP: Group = Group {
    name: "Java",
    icon: IconKind::Folder,
    // 空：产品路径与窄窗「按组堆叠」都走 `view::body`（见文件头「M101 的演示行去哪了」）。
    rows: &[],
    body: None,
};

/// 本线的消息（**只归本线**）：加变体只改本文件 + `app/update/settings/java.rs`。
#[derive(Debug, Clone)]
pub enum Message {
    /// 手动触发一次自动扫描（真扫盘，后台线程；进度与结果由心跳灌回来）
    Scan,
    /// 取消正在跑的扫描
    CancelScan,
    /// 手动路径输入框
    AddDraft(String),
    /// 提交手动添加（严格校验：真跑 `java -version`）
    Add,
    /// 把某个运行时设为默认（写 `java.selected_path`）
    SelectRuntime(String),
    /// 默认内存草稿（MB 文本）
    EditMemory(String),
    /// JVM 额外参数草稿
    EditJvm(String),
    /// 保存内存 + JVM 参数（校验 + 落盘 + 回读）
    Save,
    /// 下载并安装某个大版本的 JDK（M197：真下载，后台线程跑）
    Download(u32),
    /// `java.installRoot` 草稿
    EditInstallRoot(String),
    /// `java.auto_install` 开关
    ToggleAutoInstall(bool),
    /// 发行版顺序上移 / 下移一行（`index` 是有效顺序里的下标）
    MoveVendor {
        /// 有效顺序里的下标
        index: usize,
        /// `true` = 上移，`false` = 下移
        up: bool,
    },
    /// 发行版顺序恢复地区默认序（= 把这个键写回空）
    ResetPriority,
    /// 保存三个设置键（`installRoot` / `vendorPriority` / `auto_install`）
    SavePrefs,
}

impl Message {
    /// 一条本组消息 → 设置页的投递口（`pages::settings::GroupMessage::Java`）。
    #[must_use]
    pub fn into_group(self) -> super::GroupMessage {
        super::GroupMessage::Java(self)
    }
}
