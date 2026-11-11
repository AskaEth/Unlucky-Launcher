//! `host::launch` 的回归用例（M157 r3 复审 P2-1）：装配用的游戏版本必须是**继承链的 MC 根版本**。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

/// 一次性临时目录（同 `host::launch_args` 测试：不引第三方 tempfile）。
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ull-host-launch-{tag}-{}-{seq}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("建临时目录");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_version(env: &HostEnv, id: &str, json: &str) {
    let path = env.game.version_json(id);
    std::fs::create_dir_all(path.parent().expect("版本目录")).expect("版本目录");
    std::fs::write(&path, json).expect("写版本 JSON");
}

/// **周快照 + 加载器实例**的装配：`SnapshotInput.game_version` 要拿继承链根的 `26w14a`，
/// 不是带加载器后缀的 `26w14a-fabric-0.16.5`。
///
/// 抓得住：`build_request` 回退成 `game_version: None`（装配链用 `version_id` 判门限）——
/// 那时 `GameVersion::is_weekly_snapshot` 的 6 字符形态认不出这个 id，`--graphicsBackend`
/// 会在拼装链里被静默跳过，而详情页的门槛（`host::launch_args::backend_gate`，用根版本）
/// 显示「可用」——正是复审复现的「UI 说能开、命令行里没有」。
/// 抓不住：拼装本身（`crates/core/src/launch/overrides/tests.rs` 的
/// `modded_weekly_snapshot_needs_the_root_version_for_the_gate` 出完整 argv）。
#[test]
fn weekly_loader_instance_assembles_against_the_mc_root_version() {
    let dir = TempDir::new("weekly");
    let env = HostEnv::at_root(dir.path());
    std::fs::create_dir_all(env.layout.config()).expect("config 目录");

    // 周快照本体：库表带 lwjgl-vulkan（能力位），主类齐全。
    write_version(
        &env,
        "26w14a",
        r#"{"id":"26w14a","type":"snapshot","mainClass":"net.minecraft.client.main.Main",
            "libraries":[{"name":"org.lwjgl:lwjgl-vulkan:3.4.3"}]}"#,
    );
    // Fabric 层：只写自己的主类 + inheritsFrom（真实落盘形状）。
    write_version(
        &env,
        "26w14a-fabric-0.16.5",
        r#"{"id":"26w14a-fabric-0.16.5","inheritsFrom":"26w14a",
            "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient"}"#,
    );

    let target = LaunchTarget {
        id: "inst-1".to_string(),
        name: "周快照 Fabric".to_string(),
        version_id: "26w14a-fabric-0.16.5".to_string(),
        account: None,
    };
    let (request, _warnings) = build_request(&env, &target).expect("装配应成功");

    assert_eq!(
        request.instance.version.raw(),
        "26w14a",
        "装配用的游戏版本必须是继承链的 MC 根版本（带加载器后缀的 id 会让周快照门限失效）"
    );
    assert!(
        request.instance.version.is_weekly_snapshot(),
        "根版本要能被识别成周快照形态"
    );
    assert!(
        request.manifest.has_vulkan_backend,
        "能力位来自继承链解析后的库表（lwjgl-vulkan 在父层）"
    );
    assert!(
        !request.instance.version.is_known(),
        "周快照刻意不可排序（is_known=false），门限只能靠库表兜底"
    );
}
