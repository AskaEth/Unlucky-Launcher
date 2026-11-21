//! M273：实例快照 = **档案 ∪ 盘上的 `versions/*`**（外部实例发现）。
//!
//! 用户实测的症状是「HMCL 能看见 `<.minecraft>/versions/樱落怡然客户端`，ULL 的实例页是空的」。
//! 这组的三个用例正好按那次事故的形状造数据：临时数据根里只有 `versions/`，**没有**
//! `profiles.json`。
//!
//! | 用例 | 抓得住什么 | 抓不住什么 |
//! |---|---|---|
//! | [`versions_on_disk_become_instances`] | 列表漏读外部版本（用户那次事故的正面）；坏目录被当成实例；档案绑定的版本被重复列一遍 | 外部实例的**加载器/版本号识别是否准确**（那是 `describe_version` 的事）；真 `versions/<id>/<id>.json` 的全部变体（扫描判据在 core 的 `adopt` 测试里） |
//! | [`external_instance_writes_nothing_until_first_edit`] | 「一打开启动器就改别人的文件夹」——`instances()` 前后目录里不许多个文件；第一次改档位之后档案必须落盘且**幂等** | 别的写路径（启动参数写回、安装 mod）是否也守「先不写」——那条由 [`external_instance_launch_args_are_usable`] 与 core 的 adopt 用例分担 |
//! | [`external_instance_launch_args_are_usable`] | 外部实例能拿到可用的启动参数视图、能写回（不再报「找不到实例」） | 启动真的能进游戏（要真 jvm/账号/网络，见报告「没验的部分」） |

use super::super::CoreHost;
use super::*;

/// 临时数据根（每个用例一个目录；守卫在断言 panic 时也会删）。
mod temp {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    pub struct Root(pub PathBuf);

    impl Root {
        pub fn new(tag: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("{tag}-{}-{seq}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("建临时目录");
            Self(path)
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(Path::new(&self.0));
        }
    }

    /// 目录下所有文件/目录的相对路径（排序后比较，用来做「写盘前后」快照）。
    pub fn entries_under(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&current) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                if let Ok(relative) = path.strip_prefix(dir) {
                    out.push(relative.to_string_lossy().replace('\\', "/"));
                }
                if path.is_dir() {
                    stack.push(path);
                }
            }
        }
        out.sort();
        out
    }
}

/// 造一个「别人的 `.minecraft`」：`versions/<id>/<id>.json`，没有 `profiles.json`。
fn write_version(env: &HostEnv, id: &str) {
    let path = env.game.version_json(id);
    std::fs::create_dir_all(path.parent().expect("版本目录")).expect("版本目录");
    std::fs::write(&path, format!(r#"{{"id":"{id}"}}"#)).expect("写版本 JSON");
}

/// 盘上有版本、没有档案 ⇒ 列表里必须有它（`versions/A/A.json` 合法 + `versions/B` 无清单）。
///
/// 「B 没有清单」是用户现场的另一半形状（别的启动器的残留目录）：它**不是**实例。
#[test]
fn versions_on_disk_become_instances() {
    let root = temp::Root::new("ull-host-game-union");
    let env = HostEnv::at_root(&root.0);
    write_version(&env, "A");
    std::fs::create_dir_all(env.game.version_dir("B")).expect("B 目录");

    let snapshot = instances(&env);
    assert_eq!(
        snapshot.len(),
        1,
        "只有 A 算实例（B 没有清单）：{snapshot:?}"
    );
    let tile = &snapshot[0];
    assert_eq!(tile.version_id, "A");
    assert_eq!(tile.name, "A", "显示名 = 版本目录名");
    assert_eq!(tile.mc_version, "A", "没有 inheritsFrom 时根就是自己");
    assert!(tile.version_present, "清单在盘上，磁贴不该亮黄条");

    // 实例 id 必须是**合法实例 id**（`config/instances/<id>.json` 与启动链都按它校验）。
    // 版本目录名可以是中文（用户现场就是），所以 id 不能直接沿用版本 id。
    assert!(
        unlucky_core::platform::layout::is_valid_instance_id(&tile.id),
        "合成的实例 id 不合法，启动链会在读实例设置时失败：{}",
        tile.id
    );

    // 同一个版本再被档案绑定（不论档案 id）⇒ 只出一条，且以档案为准（名字是用户改过的那个）。
    let profile = InstanceProfile {
        id: ProfileId::new("renamed-1"),
        name: "改过名的 A".to_string(),
        version_id: "A".to_string(),
        ..InstanceProfile::default()
    };
    write_profiles(
        &env.layout,
        &ProfileList {
            profiles: vec![profile],
            ..ProfileList::default()
        },
    )
    .expect("写清单");

    let snapshot = instances(&env);
    assert_eq!(
        snapshot.len(),
        1,
        "档案与外部版本不许重复列两遍：{snapshot:?}"
    );
    assert_eq!(snapshot[0].name, "改过名的 A");
    assert_eq!(snapshot[0].id, "renamed-1");
}

/// 浏览**不写盘**；第一次改隔离档才懒创建档案（并且幂等）。
///
/// 这是「不擅自动别人的文件夹」这条不变的唯一可验证形态：`instances()` 前后做一次目录快照，
/// 两边必须一模一样。
#[test]
fn external_instance_writes_nothing_until_first_edit() {
    let root = temp::Root::new("ull-host-game-union-readonly");
    let env = HostEnv::at_root(&root.0);
    write_version(&env, "A");

    let before = temp::entries_under(&root.0);
    let snapshot = instances(&env);
    let after = temp::entries_under(&root.0);
    assert_eq!(
        before, after,
        "读一次列表就在盘上多了东西（用户明确不要自动导入）"
    );

    let id = snapshot[0].id.clone();
    assert_eq!(
        isolation_of(&env, &env.game, &id, "A"),
        "shared",
        "`versions/A/` 里没有 mods / saves / config / options.txt ⇒ 基底档是 shared（按共用目录读）"
    );

    set_isolation(&env, &id, "isolated").expect("第一次改档位 = 懒创建档案");
    assert_eq!(
        isolation_of(&env, &env.game, &id, "A"),
        "isolated",
        "实例文件里写下的档位优先于基底档"
    );

    let list = unlucky_core::game::install::instance::read_profiles(&env.layout).expect("读档案");
    assert_eq!(list.profiles.len(), 1, "懒创建只该写一条");
    let created = &list.profiles[0];
    assert_eq!(
        created.id.as_str(),
        id,
        "档案 id 必须与视图 id 一致（否则下次列表出两条）"
    );
    assert_eq!(created.version_id, "A");
    assert_eq!(created.name, "A");

    // 幂等：再改一次不会长出第二条档案。
    set_isolation(&env, &id, "shared").expect("第二次改档位");
    let list = unlucky_core::game::install::instance::read_profiles(&env.layout).expect("读档案");
    assert_eq!(list.profiles.len(), 1, "重复改档位不该再写第二条档案");

    // 外部实例仍然只出现一条（档案那条）。
    assert_eq!(instances(&env).len(), 1);
}

/// 外部实例的启动参数：读得到视图、写回不报「找不到实例」。
#[test]
fn external_instance_launch_args_are_usable() {
    let root = temp::Root::new("ull-host-game-union-args");
    let env = HostEnv::at_root(&root.0);
    write_version(&env, "A");

    let snapshot = instances(&env);
    let tile = &snapshot[0];
    assert_eq!(
        tile.launch.instance_id, tile.id,
        "启动参数视图绑的是同一个实例"
    );
    assert_eq!(tile.launch.mc_version, "A");

    let patch = crate::dto::LaunchArgsPatch {
        jvm_args: Some("-XX:+UseG1GC".to_string()),
        ..crate::dto::LaunchArgsPatch::default()
    };
    let mut host = CoreHost::headless(env.clone());
    host.apply_launch_args(&tile.id, &patch)
        .expect("外部实例写启动参数不该报「找不到实例」");

    let list = unlucky_core::game::install::instance::read_profiles(&env.layout).expect("读档案");
    assert_eq!(list.profiles.len(), 1, "写参数同时懒创建了档案");
    let reread = instances(&env);
    assert_eq!(reread.len(), 1);
    assert_eq!(reread[0].launch.jvm_args, "-XX:+UseG1GC");
}

/// **用户现场逐字复现**：`versions/樱落怡然客户端/樱落怡然客户端.json`
/// （`id` 与文件夹同名、`type` 是 `release`、没有 `inheritsFrom`）⇒ 列表里 1 条，名字就是它。
///
/// 这一条与 [`versions_on_disk_become_instances`] 的区别只在**目录名是中文**：那正是让
/// 「实例 id 直接沿用版本 id」这个看起来最自然的写法当场失败的地方
/// （`is_valid_instance_id` 只收 ASCII），而且失败点离现场很远（启动时才报实例设置读不出来）。
#[test]
fn the_chinese_version_folder_from_the_field_report_shows_up() {
    let root = temp::Root::new("ull-host-game-union-cjk");
    let env = HostEnv::at_root(&root.0);
    let name = "樱落怡然客户端";
    let path = env.game.version_json(name);
    std::fs::create_dir_all(path.parent().expect("版本目录")).expect("版本目录");
    std::fs::write(&path, format!(r#"{{"id":"{name}","type":"release"}}"#)).expect("写版本 JSON");

    let snapshot = instances(&env);
    assert_eq!(snapshot.len(), 1, "用户现场应当看到 1 个实例：{snapshot:?}");
    let tile = &snapshot[0];
    assert_eq!(tile.name, name, "显示名 = 文件夹名（不是哈希）");
    assert_eq!(tile.version_id, name);
    assert!(
        unlucky_core::platform::layout::is_valid_instance_id(&tile.id),
        "中文版本目录合成了一个非法实例 id：{}",
        tile.id
    );
    assert!(
        !tile.id.contains(name),
        "实例 id 不该是版本目录名本身（那份白名单只收 ASCII）：{}",
        tile.id
    );
}

// ------------------------------------------------------- 运行目录判据（M273 r1 · P2-1）
//
// **真值表**（自动发现的实例）：
//
// | `versions/<id>/` 里有 | 隔离档 | gameDir | 为什么 |
// |---|---|---|---|
// | `mods` / `saves` / `config` / `options.txt` 任一 | `isolated` | `versions/<id>` | 它本来就是「版本隔离」形态，别人的数据就放在里面 |
// | 一个都没有 | `shared` | 游戏根（`.minecraft`） | 从没用过隔离目录，写进 `versions/<id>` 等于往别人的版本目录里拉屎 |
//
// 两条用例都顺带断言**派生前后盘上零变化**：判据是只读的，写盘只在用户第一次改设置时发生。

/// `versions/A/` 里只有清单 ⇒ 按**共用目录**跑（gameDir = 游戏根）。
#[test]
fn external_instance_without_traces_runs_from_the_game_root() {
    let root = temp::Root::new("ull-host-game-union-shared");
    let env = HostEnv::at_root(&root.0);
    write_version(&env, "A");

    let before = temp::entries_under(&root.0);
    let snapshot = instances(&env);
    assert_eq!(snapshot.len(), 1);
    assert_eq!(
        snapshot[0].isolation, "shared",
        "没有 mods / saves / config / options.txt ⇒ 共用目录"
    );
    let dir = super::super::saves::run_dir(&env, &snapshot[0]).expect("派生运行目录");
    assert_eq!(
        dir,
        env.game.root(),
        "共用目录的运行目录应当就是 .minecraft 根"
    );
    assert_eq!(
        before,
        temp::entries_under(&root.0),
        "判定 + 派生运行目录不许在盘上多出任何东西"
    );
}

/// `versions/B/mods/` 存在 ⇒ 按**独立目录**跑（gameDir = `versions/B`）。
#[test]
fn external_instance_with_mods_runs_in_its_version_folder() {
    let root = temp::Root::new("ull-host-game-union-isolated");
    let env = HostEnv::at_root(&root.0);
    write_version(&env, "B");
    std::fs::create_dir_all(env.game.version_dir("B").join("mods")).expect("mods 目录");

    let before = temp::entries_under(&root.0);
    let snapshot = instances(&env);
    assert_eq!(snapshot.len(), 1);
    assert_eq!(
        snapshot[0].isolation, "isolated",
        "`versions/B/mods/` 是「这个版本自带数据」的痕迹 ⇒ 独立目录"
    );
    let dir = super::super::saves::run_dir(&env, &snapshot[0]).expect("派生运行目录");
    assert_eq!(
        dir,
        env.game.version_dir("B"),
        "独立目录的运行目录应当是它自己的版本文件夹"
    );
    assert_eq!(
        before,
        temp::entries_under(&root.0),
        "判定 + 派生运行目录不许在盘上多出任何东西"
    );
}
