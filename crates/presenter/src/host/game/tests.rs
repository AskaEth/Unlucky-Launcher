use super::*;

/// 版本 id → DTO 加载器的接口面（解析在 core，这里只验翻译没走样）。
///
/// 抓得住：显示路径把某个种类翻错/翻塌（例如 Quilt 写成 Forge）——`dto_kind` 的委派与
/// `dto` 的 `from_core` 都落在这一条上；以及 core 解析端退回老口径（`fabric-loader-…` 才认、
/// 自家 `1.20.4-fabric-0.16.5` 认不出）——后两条正是 M122 的 bug 面。
/// 抓不住：core 解析规则本身（真值表在 `unlucky_core::game::loader` 的测试里，那边才是
/// 真相源）；也抓不住「版本号为空时不显示品牌名」那条（在 `LoaderInfo::badge` 与
/// app-iced 的 `loader_badge`，M120 已修，别在这里重复）。
#[test]
fn loader_parsing_covers_all_shapes() {
    let cases = [
        ("1.20.4", LoaderKind::Vanilla, ""),
        // 自家口径（生成端 `LoaderKind::version_id`）：`<mc>-<kind>-<loader>`
        ("1.20.4-fabric-0.16.5", LoaderKind::Fabric, "0.16.5"),
        ("1.20.4-quilt-0.27.0", LoaderKind::Quilt, "0.27.0"),
        ("1.20.1-forge-47.2.0", LoaderKind::Forge, "47.2.0"),
        ("1.21.1-neoforge-21.1.80", LoaderKind::NeoForge, "21.1.80"),
        // 老 HMCL 口径（存量实例还会带着它们）
        ("fabric-loader-0.16.5-1.20.1", LoaderKind::Fabric, "0.16.5"),
        ("quilt-loader-0.24.0-1.20.1", LoaderKind::Quilt, "0.24.0"),
        // 第三方口径
        ("1.13.2-OptiFine_HD_U_E6", LoaderKind::OptiFine, "HD_U_E6"),
        ("1.12.2-LiteLoader", LoaderKind::LiteLoader, ""),
    ];
    for (id, kind, version) in cases {
        let info = loader_from_id(id);
        assert_eq!(info.kind, kind, "{id}");
        assert_eq!(info.version, version, "{id}");
        assert!(!info.badge().is_empty());
    }
}

/// 显示路径的**全量**锚：core 的每一种加载器，从自家版本 id 走到磁贴时仍然是**它自己**。
///
/// 清单取自 [`unlucky_core::game::LoaderKind::ALL`]，**不手抄**：M126 r3 复审实测出的缺口正是
/// 「手抄的清单跟着漏」——core 加第八家、契约与 help 都补齐、只在 `dto_kind` 那一支写
/// `=> Vanilla`，三个 crate 全绿而磁贴静默显示「原版」。`ALL` 由 core 的宏表长出来，加一家
/// 自动进圈，测试作者不需要（也没法）记得补一行。
///
/// 抓得住：显示路径任何一环把种类翻塌成原版——`dto_kind` 又变回手写映射、`dto` 的
/// `from_core` 那一支接错、`label()` 被写成「原版」；**包括新加的那种**（那时 `ALL` 先变长）。
/// 抓不住：core 的 id 解析规则本身（真值表在 `unlucky_core::game::loader`）；版本号为空的
/// 徽标判据（M120 的 `loader_badge`，本 crate 不渲染）。
#[test]
fn every_core_loader_kind_reaches_the_tile_as_itself() {
    use unlucky_core::game::LoaderKind as Core;
    for kind in Core::ALL {
        let info = loader_from_id(&format!("1.20.4-{}-0.16.5", kind.as_str()));
        assert_eq!(
            info.kind.as_str(),
            kind.as_str(),
            "core 的 {kind:?} 走到磁贴时变成了 `{}`（徽标 `{}`）",
            info.kind.as_str(),
            info.badge()
        );
        assert_eq!(
            info.badge().starts_with(LoaderKind::Vanilla.label()),
            kind == Core::Vanilla,
            "core 的 {kind:?} 在磁贴上的徽标是 `{}`",
            info.badge()
        );
    }
}

/// 端到端（数据层）：真装出来的四个加载器实例，走 [`instances`] 的快照各自带对加载器。
///
/// 造的是本启动器**真实产物**的形状：`profiles.json` 里 `versionId` 是自家 id，
/// `versions/<id>/<id>.json` 里 `inheritsFrom` 指向原版层（生成端见
/// `unlucky_core::addon::loader`）。原版版本号由此取自继承链根，不是从 id 里切的。
///
/// 抓得住：`describe_version` ↔ `loader_from_id` ↔ `instances` 这条链上任何一环把加载器
/// 丢掉（磁贴会显示「原版」——M122 的用户可见症状）；以及继承链根取错（磁贴的版本号）。
/// 抓不住：实例页真出图（走 app-iced 的视图模型与 `loader_badge`，本 crate 不渲染）；
/// 也抓不住「真下载安装了 Fabric/Quilt/Forge/NeoForge」——这里只伪造落盘结果。
#[test]
fn own_installed_instances_snapshot_carries_their_loader() {
    let dir = std::env::temp_dir().join(format!("ull-host-game-own-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = HostEnv::at_root(&dir);
    std::fs::create_dir_all(env.layout.config()).expect("config 目录");

    let cases = [
        (
            "1.20.4-fabric-0.16.5",
            "1.20.4",
            LoaderKind::Fabric,
            "Fabric 0.16.5",
        ),
        (
            "1.20.4-quilt-0.27.0",
            "1.20.4",
            LoaderKind::Quilt,
            "Quilt 0.27.0",
        ),
        (
            "1.20.1-forge-47.2.0",
            "1.20.1",
            LoaderKind::Forge,
            "Forge 47.2.0",
        ),
        (
            "1.21.1-neoforge-21.1.80",
            "1.21.1",
            LoaderKind::NeoForge,
            "NeoForge 21.1.80",
        ),
    ];
    for (version_id, mc_version, _, _) in cases {
        for (id, inherits) in [(mc_version, None), (version_id, Some(mc_version))] {
            let json = match inherits {
                Some(parent) => format!(r#"{{"id":"{id}","inheritsFrom":"{parent}"}}"#),
                None => format!(r#"{{"id":"{id}"}}"#),
            };
            let path = env.game.version_json(id);
            std::fs::create_dir_all(path.parent().expect("版本目录")).expect("版本目录");
            std::fs::write(&path, json).expect("写版本 JSON");
        }
    }

    let profiles = ProfileList {
        profiles: cases
            .iter()
            .map(
                |(version_id, _, _, _)| unlucky_core::game::InstanceProfile {
                    id: unlucky_core::game::ProfileId::generate(),
                    name: format!("装好的 {version_id}"),
                    version_id: (*version_id).to_string(),
                    game_dir: None,
                    running_dir: None,
                    java_path: None,
                    created_at: None,
                    last_played_at: None,
                    extra: BTreeMap::new(),
                },
            )
            .collect(),
        ..ProfileList::default()
    };
    unlucky_core::game::install::instance::write_profiles(&env.layout, &profiles).expect("写清单");

    let snapshot = instances(&env);
    // M273 之后 `versions/*` 也是实例来源：这个夹具为了造 `inheritsFrom` 还写了三个父版本目录
    // （`1.20.4` / `1.20.1` / `1.21.1`）而**没有**给它们建档案，于是它们会作为「外部实例」出现。
    // 这条用例守的是「这条链把加载器/版本号翻对」——所以只看它自己那四个版本，条数也在这一层断言；
    // 「盘上有版本就出现在列表里」由 `tests_union.rs` 独占。
    let snapshot: Vec<_> = snapshot
        .into_iter()
        .filter(|tile| cases.iter().any(|case| case.0 == tile.version_id))
        .collect();
    assert_eq!(snapshot.len(), cases.len());
    for ((version_id, mc_version, kind, badge), tile) in cases.iter().zip(&snapshot) {
        assert_eq!(tile.version_id.as_str(), *version_id);
        assert_eq!(
            tile.mc_version.as_str(),
            *mc_version,
            "{version_id} 的版本号取错"
        );
        assert_eq!(tile.loader.kind, *kind, "{version_id} 的加载器");
        assert_eq!(tile.loader.badge().as_str(), *badge, "{version_id} 的徽标");
        assert!(tile.version_present, "{version_id} 的载荷应当存在");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn isolation_write_then_read_round_trips() {
    let dir = std::env::temp_dir().join(format!("ull-host-game-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = HostEnv::at_root(&dir);
    std::fs::create_dir_all(env.layout.config()).expect("config 目录");

    // 直接建一份 profiles.json（register 需要版本本体，这里只测隔离写读）
    let profiles = ProfileList {
        profiles: vec![unlucky_core::game::InstanceProfile {
            id: unlucky_core::game::ProfileId::generate(),
            name: "demo".to_string(),
            version_id: "1.20.1".to_string(),
            game_dir: None,
            running_dir: None,
            java_path: None,
            created_at: None,
            last_played_at: None,
            extra: BTreeMap::new(),
        }],
        ..ProfileList::default()
    };
    unlucky_core::game::install::instance::write_profiles(&env.layout, &profiles).expect("写清单");
    let id = profiles.profiles[0].id.as_str().to_string();

    // 有档案的实例：基底档 = 全局档（`isolated`），与 M273 之前一字不差。
    assert_eq!(isolation_of(&env, &env.game, &id, "1.20.1"), "isolated");
    set_isolation(&env, &id, "shared").expect("切档位");
    assert_eq!(isolation_of(&env, &env.game, &id, "1.20.1"), "shared");
    assert!(set_isolation(&env, &id, "nope").is_err());
    std::fs::remove_dir_all(&dir).ok();
}

/// **实例 id 的哈希公式钉死**（M304 r1：把「老档案放行」那条判据挂在这两个函数上，公式一漂就会
/// 误杀所有存量实例）。
///
/// 两个字面量是 FNV-1a 64 的独立复算结果（`ext-` 前缀 + 16 位小写十六进制）：
///
/// | 输入 | 期望 |
/// |---|---|
/// | `legacy_external_instance_id("1.20.4")` | `ext-52d6191436663b4e`（= M273 的「只哈希版本目录名」）|
/// | `external_instance_id("c:/mc/.minecraft", "1.20.4")` | `ext-1e6c24c50b4a95c2`（= root + `0` + version）|
///
/// 抓得住：漏掉段间那次乘法、或改动迭代顺序——那时**所有**存量 `ext-` id 都会换一个值
/// （`profiles.json` 里旧的 `ext-` 档案一律反查不到）。段间那个分隔符**本身**不单独进哈希：它
/// 等价于多乘一次素数（`hash ^= 0` 是恒等），痕迹只在这一次乘法上——所以「分隔符」与「那一次
/// 乘法」是同一件事，不是两处可分别改动的输入。M304 把哈希抽成 [`super::discovered`] 里的一个
/// 多段函数，这条就是那次重构的回归网。
#[test]
fn instance_id_hashes_are_the_released_ones() {
    assert_eq!(
        super::discovered::legacy_external_instance_id("1.20.4"),
        "ext-52d6191436663b4e",
        "M273 老档案的 id 公式变了（存量实例会被判成「根不见了」）"
    );
    assert_eq!(
        super::discovered::external_instance_id("c:/mc/.minecraft", "1.20.4"),
        "ext-1e6c24c50b4a95c2",
        "M274 的 id 公式变了（存量 ext- 档案一律反查不到）"
    );
}

/// 一条 `ext-` 磁贴，版本本体**不在盘上**（这个临时根里没有 `versions/`）。
///
/// 走生产的构造函数（[`super::discovered::external_summary`]，id = M274 的 `fnv(根 ‖ 0 ‖ 版本)`），
/// 调用方只把 id 换成要判的那一种写法——这样「磁贴的形状」与界面上的永远同源。`roots[0]` 恒为
/// 配置根（[`super::roots::all`] 的不变式：配置根永远第一位）。
fn ext_tile(env: &HostEnv, version_id: &str) -> InstanceSummary {
    let roots = super::roots::all(env);
    super::discovered::external_summary(env, &roots[0], version_id, &BTreeMap::new())
}

/// 建一个只有 `config/` 的临时根（用例自己的，`pid` + 标签让并行的两条不撞）。
fn bare_root(label: &str) -> (HostEnv, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ull-host-game-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = HostEnv::at_root(&dir);
    std::fs::create_dir_all(env.layout.config()).expect("config 目录");
    (env, dir)
}

/// **M273 老档案必须放行**（M306：给 `refresh/live.rs::discovered` 最后那三行挂网）。
///
/// 判据：`ext-` id 只哈希版本目录名（M274 之前的公式）⇒ `roots::all` 的每个根都反查不到它。
/// 这种 id 的语义本来就是「按配置根解析」，真没版本时启动链给具名阶段失败（进程一个都没起来）
/// ——所以这里必须 `Ok(())`。拒绝它 = 所有存量 M273 用户的 `ext-` 磁贴集体点不动，还配一句错的
/// 原因（「所属的游戏目录已不在」）。
///
/// 抓得住：**只**删掉 `live.rs::discovered` 里
/// `if legacy_external_instance_id(&instance.version_id) == instance.id { return Ok(()); }` 这三行
/// ——本条立即红（M306 实测：`Err("实例 `1.20.4` 所属的游戏目录已不在…")`）。
///
/// 但**别把编译器当第二道网**（M306 r1 复审实测，本文件早先那句「编译器一声不吭」是错的）：
/// `#![allow(dead_code)]` 只盖「函数变死码」，盖不住「use 行变空」——`unused_imports` 属于
/// `unused` 组里另一条 lint，删掉那三行时它会点名 `legacy_external_instance_id`，CI 的
/// `-D warnings` 当场红。可**这道保险也能绕过**：连 import 一起删、或在那行加
/// `#[allow(unused_imports)]`，编译器就真的不吭声了（那时 `cargo test` 依旧全绿）。
/// 所以源码侧由下面的 `the_legacy_pass_through_is_still_in_the_source` 兜底。
/// 抓不住：反查落空后「该拒绝」那一侧（下一条用例管）。
#[test]
fn a_m273_legacy_ext_id_is_allowed() {
    let (env, dir) = bare_root("legacy-ok");
    let mut tile = ext_tile(&env, "1.20.4");
    tile.id = super::discovered::legacy_external_instance_id("1.20.4");

    // 先清临时根再断言：断言失败也不在 /tmp 留空壳。
    let verdict = Scan::revalidate(&env, &tile);
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        verdict.is_ok(),
        "M273 老档案（id 只哈希版本目录名）必须放行，否则存量 `ext-` 磁贴全部点不动：{verdict:?}"
    );
}

/// **换个根的 `ext-` id、反查落空，必须拒绝并具名**（同一条分叉的另一侧）。
///
/// 与上一条**同一个版本**，只把 id 换成 `external_instance_id(别的根, 版本)`：盘上根本没有那个根
/// ⇒ 一个根都对不上，而它也不是老档案 ⇒ 必须 `Err`。放行会让 `game_for` 退回配置根，配置根里
/// 恰好有同名版本就把**另一份**拉起来（gameDir / 存档都在另一边）。
///
/// 抓得住：把那条拒绝改写成 `Ok(())`（复审现场的另一半）。它与上一条各钉分叉的一侧，所以
/// 只删老档案那三行时本条**不会**假红——红色只会出现在上一条。
#[test]
fn an_ext_id_from_no_visible_root_is_refused() {
    let (env, dir) = bare_root("legacy-refused");
    let mut tile = ext_tile(&env, "1.20.4");
    // 盘上不存在的根：`roots::all` 只给配置根 ⇒ 这个 id 一个根都对不上，且它不是老档案的公式。
    tile.id = super::discovered::external_instance_id("d:/m306-no-such-root/.minecraft", "1.20.4");

    let verdict = Scan::revalidate(&env, &tile);
    std::fs::remove_dir_all(&dir).ok();
    let reason = verdict.expect_err(
        "反查落空又不是老档案 ⇒ 必须拒绝（放行会让 game_for 退回配置根，把同名版本拉起来）",
    );
    assert!(
        reason.contains("游戏目录已不在") && reason.contains("1.20.4"),
        "具名原因要说清是哪条实例、为什么（现在是：{reason}）"
    );
}

/// `live.rs` 的**代码**：每行剥掉 `//` 之后的部分（`//` / `///` / `//!` 都从此处走）。
///
/// 必须剥：`live.rs` 的文档里写着那条 `if` 的原文（就是给下一个「清死代码」的人看的），注释里
/// 出现不算数——否则下面的守卫会**自证**地永远绿（M306 r1 复审要的正是这道网，别让它空转）。
fn live_code() -> String {
    include_str!("refresh/live.rs")
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

/// **源码级守卫**：`live.rs` 里那条「M273 老档案放行」必须**原样还在**，且没人把编译器的提醒关掉。
///
/// 为什么行为用例不够（M306 r1 复审证明）：删掉那三行时编译器**会**给信号（`unused_imports`
/// 点名 `legacy_external_instance_id`，`-D warnings` 判红）——**但这道保险可绕过**：把 import
/// 一起删掉、或加 `#[allow(unused_imports)]`，编译期**一条 warning 都不打**（M306 r1 实测：
/// `grep -c '^warning'` = 0），`cargo test` 也依旧全绿。
/// 那时只有扫源码能说话：`a_m273_legacy_ext_id_is_allowed` 判**行为**，本条判**判据还在不在**。
///
/// 抓得住：那三行被删（注释里残留的原文不算）／import 里的名字被删／有人加 `unused_imports` 豁免。
/// 抓不住：把判据换个写法搬去别处——那时本条会红，请顺手把下面的字面量改成新形状（别把守卫删了）。
#[test]
fn the_legacy_pass_through_is_still_in_the_source() {
    let code = live_code();
    let branch = "if legacy_external_instance_id(&instance.version_id) == instance.id {";
    let import =
        "use super::super::discovered::{external_instance_id, legacy_external_instance_id};";
    assert!(
        code.contains(branch) && code.contains(import),
        "live.rs::discovered 里那条「M273 老档案放行」不见了（分支在 = {}，import 在 = {}）：\
         删掉它会让所有存量 M273 用户的 `ext-` 磁贴点不动，还配一句错的「所属的游戏目录已不在」",
        code.contains(branch),
        code.contains(import)
    );
    assert!(
        !code.contains("unused_imports"),
        "live.rs 里出现了 `unused_imports` 豁免：它会把「删掉放行分支后 use 行变空」这条唯一的\
         编译期提醒关掉——那是本条守卫存在的理由，不许豁免"
    );
}
