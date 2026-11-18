//! M274 **r2 复审 P1-1**：写设置那道门（`ensure_profile` → `external_version_of`）必须按
//! **(根, 版本)** 判「这条档案是否占住配置根的同名版本」，不许只比 `version_id`。
//!
//! 现场（复审探针，夹具就是 mission 判据⑤那份「两根各一个同名 `A`」）：
//!
//! ```text
//! 官方那条 set_isolation => Ok(())
//! 配置根那条 set_isolation => Err("找不到实例 `ext-cf5883f9d893ad24`")   ← 而磁贴还好好列着
//! ```
//!
//! 方向还是不对称的（先改配置根那条再改官方那条是通的）——同一台机器上「有时能改有时不能改」，
//! 最难排的那种。根因是列表侧（`instances()` 的 `bound`）按根判、写侧（`external_version_of`）
//! 只比 `version_id`；现在两处共用 [`super::discovered::binds_configured_root`]。
//!
//! | 用例 | 抓得住什么 | 抓不住什么 |
//! |---|---|---|
//! | [`both_same_named_instances_take_settings_in_either_order`] | 两个根的**同名**实例都改得动设置，两个方向都行，且写进去的值读得回来 | 图形界面点按钮那一路（`Element` 无内省；这里走的是同一条 host 函数） |

use std::path::PathBuf;

use unlucky_core::game::install::instance::{read_instance_config, Isolation};
use unlucky_core::game::GameLayout;

use super::*;

/// 临时根（每个用例一个目录；断言 panic 时也会删）。
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
}

/// 造两个根：配置根 = `<root>/.minecraft`，官方位置 = `<root>/official/.minecraft`。
fn two_roots(tag: &str) -> (temp::Root, HostEnv) {
    let root = temp::Root::new(tag);
    let mut env = HostEnv::at_root(&root.0);
    let official = root.0.join("official").join(".minecraft");
    std::fs::create_dir_all(&official).expect("官方根目录");
    env.standard_game = Some(official);
    (root, env)
}

fn official(env: &HostEnv) -> PathBuf {
    env.standard_game.clone().expect("用例里一定钉了官方根")
}

fn write_version(game: &GameLayout, id: &str) {
    let path = game.version_json(id);
    std::fs::create_dir_all(path.parent().expect("版本目录")).expect("版本目录");
    std::fs::write(&path, format!(r#"{{"id":"{id}"}}"#)).expect("写版本 JSON");
}

/// 列表里满足 `hit` 的那一条的 id（两条磁贴都含 `A`，所以判据写在调用处）。
fn tile_id(env: &HostEnv, hit: impl Fn(&crate::dto::InstanceSummary) -> bool) -> String {
    instances(env)
        .into_iter()
        .find(|tile| hit(tile))
        .unwrap_or_else(|| panic!("列表里没有那一条：{:?}", instances(env)))
        .id
}

/// 两个根的**同名**实例都改得动设置：正序、反序各跑一遍，每条都断言「写进去读得回来」。
///
/// 变异证明（复审给的形状）：把 `external_version_of` 的 `bound` 改回
/// 「只比 `version_id`」⇒ 正序那条（先官方、后配置根）立刻红，报
/// `找不到实例 ext-…`，而两个方向的第二条都还是绿——这正是它被抓到的现场形状。
#[test]
fn both_same_named_instances_take_settings_in_either_order() {
    for official_first in [true, false] {
        let (_root, env) = two_roots("ull-host-multiroot3-order");
        write_version(&env.game, "A");
        write_version(&GameLayout::new(official(&env)), "A");

        // 官方根那条的名字带根路径；配置根那条保持版本目录名（见 `discovered::display_name`）。
        let official_id = tile_id(&env, |tile| tile.name.contains("official"));
        let configured_id = tile_id(&env, |tile| tile.name == "A");
        assert_ne!(official_id, configured_id, "两个根的同名版本必须是两条实例");

        let order: [(&str, &str); 2] = if official_first {
            [(&official_id, "official"), (&configured_id, "configured")]
        } else {
            [(&configured_id, "configured"), (&official_id, "official")]
        };

        for (index, (id, which)) in order.iter().enumerate() {
            // 两条生产入口铺开用：一条走 `set_isolation`，另一条走 `apply_launch_args`。
            if index == 0 {
                set_isolation(&env, id, "isolated").expect("改隔离档不该报「找不到实例」");
                let config = read_instance_config(&env.layout, id)
                    .expect("读实例设置")
                    .expect("实例设置文件应当已写盘");
                assert_eq!(
                    config.isolation,
                    Some(Isolation::Isolated),
                    "{which} 那条写进去的隔离档读不回来"
                );
            } else {
                let patch = crate::dto::LaunchArgsPatch {
                    jvm_args: Some("-XX:+UseG1GC".to_string()),
                    ..crate::dto::LaunchArgsPatch::default()
                };
                let mut host = super::super::CoreHost::headless(env.clone());
                host.apply_launch_args(id, &patch)
                    .expect("写启动参数不该报「找不到实例」");
                let config = read_instance_config(&env.layout, id)
                    .expect("读实例设置")
                    .expect("实例设置文件应当已写盘");
                assert_eq!(
                    config.launch.jvm_args.as_deref(),
                    Some("-XX:+UseG1GC"),
                    "{which} 那条写进去的启动参数读不回来"
                );
            }
            // 写完一次之后，另一条**还在列表里**、而且还能继续改（不许被吞掉）。
            let snapshot = instances(&env);
            assert_eq!(
                snapshot.len(),
                2,
                "写了 {which} 那条之后列表变了：{snapshot:?}"
            );
            assert!(
                snapshot.iter().any(|tile| tile.id == official_id),
                "官方根那条消失了：{snapshot:?}"
            );
            assert!(
                snapshot.iter().any(|tile| tile.id == configured_id),
                "配置根那条消失了：{snapshot:?}"
            );
        }

        // 两条都写过之后：id → 根的反查不许漂（写盘与读盘同一份身份）。
        assert_eq!(
            game_for(&env, &official_id).root(),
            official(&env),
            "官方根那条写完设置之后反查退回了配置根"
        );
        assert_eq!(
            game_for(&env, &configured_id).root(),
            env.game.root(),
            "配置根那条反查错了根"
        );
        let profiles = read_profiles(&env.layout).expect("读档案");
        assert_eq!(profiles.profiles.len(), 2, "两条实例各一份档案，不多不少");
    }
}

// ------------------------------------------------- 反查回落（M274 r3 复审 P2-1 / ②）

/// M273 时代的 `ext-` id：**只哈希版本目录名**（不含根）。M274 之后这种 id 永远反查不到。
fn m273_external_id(version_id: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in version_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("ext-{hash:016x}")
}

/// 在**我们自己的日志目录**里数这个实例的回落行（重试到「登记窗口没被并行用例抢走」为止）。
///
/// `launcher_log` 的目录是**进程级全局**（`set_dir` 以最后一次为准），而 presenter 的用例并行跑、
/// 七十多处 `HostEnv::at_root` 都会重新登记。所以这里每次尝试换一个**新目录**：抢到了就能在
/// 一个全新文件里看到**恰好一行**；没抢到（文件不存在）就再试。抢不到 64 次视为用例失败（不当跳过）。
fn fallback_lines(env: &HostEnv, id: &str) -> Vec<String> {
    let file = unlucky_core::launch::launcher_log::FILE_NAME;
    let mut saw_file = false;
    for attempt in 0..64 {
        let dir = std::env::temp_dir().join(format!(
            "ull-host-multiroot3-log-{}-{attempt}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        unlucky_core::launch::launcher_log::set_dir(&dir);
        let _ = game_for(env, id);
        let Ok(text) = std::fs::read_to_string(dir.join(file)) else {
            // 文件没建出来：这一拍的登记窗口被并行用例抢走了（它们也会 `set_dir`），再试。
            continue;
        };
        let lines: Vec<String> = text
            .lines()
            .filter(|line| line.contains(id))
            .map(str::to_string)
            .collect();
        std::fs::remove_dir_all(&dir).ok();
        if !lines.is_empty() {
            return lines;
        }
        // 文件在、却没有我们的行：可能是别人的行占了这个文件，也可能这条路径**根本没写**。
        // 再试几次，全都不中就当「0 行」交给断言去红（比在这里 panic 好读）。
        saw_file = true;
    }
    if saw_file {
        return Vec::new();
    }
    panic!("64 次都没看到日志文件：回落日志没写，或登记窗口每次都被并行用例抢走");
}

/// **老 `ext-` 档案**（M273 形状）反查不到 ⇒ 落配置根，并且日志里**恰好一行** `[multiroot]`，
/// 文案**两因并列**（老档案 ‖ 此刻不可登记）。
///
/// 这条路径此前零覆盖（M274 r3 复审指出）：它是所有 M273 老用户升级后的**常态**，不是故障；
/// 文案只写「清单半写/被锁/目录已不在」会让每次点开详情页都往日志里加一条假警报。
///
/// 变异证明：把文案只留「此刻不可登记」那半句 ⇒ 下面的关键词断言红；把回落日志整段删掉 ⇒
/// 「恰好一行」红（0 行）。
#[test]
fn an_old_m273_instance_falls_back_with_one_two_cause_log_line() {
    let (_root, env) = two_roots("ull-host-multiroot3-fallback");
    write_version(&env.game, "A");
    let old_id = m273_external_id("A");
    assert!(
        crate::dto::is_discovered_instance(&old_id),
        "老 id 也带 `ext-` 前缀（这正是「永远反查不到」的来源）"
    );

    assert_eq!(
        game_for(&env, &old_id).root(),
        env.game.root(),
        "老档案反查不到 ⇒ 只能按配置根解析"
    );
    let lines = fallback_lines(&env, &old_id);
    assert_eq!(
        lines.len(),
        1,
        "一次回落只该写一行（多了会把日志刷成噪声）：{lines:?}"
    );
    let line = &lines[0];
    assert!(line.contains("[multiroot]"), "分类名要可 grep：{line}");
    assert!(
        line.contains("老档案"),
        "M273 老 id 是**常态**，文案必须并列这一因，不能只说「清单半写/被锁」：{line}"
    );
    assert!(
        line.contains("不可登记"),
        "另一因（官方启动器正在更新）也要在：{line}"
    );
}

/// **已知边界**（不是设计意图）：官方根的版本目录消失后，那条死档案会「吞掉」配置根的同名版本。
///
/// 现场（M274 r3 复审 ②）：两根各一个 `A`，官方那条先懒创建；官方根的 `versions/` 随后不见
/// （官方启动器升级 / 清理 / 卸载 / 换盘）。此时：
/// - 列表只剩 **1 条**，名字还写着 `A · …\official\.minecraft`（指向官方根）；
/// - 而 `version_present` / 读盘路径却全从**配置根**读出来（`game_for` 回落）；
/// - ⇒ 真正的配置根那条 `A` **从列表消失**（被 `bound` 判成「已被档案占住」）。
///
/// 这是 r1 判 P1 的那种「显示与行为分叉」的反向形态。**这轮不改结构**：真正的修法是给
/// `InstanceProfile` 加「根」字段（`profiles.json` schema + 迁移），代价超出本 mission。
/// 所以这条用例钉的是**现状**——将来做 schema 迁移时它会红，届时要跟着改成「两条都在」。
#[test]
fn a_dead_official_root_is_swallowed_by_its_stale_profile() {
    let (_root, env) = two_roots("ull-host-multiroot3-deadroot");
    write_version(&env.game, "A");
    write_version(&GameLayout::new(official(&env)), "A");

    let official_id = tile_id(&env, |tile| tile.name.contains("official"));
    set_isolation(&env, &official_id, "shared").expect("官方那条先懒创建档案");
    // 官方启动器把那个版本清理掉了（目录整个不见）。
    std::fs::remove_dir_all(GameLayout::new(official(&env)).versions_dir())
        .expect("删官方 versions");

    let snapshot = instances(&env);
    assert_eq!(
        snapshot.len(),
        1,
        "现状：只有档案那一条（配置根的同名版本被 bound 吞掉）：{snapshot:?}"
    );
    let tile = &snapshot[0];
    assert_eq!(tile.id, official_id);
    assert!(
        tile.name.contains("official"),
        "名字还指着**已经不存在的**官方根：{tile:?}"
    );
    assert!(
        tile.version_present && game_for(&env, &tile.id).root() == env.game.root(),
        "而读盘/显示已经悄悄落回配置根（显示与行为分叉的反向形态）：{tile:?}"
    );
    let dir = super::super::saves::run_dir(&env, tile).expect("派生运行目录");
    assert_eq!(
        dir,
        env.game.root(),
        "存档也落回配置根——这就是「死档案吞掉同名版本」的后果"
    );
}
