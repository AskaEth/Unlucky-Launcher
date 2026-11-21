//! M304 r1 复审的两条判据：**所属根不见了必须拒绝**、**运行时态事件要落进已发布快照**。
//!
//! 前半条是真风险（复审 P2-1）：`revalidate` 对 `ext-` 实例是「逐个根重算 id 定位」，而
//! `roots::all` 对非配置根是 `if !extra.is_dir() { continue; }`。根被删 / 改名 / 盘没挂上 ⇒ 一个都
//! 对不上 ⇒ 若照旧放行，`build_request` → `game_for` 退回**配置根**，配置根里恰好有同名版本就把
//! **另一份**拉起来（gameDir / 存档都在另一边）。改前的同步扫盘扫不出来 ⇒ 拒绝，所以放行是回归。
//!
//! 夹具见 [`super::fixtures`]，其余判据见 [`super::target_tests`]。

use crate::dto::{AppCommand, AppEvent, Backend, CoreEvent, InstanceRuntime};

use super::super::CoreHost;
use super::fixtures::{error_toast, host_with_official, land_snapshot, profile};

/// 从事件里挑出一条「运行时变成 Busy」的 id（受理成功的证据）。
fn busy_id(events: &[AppEvent]) -> Option<String> {
    events.iter().find_map(|event| match event {
        AppEvent::Core(CoreEvent::InstanceRuntimeChanged {
            id,
            runtime: InstanceRuntime::Busy { .. },
        }) => Some(id.clone()),
        _ => None,
    })
}

/// **P2-1**：实例属于另一个根，快照落地之后那个根不见了 ⇒ 点它必须**拒绝并具名**。
///
/// 抓得住：`revalidate` 在反查落空时直接 `Ok(())`（= 复审的现场，这条立即红）。
/// 抓不住：M273 老档案那条**放行**分支——它由 presenter 的 `game::tests_multiroot3`（老 id 的
/// 反查回落）与 `host::game::refresh` 的 `legacy_external_instance_id` 共用同一份哈希守。
#[test]
fn an_instance_whose_root_disappeared_is_refused_with_a_named_reason() {
    let demo = profile("demo", "1.20.1");
    let (mut host, _root, official) =
        host_with_official("root-gone", std::slice::from_ref(&demo), "1.20.9");
    land_snapshot(&mut host);
    let tile = tile_of_version(&host, "1.20.9");

    // 快照之后根没了（盘掉线 / 被移走 / 改名）
    std::fs::remove_dir_all(&official).expect("删掉官方根");

    host.dispatch(AppCommand::LaunchInstance(tile));
    let events = host.drain_events();
    assert!(
        busy_id(&events).is_none(),
        "所属根不见了不许受理（放行会让 game_for 退回配置根，把另一份拉起来）：{events:?}"
    );
    assert!(host.pending.is_empty(), "拒绝受理 ⇒ 不进 pending");
    let message = error_toast(&events);
    assert!(
        message.contains("游戏目录已不在") && message.contains("1.20.9"),
        "具名原因要说出是哪个实例、为什么（现在是：{message}）"
    );
}

/// 快照里那个版本的磁贴 id（`ext-` 前缀 = 自动发现的实例）。
fn tile_of_version(host: &CoreHost, version: &str) -> String {
    host.published
        .iter()
        .find(|instance| instance.version_id == version)
        .map(|instance| instance.id.clone())
        .unwrap_or_else(|| panic!("快照里没有版本 `{version}` 的磁贴：{:?}", host.published))
}
