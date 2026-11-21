//! D20 实例隔离：覆盖链派生、双侧切换检测、稀疏落盘不变量。
//!
//! 对应 mission 的验收点：三档派生 / 覆盖链优先级 / 切换检测（双侧空、单侧、双侧）/
//! `inherit` 不落盘 / 恒共享层不可配。

use std::path::Path;

use unlucky_core::game::install::instance::{InstanceConfig, Isolation};
use unlucky_core::game::isolation::{
    derive_game_dir, probe_both_sides, probe_side, IsolationMode, IsolationSource,
    PROBED_CONTENT_DIRS, SHARED_LAYERS,
};
use unlucky_core::game::GameError;

use super::helpers::{layout, seed, TempDir};

// ---------------------------------------------------------------- 派生（三档）

#[test]
fn isolated_mode_derives_version_dir() {
    let temp = TempDir::new("iso-derived");
    let game = layout(temp.path());
    let derived = derive_game_dir(&game, "1.20.1", IsolationMode::Isolated, None, None)
        .expect("isolated 一定能派生");
    assert_eq!(derived.isolation, Isolation::Isolated);
    assert_eq!(derived.game_dir, game.version_dir("1.20.1"));
    assert_eq!(derived.source, IsolationSource::Global);
}

#[test]
fn shared_mode_derives_game_root() {
    let temp = TempDir::new("iso-shared");
    let game = layout(temp.path());
    let derived = derive_game_dir(&game, "1.20.1", IsolationMode::Shared, None, None)
        .expect("shared 一定能派生");
    assert_eq!(derived.isolation, Isolation::Shared);
    assert_eq!(derived.game_dir, game.root());
}

#[test]
fn custom_mode_uses_instance_game_dir() {
    let temp = TempDir::new("iso-custom");
    let game = layout(temp.path());
    let custom = temp.path().join("elsewhere").join("farm");
    let mut config = InstanceConfig::new("id-1", "1.20.1");
    config.isolation = Some(Isolation::Custom);
    config.game_dir = Some(custom.clone());
    let derived = derive_game_dir(
        &game,
        "1.20.1",
        IsolationMode::Isolated,
        Some(&config),
        None,
    )
    .expect("custom 有目录就能派生");
    assert_eq!(derived.isolation, Isolation::Custom);
    assert_eq!(derived.game_dir, custom);
    assert_eq!(derived.source, IsolationSource::Instance);
}

#[test]
fn custom_without_game_dir_is_an_error() {
    let temp = TempDir::new("iso-custom-missing");
    let game = layout(temp.path());
    let mut config = InstanceConfig::new("id-missing", "1.20.1");
    config.isolation = Some(Isolation::Custom);
    let error = derive_game_dir(
        &game,
        "1.20.1",
        IsolationMode::Isolated,
        Some(&config),
        None,
    )
    .expect_err("custom 缺目录必须报错，不许猜默认值");
    assert!(matches!(error, GameError::CustomIsolationWithoutDir { .. }));
    assert!(error.to_string().contains("id-missing"), "{error}");
}

// ------------------------------------------------------------ 覆盖链（优先级）

#[test]
fn instance_isolation_beats_global_mode() {
    let temp = TempDir::new("iso-priority");
    let game = layout(temp.path());
    let mut config = InstanceConfig::new("id-2", "1.20.1");
    config.isolation = Some(Isolation::Shared);
    let derived = derive_game_dir(
        &game,
        "1.20.1",
        IsolationMode::Isolated,
        Some(&config),
        None,
    )
    .expect("派生");
    assert_eq!(derived.isolation, Isolation::Shared);
    assert_eq!(derived.game_dir, game.root());
    assert_eq!(derived.source, IsolationSource::Instance);
}

#[test]
fn inherit_falls_back_to_global_mode() {
    let temp = TempDir::new("iso-inherit");
    let game = layout(temp.path());
    let config = InstanceConfig::new("id-3", "1.20.1");
    assert!(config.inherits_isolation(), "新实例默认 inherit");
    for (mode, expected) in [
        (IsolationMode::Isolated, game.version_dir("1.20.1")),
        (IsolationMode::Shared, game.root().to_path_buf()),
    ] {
        let derived = derive_game_dir(&game, "1.20.1", mode, Some(&config), None).expect("派生");
        assert_eq!(
            derived.game_dir,
            expected,
            "全局 {} 应决定 inherit",
            mode.as_str()
        );
        assert_eq!(derived.source, IsolationSource::Global);
    }
}

#[test]
fn temporary_override_wins_over_everything() {
    let temp = TempDir::new("iso-temporary");
    let game = layout(temp.path());
    let mut config = InstanceConfig::new("id-4", "1.20.1");
    config.isolation = Some(Isolation::Isolated);
    let forced = temp.path().join("forced");
    let derived = derive_game_dir(
        &game,
        "1.20.1",
        IsolationMode::Shared,
        Some(&config),
        Some(&forced),
    )
    .expect("派生");
    assert_eq!(derived.game_dir, forced);
    assert_eq!(derived.source, IsolationSource::Temporary);
    // 档位本身仍是实例档位（临时参数只改目录，不改语义）。
    assert_eq!(derived.isolation, Isolation::Isolated);

    // 空路径 = 没给临时参数。
    let derived = derive_game_dir(
        &game,
        "1.20.1",
        IsolationMode::Shared,
        Some(&config),
        Some(Path::new("")),
    )
    .expect("派生");
    assert_eq!(derived.source, IsolationSource::Instance);
}

#[test]
fn shared_layers_are_fixed_and_unconfigurable() {
    assert_eq!(SHARED_LAYERS, ["versions", "assets", "libraries"]);
    // natives 不在恒共享层里：它按实例解包（两家启动器一致）。
    assert!(!SHARED_LAYERS.contains(&"natives"));
    assert_eq!(PROBED_CONTENT_DIRS, ["mods", "saves", "config"]);
}

// -------------------------------------------------------------- 切换检测（探测）

#[test]
fn probe_counts_only_player_content() {
    let temp = TempDir::new("iso-probe");
    seed(temp.path(), "mods");
    seed(temp.path(), "saves");
    seed(temp.path(), "screenshots");
    let side = probe_side(temp.path()).expect("探测");
    assert_eq!(side.entries, 2, "screenshots 不算玩家内容");
    assert!(side.occupied());
}

#[test]
fn probe_missing_dir_counts_as_zero() {
    let temp = TempDir::new("iso-probe-empty");
    let side = probe_side(&temp.path().join("nope")).expect("目录不存在不算错误");
    assert_eq!(side.entries, 0);
    assert!(!side.occupied());
}

#[test]
fn probe_both_sides_reads_isolated_and_shared() {
    let temp = TempDir::new("iso-probe-both");
    let game = layout(temp.path());
    seed(&game.version_dir("1.20.1"), "saves");
    seed(game.root(), "mods");
    seed(game.root(), "config");
    let (isolated, shared) = probe_both_sides(&game, "1.20.1").expect("探测");
    assert_eq!(isolated.entries, 1);
    assert_eq!(shared.entries, 2);
    assert_eq!(isolated.path, game.version_dir("1.20.1"));
    assert_eq!(shared.path, game.root());
}
