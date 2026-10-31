//! 设置页 DTO：设置分组的通用行模型。
//!
//! 「行」是 UI 与 `core::setting` schema 之间的唯一形状：一行 = 一个键 + 一种控件 +
//! 当前值。写回走 [`crate::dto::AppCommand::SetSetting`]，由 host 用 `SettingStore::set`
//! 落盘（不自己拼 JSON、不另立配置文件，`AGENTS §6`）。

use serde::{Deserialize, Serialize};

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
