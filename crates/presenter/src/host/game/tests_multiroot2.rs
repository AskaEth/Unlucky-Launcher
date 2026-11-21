//! M274 **r1 复审**补的判据（P1-1 / P1-2 / P2-3）。
//!
//! 上一轮（`tests_multiroot.rs`）钉的是「两个根里的同名版本是两条实例、各自的书架/存档/启动按自己
//! 那个根解析」。复审把「同一句话没走完」的两处揪了出来——**根身份有了，但两处「描述 / 前置要求」
//! 还在读配置根**——这一组就是那两处，加上「只读」那条升级成内容级快照。
//!
//! | 用例 | 抓得住什么 | 抓不住什么 |
//! |---|---|---|
//! | [`the_tile_describes_a_lazily_created_profile_from_its_own_root`] | 官方根实例被懒创建成档案之后，磁贴的 `version_present` / `isolation` / `mc_version` **逐项不变**（复审 P1-1 的三项同时错：显示与行为分叉） | 磁贴之外的显示（`element` 没有内省接口，见 `pages/instances/tests` 的口径） |
//! | [`pick_java_reads_the_requirement_from_the_instance_own_root`] | 启动挑 Java 时读的是**实例自己那个根**的 `javaVersion`（复审 P1-2：配置根读出来 8，实例是 21，实际选中 17） | 真的有 21 的 JDK 被选中（那要装 JDK；这里钉的是「要求」与回落，不是 `pick` 的最优选择） |
//! | [`nothing_is_written_into_the_official_root`] | 列实例 / 列存档 / 装配启动 / 懒创建档案之后，官方根的**文件内容、大小、mtime** 一个都没变（复审 P2-3：原来只比条目名） | natives 解压（那发生在 `Launcher::spawn`，用例不拉进程）；官方启动器同时在写的并发（TOCTOU） |

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

/// 官方根 + 配置根各自的布局。
fn official_layout(env: &HostEnv) -> GameLayout {
    GameLayout::new(official(env))
}

fn write_manifest(game: &GameLayout, id: &str, json: &str) {
    let path = game.version_json(id);
    std::fs::create_dir_all(path.parent().expect("版本目录")).expect("版本目录");
    std::fs::write(&path, json).expect("写版本 JSON");
}

// ---------------------------------------------------------------- 判据①（P1-1）：磁贴按自己那个根描述

/// 官方根里的实例被懒创建成档案之后，磁贴的三格必须**逐项不变**。
///
/// 复审 P1-1 的探针（夹具几乎一样）：`version_present` true→false、`isolation` isolated→shared、
/// `mc_version` `1.20.4`→`A`。三格同时错，而实际读盘（saves / mods / launch）走 `game_for` 是对的
/// ⇒ **显示与行为分叉**：用户以为存档在 ULL 便携根，实际在官方根。
///
/// 变异证明：把 `profile_summary` 的 `home.unwrap_or(&env.game)` 改回 `&env.game` 就红。
#[test]
fn the_tile_describes_a_lazily_created_profile_from_its_own_root() {
    let (_root, env) = two_roots("ull-host-multiroot2-tile");
    // 官方根：`A` 继承 `1.20.4`，且 `versions/A/mods/` 存在（版本隔离形态）。配置根里**没有** `A`。
    let official = official_layout(&env);
    write_manifest(&official, "1.20.4", r#"{"id":"1.20.4","type":"release"}"#);
    write_manifest(
        &official,
        "A",
        r#"{"id":"A","inheritsFrom":"1.20.4","type":"release"}"#,
    );
    std::fs::create_dir_all(official.version_dir("A").join("mods")).expect("mods 目录");

    // `1.20.4` 本身也是个可登记的版本 ⇒ 列表里会有它；这里只盯 `A`。
    let before = instances(&env);
    let before = before
        .iter()
        .find(|tile| tile.version_id == "A")
        .expect("`A` 在列表里")
        .clone();
    assert_eq!(before.mc_version, "1.20.4", "继承链要走到父版本");
    assert!(before.version_present, "清单在官方根里");
    assert_eq!(
        before.isolation, "isolated",
        "`versions/A/mods/` 是自带数据的痕迹"
    );

    // 用户第一次改这个实例（写启动参数 = 懒创建档案，**不**写隔离档）——正是「官方实例」的常态路径。
    let patch = crate::dto::LaunchArgsPatch {
        jvm_args: Some("-XX:+UseG1GC".to_string()),
        ..crate::dto::LaunchArgsPatch::default()
    };
    let mut host = super::super::CoreHost::headless(env.clone());
    host.apply_launch_args(&before.id, &patch)
        .expect("官方根实例写启动参数不该报「找不到实例」");

    let after = instances(&env);
    let after = after
        .iter()
        .find(|tile| tile.id == before.id)
        .expect("懒创建之后同一条实例还在");
    assert_eq!(
        instances(&env).len(),
        2,
        "懒创建之后不该多出第三条（`1.20.4` + `A` 就是全部）"
    );
    assert_eq!(
        after.version_present, before.version_present,
        "`version_present` 变了（磁贴会显示「文件缺失」）：{before:?} → {after:?}"
    );
    assert_eq!(
        after.isolation, before.isolation,
        "`isolation` 变了（启动提示会说错运行目录）：{before:?} → {after:?}"
    );
    assert_eq!(
        after.mc_version, before.mc_version,
        "`mc_version` 变了（继承链丢了）：{before:?} → {after:?}"
    );
    assert_eq!(
        after.launch.jvm_args, "-XX:+UseG1GC",
        "写回的那一项要读得到"
    );
}

// ---------------------------------------------------------------- 判据②（P1-2）：挑 Java 按自己那个根

/// 两个根里各一个**同名**版本，`javaVersion` 要求不同 ⇒ 各自按自己那个根的要求回落。
///
/// 复审 P1-2 的探针：官方根里的自定义名版本声明 `javaVersion.majorVersion = 21`，配置根读不到清单
/// ⇒ 退回静态表 ⇒ `minimum = 8` ⇒ 实际选中 17，把要 21 的 modpack 拉起来。
///
/// 用「显式路径探测失败」这条确定性回落（`probe` 拿不到版本 ⇒ `major = required.minimum`），
/// 所以不需要机器上真的装着 JDK 8/21：断言的是**要求**与回落，不是 `pick` 的最优选择。
#[test]
fn pick_java_reads_the_requirement_from_the_instance_own_root() {
    let (_root, env) = two_roots("ull-host-multiroot2-java");
    let official = official_layout(&env);
    write_manifest(
        &env.game,
        "A",
        r#"{"id":"A","javaVersion":{"majorVersion":8}}"#,
    );
    write_manifest(
        &official,
        "A",
        r#"{"id":"A","javaVersion":{"majorVersion":21}}"#,
    );

    let tiles = instances(&env);
    assert_eq!(tiles.len(), 2, "{tiles:?}");
    // 探测必然失败（路径不存在）⇒ 落回「版本要求」。
    let missing = Path::new("Z:/ull-no-such-java/bin/java.exe");
    let mut majors: Vec<u32> = tiles
        .iter()
        .map(|tile| {
            let game = game_for(&env, &tile.id);
            super::super::launch::pick_java(&env, &game, "A", Some(missing))
                .selection
                .major
        })
        .collect();
    majors.sort_unstable();
    assert_eq!(
        majors,
        vec![8, 21],
        "两个根里的同名版本要求不同，挑 Java 必须各读各自的清单"
    );

    // 变异证明：拿配置根去读官方根那个实例 ⇒ 只会得到 8（复审实测这条会导致 17 去跑要 21 的包）。
    let wrong = super::super::launch::pick_java(&env, &env.game, "A", Some(missing));
    assert_eq!(
        wrong.selection.major, 8,
        "按配置根读会挑错——这条断言钉住「错在哪」"
    );

    // **走真启动装配**（`build_request` → `pick_java`）：这条才抓得住「调用点传错根」。
    // 每个实例在自己的设置里钉一个探测必然失败的 java 路径 ⇒ 生效的 major 就是它自己那个根的要求。
    let mut majors: Vec<u32> = Vec::new();
    for tile in &tiles {
        crate::host::launch_args::apply(
            &env,
            &tile.id,
            &tile.version_id,
            &crate::dto::LaunchArgsPatch {
                java_path: Some(missing.display().to_string()),
                ..crate::dto::LaunchArgsPatch::default()
            },
        )
        .expect("钉 java 路径");
        let target = super::super::launch::LaunchTarget {
            id: tile.id.clone(),
            name: tile.name.clone(),
            version_id: tile.version_id.clone(),
            account: None,
        };
        let (request, _warnings) =
            super::super::launch::build_request(&env, &target).expect("装配应当成功");
        majors.push(request.java.major);
    }
    majors.sort_unstable();
    assert_eq!(
        majors,
        vec![8, 21],
        "启动装配读的必须是实例自己那个根的要求（调用点传 `env.game` 就会全是 8）"
    );
}

// ---------------------------------------------------------------- 判据③（P2-3）：只读发现升级成内容级

/// **内容级**快照：相对路径 + 字节数 + mtime(ms) + 内容哈希（fnv-1a）。
///
/// 为什么不是只比条目名（复审 P2-3）：只比名字抓得住「多出新文件」，抓不住「已有文件被覆盖」
/// ——natives 的 dll 被改写、mtime 被刷新这两种都漏。这里四样一起比：内容改一个字节、大小变、
/// mtime 变都会红。
fn content_snapshot(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let Ok(relative) = path.strip_prefix(dir) else {
                continue;
            };
            let name = relative.to_string_lossy().replace('\\', "/");
            if path.is_dir() {
                out.push(format!("{name}|dir"));
                stack.push(path);
                continue;
            }
            let meta = std::fs::metadata(&path).ok();
            let bytes = meta.as_ref().map_or(0, std::fs::Metadata::len);
            let mtime = meta
                .and_then(|meta| meta.modified().ok())
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_millis())
                .unwrap_or(0);
            let content = std::fs::read(&path).unwrap_or_default();
            let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
            for byte in content {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            out.push(format!("{name}|{bytes}|{mtime}|{hash:016x}"));
        }
    }
    out.sort();
    out
}

/// 列实例 / 列存档 / 装配启动 / 懒创建档案之后，官方根里**一个字节都不许变**，也不许凭空建档案。
///
/// 升级点（复审 P2-3）：原来只比条目名集合，现在比「相对路径 + 大小 + mtime + 内容哈希」，
/// 并且把「懒创建档案」这条写盘路径也带进来（那是本 mission 唯一会在用户动作后写盘的地方，
/// 而它必须写进 `.ULL`、不碰官方根）。
///
/// 抓不住：`Launcher::spawn` 解压 natives（用例不拉进程）；官方启动器同时在写的并发（TOCTOU）。
#[test]
fn nothing_is_written_into_the_official_root() {
    let (_root, env) = two_roots("ull-host-multiroot2-readonly");
    let official = official_layout(&env);
    write_manifest(&official, "1.20.4", r#"{"id":"1.20.4","type":"release"}"#);
    write_manifest(
        &official,
        "A",
        r#"{"id":"A","inheritsFrom":"1.20.4","mainClass":"net.minecraft.client.main.Main"}"#,
    );
    // 一个已经在的 natives 文件（正是「被覆写」这种错要抓的东西）。
    let natives = official.version_dir("A").join("natives");
    std::fs::create_dir_all(&natives).expect("natives 目录");
    std::fs::write(natives.join("lwjgl.dll"), b"original-content").expect("natives 落盘");
    let world = official.root().join("saves").join("world");
    std::fs::create_dir_all(&world).expect("世界目录");
    std::fs::write(world.join("level.dat"), b"fixture").expect("level.dat");

    let before = content_snapshot(official.root());

    // ① 列实例 ② 列存档（含截图目录扫） ③ 装配启动请求（natives 解压发生在 spawn，不在装配里）
    let snapshot = instances(&env);
    let tile = snapshot
        .iter()
        .find(|tile| tile.version_id == "A")
        .expect("`A` 在列表里");
    for tile in &snapshot {
        if let Ok(dir) = super::super::saves::run_dir(&env, tile) {
            let _ = super::super::saves::snapshot(tile, &dir, None);
        }
        let target = super::super::launch::LaunchTarget {
            id: tile.id.clone(),
            name: tile.name.clone(),
            version_id: tile.version_id.clone(),
            account: None,
        };
        let _ = super::super::launch::build_request(&env, &target);
    }
    // ④ 用户改一次设置 ⇒ 懒创建档案（写盘必须落 `.ULL`）
    set_isolation(&env, &tile.id, "isolated").expect("懒创建档案");
    let _ = instances(&env);

    assert_eq!(
        before,
        content_snapshot(official.root()),
        "官方根的条目 / 大小 / mtime / 内容有任何一格变了（发现与设置都只许读它）"
    );
    let profiles = read_profiles(&env.layout).expect("读档案");
    assert_eq!(profiles.profiles.len(), 1, "档案落在 .ULL 里");
    assert!(
        env.layout.profiles_config().starts_with(env.layout.home()),
        "档案路径必须在 .ULL 内：{}",
        env.layout.profiles_config().display()
    );
}
