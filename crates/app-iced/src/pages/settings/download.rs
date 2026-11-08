//! 设置页「Downloads」分组：首选源 / 并发数 / 校验（演示行）。
//!
//! **这一组归 download 线**：接真控件时在本文件把 `body` 换成 `Some(download::body)`（渲染器也写在本文件），
//! `settings/mod.rs` 的注册表不用动。
//!
//! 注意与下载页（`pages/download*`）的区别：那是「装哪个版本」，这里是「怎么下」的默认值。

use unlucky_design::icons::IconKind;

use super::Group;

/// Downloads 分组。
pub(super) const GROUP: Group = Group {
    name: "Downloads",
    icon: IconKind::Download,
    rows: &[
        ("Preferred source", "Automatic (region)"),
        ("Concurrency", "8 threads"),
        ("Verify checksums", "On"),
    ],
    body: None,
};

/// Downloads 分组的运行期状态（M101 r2：**只归 download 线**）。
///
/// 加字段只改本文件：`Groups` 只认 `Groups.download` 这一个字段。
#[derive(Debug, Default)]
pub struct State {}

/// Downloads 分组的消息（M101：**只归 download 线**，加变体只改本文件）。
///
/// 落点是 `app/update/settings/download.rs`；`pages/mod.rs` 与 `page_messages.rs` 都不用动（它们只认
/// [`GroupMessage`](super::GroupMessage)）。异步结果（例如「测这个源可用」）在自己那条落点里构造
/// `Message::Page(pages::Message::Settings(GroupMessage::Download(…)))` 回来即可。
#[derive(Debug, Clone)]
pub enum Message {}
