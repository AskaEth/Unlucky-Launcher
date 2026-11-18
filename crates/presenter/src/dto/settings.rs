//! 设置页 DTO：设置分组的通用行模型。
//!
//! 「行」是 UI 与 `core::setting` schema 之间的唯一形状：一行 = 一个键 + 一种控件 +
//! 当前值。写回走 [`crate::dto::AppCommand::SetSetting`]，由 host 用 `SettingStore::set`
//! 落盘（不自己拼 JSON、不另立配置文件，`AGENTS §6`）。

use serde::{Deserialize, Serialize};

/// 设置页 Java 组「写回完成，请重发快照」的动作键（M204）。
///
/// ## 它解决的问题
///
/// Java 组把三键（`java.default_memory_mb` / `java.extra_args` / `java.selected_path`）**一次原子**
/// 写进 `launcher.json`（`app-iced` 的 `pages/settings/java/store.rs`：`set` 几个键 + 一次
/// `save()`），走的是页面自己的 `SettingStore`，**不经过** [`crate::dto::AppCommand::SetSetting`]
/// 的写盘分支。而 `CoreEvent::SettingsChanged` 全仓只有两处发出（`host/dispatch.rs` 的
/// `SetSetting` 分支、`host/mod.rs` 的 `refresh_all`）。于是三键写成功之后，presenter 的 java 组
/// 快照会陈旧到下一次任何别的写入——`AppState::settings_groups` 里的 `java.selected_path` /
/// `java.extra_args` 还是旧值。
///
/// ## 为什么不是「把写盘搬进 SetSetting」
///
/// 那会把一次原子写拆成最多三条命令（`SetSetting` 一次只收一个键），于是内存与 JVM 参数的保存
/// 会出现「第一键落盘、第二键被 schema 拒」的中间态；每条命令还要各走一次异步落盘，用户点一次
/// 保存会看到三次写盘与三次快照回流。取舍见 M204 报告。
///
/// ## 形状
///
/// 与 `host::engine::DETECT_REGION_ACTION`（`download.action.detect_region`，M165）同一族：
/// **动作键不写盘**，`host::settings::set` 见到它就返回 `Ok(())`，于是 `dispatch.rs` 的
/// `SetSetting` 分支照常执行它那四个副作用臂（重建引擎 / 重发 `SettingsChanged` / 清 Java 链）。
/// 键名带 `action` 段，是为了让人一眼看出它**不是** schema 键（`core::setting::keys` 里没有它，
/// 手写进 `launcher.json` 也只会被当成未知键忽略）。
pub const JAVA_REFRESH_ACTION: &str = "java.action.refresh";

/// 控件类型（决定设置页画什么）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingControl {
    /// 开关
    Toggle,
    /// 分段单选（`options` + `selected`）
    Choice,
    /// 单行文本（`value_text`）
    Text,
    /// 只读展示（当前路径/当前版本这类）
    ReadOnly,
    /// 动作按钮（`action` 给回调 id）
    Action,
}

/// 一行设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingRowView {
    /// schema 键（`download.concurrency`）
    pub key: String,
    /// 行标题
    pub label: String,
    /// 说明/当前值附注
    pub description: String,
    /// 控件
    pub control: SettingControl,
    /// 开关值
    pub flag: bool,
    /// 单选序号
    pub selected: i32,
    /// 单选项文案
    pub options: Vec<String>,
    /// 文本值
    pub value_text: String,
    /// 动作 id（`detect_region` 这类）
    pub action: String,
}

impl SettingRowView {
    /// 开关行。
    pub fn toggle(key: &str, label: &str, description: &str, value: bool) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            description: description.to_string(),
            control: SettingControl::Toggle,
            flag: value,
            selected: 0,
            options: Vec::new(),
            value_text: String::new(),
            action: String::new(),
        }
    }

    /// 单选行。
    pub fn choice(
        key: &str,
        label: &str,
        description: &str,
        options: &[&str],
        selected: i32,
    ) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            description: description.to_string(),
            control: SettingControl::Choice,
            flag: false,
            selected,
            options: options.iter().map(|item| (*item).to_string()).collect(),
            value_text: String::new(),
            action: String::new(),
        }
    }

    /// 文本行。
    pub fn text(key: &str, label: &str, description: &str, value: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            description: description.to_string(),
            control: SettingControl::Text,
            flag: false,
            selected: 0,
            options: Vec::new(),
            value_text: value.to_string(),
            action: String::new(),
        }
    }

    /// 只读行。
    pub fn read_only(label: &str, value: &str) -> Self {
        Self {
            key: String::new(),
            label: label.to_string(),
            description: value.to_string(),
            control: SettingControl::ReadOnly,
            flag: false,
            selected: 0,
            options: Vec::new(),
            value_text: value.to_string(),
            action: String::new(),
        }
    }

    /// 动作行。
    pub fn action(key: &str, label: &str, description: &str, action: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            description: description.to_string(),
            control: SettingControl::Action,
            flag: false,
            selected: 0,
            options: Vec::new(),
            value_text: String::new(),
            action: action.to_string(),
        }
    }
}

/// 一个设置分组（设置页右栏的一段）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingGroupView {
    /// 分组 id（与设置页子导航的 `SettingSection` 同名：download / java / data / ai / mcp）
    pub id: String,
    /// 分组标题
    pub title: String,
    /// 行
    pub rows: Vec<SettingRowView>,
}

impl SettingGroupView {
    /// 建组。
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            rows: Vec::new(),
        }
    }

    /// 追加一行（链式）。
    pub fn with(mut self, row: SettingRowView) -> Self {
        self.rows.push(row);
        self
    }
}

/// 在分组快照里找一个键的当前值（设置页回填用）。
pub fn find_row<'a>(groups: &'a [SettingGroupView], key: &str) -> Option<&'a SettingRowView> {
    groups
        .iter()
        .flat_map(|group| group.rows.iter())
        .find(|row| row.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_carry_exactly_one_control_kind() {
        let toggle = SettingRowView::toggle("agent.mcp.enabled", "MCP", "", true);
        assert_eq!(toggle.control, SettingControl::Toggle);
        assert!(toggle.flag);

        let choice = SettingRowView::choice("download.region", "地区", "", &["auto", "cn"], 1);
        assert_eq!(choice.options, vec!["auto", "cn"]);
        assert_eq!(choice.selected, 1);

        let action = SettingRowView::action("java.action", "检测", "", "detect_region");
        assert_eq!(action.action, "detect_region");
    }

    #[test]
    fn find_row_scans_all_groups() {
        let groups = vec![
            SettingGroupView::new("download", "下载").with(SettingRowView::text(
                "download.proxy_url",
                "代理",
                "",
                "",
            )),
            SettingGroupView::new("mcp", "MCP").with(SettingRowView::toggle(
                "agent.mcp.enabled",
                "开关",
                "",
                false,
            )),
        ];
        assert_eq!(
            find_row(&groups, "agent.mcp.enabled").map(|row| row.label.as_str()),
            Some("开关")
        );
        assert!(find_row(&groups, "nope").is_none());
    }

    #[test]
    fn rows_are_serde_round_trip_safe() {
        let group = SettingGroupView::new("download", "下载").with(SettingRowView::choice(
            "download.concurrency",
            "并发",
            "",
            &["4", "8", "16"],
            1,
        ));
        let text = serde_json::to_string(&group).expect("序列化");
        let back: SettingGroupView = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(back, group);
    }
}
