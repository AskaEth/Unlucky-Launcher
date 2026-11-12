//! 版本能力位：从版本 JSON 的**库表**推断「这个版本带不带某个渲染后端」。
//!
//! 依据 `DevDocs/PlanDoc/research/05-mc-vulkan.md`（M26）§2.2 与 §4.2：
//! Vulkan 的三件套（`lwjgl-vulkan` / `lwjgl-shaderc` / `lwjgl-spvc` / `lwjgl-vma`）从
//! **`26.2-snapshot-1`（2026-04-07）** 起进入官方库列表，26.1 全线（含 `26w14a` 这类周快照）没有。
//!
//! # 为什么用库表而不是版本号
//!
//! 硬编码 `is_at_least("26.2-snapshot-2")` 对官方主线是对的，但周快照（`26w14a`）、
//! 继承版本、第三方改版都拿不到可靠的比较结果（`GameVersion::is_known() == false` 时
//! 门限恒假）。库表是版本自己声明的**事实**，比版本号串稳。
//!
//! # ⚠️ 「库存在」≠「参数可用」
//!
//! `26.2-snapshot-1` 已经有 `lwjgl-vulkan`，但 `--graphicsBackend` 要到
//! **`26.2-snapshot-2`（2026-04-09）** 才进 `Main.class`，两者差两天（实测结论）。
//! 因此能力位只适合做：
//! - **必要条件的快速否定**（没有库 → 一定没有 Vulkan 后端）；
//! - UI 可用性提示（渲染设置里出不出「图形 API」三选一）；
//! - **无法排序的版本**（周快照）的兜底判据。
//!
//! 可解析版本的下发门限仍以 `26.2-snapshot-2` 为准，见
//! [`crate::launch::game_args::graphics_backend_allowed`]。

use super::manifest::VersionJson;

/// Vulkan 后端的 Maven 坐标（`group:artifact`）。
pub const VULKAN_LIBRARY_GROUP: &str = "org.lwjgl";
/// Vulkan 后端的 Maven 坐标（artifact）。
pub const VULKAN_LIBRARY_ARTIFACT: &str = "lwjgl-vulkan";

/// 一条库名是否是 LWJGL 的 Vulkan 绑定。
///
/// 吃 `group:artifact:version[:classifier][@ext]` 全形态；只比对前两段，因此
/// `org.lwjgl:lwjgl-vulkan:3.4.3`、`org.lwjgl:lwjgl-vulkan:3.4.3:natives-macos`
/// 与 `org.lwjgl:lwjgl-vulkan:3.4.3@jar` 都算命中。
#[must_use]
pub fn is_vulkan_library(name: &str) -> bool {
    let mut parts = name.split(':');
    let (Some(group), Some(artifact)) = (parts.next(), parts.next()) else {
        return false;
    };
    // classifier / `@ext` 可能粘在 artifact 上（`lwjgl-vulkan@jar`）。
    let artifact = artifact.split('@').next().unwrap_or(artifact);
    group.trim() == VULKAN_LIBRARY_GROUP && artifact.trim() == VULKAN_LIBRARY_ARTIFACT
}

/// 从一份版本 JSON 推出的能力位（目前只有 Vulkan 一项，后续可加）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VersionCapabilities {
    /// `libraries` 里存在 `org.lwjgl:lwjgl-vulkan`。
    ///
    /// **注意语义**：这是「版本带 Vulkan 后端实现」的**必要条件**，不是
    /// 「`--graphicsBackend` 可下发」的充分条件（见模块文档）。
    pub has_vulkan_backend: bool,
}

impl VersionCapabilities {
    /// 从版本 JSON 解析能力位（只读 `libraries[].name`，不看下载信息与规则）。
    #[must_use]
    pub fn from_json(json: &VersionJson) -> Self {
        Self {
            has_vulkan_backend: json
                .libraries
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|library| is_vulkan_library(&library.name)),
        }
    }

    /// 是否带 Vulkan 后端。
    #[must_use]
    pub const fn has_vulkan_backend(&self) -> bool {
        self.has_vulkan_backend
    }
}

/// 快捷方式：库名列表里是否有 Vulkan 绑定（launch 侧拿到的库名表也能直接用）。
#[must_use]
pub fn library_names_have_vulkan<'a>(names: impl IntoIterator<Item = &'a str>) -> bool {
    names.into_iter().any(is_vulkan_library)
}
