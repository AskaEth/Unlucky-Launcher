//! `--graphicsBackend` 的两套判据（能力位 vs 精确版本门）与周快照放行。
//!
//! 事实基线（`research/05-mc-vulkan.md` §3.2/§4.2，实测）：
//! - `--graphicsBackend` 从 **26.2-snapshot-2**（2026-04-09）才有；
//! - `org.lwjgl:lwjgl-vulkan` 从 **26.2-snapshot-1**（2026-04-07）就有 —— 差两天；
//! - 周快照（`26w14a`）无法与 `26.2-snapshot-N` 排序，只能靠库表判。
//!
//! 结论：能力位**不能**替代精确版本门，两者的分工是本文件的断言对象。

use unlucky_core::launch::game_args::{graphics_backend_allowed, GRAPHICS_BACKEND_MIN_VERSION};

use crate::prelude::*;

#[test]
fn known_versions_use_the_exact_threshold_and_ignore_the_capability_bit() {
    // 26.2-snapshot-1：有 Vulkan 库却没有参数，绝不能因为能力位放行
    // （jopt-simple 未开 allowsUnrecognizedOptions，未声明选项会让游戏直接起不来）。
    assert!(!graphics_backend_allowed(
        &GameVersion::parse("26.2-snapshot-1"),
        true
    ));
    assert!(!graphics_backend_allowed(
        &GameVersion::parse("26.2-snapshot-1"),
        false
    ));
    // 门限上的那一版起放行（能力位无关）。
    assert!(graphics_backend_allowed(
        &GameVersion::parse(GRAPHICS_BACKEND_MIN_VERSION),
        false
    ));
    assert!(graphics_backend_allowed(&GameVersion::parse("26.2"), false));
    assert!(graphics_backend_allowed(&GameVersion::parse("26.3"), false));
    // 可解析的老版本：能力位再真也不放行。
    assert!(!graphics_backend_allowed(
        &GameVersion::parse("1.20.1"),
        true
    ));
}

#[test]
fn weekly_snapshots_fall_back_to_the_capability_bit() {
    let weekly = GameVersion::parse("26w14a");
    assert!(!weekly.is_known(), "周快照不做数字排序（不猜）");
    assert!(weekly.is_weekly_snapshot());

    // 26.1 线（含周快照）没有 lwjgl-vulkan → 丢弃，和以前一样。
    assert!(!graphics_backend_allowed(&weekly, false));
    // 带 Vulkan 库的周快照（26.2 之后的周快照）→ 不再一律丢弃（这就是被修的误伤）。
    assert!(graphics_backend_allowed(&weekly, true));
    assert!(graphics_backend_allowed(
        &GameVersion::parse("26w30a"),
        true
    ));
}

#[test]
fn non_weekly_unknown_versions_are_still_dropped() {
    // 第三方自定义版本串没有可依据的语义，能力位为真也不猜。
    let custom = GameVersion::parse("custom-build");
    assert!(!custom.is_known());
    assert!(!custom.is_weekly_snapshot());
    assert!(!graphics_backend_allowed(&custom, true));
    assert!(!graphics_backend_allowed(&GameVersion::default(), true));
}

#[test]
fn weekly_snapshot_shape_is_parsed_narrowly() {
    for shape in ["20w14a", "26w14a", "26W14A", "12w01a"] {
        assert!(
            GameVersion::parse(shape).is_weekly_snapshot(),
            "{shape} 应识别为周快照"
        );
    }
    for other in ["26.2-snapshot-1", "26w14", "26ww14a", "w14a", "26w14ab", ""] {
        assert!(
            !GameVersion::parse(other).is_weekly_snapshot(),
            "{other} 不该被当成周快照"
        );
    }
}

#[test]
fn assemble_lets_a_vulkan_weekly_snapshot_through() {
    let (request, ctx) = weekly_request("26w30a", true);
    let assembled = assemble(&request, &ctx).expect("组装应成功");
    assert!(
        assembled
            .args
            .windows(2)
            .any(|pair| pair[0] == "--graphicsBackend" && pair[1] == "vulkan"),
        "带 Vulkan 库的周快照应下发 --graphicsBackend：{:?}",
        assembled.args
    );
    assert!(!assembled
        .warnings
        .iter()
        .any(|warning| matches!(warning, AssemblyWarning::GraphicsBackendSkipped { .. })));
}

#[test]
fn assemble_still_drops_it_for_a_pre_vulkan_weekly_snapshot() {
    let (request, ctx) = weekly_request("26w14a", false);
    let assembled = assemble(&request, &ctx).expect("组装应成功");
    assert!(!assembled.args.iter().any(|arg| arg == "--graphicsBackend"));
    assert!(assembled
        .warnings
        .iter()
        .any(|warning| matches!(warning, AssemblyWarning::GraphicsBackendSkipped { .. })));
}

#[test]
fn adapter_derives_the_capability_bit_from_the_library_names() {
    let dir = temp_dir("adapter-vulkan-capability");
    let with_vulkan = vec![
        LibraryInput::new(dir.join("libraries/lwjgl.jar"), "org.lwjgl:lwjgl:3.4.3"),
        LibraryInput::new(
            dir.join("libraries/lwjgl-vulkan.jar"),
            "org.lwjgl:lwjgl-vulkan:3.4.3",
        ),
    ];
    let without_vulkan = vec![LibraryInput::new(
        dir.join("libraries/lwjgl.jar"),
        "org.lwjgl:lwjgl:3.4.3",
    )];

    for (libraries, expected) in [(&with_vulkan, true), (&without_vulkan, false)] {
        let snapshot = SnapshotInput {
            main_class: Some("net.minecraft.client.main.Main"),
            libraries,
            ..SnapshotInput::minimal("26.3", "")
        };
        let output = build_request(&snapshot, AdapterOptions::strict());
        assert_eq!(
            output.request.manifest.has_vulkan_backend, expected,
            "库表 {libraries:?} 的能力位判定不对"
        );
    }
}

/// 一个只关心 `--graphicsBackend` 的请求（周快照版本 + 指定能力位）。
fn weekly_request(version: &str, has_vulkan_backend: bool) -> (LaunchRequest, PlatformCtx) {
    let (mut request, ctx) = legacy_forge_request();
    request.instance.version = GameVersion::parse(version);
    request.manifest.has_vulkan_backend = has_vulkan_backend;
    request.graphics_backend = Some("vulkan".to_string());
    (request, ctx)
}
