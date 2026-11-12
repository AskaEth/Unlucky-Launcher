//! 实例 DTO 的单测（词表契约 / 徽标 / 序列化往返）。

use super::*;
use unlucky_core::game::LoaderKind as CoreLoaderKind;

use crate::dto::LaunchArgsView;

#[test]
fn loader_badges_and_ids() {
    let vanilla = LoaderInfo::default();
    assert_eq!(vanilla.badge(), "原版");
    let fabric = LoaderInfo {
        kind: LoaderKind::Fabric,
        version: "0.16.5".to_string(),
    };
    assert_eq!(fabric.badge(), "Fabric 0.16.5");
    let liteloader = LoaderInfo {
        kind: LoaderKind::LiteLoader,
        version: "1.12.2-SNAPSHOT".to_string(),
    };
    assert_eq!(liteloader.badge(), "LiteLoader 1.12.2-SNAPSHOT");
    for kind in LoaderKind::wizard_choices() {
        assert_eq!(LoaderKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(LoaderKind::parse("fabric"), Some(LoaderKind::Fabric));
    assert_eq!(LoaderKind::parse("unknown"), None);
}

/// 向导可选集：**每一支都能被 core 的安装侧接受**（OptiFine 不在其中，因为它装不了）。
///
/// 抓得住：把 OptiFine 塞进向导（选完才失败）、漏掉 LiteLoader（GUI 走不到真实路线）。
#[test]
fn wizard_choices_are_all_installable() {
    let installable: Vec<&str> = LoaderKind::wizard_choices()
        .iter()
        .filter(|kind| **kind != LoaderKind::Vanilla)
        .map(|kind| kind.as_str())
        .collect();
    for id in &installable {
        assert!(
            unlucky_core::addon::loader::LoaderKind::parse(id).is_some(),
            "`{id}` 在向导里但安装侧不认"
        );
    }
    assert_eq!(installable.len(), 5, "原版 + 五家能装的：{installable:?}");
    assert!(!installable.contains(&"optifine"));
}

/// 对外契约：七个 id 在测试里**独立抄一遍**（0.2.0 已发布形状），并与 core 的 `ALL` 做
/// **集合比较**——这是对外词表的守卫（与 CLI 的 help 守卫互补）。显示路径另有守卫：
/// `presenter::host::game::every_core_loader_kind_reaches_the_tile_as_itself`（按 `ALL` 遍历）。
///
/// 为什么不是编译器管：编译期只拦得到「漏 match」，而新种类在还有 match 的消费方用最省事的
/// `=> Vanilla` 糊过去**编得过**，那时新加载器会静默显示成原版（`game::loader` 模块文档
/// 第 ③ 步）。对外词表由下面第 ② 段的集合断言守，显示路径由上面那条测试守。
///
/// 抓得住：改词表（`neoforge` → `neo_forge`）；**新增一家加载器而没主动更新这份契约**
/// （core 的 `ALL` 多一支 ⇒ 集合不等）；DTO 委派接错支（`OptiFine` 映到 `LiteLoader`）；
/// `parse` 收进不该有的别名。
/// 抓不住：core 的解析规则（那是 `game::loader` 的
/// `every_stable_id_parses_back_to_its_kind` 等测试的职责），以及真二进制输出面的 JSON
/// （CLI 的 `json_rows_expose_loader_kind_and_version`）。
#[test]
fn loader_kind_ids_are_the_released_contract() {
    const CONTRACT: [(LoaderKind, &str); 7] = [
        (LoaderKind::Vanilla, "vanilla"),
        (LoaderKind::Fabric, "fabric"),
        (LoaderKind::Quilt, "quilt"),
        (LoaderKind::Forge, "forge"),
        (LoaderKind::NeoForge, "neoforge"),
        (LoaderKind::OptiFine, "optifine"),
        (LoaderKind::LiteLoader, "liteloader"),
    ];

    // ① DTO 这一层：每支的 id、反查、大小写。
    for (kind, id) in CONTRACT {
        assert_eq!(kind.as_str(), id, "{kind:?} 的稳定 id 变了");
        assert_eq!(LoaderKind::parse(id), Some(kind), "`{id}` 认不回来");
        let upper = id.to_ascii_uppercase();
        assert_eq!(LoaderKind::parse(&upper), Some(kind), "大写 `{upper}`");
    }

    // ② core 的词表 ↔ 这份契约：**集合相等**才过（多一支、少一支、改一个字都红）。
    let mut actual: Vec<&str> = CoreLoaderKind::ALL
        .iter()
        .map(|kind| kind.as_str())
        .collect();
    actual.sort_unstable();
    let mut expected: Vec<&str> = CONTRACT.iter().map(|(_, id)| *id).collect();
    expected.sort_unstable();
    assert_eq!(
        actual, expected,
        "core 的 `LoaderKind::ALL` 必须逐字等于对外契约：加一家加载器时先改这里 + help"
    );

    // GUI 输入别名（不是对外 id）：不装加载器有三种写法；别名与拼法都不许混进词表。
    for alias in ["none", "", "   "] {
        assert_eq!(
            LoaderKind::parse(alias),
            Some(LoaderKind::Vanilla),
            "`{alias}`"
        );
    }
    assert_eq!(LoaderKind::parse("neo-forge"), None, "别名只归安装侧");
}

#[test]
fn progress_ratio_handles_unknown_total() {
    assert_eq!(Progress { done: 1, total: 0 }.ratio(), 0.0);
    assert!((Progress { done: 1, total: 4 }.ratio() - 0.25).abs() < f32::EPSILON);
}

#[test]
fn create_request_defaults_name_to_version() {
    let request = CreateRequest {
        version: "1.20.4".to_string(),
        loader: LoaderKind::Vanilla,
        loader_version: String::new(),
        name: "  ".to_string(),
    };
    assert_eq!(request.instance_name(), "1.20.4");
}

#[test]
fn instance_summary_is_serde_round_trip_safe() {
    let instance = InstanceSummary {
        id: "inst-1".to_string(),
        name: "生存".to_string(),
        mc_version: "1.20.4".to_string(),
        loader: LoaderInfo {
            kind: LoaderKind::Fabric,
            version: "0.16.5".to_string(),
        },
        last_played: Some("2026-10-31T12:00:00.000Z".to_string()),
        runtime: InstanceRuntime::Busy {
            stage: LaunchStage::Window,
            progress: Progress { done: 1, total: 2 },
        },
        icon: None,
        isolation: "isolated".to_string(),
        version_id: "1.20.4".to_string(),
        version_present: true,
        launch: LaunchArgsView::default(),
    };
    let text = serde_json::to_string(&instance).expect("序列化");
    let back: InstanceSummary = serde_json::from_str(&text).expect("反序列化");
    assert_eq!(back, instance);
}
