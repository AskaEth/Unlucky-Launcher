//! 实例启动参数 DTO（M157）：实例配置页「启动参数」区的唯一数据源 + 写回补丁。
//!
//! 口径三条：
//! 1. **实例层与生效值分开给**：`jvm_args` 等是实例文件里的原文（空 = 跟随全局），
//!    `*_effective` 是三级回落（实例 → 全局 → 默认）之后真正会下发的值。UI 要把两件事同时
//!    画出来（「留空 = 跟随全局 {global}」这种提示与「生效值」是两回事）；
//! 2. **图形后端的可选值与门槛由 host 算好**：`graphics_backend_options` 来自 core 的词表
//!    （[`LaunchArgsView::graphics_backend_options`]），`graphics_backend_allowed` 来自 core 的
//!    版本门槛判定——UI 不复制任何一条判据（任务书：判定用 core capability）；
//! 3. **本模块不含 core 类型**（D42：dto 只放扁平 IPC 类型）。
//!
//! 补丁的「不改 / 清空」二分：`None` = 这一项不动；`Some("")` = 清空（回到跟随）。

use serde::{Deserialize, Serialize};

/// 实例启动参数的**读取**视图（详情页「启动参数」区）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LaunchArgsView {
    /// 实例 id。
    pub instance_id: String,
    /// 实例层 JVM 参数原文（空 = 只用全局 `java.extra_args`）。
    pub jvm_args: String,
    /// 实例层游戏参数原文（空 = 不追加）。
    pub game_args: String,
    /// 实例层最大堆（`None` = 跟随全局）。
    pub memory_mb: Option<u32>,
    /// 生效最大堆（三级回落 + 出厂默认，恒有值）。
    pub memory_effective_mb: u32,
    /// 实例层 Java 可执行文件原文（空 = 跟随全局 / 自动挑）。
    pub java_path: String,
    /// 生效 Java 可执行文件（空 = 自动挑；展示用，不代表已探测）。
    pub java_effective: String,
    /// 实例层图形后端（`default` / `opengl` / `vulkan`）。
    pub graphics_backend: String,
    /// 生效图形后端（三级回落之后）。
    pub graphics_backend_effective: String,
    /// 该实例版本能不能用 `--graphicsBackend`（core 的版本门槛判定；`false` ⇒ UI 灰化 + 提示）。
    pub graphics_backend_allowed: bool,
    /// 判定所依据的游戏版本（提示文案「需 MC 26.2+」旁边给出实际版本）。
    pub mc_version: String,
    /// 门槛的最低版本（core 的 `GRAPHICS_BACKEND_MIN_VERSION`；文案与测试共用一份来源）。
    pub graphics_backend_min_version: String,
    /// 图形后端三态的词表（顺序即下拉顺序；来自 core，不让 UI 手抄）。
    pub graphics_backend_options: Vec<String>,
}

impl LaunchArgsView {
    /// 三态下拉的 id → 是不是当前生效值（UI 高亮用；未知值原样比较）。
    #[must_use]
    pub fn is_effective_backend(&self, id: &str) -> bool {
        self.graphics_backend_effective == id
    }
}

/// 实例启动参数的**写回**补丁（GUI 表单与 CLI 共用）。
///
/// 每个字段：`None` = 不改；`Some("")` = 清空（回到跟随下一层）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LaunchArgsPatch {
    /// 实例层 JVM 参数（文本，空白分隔）。
    pub jvm_args: Option<String>,
    /// 实例层游戏参数（文本）。
    pub game_args: Option<String>,
    /// 实例层最大堆（文本形态，host 解析；空 = 跟随全局）。
    pub memory_mb: Option<String>,
    /// 实例层 Java 可执行文件。
    pub java_path: Option<String>,
    /// 实例层图形后端（`default` / `opengl` / `vulkan`）。
    pub graphics_backend: Option<String>,
}

impl LaunchArgsPatch {
    /// 是否什么都不改（UI 没动过表单时的短路）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.jvm_args.is_none()
            && self.game_args.is_none()
            && self.memory_mb.is_none()
            && self.java_path.is_none()
            && self.graphics_backend.is_none()
    }

    /// 只改图形后端（下拉切换的最小补丁）。
    #[must_use]
    pub fn backend(id: &str) -> Self {
        Self {
            graphics_backend: Some(id.to_string()),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_patch_touches_nothing() {
        assert!(LaunchArgsPatch::default().is_empty());
        assert!(!LaunchArgsPatch::backend("vulkan").is_empty());
        // `Some("")` 是「清空」而不是「不改」——两者不能折叠成一个判据。
        let clear = LaunchArgsPatch {
            jvm_args: Some(String::new()),
            ..LaunchArgsPatch::default()
        };
        assert!(!clear.is_empty());
    }

    #[test]
    fn view_round_trips_as_ipc() {
        let view = LaunchArgsView {
            instance_id: "inst-1".to_string(),
            jvm_args: "-XX:+UseG1GC".to_string(),
            game_args: String::new(),
            memory_mb: Some(4096),
            memory_effective_mb: 4096,
            java_path: String::new(),
            java_effective: "C:/jdk21/bin/java.exe".to_string(),
            graphics_backend: "vulkan".to_string(),
            graphics_backend_effective: "vulkan".to_string(),
            graphics_backend_allowed: true,
            mc_version: "26.2-snapshot-2".to_string(),
            graphics_backend_min_version: "26.2-snapshot-2".to_string(),
            graphics_backend_options: vec![
                "default".to_string(),
                "opengl".to_string(),
                "vulkan".to_string(),
            ],
        };
        let text = serde_json::to_string(&view).expect("序列化");
        let back: LaunchArgsView = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(back, view);
        assert!(back.is_effective_backend("vulkan"));
        assert!(!back.is_effective_backend("opengl"));
    }
}
