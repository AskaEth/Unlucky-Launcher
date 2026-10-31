//! D20 切换检测与稀疏落盘：observe/warn 语义、`inherit` 与 `lastModeChange` 不落盘、
//! 旧字面量读兼容（从 `isolation.rs` 拆出，保持单文件 ≤400 行）。

use std::path::PathBuf;

use unlucky_core::game::install::instance::{InstanceConfig, Isolation};
use unlucky_core::game::isolation::{
    derive_game_dir, observe_mode, IsolationMode, IsolationSource,
};

use super::helpers::{layout, seed, TempDir};

// ------------------------------------------------- 切换记录（幂等 + 只提示一次）

#[test]
fn observe_mode_records_baseline_without_warning() {
    let temp = TempDir::new("iso-observe-baseline");
    let game = layout(temp.path());
    seed(game.root(), "saves");
    let mut config = InstanceConfig::new("id-5", "1.20.1");
    let changed = observe_mode(
        &mut config,
        &game,
        "2026-10-30T00:00:00Z",
        Isolation::Isolated,
    )
    .expect("观察");
    assert!(changed, "首次观察要建基线");
    let change = config.last_mode_change.clone().expect("基线");
    assert_eq!(change.from, change.to);
    assert!(!change.is_real_change());
    assert!(change.warned, "基线不该弹提示");
    assert_eq!(change.probe.shared_side, 1);
    assert!(
        config.take_mode_change_warning().is_none(),
        "基线即使两侧都有内容也不提示"
    );
}

#[test]
fn observe_mode_is_idempotent() {
    let temp = TempDir::new("iso-observe-idempotent");
    let game = layout(temp.path());
    let mut config = InstanceConfig::new("id-6", "1.20.1");
    assert!(
        observe_mode(&mut config, &game, "t1", Isolation::Isolated).expect("观察"),
        "首次写入"
    );
    assert!(
        !observe_mode(&mut config, &game, "t2", Isolation::Isolated).expect("观察"),
        "同一档位不重复记"
    );
    assert_eq!(
        config.last_mode_change.expect("记录仍在").at,
        "t1",
        "时间戳不该被刷新"
    );
}

#[test]
fn mode_change_with_both_sides_warns_exactly_once() {
    let temp = TempDir::new("iso-warn-once");
    let game = layout(temp.path());
    seed(&game.version_dir("1.20.1"), "saves");
    seed(&game.version_dir("1.20.1"), "mods");
    seed(game.root(), "saves");
    let mut config = InstanceConfig::new("id-7", "1.20.1");
    config.isolation = Some(Isolation::Shared);
    observe_mode(&mut config, &game, "t0", Isolation::Shared).expect("基线");
    config.isolation = Some(Isolation::Isolated);
    assert!(
        observe_mode(&mut config, &game, "t1", Isolation::Isolated).expect("记切换"),
        "档位变了要写记录"
    );
    let change = config.last_mode_change.clone().expect("记录");
    assert_eq!(change.from, Isolation::Shared);
    assert_eq!(change.to, Isolation::Isolated);
    assert!(change.is_real_change());
    assert_eq!(change.probe.isolated_side, 2);
    assert_eq!(change.probe.shared_side, 1);

    let warning = config
        .take_mode_change_warning()
        .expect("两侧都有内容要提示");
    assert!(warning.probe.both_occupied());
    let message = warning.message();
    assert!(message.contains("isolated"), "{message}");
    assert!(
        message.contains("不会自动搬"),
        "文案必须说清不迁移：{message}"
    );
    assert!(
        config.take_mode_change_warning().is_none(),
        "同一次切换只提示一次"
    );
    assert!(config.last_mode_change.expect("记录").warned);
}

#[test]
fn mode_change_with_one_empty_side_is_quiet() {
    let temp = TempDir::new("iso-warn-single-side");
    let game = layout(temp.path());
    // 只有共享侧有内容：隔离侧空 → 不提示（切到共享还能看见存档，反之亦然）。
    seed(game.root(), "saves");
    let mut config = InstanceConfig::new("id-8", "1.20.1");
    observe_mode(&mut config, &game, "t0", Isolation::Shared).expect("基线");
    observe_mode(&mut config, &game, "t1", Isolation::Isolated).expect("记切换");
    assert!(config.take_mode_change_warning().is_none());

    // 两侧都空同样安静。
    let empty = TempDir::new("iso-warn-both-empty");
    let game = layout(empty.path());
    let mut config = InstanceConfig::new("id-9", "1.20.1");
    observe_mode(&mut config, &game, "t0", Isolation::Shared).expect("基线");
    observe_mode(&mut config, &game, "t1", Isolation::Isolated).expect("记切换");
    assert!(config.take_mode_change_warning().is_none());
}

// -------------------------------------------------------------- 稀疏落盘不变量

#[test]
fn inherit_and_no_change_stay_out_of_the_file() {
    let config = InstanceConfig::new("id-10", "1.20.1");
    let json = config.to_json_string().expect("序列化");
    assert!(!json.contains("isolation"), "inherit 不许落盘：{json}");
    assert!(!json.contains("lastModeChange"), "没切过就不写：{json}");
    assert!(!json.contains("gameDir"), "没目录就不写：{json}");
    let back = InstanceConfig::from_json_str(&json).expect("回读");
    assert_eq!(back, config);
}

#[test]
fn last_mode_change_round_trips_in_camel_case() {
    let temp = TempDir::new("iso-roundtrip");
    let game = layout(temp.path());
    let mut config = InstanceConfig::new("id-11", "1.20.1");
    observe_mode(&mut config, &game, "t0", Isolation::Shared).expect("基线");
    let json = config.to_json_string().expect("序列化");
    assert!(json.contains("\"lastModeChange\""), "{json}");
    assert!(json.contains("\"isolatedSide\""), "{json}");
    assert!(json.contains("\"sharedSide\""), "{json}");
    let back = InstanceConfig::from_json_str(&json).expect("回读");
    assert_eq!(back, config);
}

#[test]
fn legacy_per_version_value_reads_as_isolated() {
    let raw = r#"{
        "id": "id-12",
        "versionId": "1.20.1",
        "isolation": "per_version"
    }"#;
    let config = InstanceConfig::from_json_str(raw).expect("旧字面量读兼容");
    assert_eq!(config.isolation, Some(Isolation::Isolated));
    assert_eq!(Isolation::parse("per_version"), Some(Isolation::Isolated));
    assert_eq!(Isolation::parse("isolated"), Some(Isolation::Isolated));
    assert_eq!(Isolation::parse("nonsense"), None);
}

#[test]
fn effective_isolation_resolves_inherit() {
    let config = InstanceConfig::new("id-13", "1.20.1");
    assert_eq!(
        config.effective_isolation(IsolationMode::Shared),
        Isolation::Shared
    );
    let mut config = InstanceConfig::new("id-14", "1.20.1");
    config.isolation = Some(Isolation::Custom);
    assert_eq!(
        config.effective_isolation(IsolationMode::Isolated),
        Isolation::Custom
    );
}

#[test]
fn custom_dir_from_relative_path_is_kept_verbatim() {
    // custom 目录不做规范化（可能是相对路径或系统符号链接），派生层只透传。
    let temp = TempDir::new("iso-custom-relative");
    let game = layout(temp.path());
    let mut config = InstanceConfig::new("id-15", "1.20.1");
    config.isolation = Some(Isolation::Custom);
    config.game_dir = Some(PathBuf::from("relative/mc"));
    let derived = derive_game_dir(
        &game,
        "1.20.1",
        IsolationMode::Isolated,
        Some(&config),
        None,
    )
    .expect("派生");
    assert_eq!(derived.game_dir, PathBuf::from("relative/mc"));
}

#[test]
fn explicit_inherit_reads_back_as_absent() {
    // 回归 P2-2（review r1）：schema 允许写 `"isolation": "inherit"`，
    // 读路径必须把它折叠成「键不存在」，否则设置层写回默认值后启动会报 unknown variant。
    let raw = r#"{
        "id": "id-16",
        "versionId": "1.20.1",
        "isolation": "inherit"
    }"#;
    let config = InstanceConfig::from_json_str(raw).expect("显式 inherit 必须能读");
    assert!(config.inherits_isolation());
    assert_eq!(config.isolation, None);
    let json = config.to_json_string().expect("序列化");
    assert!(
        !json.contains("isolation"),
        "写回时把 inherit 落成键缺失：{json}"
    );
    // 走到派生层就是「跟随全局」，不再是错误路径。
    let temp = TempDir::new("iso-inherit-explicit");
    let game = layout(temp.path());
    let derived =
        derive_game_dir(&game, "1.20.1", IsolationMode::Shared, Some(&config), None).expect("派生");
    assert_eq!(derived.isolation, Isolation::Shared);
    assert_eq!(derived.source, IsolationSource::Global);

    // 其他非法字面量仍然报错（不静默取默认值）。
    let bad = r#"{ "id": "id-17", "versionId": "1.20.1", "isolation": "maybe" }"#;
    assert!(InstanceConfig::from_json_str(bad).is_err());
}
