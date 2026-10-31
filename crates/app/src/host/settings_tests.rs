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
