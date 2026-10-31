//! `ull java list` / `ull java install` / `ull launch` 的 Java 选择（D13）CLI 端。
//!
//! 不联网：`install` 只测参数校验与稳定错误码；`launch --dry-run` 断言 `java` 块的结构
//! 与「实例覆盖优先」——真正的下载/解压由 `core::game::java` 的测试与本机验收覆盖。

#[path = "support/mod.rs"]
mod support;

use support::{parse_json, run_cli, write_version, TempDir};

/// 一个可组装的版本 id。
const VERSION: &str = "1.20.1";

#[test]
fn java_list_reports_the_managed_root() {
    let dir = TempDir::new("java-list");
    let (code, stdout, stderr) = run_cli(&["--json", "--data-dir", &dir.arg(), "java", "list"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let envelope = parse_json(&stdout);
    let json = &envelope["data"];
    assert!(
        json["managed_root"]
            .as_str()
            .is_some_and(|root| root.ends_with("java-managed")),
        "managed_root 应是 data/java-managed：{json}"
    );
    // D23：多根扫描 + 落点模式要说清（`--data-dir` 是 custom 数据模式 → portable 落点）。
    assert!(
        json["managed_roots"]
            .as_array()
            .is_some_and(|roots| !roots.is_empty()),
        "{json}"
    );
    assert_eq!(json["install"]["mode"], "portable", "{json}");
    assert!(
        json["install"]["root"]
            .as_str()
            .is_some_and(|root| root.ends_with("java-managed")),
        "落点根应是数据根里的 java-managed：{json}"
    );
    assert!(json["candidates"].is_array(), "{json}");
}

#[test]
fn java_vendors_lists_the_four_vendors_with_the_current_order() {
    let dir = TempDir::new("java-vendors");
    let (code, stdout, stderr) = run_cli(&["--json", "--data-dir", &dir.arg(), "java", "vendors"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let envelope = parse_json(&stdout);
    let json = &envelope["data"];
    let order: Vec<&str> = json["order"]
        .as_array()
        .expect("order 数组")
        .iter()
        .filter_map(|item| item.as_str())
        .collect();
    assert_eq!(order.len(), 4, "{json}");
    assert!(order.contains(&"temurin"), "{json}");
    assert!(order.contains(&"oracle"), "{json}");
    assert!(order.contains(&"microsoft"), "{json}");
    assert!(order.contains(&"zulu"), "{json}");
    // 本机时区/语言决定 domestic/global，两种默认序都合法，但来源必须明示。
    let source = json["priority_source"].as_str().unwrap_or("?");
    assert_eq!(
        source, "region_default",
        "没设 java.vendorPriority 时：{json}"
    );
    let vendors = json["vendors"].as_array().expect("vendors 数组");
    assert_eq!(vendors.len(), 4);
    let microsoft = vendors
        .iter()
        .find(|row| row["vendor"] == "microsoft")
        .expect("应有 microsoft 行");
    assert_eq!(
        microsoft["supported"].as_array().map(Vec::len),
        Some(4),
        "MS 只发 25/21/17/11：{microsoft}"
    );
    assert!(
        microsoft["sources"]
            .as_array()
            .is_some_and(|sources| !sources.is_empty()),
        "MS 的源（aka.ms → VS CDN）要列出来：{microsoft}"
    );
}

#[test]
fn java_install_rejects_an_unknown_vendor_without_network() {
    let dir = TempDir::new("java-vendor-bad");
    let (code, stdout, _stderr) = run_cli(&[
        "--json",
        "--data-dir",
        &dir.arg(),
        "java",
        "install",
        "21",
        "--vendor",
        "graalvm",
    ]);
    assert_eq!(code, 1);
    let envelope = parse_json(&stdout);
    assert_eq!(envelope["error"]["code"], "usage", "{envelope}");
}

#[test]
fn java_install_rejects_an_absurd_major_without_network() {
    let dir = TempDir::new("java-install");
    let (code, stdout, _stderr) =
        run_cli(&["--json", "--data-dir", &dir.arg(), "java", "install", "4"]);
    assert_eq!(code, 1);
    let envelope = parse_json(&stdout);
    assert_eq!(envelope["error"]["code"], "usage", "{envelope}");
}

#[test]
fn launch_dry_run_reports_the_java_requirement_of_the_version() {
    let dir = TempDir::new("java-launch");
    write_version(dir.path(), VERSION);
    let (code, stdout, stderr) = run_cli(&[
        "--json",
        "--data-dir",
        &dir.arg(),
        "launch",
        VERSION,
        "--dry-run",
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let envelope = parse_json(&stdout);
    let json = &envelope["data"];
    let java = &json["java"];
    // 1.20.1 的清单没写 javaVersion → 静态表兜底 17。
    assert_eq!(java["required_major"], 17, "{json}");
    assert_eq!(java["requirement_source"], "table", "{json}");
    assert_eq!(json["version_id"], VERSION);
    // `source` 只可能是探测到的来源之一（本机未必装了 Java）。
    let source = java["source"].as_str().unwrap_or("none");
    assert!(
        ["instance", "managed", "JAVA_HOME", "PATH", "none"].contains(&source),
        "意外的来源：{source}"
    );
}

#[test]
fn instance_java_override_wins_over_the_system_java() {
    let dir = TempDir::new("java-override");
    write_version(dir.path(), VERSION);
    // 故意给一个不存在的路径：只要能进 `source=instance` 就说明覆盖优先级生效
    // （探测失败只影响 major，不该把用户指定的路径换成系统 Java）。
    let fake = dir.path().join("some-jdk").join("bin").join("java");
    let (code, stdout, stderr) = run_cli(&[
        "--json",
        "--data-dir",
        &dir.arg(),
        "launch",
        VERSION,
        "--dry-run",
        "--java",
        &fake.display().to_string(),
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let envelope = parse_json(&stdout);
    let json = &envelope["data"];
    assert_eq!(json["java"]["source"], "instance", "{json}");
    assert_eq!(
        json["java"]["binary"].as_str(),
        Some(fake.display().to_string().as_str())
    );
}

#[test]
fn legacy_version_requires_java_8_and_flags_a_newer_runtime() {
    let dir = TempDir::new("java-legacy");
    write_version(dir.path(), "1.12.2");
    let (code, stdout, stderr) = run_cli(&[
        "--json",
        "--data-dir",
        &dir.arg(),
        "launch",
        "1.12.2",
        "--dry-run",
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let envelope = parse_json(&stdout);
    let json = &envelope["data"];
    assert_eq!(json["java"]["required_major"], 8, "{json}");
    assert_eq!(json["java"]["required"], "Java 8", "{json}");
    // 本机若有比 8 新的 Java，必须给出 java_too_new 告警（不能只靠「最低版本」）。
    if let Some(code) = json["java"]["mismatch"]["code"].as_str() {
        assert!(code == "java_too_new" || code == "java_too_old", "{json}");
        assert!(
            json["warnings"]
                .as_array()
                .is_some_and(|warnings| !warnings.is_empty()),
            "不符时必须进 warnings：{json}"
        );
    }
}
