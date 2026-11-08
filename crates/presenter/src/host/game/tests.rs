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

    assert_eq!(isolation_of(&env, &id), "isolated");
    set_isolation(&env, &id, "shared").expect("切档位");
    assert_eq!(isolation_of(&env, &id), "shared");
    assert!(set_isolation(&env, &id, "nope").is_err());
    std::fs::remove_dir_all(&dir).ok();
}
