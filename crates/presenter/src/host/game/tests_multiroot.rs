//! M274：**多根发现**（配置根 + 官方启动器的标准位置）的判据用例。
//!
//! 只读那条用例搬到了 `tests_multiroot2.rs` 并升级成内容级快照（M274 r1 复审 P2-3）；
//! 写设置那道门的两条方向用例在 `tests_multiroot3.rs`（M274 r2 复审 P1-1）。
//!
//! 用户原话：「其他第三方启动器甚至能自动读到官方启动器的版本和存档」。官方启动器用的是
//! `%APPDATA%\.minecraft`，便携模式的 ULL 用的是 `<exe>/.minecraft`——两个不同的目录。
//!
//! **本机没有装官方启动器**（`paths::standard_game_home()` 解析出的路径不存在），所以这一组的
//! 每个根都是**临时目录里造出来的夹具**：`<root>/.minecraft/versions/A/A.json`。这不是缺陷，
//! 是环境事实；「真的去读 `%APPDATA%`」那一步**未验**（见报告）。
//!
//! | 用例 | 抓得住什么 | 抓不住什么 |
//! |---|---|---|
//! | [`the_same_version_in_two_roots_is_two_instances`] | 两个根里的同名版本 = **2 条**磁贴、id 不同、各自指向自己的根、名字分辨得出来 | 真实官方安装的目录形状（`launcher_profiles.json` / 加密的账户文件一概不读） |
//! | [`a_junction_to_the_configured_root_is_not_a_second_root`] | 同一路径的第二种写法（junction / 符号链接）**不许**让同一批实例列两遍 | 8.3 短名（用例盘上造不出来；判据靠 `canonicalize` 的长名展开，见 `roots::key_of` 的文档） |
//! | [`saves_come_from_the_instance_own_root`] | 每个实例的存档 / 截图列表只来自**它自己那个根**（不许把外部实例的存档当成当前根的） | 游戏进程正在跑时的文件锁 / 半写状态（未处理，见报告） |
//! | [`a_lazily_created_profile_stays_on_its_own_root`] | 懒创建档案（用户第一次改设置）之后 id → 根的反查不退化，且没把配置根的同名版本一并抹掉 | 档案里**没有**根字段这一事实本身（根只从 id 反查，见 `discovered::Found`） |
//! | [`the_launch_reads_the_manifest_from_the_instance_root`] | 启动装配的清单 / 运行目录走实例自己的根 | 真的把游戏拉起来（要真 jvm / 账号 / 网络） |

use std::path::{Path, PathBuf};

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

/// 官方根（＝第二个候选根）的**字面**路径。
fn official(env: &HostEnv) -> PathBuf {
    env.standard_game.clone().expect("用例里一定钉了官方根")
}

fn write_version(game: &GameLayout, id: &str) {
    let path = game.version_json(id);
    std::fs::create_dir_all(path.parent().expect("版本目录")).expect("版本目录");
    std::fs::write(&path, format!(r#"{{"id":"{id}"}}"#)).expect("写版本 JSON");
}

/// 世界目录（`<根>/saves/<name>/level.dat`）。
fn write_world(root: &Path, name: &str) {
    let dir = root.join("saves").join(name);
    std::fs::create_dir_all(&dir).expect("世界目录");
    std::fs::write(dir.join("level.dat"), b"fixture").expect("level.dat");
}

// 目录链接夹具走 `host::game::roots::make_dir_link`——一份代码两处共用（M291）。别在这里再写一份
// `cmd /c mklink /J`：多一个 spawn 点就多一个绕过 `platform::child::quiet` 的机会。

// ------------------------------------------------------------------ 判据①：两个根各一条

/// 两个临时根各放一个同名实例（`versions/A/A.json`）⇒ 列表出 **2 条**，各自指向自己的根。
#[test]
fn the_same_version_in_two_roots_is_two_instances() {
    let (_root, env) = two_roots("ull-host-multiroot-two");
    write_version(&env.game, "A");
    write_version(&GameLayout::new(official(&env)), "A");

    let snapshot = instances(&env);
    assert_eq!(
        snapshot.len(),
        2,
        "两个根里的同名版本必须是 2 条：{snapshot:?}"
    );
    let ids: std::collections::BTreeSet<&str> =
        snapshot.iter().map(|tile| tile.id.as_str()).collect();
    assert_eq!(
        ids.len(),
        2,
        "两个根的 id 撞了（哈希没喂根？）：{snapshot:?}"
    );

    for tile in &snapshot {
        assert_eq!(tile.version_id, "A");
        assert!(
            unlucky_core::platform::layout::is_valid_instance_id(&tile.id),
            "合成的实例 id 不合法：{}",
            tile.id
        );
        let game = game_for(&env, &tile.id);
        assert!(
            game.root() == env.game.root() || game.root() == official(&env),
            "实例 {} 的根既不是配置根也不是官方根：{}",
            tile.id,
            game.root().display()
        );
    }

    // 两条磁贴的**名字**必须分辨得出来（否则用户不知道点哪一条，而两条的存档在不同目录里）。
    assert_ne!(
        snapshot[0].name, snapshot[1].name,
        "两条磁贴同名，用户无从分辨：{snapshot:?}"
    );
    assert!(
        snapshot.iter().any(|tile| tile.name == "A"),
        "配置根里的那条仍叫版本目录名（绝大多数用户看到的就是它）：{snapshot:?}"
    );
    let other = snapshot
        .iter()
        .find(|tile| tile.name != "A")
        .expect("另一条存在");
    assert!(
        other.name.contains("official"),
        "另一条的名字得带上它的根（现在是 {}）",
        other.name
    );

    // 同一个 (根, 版本) 反查稳定：换一次快照，id 不变（懒创建幂等的前提）。
    let again = instances(&env);
    assert_eq!(
        again.iter().map(|tile| tile.id.clone()).collect::<Vec<_>>(),
        snapshot
            .iter()
            .map(|tile| tile.id.clone())
            .collect::<Vec<_>>(),
        "同一批盘上内容两次快照给了不同 id（跨重启就会长出第二条档案）"
    );
}

/// **存在但是空的**官方根（本机的真实形状）⇒ 一条实例都不许多，也不许报错。
///
/// 这条不是假想：开发机上 `%APPDATA%\.minecraft` **目录是在的**，里面只有一个空的 `cache/`
/// （没有 `versions/`、没有 `launcher_profiles.json`）。判据必须把「有个空目录」与「装了官方启动器」
/// 分开——否则用户会看到一条来路不明的实例（或者实例页整页报错）。
#[test]
fn an_empty_official_root_adds_no_instances() {
    let (_root, env) = two_roots("ull-host-multiroot-empty");
    write_version(&env.game, "A");
    std::fs::create_dir_all(official(&env).join("cache")).expect("cache 目录");

    let snapshot = instances(&env);
    assert_eq!(snapshot.len(), 1, "空的官方根不该长出实例：{snapshot:?}");
    assert_eq!(snapshot[0].version_id, "A");
    assert_eq!(
        game_for(&env, &snapshot[0].id).root(),
        env.game.root(),
        "唯一那条属于配置根"
    );
}

// ------------------------------------------------------- 判据②：同一路径的两种写法只算一个根

/// 让两个路径指向同一个目录（junction / 符号链接）⇒ 列表只能出 **1 条**。
#[test]
fn a_junction_to_the_configured_root_is_not_a_second_root() {
    let root = temp::Root::new("ull-host-multiroot-dedupe");
    let mut env = HostEnv::at_root(&root.0);
    write_version(&env.game, "A");
    let link = root.0.join("linked");
    if !super::roots::make_dir_link(&link, env.game.root()) {
        // 造不出链接就**不假绿**：这一条判据在造不出链接的盘上等于没验。
        eprintln!("跳过：这个环境造不出目录链接（{}）", link.display());
        return;
    }
    env.standard_game = Some(link);
    assert_eq!(
        super::roots::all(&env).len(),
        1,
        "联接与配置根是同一个物理目录，不该算两个根"
    );
    let snapshot = instances(&env);
    assert_eq!(
        snapshot.len(),
        1,
        "同一批实例列了两遍（去重没落到真实路径上）：{snapshot:?}"
    );
}

// -------------------------------------------------------- 判据③：存档按各自的根解析

/// 两个根各一个同名实例 + 各一个世界 ⇒ 每条实例的存档列表只来自**它自己那个根**。
#[test]
fn saves_come_from_the_instance_own_root() {
    let (_root, env) = two_roots("ull-host-multiroot-saves");
    write_version(&env.game, "A");
    write_version(&GameLayout::new(official(&env)), "A");
    write_world(env.game.root(), "configured-world");
    write_world(&official(&env), "official-world");

    let snapshot = instances(&env);
    assert_eq!(snapshot.len(), 2, "{snapshot:?}");
    let mut seen: Vec<PathBuf> = Vec::new();
    for tile in &snapshot {
        let dir = super::super::saves::run_dir(&env, tile).expect("派生运行目录");
        let view = super::super::saves::snapshot(tile, &dir, None).expect("列存档");
        let worlds: Vec<&str> = view.worlds.iter().map(|world| world.dir.as_str()).collect();
        if dir == env.game.root() {
            assert_eq!(
                worlds,
                vec!["configured-world"],
                "配置根那条读串了：{view:?}"
            );
        } else {
            assert_eq!(dir, official(&env), "外部实例的运行目录应当是它自己的根");
            assert_eq!(worlds, vec!["official-world"], "官方根那条读串了：{view:?}");
        }
        seen.push(dir);
    }
    assert_ne!(
        seen[0], seen[1],
        "两个根的实例派生出同一个运行目录（存档会互相覆盖）"
    );
}

// ------------------------------------------- 判据⑤：懒创建的档案仍认自己那个根（不吞同名）

/// 用户第一次给官方根里的实例改设置 ⇒ 懒创建档案，但：
/// 1. id → 根的反查**不退化**（存档 / 模组 / 启动还读官方根）；
/// 2. 配置根里那个同名版本**还在**（它是另一个实例，档案不该连它一起吞掉）。
#[test]
fn a_lazily_created_profile_stays_on_its_own_root() {
    let (_root, env) = two_roots("ull-host-multiroot-lazy");
    write_version(&env.game, "A");
    write_version(&GameLayout::new(official(&env)), "A");
    // 官方根里那个版本带 `mods/`（「版本隔离」形态）⇒ 基底档是 isolated，换根之后不许变。
    std::fs::create_dir_all(
        GameLayout::new(official(&env))
            .version_dir("A")
            .join("mods"),
    )
    .expect("mods 目录");

    let snapshot = instances(&env);
    let tile = snapshot
        .iter()
        .find(|tile| tile.name.contains("official"))
        .expect("官方根那条在列表里")
        .clone();
    let game = game_for(&env, &tile.id);
    assert_eq!(game.root(), official(&env), "反查没落到官方根上");
    assert_eq!(
        isolation_of(&env, &game, &tile.id, "A"),
        "isolated",
        "官方根里 `versions/A/mods/` 是「自带数据」的痕迹 ⇒ 独立目录"
    );

    set_isolation(&env, &tile.id, "shared").expect("第一次改档位 = 懒创建档案");
    assert_eq!(
        game_for(&env, &tile.id).root(),
        official(&env),
        "懒创建之后反查退回了配置根（存档会读串）"
    );

    let after = instances(&env);
    assert_eq!(
        after.len(),
        2,
        "懒创建之后配置根那个同名版本被档案吞掉了：{after:?}"
    );
    assert_eq!(
        after
            .iter()
            .filter(|tile| tile.id == tile_id(&snapshot, "official"))
            .count(),
        1,
        "懒创建之后官方根那条被列了两遍（档案 + 合成视图）"
    );
    // 档案里的 version_id 只是版本名，**根只从 id 反查**——这条用例钉住的就是这一点。
    let profiles = read_profiles(&env.layout).expect("读档案");
    assert_eq!(profiles.profiles.len(), 1);
    assert_eq!(profiles.profiles[0].version_id, "A");
}

/// 列表里「名字带 `needle` 的那一条」的 id（懒创建前后的同一个实例）。
fn tile_id(snapshot: &[crate::dto::InstanceSummary], needle: &str) -> String {
    snapshot
        .iter()
        .find(|tile| tile.name.contains(needle))
        .expect("那一条在列表里")
        .id
        .clone()
}

// ------------------------------------------------ 判据⑥：启动装配按实例自己的根读清单

/// 清单**只在官方根里** ⇒ 装配仍要成功，且 `gameDir` 是官方根（共用目录档）。
#[test]
fn the_launch_reads_the_manifest_from_the_instance_root() {
    let (_root, env) = two_roots("ull-host-multiroot-launch");
    std::fs::create_dir_all(env.layout.config()).expect("config 目录");
    write_version(&GameLayout::new(official(&env)), "A");

    let snapshot = instances(&env);
    assert_eq!(snapshot.len(), 1, "只有官方根里有版本：{snapshot:?}");
    let tile = &snapshot[0];
    let target = super::super::launch::LaunchTarget {
        id: tile.id.clone(),
        name: tile.name.clone(),
        version_id: "A".to_string(),
        account: None,
    };
    let (request, _warnings) =
        super::super::launch::build_request(&env, &target).expect("清单在官方根里，装配应当成功");
    assert_eq!(
        request.dirs.game_dir,
        official(&env),
        "共用目录档的运行目录应当是**它自己那个根**（不是 ULL 便携根）"
    );
    assert_eq!(request.instance.id, tile.id);
}
