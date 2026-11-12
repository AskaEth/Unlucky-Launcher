//! 本地 Mod 列表与启停的 IPC 面（M224 · MODTOGGLE）。
//!
//! M218 的结论是这块要**新建三层**（core 读写目录 + presenter DTO + 实例详情页），且**不许**
//! 给内容页加第五个 `ContentKind`（会打断 4 处穷尽 match）。本文件是中间那层：
//!
//! | 方向 | 类型 |
//! |---|---|
//! | app → host | [`ModsCommand`]（列目录 / 启停一个文件） |
//! | host → app | [`ModListView`]（整份快照）或 [`ModFailureView`]（具名原因） |
//!
//! ## 为什么不复用内容页的 `ContentInstallRequest`
//!
//! 那条通路是「远端 → 本地」，入口在内容页；启停是「本地 → 本地」，入口在实例详情页。
//! 把两者塞进一个 DTO 会让内容页为了一个它不画的字段多一次穷尽 match——正是 §3 点名要避免的。
//!
//! ## 文案纪律（M161）
//!
//! 本模块**不带人读散文**：失败给的是结构（[`ModFailureKind`] + 底层 detail 串），
//! 由 app 层按 i18n key 现求。`detail` 是 OS 原文（`Access is denied.` 之类），只进括号，
//! 不进主句——主句必须能翻译。

use serde::{Deserialize, Serialize};

use crate::dto::{InstanceId, InstanceSummary};

/// app → host 的本地 Mod 指令。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModsCommand {
    /// 列某个实例的 `mods/`（进详情页时发、启停成功后 host 自己再发一次）。
    List(InstanceId),
    /// 启停一个文件（`file` 是**磁盘上的文件名**，含后缀）。
    SetEnabled {
        /// 实例 id
        id: InstanceId,
        /// 磁盘文件名（`sodium.jar.disabled`）
        file: String,
        /// 期望的启用状态
        enabled: bool,
    },
}

/// 列表里的一条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModEntryView {
    /// 磁盘文件名（含 `.disabled` / `.old`，UI 的开关消息原样带回去）。
    pub file: String,
    /// 展示名（去掉两个后缀）。
    pub name: String,
    /// 是否启用。
    pub enabled: bool,
    /// 字节数（UI 自己格式化，不在这里拼 "1.2 MiB"）。
    pub bytes: u64,
    /// 是否带 `.old`（旧版本残留，只展示不清理）。
    pub old: bool,
}

/// 没列出来的一条（UI 只需计数 + 一句"为什么"）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModSkipView {
    /// 文件名。
    pub file: String,
    /// 跳过原因（结构，不翻译）。
    pub reason: ModSkipKind,
}

/// 跳过的原因（与 core 的 `addon::mods::SkipReason` 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModSkipKind {
    /// 符号链接（不跟随）。
    SymbolicLink,
    /// 不是普通文件（目录 / 设备）。
    NotAFile,
    /// 名字过不了路径规则。
    UnsafeName,
    /// 元数据读不出来。
    Unreadable,
}

/// 一次列目录的完整快照（**整体替换**，不做增量）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModListView {
    /// 实例 id
    pub instance: InstanceId,
    /// 被列的 `mods/` 目录（绝对路径，UI 的"打开文件夹"用）。
    pub dir: String,
    /// 目录存不存在（`false` = 这个实例还没装过 mod）。
    pub exists: bool,
    /// 列表（按展示名排序）。
    pub entries: Vec<ModEntryView>,
    /// 跳过条目。
    pub skipped: Vec<ModSkipView>,
    /// 这一份快照采样的时刻，游戏是否在该实例上运行（运行中开关只读）。
    pub running: bool,
    /// 采样时的实例快照里有没有这个实例（`false` = 实例被删了，页面该退回列表）。
    pub known: bool,
}

impl ModListView {
    /// 启用中的条数。
    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.enabled).count()
    }

    /// 禁用中的条数。
    #[must_use]
    pub fn disabled_count(&self) -> usize {
        self.entries.len() - self.enabled_count()
    }
}

/// 一次启停失败的具名原因（app 按 key 现求文案）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModFailureView {
    /// 实例 id（迟到/串台的结果靠它判）。
    pub instance: InstanceId,
    /// 涉及的文件（列表里没有时为空串）。
    pub file: String,
    /// 失败大类。
    pub kind: ModFailureKind,
    /// 底层原文（OS 错误 / 路径细节）；只进括号，不进主句。
    pub detail: String,
}

/// 失败大类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModFailureKind {
    /// 文件名不是裸文件名。
    UnsafeName,
    /// `mods/` 不可用（不存在 / 符号链接 / 不是目录 / 读不了）。
    ModsDirUnusable,
    /// 文件不在了。
    NotFound,
    /// 目标不是普通文件。
    NotARegularFile,
    /// 目标名已被占用（不覆盖）。
    Collision,
    /// 改名失败。
    Rename,
    /// 实例不在了（快照里找不到），或隔离档位派生不出运行目录。
    InstanceUnavailable,
    /// 游戏正在这个实例上运行（host 侧主动挡：Windows 上 rename 会失败，别让用户猜）。
    Running,
}

/// 实例 id 在快照里的最小定位结果（`None` = 实例已不在）。
#[must_use]
pub fn locate<'a>(instances: &'a [InstanceSummary], id: &str) -> Option<&'a InstanceSummary> {
    instances
        .iter()
        .find(|instance| instance.id == id || instance.name == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::InstanceRuntime;

    fn summary(id: &str, name: &str) -> InstanceSummary {
        InstanceSummary {
            id: id.to_string(),
            name: name.to_string(),
            mc_version: "1.20.1".to_string(),
            loader: crate::dto::LoaderInfo {
                kind: crate::dto::LoaderKind::Vanilla,
                version: String::new(),
            },
            last_played: None,
            runtime: InstanceRuntime::Idle,
            icon: None,
            isolation: "isolated".to_string(),
            version_id: "1.20.1".to_string(),
            version_present: true,
            launch: crate::dto::LaunchArgsView::default(),
        }
    }

    /// 快照计数与 IPC round-trip（AGENTS §6：跨界类型必须能序列化）。
    #[test]
    fn view_counts_and_round_trips() {
        let view = ModListView {
            instance: "inst".to_string(),
            dir: "/tmp/run/mods".to_string(),
            exists: true,
            entries: vec![
                ModEntryView {
                    file: "a.jar".to_string(),
                    name: "a.jar".to_string(),
                    enabled: true,
                    bytes: 10,
                    old: false,
                },
                ModEntryView {
                    file: "b.jar.disabled".to_string(),
                    name: "b.jar".to_string(),
                    enabled: false,
                    bytes: 20,
                    old: false,
                },
            ],
            skipped: Vec::new(),
            running: false,
            known: true,
        };
        assert_eq!(view.enabled_count(), 1);
        assert_eq!(view.disabled_count(), 1);

        let text = serde_json::to_string(&view).expect("序列化");
        let back: ModListView = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(back, view);

        let command = ModsCommand::SetEnabled {
            id: "inst".to_string(),
            file: "a.jar".to_string(),
            enabled: false,
        };
        let text = serde_json::to_string(&command).expect("序列化");
        assert_eq!(
            serde_json::from_str::<ModsCommand>(&text).expect("反序列化"),
            command
        );
    }

    /// 定位：id 优先、名字兜底（与 `launch_target` 同口径）。
    #[test]
    fn locate_matches_by_id_then_name() {
        let instances = vec![summary("survival", "生存"), summary("other", "别的")];
        assert_eq!(
            locate(&instances, "survival").map(|entry| entry.name.as_str()),
            Some("生存")
        );
        assert_eq!(
            locate(&instances, "别的").map(|entry| entry.id.as_str()),
            Some("other")
        );
        assert!(locate(&instances, "gone").is_none());
    }
}
