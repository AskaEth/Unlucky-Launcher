//! MC↔Java 映射：版本 JSON 的 `javaVersion` 优先（含继承链），上限只属于 LaunchWrapper 系加载器。

use std::path::{Path, PathBuf};

use unlucky_core::game::java::mapping;

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ull-java-{tag}-{}-{nanos}-{seq}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建临时目录");
    dir
}

/// 写一份版本 JSON（`id` 决定文件名，内容由调用方给）。
fn write(root: &Path, id: &str, json: serde_json::Value) {
    let dir = root.join("versions").join(id);
    std::fs::create_dir_all(&dir).expect("建版本目录");
    std::fs::write(
        dir.join(format!("{id}.json")),
        serde_json::to_vec(&json).expect("序列化"),
    )
    .expect("写版本 JSON");
}

#[test]
fn version_json_java_version_beats_the_static_table() {
    let root = temp_dir("mapping");
    let layout = unlucky_core::game::GameLayout::new(&root);

    // 1.17 的清单自己说是 16（静态表也是 16，但来源必须标 manifest）。
    write(
        &root,
        "1.17.1",
        serde_json::json!({
            "id": "1.17.1",
            "mainClass": "net.minecraft.client.main.Main",
            "javaVersion": {"component": "java-runtime-alpha", "majorVersion": 16}
        }),
    );
    let manifest = mapping::requirement_for_version(&layout, "1.17.1");
    assert_eq!(manifest.minimum, 16);
    assert_eq!(manifest.source.as_str(), "manifest");

    // 魔改实例覆盖父版本：21 应当生效。
    write(
        &root,
        "1.20.1",
        serde_json::json!({"id": "1.20.1", "mainClass": "net.minecraft.client.main.Main"}),
    );
    write(
        &root,
        "1.20.1-forge",
        serde_json::json!({
            "id": "1.20.1-forge",
            "inheritsFrom": "1.20.1",
            "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "javaVersion": {"majorVersion": 21}
        }),
    );
    assert_eq!(
        mapping::requirement_for_version(&layout, "1.20.1-forge").minimum,
        21
    );

    // 清单没写 javaVersion：父版本也没有 → 静态表兜底。
    let table = mapping::requirement_for_version(&layout, "1.20.1");
    assert_eq!(table.minimum, 17);
    assert_eq!(table.source.as_str(), "table");

    // 版本根本没装：按 id 走静态表，不报错。
    let missing = mapping::requirement_for_version(&layout, "26.3");
    assert_eq!(missing.minimum, 25);
    let _ = std::fs::remove_dir_all(&root);
}

/// **M348 判据①（清单分支）**：清单声明了 `javaVersion` 的原版 `<1.13` **没有上限**。
///
/// 夹具刻意**写**了 `javaVersion: 8`（真 1.7.10 / 1.8.9 / 1.12.2 的官方 JSON 都带这一行，
/// 实测见报告）：这条走的是 `requirement_for_version` 的**清单分支**，`minimum` 取清单、
/// `maximum` 由 [`mapping::uses_launch_wrapper`] 决定。
///
/// 抓得住（r1 复审 P2-1：这条锚是**重放过**的）：把 `with_launch_wrapper_bound` 里的
/// `uses_launch_wrapper(..) &&` 去掉（上限写回所有 `<1.13`）⇒ `maximum` 变 `Some(8)`、
/// `check(17)` 变 `Some(TooNew)`，本用例红。
///
/// 抓不住：静态表那一格（本文件里不做清单声明时走它，见
/// [`a_vanilla_legacy_version_without_manifest_java_version_has_no_upper_bound`]）。
#[test]
fn a_vanilla_legacy_version_has_no_upper_bound() {
    let root = temp_dir("vanilla-legacy");
    let layout = unlucky_core::game::GameLayout::new(&root);
    write(
        &root,
        "1.12.2",
        serde_json::json!({
            "id": "1.12.2",
            "mainClass": "net.minecraft.client.main.Main",
            "javaVersion": {"component": "jre-legacy", "majorVersion": 8}
        }),
    );
    let vanilla = mapping::requirement_for_version(&layout, "1.12.2");
    assert_eq!(vanilla.minimum, 8, "下限不变：还是要 8 起");
    assert_eq!(vanilla.maximum, None, "原版 1.12.2 不该有上限");
    assert_eq!(vanilla.source.as_str(), "manifest");
    assert!(
        mapping::check(17, &vanilla).is_none(),
        "原版 1.12.2 + Java 17 不再判不符（M347 实测到主菜单，进世界未验）"
    );
    assert!(mapping::check(25, &vanilla).is_none(), "原版同样不封顶");
    // 下限那一侧照旧：Java 7 仍然太老。
    assert_eq!(
        mapping::check(7, &vanilla).map(|mismatch| mismatch.code()),
        Some("java_too_old")
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **M348 判据①（静态表分支）**：清单**没写** `javaVersion` 的原版 `<1.13` 同样没有上限。
///
/// 这条是真实存在的一条路，不是假想：Mojang 的 1.6.4 官方 JSON 就没有 `javaVersion`
/// （1.7.10 起才有，实测报文见报告），整合包/第三方启动器重写过的清单也常常没有。
/// 它走 `requirement_for_version` 的**静态表兜底**分支——那正是这次改判据的**另一处**：
/// 改前 `requirement_for` 对 `<1.13` 一律返回 `(8, Some(8))`。
///
/// 抓得住（r1 复审 P2-1 要求补的那条锚，已重放）：把静态表末行改回
/// `JavaRequirement::table(8, Some(8))` ⇒ `maximum` 变 `Some(8)`、`check(17)` 变
/// `Some(TooNew)`，本用例红。
#[test]
fn a_vanilla_legacy_version_without_manifest_java_version_has_no_upper_bound() {
    let root = temp_dir("vanilla-legacy-table");
    let layout = unlucky_core::game::GameLayout::new(&root);
    // 1.6.4 官方 JSON 的真形状：有 mainClass、**没有** javaVersion。
    write(
        &root,
        "1.6.4",
        serde_json::json!({
            "id": "1.6.4",
            "mainClass": "net.minecraft.client.main.Main"
        }),
    );
    let vanilla = mapping::requirement_for_version(&layout, "1.6.4");
    assert_eq!(vanilla.minimum, 8, "静态表兜底的下限还是 8");
    assert_eq!(
        vanilla.maximum, None,
        "原版 1.6.4 不该有上限（静态表那处已改）"
    );
    assert_eq!(vanilla.source.as_str(), "table", "这条必须真的走静态表分支");
    assert!(
        mapping::check(17, &vanilla).is_none(),
        "原版 1.6.4 + Java 17 不判「太新」（起得来起不来没验，见报告「没验的部分」）"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **M348 判据②**：带 **LaunchWrapper 系加载器**的 `<1.13` 仍然锁 `maximum = 8`。
///
/// 两条证据各钉一次：① 清单主类是 `net.minecraft.launchwrapper.Launch`（真清单形状）；
/// ② 清单读不到 / 没写主类时按版本 id 的 `forge` / `liteloader` / `optifine` 标记兜底。
///
/// 抓得住（已重放）：把 `with_launch_wrapper_bound` 里那一步整段删掉（谁都不设上限）——
/// ①② 都红。
#[test]
fn a_launch_wrapper_legacy_version_still_caps_at_java_8() {
    let root = temp_dir("launch-wrapper");
    let layout = unlucky_core::game::GameLayout::new(&root);
    write(
        &root,
        "1.12.2",
        serde_json::json!({
            "id": "1.12.2",
            "mainClass": "net.minecraft.client.main.Main",
            "javaVersion": {"component": "jre-legacy", "majorVersion": 8}
        }),
    );
    // ① 主类判据（真 Forge 1.12.2 清单的形状，见 `tests/liteloader/support.rs`）。
    write(
        &root,
        "1.12.2-forge-14.23.5.2860",
        serde_json::json!({
            "id": "1.12.2-forge-14.23.5.2860",
            "inheritsFrom": "1.12.2",
            "mainClass": "net.minecraft.launchwrapper.Launch"
        }),
    );
    let forge = mapping::requirement_for_version(&layout, "1.12.2-forge-14.23.5.2860");
    assert_eq!(
        (forge.minimum, forge.maximum),
        (8, Some(8)),
        "LaunchWrapper 仍锁 8"
    );
    assert_eq!(
        mapping::check(17, &forge).map(|mismatch| mismatch.code()),
        Some("java_too_new"),
        "Forge 1.12.2 + Java 17 必须判「太新」"
    );
    // ② id 标记兜底：版本 JSON 没装（读不到）也不能漏掉这道上限。
    let unreadable = mapping::requirement_for_version(&layout, "1.12.2-LiteLoader");
    assert_eq!((unreadable.minimum, unreadable.maximum), (8, Some(8)));
    // ≤1.12 的 OptiFine 也在 LaunchWrapper 上（`02-launch-pipeline-p1.md:204`），
    // 但 1.13+ 的 OptiFine 属于 ModLauncher 那一代 ⇒ 不因 id 被收窄。
    let optifine_legacy = mapping::requirement_for_version(&layout, "1.12.2-OptiFine_HD_U_C7");
    assert_eq!(
        (optifine_legacy.minimum, optifine_legacy.maximum),
        (8, Some(8))
    );
    let optifine_modern = mapping::requirement_for_version(&layout, "1.13.2-OptiFine_HD_U_E6");
    assert_eq!(
        (optifine_modern.minimum, optifine_modern.maximum),
        (8, None),
        "1.13+ 的 OptiFine 不该被 id 兜底收窄（`<1.13` 是硬门槛）"
    );
    // 1.13+ 的 Forge 换成 ModLauncher / BootstrapLauncher，不在这一条约束里。
    write(
        &root,
        "1.16.5",
        serde_json::json!({"id": "1.16.5", "mainClass": "net.minecraft.client.main.Main"}),
    );
    write(
        &root,
        "1.16.5-forge-36.2.0",
        serde_json::json!({
            "id": "1.16.5-forge-36.2.0",
            "inheritsFrom": "1.16.5",
            "mainClass": "cpw.mods.modlauncher.Launcher"
        }),
    );
    let modlauncher = mapping::requirement_for_version(&layout, "1.16.5-forge-36.2.0");
    assert_eq!(
        (modlauncher.minimum, modlauncher.maximum),
        (8, None),
        "1.13+ 的 Forge 不在这条上限里（回归：口径不许放宽成「所有 Forge」）"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn parent_java_version_is_inherited() {
    let root = temp_dir("inherit");
    let layout = unlucky_core::game::GameLayout::new(&root);
    write(
        &root,
        "1.20.5",
        serde_json::json!({
            "id": "1.20.5",
            "mainClass": "net.minecraft.client.main.Main",
            "javaVersion": {"component": "java-runtime-delta", "majorVersion": 21}
        }),
    );
    write(
        &root,
        "1.20.5-fabric",
        serde_json::json!({
            "id": "1.20.5-fabric",
            "inheritsFrom": "1.20.5",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient"
        }),
    );
    let requirement = mapping::requirement_for_version(&layout, "1.20.5-fabric");
    assert_eq!(requirement.minimum, 21);
    assert_eq!(requirement.maximum, None);
    assert_eq!(requirement.source.as_str(), "manifest");
    let _ = std::fs::remove_dir_all(&root);
}
