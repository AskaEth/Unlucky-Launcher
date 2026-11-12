//! `host::mods` 的用例（从 `host/mods.rs` 拆出：那边 457 行越了 400 行硬线）。
//!
//! 三条：**真盘端到端**（派生运行目录 → core 改名 → 快照回来）、跳过原因映射、错误分类。
//! 端到端那条是这个模块存在的主要理由——启停的正确性只有"真的建一个实例根再改一次盘"能验。

use super::*;
use crate::dto::{InstanceRuntime, LaunchArgsView, LoaderInfo, LoaderKind};
use unlucky_core::addon::mods::ModSkip;

const ID: &str = "survival";
const VERSION: &str = "1.20.4-fabric-0.16.5";

/// 一次性临时根（与本 crate 其它 host 测试同款：不引第三方 tempfile）。
struct TempRoot(PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 一个可用的便携根：`mods/`（给定文件）+ `profiles.json` + `custom` 档位的实例设置。
///
/// 走 `custom` 是**故意**的：它是 D20 里唯一「运行目录由实例自己指」的档位，于是用例不必造
/// `versions/<id>/` 也能验「派生出来的目录就是启停的目标目录」。
fn fixture(tag: &str, files: &[(&str, &[u8])]) -> (TempRoot, HostEnv, PathBuf) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("{tag}-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    let root = TempRoot(path);
    std::fs::create_dir_all(&root.0).expect("建临时目录");
    let env = HostEnv::at_root(&root.0);
    let run = root.0.join("run");
    std::fs::create_dir_all(run.join("mods")).expect("建 mods");
    for (name, body) in files {
        std::fs::write(run.join("mods").join(name), body).expect("写文件");
    }
    std::fs::create_dir_all(env.layout.config()).expect("建 config");
    let profile = format!(r#"{{"id":"{ID}","name":"生存","versionId":"{VERSION}"}}"#);
    std::fs::write(
        env.layout.profiles_config(),
        format!(r#"{{"schemaVersion":1,"profiles":[{profile}]}}"#),
    )
    .expect("写 profiles");
    let config = format!(
        r#"{{"schemaVersion":1,"id":"{ID}","versionId":"{VERSION}","isolation":"custom","gameDir":{}}}"#,
        serde_json::to_string(&run).expect("路径转 JSON")
    );
    let path = env.layout.instance_config(ID).expect("实例 id");
    std::fs::create_dir_all(path.parent().expect("instances 目录")).expect("建 instances");
    std::fs::write(path, config).expect("写实例设置");
    (root, env, run)
}

fn instance() -> InstanceSummary {
    InstanceSummary {
        id: ID.to_string(),
        name: "生存".to_string(),
        mc_version: "1.20.4".to_string(),
        loader: LoaderInfo {
            kind: LoaderKind::Fabric,
            version: "0.16.5".to_string(),
        },
        last_played: None,
        runtime: InstanceRuntime::Idle,
        icon: None,
        isolation: "isolated".to_string(),
        version_id: VERSION.to_string(),
        version_present: true,
        launch: LaunchArgsView::default(),
    }
}

/// **端到端**（真盘 + 真派生）：列目录 → 启停 → 再列，顺带验运行中被挡。
///
/// 打通「派生运行目录 → core 改盘 → 快照回来」整条链。抓得住：派生用错目录、启停后没重扫、
/// `.disabled` 判据与 core 不一致。抓不住：Windows 上的占用/权限失败（那要靠真跑）。
#[test]
fn list_and_toggle_round_trip_on_a_real_root() {
    let (_root, env, run) = fixture(
        "mods-e2e",
        &[
            ("sodium.jar", b"aa"),
            ("jei.jar.disabled", b"bbb"),
            ("old.jar.old", b"cccc"),
        ],
    );

    let listed = list(&env, ID, false).expect("列目录");
    assert!(listed.exists && listed.known && !listed.running);
    assert_eq!((listed.enabled_count(), listed.disabled_count()), (2, 1));
    assert!(listed.entries.iter().any(|entry| entry.old));

    let report = toggle(&env, ID, "sodium.jar", false, false);
    assert!(report.toggled.expect("要报告改了什么").renamed);
    assert!(run.join("mods/sodium.jar.disabled").is_file());
    assert_eq!(report.list.expect("新快照").enabled_count(), 1);

    // 再启用回去（`.disabled` 剥掉，字节不动）
    let report = toggle(&env, ID, "sodium.jar.disabled", true, false);
    assert!(report.toggled.expect("报告").renamed);
    assert_eq!(report.list.expect("快照").enabled_count(), 2);
    assert_eq!(
        std::fs::read(run.join("mods/sodium.jar")).expect("读回"),
        b"aa"
    );

    // 游戏在跑：host 直接挡（不碰盘、不改名）
    let blocked = toggle(&env, ID, "sodium.jar", false, true);
    assert_eq!(
        blocked.list.expect_err("运行中必须拒").kind,
        ModFailureKind::Running
    );
    assert!(blocked.toggled.is_none());
    assert!(run.join("mods/sodium.jar").is_file(), "盘被动了");
    assert!(!run.join("mods/sodium.jar.disabled").exists());
}

/// 跳过原因一一对应，且 runtime/known 标记原样过桥。
///
/// 抓得住：`NotAFile` 归到 `SymbolicLink`（"为什么没列出来"撒谎）、`running` 被写死。
#[test]
fn the_snapshot_keeps_skip_reasons_and_flags() {
    let scan = ModScan {
        dir: PathBuf::from("m"),
        exists: true,
        entries: Vec::new(),
        skipped: vec![ModSkip {
            file: "x".to_string(),
            reason: SkipReason::SymbolicLink,
        }],
    };
    let listed = view(&instance(), &scan, true);
    assert!(listed.running && listed.known && listed.exists);

    let kinds = [
        (SkipReason::SymbolicLink, ModSkipKind::SymbolicLink),
        (SkipReason::NotAFile, ModSkipKind::NotAFile),
        (SkipReason::UnsafeName, ModSkipKind::UnsafeName),
        (SkipReason::Unreadable, ModSkipKind::Unreadable),
    ];
    for (reason, expected) in kinds {
        let mut scan = scan.clone();
        scan.skipped[0].reason = reason;
        assert_eq!(view(&instance(), &scan, false).skipped[0].reason, expected);
    }
}

/// 错误分类：每个 core 变体都落到自己的大类（没有「未知 → 一律 IO」的兜底），原因不丢。
#[test]
fn every_core_error_keeps_its_own_kind() {
    let dir = PathBuf::from("m");
    let cases = [
        (
            ModError::UnsafeName {
                file: "a/b".into(),
                detail: "含分隔符".into(),
            },
            ModFailureKind::UnsafeName,
        ),
        (
            ModError::NotFound {
                file: "x.jar".into(),
                dir: dir.clone(),
            },
            ModFailureKind::NotFound,
        ),
        (
            ModError::Collision {
                file: "x.jar".into(),
                target: "x.jar.disabled".into(),
                dir,
            },
            ModFailureKind::Collision,
        ),
        (
            ModError::Rename {
                file: "x.jar".into(),
                target: "x.jar.disabled".into(),
                detail: "被占用".into(),
            },
            ModFailureKind::Rename,
        ),
    ];
    for (error, expected) in cases {
        let (kind, detail) = classify(&error);
        assert_eq!(kind, expected, "{error}");
        assert!(!detail.is_empty(), "{error} 的分类把原因吃掉了");
    }
}
