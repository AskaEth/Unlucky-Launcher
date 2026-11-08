//! 实例页的**页面状态**（M157）：启动参数区的表单。
//!
//! ## 为什么状态在 app、页面只借一帧
//!
//! iced 的 `view` 不能持有跨帧状态，而启动参数区是可编辑的（两个多行框 + 两行单行框 +
//! 三态 chips）。所以表单状态的所有者是 `App`（`app/state.rs` 的 `launch_form` 字段），
//! 页面层只在这一帧借用；这与设置页「组状态归 `App`」是同一条纪律（`pages/settings/state.rs`）。
//!
//! ## 与快照的关系（唯一的重播种点）
//!
//! [`LaunchForm::sync`] 由 `App::refresh_view` 调用：
//!
//! | 情况 | 行为 |
//! |---|---|
//! | 换了一个实例 | **重播种**（用户没写完的草稿不能串到别的实例上） |
//! | 同一个实例、没改过 | 重播种（保存/外部改了配置后跟着刷新） |
//! | 同一个实例、改过（`dirty`） | **不覆盖**（每来一条事件就把输入抹掉是不可接受的） |
//!
//! 「留空 = 跟随下一层」由提交时把空串原样交给 host 表达（`Some("")` = 清空），
//! 表单自己不做任何回落计算——回落只发生在 core。

use unlucky_presenter::dto::{LaunchArgsPatch, LaunchArgsView};

/// 启动参数表单（五项 + 脏标记 + 绑定的实例 id）。
#[derive(Debug)]
pub struct LaunchForm {
    /// 这份草稿属于哪个实例（`None` = 还没播种）。
    pub bound: Option<String>,
    /// 实例级 JVM 参数（空白分隔的文本）。
    pub jvm: String,
    /// 实例级游戏参数（空白分隔的文本）。
    pub game: String,
    /// 最大堆（文本形态，空 = 跟随全局）。
    pub memory: String,
    /// Java 可执行文件（空 = 跟随全局 / 自动挑）。
    pub java: String,
    /// 图形后端（`default` / `opengl` / `vulkan`）。
    pub backend: String,
    /// 用户改过、还没保存。
    pub dirty: bool,
}

impl Default for LaunchForm {
    fn default() -> Self {
        Self::from_view(None, &LaunchArgsView::default())
    }
}

impl LaunchForm {
    /// 用一份快照播种（`bound` 为 `None` = 还没绑定实例）。
    #[must_use]
    pub fn from_view(bound: Option<&str>, view: &LaunchArgsView) -> Self {
        Self {
            bound: bound.map(str::to_string),
            jvm: view.jvm_args.clone(),
            game: view.game_args.clone(),
            memory: view.memory_mb.map(|mb| mb.to_string()).unwrap_or_default(),
            java: view.java_path.clone(),
            backend: view.graphics_backend.clone(),
            dirty: false,
        }
    }

    /// 这份草稿是否正对着该实例。
    #[must_use]
    pub fn bound_to(&self, id: &str) -> bool {
        self.bound.as_deref() == Some(id)
    }

    /// 重播种（见模块文档的三档）。
    ///
    /// **脏草稿只在「落盘已确认」时被替换**：保存把命令派发出去之后，回程可能是一条错误事件
    /// （内存写了 `4g`、实例文件写不进去），那时 pump 会带着**旧快照**重建视图——如果这里无条件
    /// 覆盖，用户刚敲的东西就没了（M157 r3 复审 P2-2）。判据是「快照已经等于表单」：
    /// 相等 ⇒ 这次保存落了盘，清脏并重播种；不相等 ⇒ 保留草稿等用户改。
    pub fn sync(&mut self, view: &LaunchArgsView) {
        let same_instance = self.bound_to(&view.instance_id);
        if same_instance && self.dirty {
            if self.matches(view) {
                self.dirty = false;
                *self = Self::from_view(Some(&view.instance_id), view);
            }
            return;
        }
        *self = Self::from_view(Some(&view.instance_id), view);
    }

    /// 表单当前内容是否与一份快照（= 盘上已持久化的值）完全一致。
    ///
    /// **内存按数值比**：host 会把 `"04096"` / `"+4096"` 归一成 `4096` 落盘，回程快照给的是
    /// `Some(4096)`——按文本比会永远不等，于是表单一直脏、也不再跟随外部改动（r4 复审 P3-b）。
    /// 解析失败（用户正在敲半截）才退回文本口径。
    /// 其余字段直接比原文（JVM/游戏参数由 core 的 `tokenize` 消费，不做归一）。
    #[must_use]
    pub fn matches(&self, view: &LaunchArgsView) -> bool {
        self.jvm.trim() == view.jvm_args.trim()
            && self.game.trim() == view.game_args.trim()
            && self.memory_matches(view.memory_mb)
            && self.java.trim() == view.java_path.trim()
            && self.backend == view.graphics_backend
    }

    /// 内存字段是否等于快照里的值（数值优先，解析失败退文本）。
    fn memory_matches(&self, persisted: Option<u32>) -> bool {
        let typed = self.memory.trim();
        match (typed.parse::<u32>(), persisted) {
            (Ok(typed), Some(persisted)) => typed == persisted,
            // 空输入 = 跟随全局：快照那边也应当是「没有实例覆盖」。
            (Err(_), None) => typed.is_empty(),
            _ => false,
        }
    }

    /// 提交补丁：**五项全给**（表单是整份可见的，不用增量语义会让「清空」无法表达）。
    #[must_use]
    pub fn patch(&self) -> LaunchArgsPatch {
        LaunchArgsPatch {
            jvm_args: Some(self.jvm.clone()),
            game_args: Some(self.game.clone()),
            memory_mb: Some(self.memory.clone()),
            java_path: Some(self.java.clone()),
            graphics_backend: Some(self.backend.clone()),
        }
    }

    /// 「恢复跟随全局」：五项清空，脏标记置位（等用户点保存）。
    pub fn reset(&mut self) {
        self.jvm.clear();
        self.game.clear();
        self.memory.clear();
        self.java.clear();
        self.backend = "default".to_string();
        self.dirty = true;
    }
}

/// 启动参数区的消息（`pages::Message::Instances` 的载荷）。
///
/// 只表达**页面意图**：编辑写进表单，保存/恢复才需要 app 层理解（前者 dispatch 一条
/// `AppCommand::SetInstanceLaunchArgs`，后者只是清表单）。
#[derive(Debug, Clone)]
pub enum Message {
    /// 编辑 JVM 参数。
    Jvm(String),
    /// 编辑游戏参数。
    Game(String),
    /// 编辑最大内存（单行文本）。
    Memory(String),
    /// 编辑 Java 路径（单行文本）。
    Java(String),
    /// 选图形后端（三态 chips）。
    Backend(String),
    /// 保存（派发命令）。
    Save,
    /// 恢复跟随全局（清表单，仍需保存）。
    Reset,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(instance: &str, jvm: &str, memory: Option<u32>, backend: &str) -> LaunchArgsView {
        LaunchArgsView {
            instance_id: instance.to_string(),
            jvm_args: jvm.to_string(),
            game_args: String::new(),
            memory_mb: memory,
            memory_effective_mb: memory.unwrap_or(2048),
            java_path: String::new(),
            java_effective: String::new(),
            graphics_backend: backend.to_string(),
            graphics_backend_effective: backend.to_string(),
            graphics_backend_allowed: true,
            mc_version: "26.2".to_string(),
            graphics_backend_min_version: "26.2-snapshot-2".to_string(),
            graphics_backend_options: vec![
                "default".to_string(),
                "opengl".to_string(),
                "vulkan".to_string(),
            ],
        }
    }

    /// 换实例必重播种；同实例的脏草稿不被事件冲掉。
    #[test]
    fn sync_reseeds_per_instance_but_keeps_dirty_edits() {
        let mut form = LaunchForm::default();
        form.sync(&view("a", "-Xss4m", Some(4096), "vulkan"));
        assert_eq!(form.jvm, "-Xss4m");
        assert_eq!(form.memory, "4096");
        assert!(!form.dirty);

        // 用户改动 → 脏 → 同实例的快照刷新不许覆盖。
        form.memory = "8192".to_string();
        form.dirty = true;
        form.sync(&view("a", "-Xss4m", Some(4096), "vulkan"));
        assert_eq!(form.memory, "8192", "脏草稿被事件冲掉了");

        // 换实例 → 重播种（草稿不串台）。
        form.sync(&view("b", "", None, "default"));
        assert_eq!(form.memory, "");
        assert!(!form.dirty);
        assert!(form.bound_to("b"));
    }

    /// **保存失败不许吃掉草稿**（M157 r3 复审 P2-2）：脏表单 + 旧快照回来 ⇒ 原样保留。
    ///
    /// 抓得住：`sync` 在「同实例 + 脏」时无条件重播种（用户敲的 `4g` 被磁盘上的 4096 覆盖）。
    /// 抓不住：真实事件时序（那在 `app/update/instances.rs` 的用例里按消息顺序模拟）。
    #[test]
    fn failed_save_keeps_the_draft() {
        let mut form = LaunchForm::from_view(Some("a"), &view("a", "", Some(4096), "default"));
        // 用户把内存改成非法值（host 会拒），此时表单是脏的。
        form.memory = "4g".to_string();
        form.jvm = "-Xmx3g".to_string();
        form.dirty = true;

        // 回程事件带来的仍是**旧**快照（保存失败 ⇒ 盘上没变）。
        form.sync(&view("a", "", Some(4096), "default"));

        assert_eq!(form.memory, "4g", "失败后的重播把用户正在改的内存吃掉了");
        assert_eq!(form.jvm, "-Xmx3g");
        assert!(form.dirty, "草稿还在，脏标记也不能掉");
    }

    /// **保存成功要清脏**：回程快照等于表单 ⇒ 这一步算落盘确认，之后可以跟着外部变化重播种。
    #[test]
    fn confirmed_save_clears_dirty_and_reseeds() {
        let mut form = LaunchForm::from_view(Some("a"), &view("a", "", Some(4096), "default"));
        form.memory = "8192".to_string();
        form.backend = "vulkan".to_string();
        form.dirty = true;

        form.sync(&view("a", "", Some(8192), "vulkan"));
        assert!(!form.dirty, "盘上已经是这份值了，脏标记该清");
        assert_eq!(form.memory, "8192");

        // 之后外部改了配置（比如 CLI 写入）⇒ 干净的表格跟着更新。
        form.sync(&view("a", "-Xss4m", Some(2048), "opengl"));
        assert_eq!(form.memory, "2048");
        assert_eq!(form.jvm, "-Xss4m");
    }

    /// **内存按数值比**（r4 复审 P3-b）：`04096` / `+4096` 落盘后回程是 `4096`，必须判等。
    ///
    /// 抓得住：`matches` 退回文本比较——那样这两类输入会永远「没保存成功」，表单一直脏、
    /// 之后外部改配置也不再重播种。
    #[test]
    fn memory_is_compared_by_value_not_by_text() {
        for typed in ["04096", "+4096", " 4096 "] {
            let mut form = LaunchForm::from_view(Some("a"), &view("a", "", Some(4096), "default"));
            form.memory = typed.to_string();
            form.dirty = true;
            assert!(
                form.matches(&view("a", "", Some(4096), "default")),
                "`{typed}` 应判等"
            );
            form.sync(&view("a", "", Some(4096), "default"));
            assert!(!form.dirty, "`{typed}` 保存确认后要清脏");
        }
        // 数值不等就是不等于；半截输入（解析失败）也不许判等。
        let mut form = LaunchForm::from_view(Some("a"), &view("a", "", Some(4096), "default"));
        form.memory = "8".to_string();
        assert!(!form.matches(&view("a", "", Some(4096), "default")));
        form.memory = "4g".to_string();
        assert!(!form.matches(&view("a", "", Some(4096), "default")));
        // 清空 = 跟随全局：快照也得是「没有实例覆盖」。
        form.memory.clear();
        assert!(form.matches(&view("a", "", None, "default")));
    }

    /// 补丁五项全给：空串就是「清空」，不留 `None` 让 host 猜。
    #[test]
    fn patch_always_covers_all_five_fields() {
        let mut form = LaunchForm::from_view(Some("a"), &view("a", "", None, "default"));
        form.memory = "6144".to_string();
        form.backend = "opengl".to_string();
        let patch = form.patch();
        assert_eq!(patch.jvm_args.as_deref(), Some(""));
        assert_eq!(patch.memory_mb.as_deref(), Some("6144"));
        assert_eq!(patch.graphics_backend.as_deref(), Some("opengl"));

        form.reset();
        assert!(form.dirty);
        let patch = form.patch();
        assert_eq!(patch.jvm_args.as_deref(), Some(""));
        assert_eq!(patch.memory_mb.as_deref(), Some(""));
        assert_eq!(patch.graphics_backend.as_deref(), Some("default"));
    }
}
