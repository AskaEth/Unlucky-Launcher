//! host/settings.rs 的单测（拆文件守 400 行上限）。

use super::*;

fn env_in(dir: &std::path::Path) -> HostEnv {
    std::fs::create_dir_all(dir.join(".ULL/config")).expect("config");
    HostEnv::at_root(dir)
}

#[test]
fn groups_cover_the_six_required_areas() {
    let dir = std::env::temp_dir().join(format!("ull-host-settings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = env_in(&dir);
    let groups = groups(&env);
    let ids: Vec<&str> = groups.iter().map(|group| group.id.as_str()).collect();
    assert_eq!(ids, vec!["download", "java", "data", "ai", "mcp"]);
    let keys: Vec<&str> = groups
        .iter()
        .flat_map(|group| group.rows.iter())
        .map(|row| row.key.as_str())
        .collect();
    for key in [
        "download.region",
        "download.mirror_preference",
        "java.default_memory_mb",
        "core.data_mode",
        "agent.mcp.enabled",
        "agent.mcp.port",
        "ai.base_url",
    ] {
        assert!(keys.contains(&key), "缺少 {key}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_parses_types_and_persists() {
    let dir = std::env::temp_dir().join(format!("ull-host-settings-w-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = env_in(&dir);

    set(&env, "download.concurrency", "16").expect("写并发");
    set(&env, "agent.mcp.enabled", "true").expect("写开关");
    set(&env, "agent.mcp.port", "48000").expect("写端口");
    set(&env, "download.region", "cn").expect("写地区");

    let groups = groups(&env);
    let row = |key: &str| {
        groups
            .iter()
            .flat_map(|g| g.rows.iter())
            .find(|r| r.key == key)
            .cloned()
    };
    assert_eq!(row("download.concurrency").unwrap().selected, 2);
    assert!(row("agent.mcp.enabled").unwrap().flag);
    assert_eq!(row("agent.mcp.port").unwrap().value_text, "48000");
    assert_eq!(row("download.region").unwrap().selected, 1);

    assert!(set(&env, "download.concurrency", "abc").is_err());
    assert!(set(&env, "agent.mcp.enabled", "maybe").is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn detect_region_action_removes_cache() {
    let dir = std::env::temp_dir().join(format!("ull-host-settings-r-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = env_in(&dir);
    let path = env.layout.cache().join("region.json");
    std::fs::create_dir_all(env.layout.cache()).expect("cache");
    std::fs::write(&path, "{}").expect("写缓存");
    set(&env, "download.action.detect_region", "").expect("检测");
    assert!(!path.exists());
    std::fs::remove_dir_all(&dir).ok();
}

/// **P1-1 的回归**：整窗透明度是 `ValueKind::Float` 的键，`SetSetting` 的载荷是字符串。
///
/// 历史（复审 r1 P1-1）：`parse_value` 只认硬编码的 Int/Bool 键名，Float 键被当成字符串交给
/// `store.set`，schema 的 `as_f64()` 对 String 返回 None ⇒ `InvalidValue` ⇒ **`save()` 不执行**，
/// 于是 α 只在内存生效、重启回旧值，且每次拖动弹一条 Error toast。
///
/// 抓得住：`set` → 落盘 → **新开一个 store（= 模拟重启）** 读回同一值。
/// 抓不住：UI 侧是否真的把滑块接到 `SetSetting`（由 `app` 侧的 `on_appearance` 测试管）。
#[test]
fn float_key_round_trips_through_set_setting() {
    let dir = std::env::temp_dir().join(format!("ull-host-settings-float-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = env_in(&dir);

    set(&env, "appearance.window_opacity", "0.8").expect("写整窗透明度");
    set(&env, "appearance.panel_alpha", "0.55").expect("写面板透明度");

    // 新开一个 store：从磁盘重新加载（等价「重启进程」）。
    let reopened = env.store();
    assert_eq!(
        reopened.doc().effective_f64("appearance.window_opacity"),
        Some(0.8)
    );
    assert_eq!(
        reopened.doc().effective_f64("appearance.panel_alpha"),
        Some(0.55)
    );

    // 越界仍然被 schema 拒绝（钳位在 core 的读侧，写侧是拒绝）。
    assert!(set(&env, "appearance.window_opacity", "0.0").is_err());
    // 非数字报人话，而不是「期望数字，实际是 \"abc\"」这种 schema 原话。
    assert!(set(&env, "appearance.window_opacity", "abc").is_err());
    assert!(set(&env, "appearance.window_opacity", "NaN").is_err());
    std::fs::remove_dir_all(&dir).ok();
}

/// **集合级守门（复审 P1-1 要求的那条）**：schema 里**每一个** `ValueKind::Float` 的键都必须
/// 能被 `SetSetting` 写回，而不只是 `window_opacity`。
///
/// 为什么这条比单键测试重要：下次谁再加一个 Float 键（比如玻璃强度、模糊半径），
/// 只要 `parse_value` 又退化成「硬编码表」，这里立刻红——单键测试抓不住新键。
///
/// 抓得住：类型解析与 `save()` 是否真的执行（读回是**新开 store**，走磁盘）。
/// 抓不住：UI 控件是否接上了这些键（那是各页面的接线测试）。
#[test]
fn every_float_key_in_the_schema_can_be_written_back() {
    let dir =
        std::env::temp_dir().join(format!("ull-host-settings-allfloat-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = env_in(&dir);

    let schema = unlucky_core::setting::keys::launcher_schema();
    let mut checked = 0_usize;
    for spec in schema.specs() {
        let unlucky_core::setting::spec::ValueKind::Float { min, max } = spec.kind else {
            continue;
        };
        // 取一个落在 [min, max] 内的值（0.5 是多数区间的好选择，落不进时取中点）。
        let target = if (min..=max).contains(&0.5) {
            0.5
        } else {
            (min + max) / 2.0
        };
        set(&env, spec.key, &format!("{target}"))
            .unwrap_or_else(|error| panic!("Float 键 {} 写不回：{error}", spec.key));
        let reread = env
            .store()
            .doc()
            .effective_f64(spec.key)
            .unwrap_or_else(|| panic!("Float 键 {} 写完后读不到", spec.key));
        assert!(
            (reread - target).abs() < 1e-9,
            "Float 键 {} 写 {target} 读回 {reread}",
            spec.key
        );
        checked += 1;
    }

    assert!(
        checked >= 3,
        "schema 里 Float 键只剩 {checked} 个：本断言要求 ≥3（window_opacity / panel_alpha / bg_dim / ui.scale）。\
         若是有意删减，请同步更新这条断言"
    );
    std::fs::remove_dir_all(&dir).ok();
}
