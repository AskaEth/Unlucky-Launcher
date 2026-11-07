//! 版本能力位：从版本 JSON 的库表推 `has_vulkan_backend`。
//!
//! 事实基线（`research/05-mc-vulkan.md` §2.2，piston-meta 921 个版本逐版扫过）：
//! `org.lwjgl:lwjgl-vulkan` 从 **26.2-snapshot-1** 起出现，26.1 全线（含 `26w14a`）没有。
//! 所以「库在不在」是判断版本带不带 Vulkan 后端的可靠依据。

use unlucky_core::game::capability::{
    is_vulkan_library, library_names_have_vulkan, VersionCapabilities,
};
use unlucky_core::game::manifest::VersionJson;

use crate::helpers::parse;

/// 26.2-snapshot-1 的库表形态（Vulkan 三件套 + 常规库，实测结构）。
const VULKAN_ERA_JSON: &str = r#"{
  "id": "26.2-snapshot-1",
  "type": "snapshot",
  "mainClass": "net.minecraft.client.main.Main",
  "libraries": [
    { "name": "org.lwjgl:lwjgl:3.4.1", "downloads": { "artifact": { "path": "org/lwjgl/lwjgl/3.4.1/lwjgl-3.4.1.jar" } } },
    { "name": "org.lwjgl:lwjgl-vulkan:3.4.1", "downloads": { "artifact": { "path": "org/lwjgl/lwjgl-vulkan/3.4.1/lwjgl-vulkan-3.4.1.jar" } } },
    { "name": "org.lwjgl:lwjgl-vulkan:3.4.1:natives-macos" },
    { "name": "org.lwjgl:lwjgl-shaderc:3.4.1" },
    { "name": "com.mojang:brigadier:1.3.10" }
  ]
}"#;

/// 26.1（含 `26w14a` 周快照那一段）的库表：只有 opengl，没有 vulkan。
const PRE_VULKAN_JSON: &str = r#"{
  "id": "26.1",
  "type": "release",
  "mainClass": "net.minecraft.client.main.Main",
  "libraries": [
    { "name": "org.lwjgl:lwjgl:3.4.1" },
    { "name": "org.lwjgl:lwjgl-glfw:3.4.1" },
    { "name": "org.lwjgl:lwjgl-opengl:3.4.1" }
  ]
}"#;

#[test]
fn version_json_with_lwjgl_vulkan_carries_the_capability() {
    let version = parse(VULKAN_ERA_JSON);
    assert!(version.has_vulkan_backend());
    let capabilities = VersionCapabilities::from_json(&version);
    assert!(capabilities.has_vulkan_backend());
}

#[test]
fn version_json_without_lwjgl_vulkan_lacks_the_capability() {
    let version = parse(PRE_VULKAN_JSON);
    assert!(!version.has_vulkan_backend());
    assert_eq!(
        VersionCapabilities::from_json(&version),
        VersionCapabilities::default()
    );
}

#[test]
fn missing_libraries_field_is_not_a_capability() {
    let version = parse(r#"{ "id": "custom", "mainClass": "a.B" }"#);
    assert!(!version.has_vulkan_backend());
}

#[test]
fn coordinate_forms_are_all_recognised() {
    // 主构件、带 classifier 的 natives、带 @ext 的写法都算命中（只比对 group:artifact）。
    assert!(is_vulkan_library("org.lwjgl:lwjgl-vulkan:3.4.3"));
    assert!(is_vulkan_library(
        "org.lwjgl:lwjgl-vulkan:3.4.3:natives-macos"
    ));
    assert!(is_vulkan_library("org.lwjgl:lwjgl-vulkan:3.4.3@jar"));

    // 相邻但不该命中的（前缀相同 / group 不同）。
    assert!(!is_vulkan_library("org.lwjgl:lwjgl-opengl:3.4.3"));
    assert!(!is_vulkan_library("org.lwjgl:lwjgl:3.4.3"));
    assert!(!is_vulkan_library("com.mojang:lwjgl-vulkan:1.0"));
    assert!(!is_vulkan_library("lwjgl-vulkan"));
    assert!(!is_vulkan_library(""));
}

#[test]
fn library_name_slice_helper_matches_the_json_path() {
    let names = ["org.lwjgl:lwjgl:3.4.1", "org.lwjgl:lwjgl-vulkan:3.4.1"];
    assert!(library_names_have_vulkan(names));
    assert!(!library_names_have_vulkan(["org.lwjgl:lwjgl:3.4.1"]));
    assert!(!library_names_have_vulkan(std::iter::empty()));

    // 与 game 侧的库表判定同源：同一份 JSON 两条路径给同一结论。
    let version = parse(VULKAN_ERA_JSON);
    assert_eq!(
        library_names_have_vulkan(
            version
                .libraries
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|l| l.name.as_str())
        ),
        version.has_vulkan_backend()
    );
}

#[test]
fn capability_bit_is_not_the_parameter_gate() {
    // 26.2-snapshot-1 已经有库，却还没有 `--graphicsBackend`（实测差两天）。
    // 能力位是「必要条件」，下发门限另有精确版本门（见 launch 侧
    // `graphics_backend_allowed`），两者不能被压成一个判断。
    let snapshot_one = parse(VULKAN_ERA_JSON);
    assert!(
        snapshot_one.has_vulkan_backend(),
        "26.2-snapshot-1 有 Vulkan 库"
    );
    assert_eq!(snapshot_one.id.as_deref(), Some("26.2-snapshot-1"));
}

#[test]
fn version_json_accessor_delegates_to_capability_module() {
    for json in [VULKAN_ERA_JSON, PRE_VULKAN_JSON] {
        let version: VersionJson = parse(json);
        assert_eq!(
            version.has_vulkan_backend(),
            VersionCapabilities::from_json(&version).has_vulkan_backend()
        );
    }
}
