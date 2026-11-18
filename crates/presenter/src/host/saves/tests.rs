//! `host::saves` 的用例：**真盘端到端**（派生运行目录 → core 只读列目录 → 快照回来）。
//!
//! 端到端那条是这个模块存在的主要理由：只有"真的建一个实例根、真的放几个世界和截图再读回来"
//! 才能验「派生出来的目录就是玩家看到的那个目录」。
//!
//! 抓不住什么：`OpenFolder` **不在这里真跑**（它会拉起用户的文件管理器）；它唯一的判断
//! （打开哪个目录）由 [`open_target_degrades_from_saves_to_run_dir`] 这条纯函数用例钉住，
//! 真正 spawn 的那一步在 core 的 `platform::opener` 用例里。
//!
//! 「游戏在跑就拒」（M247）由 [`write_commands_do_not_touch_the_disk_while_the_game_is_running`]
//! 从 **`dispatch` 那一层**验：三条写命令一条都不落 core（盘上指纹前后一致 + 备份落点没被建），
//! 并且回一条 `GameRunning` 的具名失败。

use super::*;
use crate::dto::{InstanceRuntime, LaunchArgsView, LoaderInfo, LoaderKind};
use unlucky_core::game::saves::{SavesScan, SavesSkip, SkipKind};

const ID: &str = "survival";
const VERSION: &str = "1.20.4-fabric-0.16.5";

/// 一次性临时根（与本 crate 其它 host 测试同款：不引第三方 tempfile）。
struct TempRoot(PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 一个可用的便携根 + 一个 `custom` 档位的实例（运行目录由实例自己指，D20 里唯一这样表达的档位）。
///
/// 走 `custom` 是**故意**的：用例不必造 `versions/<id>/` 就能验「派生出来的目录就是读的那个」。
fn fixture(tag: &str) -> (TempRoot, HostEnv, PathBuf) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("{tag}-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    let root = TempRoot(path);
    std::fs::create_dir_all(&root.0).expect("建临时目录");
    let env = HostEnv::at_root(&root.0);
    let run = root.0.join("run");
    std::fs::create_dir_all(&run).expect("建运行目录");
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
        isolation: "custom".to_string(),
        version_id: VERSION.to_string(),
        version_present: true,
        launch: LaunchArgsView::default(),
    }
}

/// 一个真世界（`saves/<name>/level.dat`）。
fn world(run: &std::path::Path, name: &str) {
    std::fs::create_dir_all(run.join("saves").join(name)).expect("建世界");
    std::fs::write(run.join("saves").join(name).join("level.dat"), b"nbt").expect("写 level.dat");
}

/// **端到端**（真盘 + 真派生）：世界 / 截图 / 跳过原因都过桥，且快照里的路径就是派生目录。
///
/// 抓得住：派生用错目录、`icon.png` 被当成"读到了"、跳过原因被折叠、字节数丢了。
#[test]
fn list_round_trips_on_a_real_root() {
    let (_root, env, run) = fixture("saves-e2e");
    std::fs::create_dir_all(run.join("saves/My World")).expect("建世界");
    std::fs::write(run.join("saves/My World/icon.png"), b"png").expect("写图标");
    std::fs::create_dir_all(run.join("saves/Bare World")).expect("建世界 2");
    std::fs::create_dir_all(run.join("screenshots")).expect("建截图目录");
    std::fs::write(run.join("screenshots/2024-05-01_10.00.00.png"), b"abc").expect("写截图");
    std::fs::write(run.join("screenshots/notes.txt"), b"x").expect("写非图片");

    let listed = list(&env, ID).expect("列存档");
    assert!(listed.exists && listed.screenshots_exists);
    assert_eq!(std::path::PathBuf::from(&listed.dir), run.join("saves"));
    assert_eq!(
        std::path::PathBuf::from(&listed.screenshots_dir),
        run.join("screenshots")
    );

    let worlds: Vec<(&str, bool)> = listed
        .worlds
        .iter()
        .map(|world| (world.dir.as_str(), world.icon))
        .collect();
    assert_eq!(worlds, [("Bare World", false), ("My World", true)]);
    assert_eq!(listed.world_count(), 2);

    assert_eq!(listed.screenshot_count(), 1);
    assert_eq!(listed.screenshots[0].bytes, 3);
    assert_eq!(
        listed.skipped_count(),
        1,
        "非图片文件要登记原因，不能静默丢"
    );
    assert_eq!(listed.skipped[0].reason, SavesSkipKind::NotAnImage);
    assert_eq!(listed.skipped[0].area, SavesArea::Screenshots);
}

/// 「打开文件夹」打开哪个目录：`saves/` → `screenshots/` → 运行目录。
///
/// 纯函数，**不 spawn**（真拉起文件管理器那一步在 core 的 opener 用例里）。
#[test]
fn open_target_degrades_from_saves_to_run_dir() {
    let run = PathBuf::from("D:/ULL/.ULL/data/instances/x");
    let mut scan = SavesScan {
        dir: run.join("saves"),
        exists: true,
        worlds: Vec::new(),
        screenshots_dir: run.join("screenshots"),
        screenshots_exists: true,
        screenshots: Vec::new(),
        skipped: Vec::new(),
    };
    assert_eq!(open_target(&scan, &run), run.join("saves"));

    scan.exists = false;
    assert_eq!(
        open_target(&scan, &run),
        run.join("screenshots"),
        "saves 不在时退到 screenshots"
    );

    scan.screenshots_exists = false;
    assert_eq!(open_target(&scan, &run), run, "两个都没有时打开运行目录");
}

/// 找不到实例 ⇒ 具名失败（不是"空列表"：那会让页面以为这实例没存档）。
#[test]
fn missing_instance_is_a_named_failure() {
    let (_root, env, _run) = fixture("saves-unknown");
    let failure = list(&env, "no-such-instance").expect_err("不存在的实例居然列成功了");
    assert_eq!(failure.kind, SavesFailureKind::InstanceUnavailable);
    assert_eq!(failure.instance, "no-such-instance");
    assert!(!failure.detail.is_empty(), "原因被吃掉了");
}

/// `saves/` 存在但不可用 ⇒ `DirUnusable`，且原文里带着那个路径（用户能自己去看）。
#[test]
fn unusable_dir_maps_to_dir_unusable() {
    let (_root, env, run) = fixture("saves-unusable");
    std::fs::write(run.join("saves"), b"not a dir").expect("用文件占住 saves");

    let failure = list(&env, ID).expect_err("saves 是个文件，居然列成功了");
    assert_eq!(failure.kind, SavesFailureKind::DirUnusable);
    assert!(
        failure.detail.contains("saves"),
        "原文里没有路径：{}",
        failure.detail
    );
}

/// 跳过原因一一对应（`NotADirectoryExpected` → `NotADirectory` 这条尤其容易抄错）。
#[test]
fn every_core_skip_reason_keeps_its_own_kind() {
    let cases = [
        (SkipKind::SymbolicLink, SavesSkipKind::SymbolicLink),
        (
            SkipKind::NotADirectoryExpected,
            SavesSkipKind::NotADirectory,
        ),
        (SkipKind::NotAFile, SavesSkipKind::NotAFile),
        (SkipKind::UnsafeName, SavesSkipKind::UnsafeName),
        (SkipKind::Unreadable, SavesSkipKind::Unreadable),
        (SkipKind::NotAnImage, SavesSkipKind::NotAnImage),
    ];
    for (reason, expected) in cases {
        let scan = SavesScan {
            dir: PathBuf::from("s"),
            exists: true,
            worlds: Vec::new(),
            screenshots_dir: PathBuf::from("sh"),
            screenshots_exists: true,
            screenshots: Vec::new(),
            skipped: vec![SavesSkip {
                area: saves::Area::Saves,
                file: "x".to_string(),
                reason,
            }],
        };
        assert_eq!(view(ID, &scan, None).skipped[0].reason, expected);
    }
}

/// `(相对路径, 是否目录, 字节数, mtime 毫秒)` 的排序列表：「这一拍有没有写盘」的判据本体。
fn fingerprint(root: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            out.push(format!(
                "{rel}|dir={}|len={}|mtime={}",
                meta.is_dir(),
                meta.len(),
                meta.modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
                    .map_or(0, |since| since.as_millis())
            ));
            if meta.is_dir() {
                stack.push(path);
            }
        }
    }
    out.sort();
    out
}

mod write;
