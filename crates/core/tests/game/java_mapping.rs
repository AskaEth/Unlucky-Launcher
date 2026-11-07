//! MC↔Java 映射：版本 JSON 的 `javaVersion` 优先（含继承链），上限取自静态表。

use std::path::PathBuf;

use unlucky_core::game::java::mapping;

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ull-java-{tag}-{}-{nanos}-{seq}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建临时目录");
    dir
}

#[test]
fn version_json_java_version_beats_the_static_table() {
    let root = temp_dir("mapping");
    let layout = unlucky_core::game::GameLayout::new(&root);
    let write = |id: &str, json: serde_json::Value| {
        let dir = root.join("versions").join(id);
        std::fs::create_dir_all(&dir).expect("建版本目录");
        std::fs::write(
            dir.join(format!("{id}.json")),
            serde_json::to_vec(&json).expect("序列化"),
        )
        .expect("写版本 JSON");
    };

    // 1.17 的清单自己说是 16（静态表也是 16，但来源必须标 manifest）。
    write(
        "1.17.1",
        serde_json::json!({"id": "1.17.1", "javaVersion": {"component": "java-runtime-alpha", "majorVersion": 16}}),
    );
    let manifest = mapping::requirement_for_version(&layout, "1.17.1");
    assert_eq!(manifest.minimum, 16);
    assert_eq!(manifest.source.as_str(), "manifest");

    // 魔改实例覆盖父版本：21 应当生效。
    write("1.20.1", serde_json::json!({"id": "1.20.1"}));
    write(
        "1.20.1-forge",
        serde_json::json!({"id": "1.20.1-forge", "inheritsFrom": "1.20.1", "javaVersion": {"majorVersion": 21}}),
    );
    assert_eq!(
        mapping::requirement_for_version(&layout, "1.20.1-forge").minimum,
        21
    );

    // 清单没写 javaVersion：父版本也没有 → 静态表兜底。
    let table = mapping::requirement_for_version(&layout, "1.20.1");
    assert_eq!(table.minimum, 17);
    assert_eq!(table.source.as_str(), "table");

    // 1.12.2：清单只说「最低 8」，但上限（LaunchWrapper 只能 Java 8）由静态表补上。
    write(
        "1.12.2",
        serde_json::json!({"id": "1.12.2", "javaVersion": {"component": "jre-legacy", "majorVersion": 8}}),
    );
    let legacy = mapping::requirement_for_version(&layout, "1.12.2");
    assert_eq!((legacy.minimum, legacy.maximum), (8, Some(8)));
    assert_eq!(legacy.source.as_str(), "manifest");
    assert!(
        mapping::check(21, &legacy).is_some(),
        "1.12.2 + Java 21 必须报不符"
    );

    // 版本根本没装：按 id 走静态表，不报错。
    let missing = mapping::requirement_for_version(&layout, "26.3");
    assert_eq!(missing.minimum, 25);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn parent_java_version_is_inherited() {
    let root = temp_dir("inherit");
    let layout = unlucky_core::game::GameLayout::new(&root);
    let write = |id: &str, json: serde_json::Value| {
        let dir = root.join("versions").join(id);
        std::fs::create_dir_all(&dir).expect("建版本目录");
        std::fs::write(
            dir.join(format!("{id}.json")),
            serde_json::to_vec(&json).expect("序列化"),
        )
        .expect("写版本 JSON");
    };
    write(
        "1.20.5",
        serde_json::json!({"id": "1.20.5", "javaVersion": {"component": "java-runtime-delta", "majorVersion": 21}}),
    );
    write(
        "1.20.5-fabric",
        serde_json::json!({"id": "1.20.5-fabric", "inheritsFrom": "1.20.5"}),
    );
    let requirement = mapping::requirement_for_version(&layout, "1.20.5-fabric");
    assert_eq!(requirement.minimum, 21);
    assert_eq!(requirement.source.as_str(), "manifest");
    let _ = std::fs::remove_dir_all(&root);
}
