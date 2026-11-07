//! 设置页「Java」分组：运行时 / 默认内存 / JVM 参数（演示行）。
//!
//! **这一组归 java 线**：接真控件时在本文件把 `body` 换成 `Some(java::body)`（渲染器也写在本文件），
//! 再把 `rows` 换成真数据源给的文本；`settings/mod.rs` 的注册表不用动。
//!
//! 图标说明（视觉复审 P2-3）：令牌里 `Play` 是填充字形，和同屏其它描边图标语汇打架；
//! Java 分组讲的是运行时安装位置，用目录图标语义也对得上。

use unlucky_design::icons::IconKind;

use super::Group;

/// Java 分组。
pub(super) const GROUP: Group = Group {
    name: "Java",
    icon: IconKind::Folder,
    rows: &[
        ("Runtime", "Temurin 21.0.4"),
        ("Default memory", "4 GiB"),
        ("JVM arguments", "Automatic"),
    ],
    body: None,
};

/// Java 分组的运行期状态（M101 r2：**只归 java 线**）。
///
/// 加字段只改本文件：`Groups` 只认 `Groups.java`（一个字段），不认这个类型里的任何字段，
/// 所以 `settings/mod.rs` / `app/state.rs` / `app/view.rs` 都不用动。
#[derive(Debug, Default)]
pub struct State {}

/// Java 分组的消息（M101：**只归 java 线**，加变体只改本文件）。
///
/// 落点是 `app/update/settings/java.rs`；`pages/mod.rs` 与 `page_messages.rs` 都不用动（它们只认
/// [`GroupMessage`](super::GroupMessage)）。
#[derive(Debug, Clone)]
pub enum Message {
    /// **只有测试构造它**：M101 r2 留的「活的端到端证据」——外壳消息 →
    /// `on_page` → `on_settings` → `on_settings_java` 这条路在运行时真的跑通，不是只靠编译期穷尽。
    /// java 线接第一个真控件时把这一支换成真变体：`tests/java_section.rs` 的
    /// `the_java_group_is_still_demo_rows` 就是那条会提醒你的绊线（它断言 `body.is_none()`）。
    #[cfg(test)]
    Ping,
}
