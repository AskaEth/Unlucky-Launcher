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
    // M220：`binary` 是 D4 校验过的那个，`launch_binary` 是装配落地的那个——两者都得给出来。
    assert!(
        java["launch_binary"].as_str().is_some(),
        "`--json` 的 java 块要写明实际装配的 java 路径：{json}"
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

/// 写一份「清单自己声明要 Java `required`」的版本 JSON（其余字段与 `support::write_version` 同）。
///
/// 本机任何一个 Java 都不满足 `required = 99` ⇒ D4 必然要拦（`java_too_old`），
/// 与「这台机器装了哪几个 JDK」无关、也不需要联网：用例的观察点是**拦截发生在哪一步**。
fn write_version_requiring_java(root: &std::path::Path, required: u32) {
    let dir = root.join(".minecraft").join("versions").join(VERSION);
    std::fs::create_dir_all(&dir).expect("创建版本目录");
    let version = serde_json::json!({
        "id": VERSION,
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "assets": "5",
        "assetIndex": { "id": "5" },
        "javaVersion": { "majorVersion": required },
        "arguments": {
            "game": ["--username", "${auth_player_name}", "--version", "${version_name}"],
            "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"]
        },
        "libraries": [],
        "downloads": { "client": { "size": 4, "url": "http://example.invalid/client.jar" } }
    });
    std::fs::write(
        dir.join(format!("{VERSION}.json")),
        serde_json::to_vec_pretty(&version).expect("序列化版本 JSON"),
    )
    .expect("写版本 JSON");
    std::fs::write(dir.join(format!("{VERSION}.jar")), b"junk").expect("写 client jar");
}

/// M220 nit3：D4 的「Java 不符」拦截必须排在身份解析**之前**。
///
/// 身份解析有副作用（微软轨会联网刷新令牌、刷新成功还会回写凭据），一个注定要被拦下的
/// 启动不该先跑那一遍。观察点：数据根里放两个离线账户 ⇒ 解析身份必然报 `account_ambiguous`，
/// 而这次启动注定被 D4 拦下 ⇒ 报出来的必须是 `java_mismatch`。
/// 老实现（拦在身份之后）在这里会先撞上 `account_ambiguous`，用例必红。
///
/// 对照组：`--dry-run` 本来就只提示不拦（D4 只管真启动），所以同一条版本在 dry-run 下
/// 会走到身份解析那一步——这一对断言把「拦截在哪一步」钉死。
#[test]
fn the_java_mismatch_block_runs_before_identity_resolution() {
    let dir = TempDir::new("java-block-order");
    write_version_requiring_java(dir.path(), 99);
    for name in ["alpha", "beta"] {
        let (code, _, stderr) =
            run_cli(&["accounts", "add", "offline", name, "--data-dir", &dir.arg()]);
        assert_eq!(code, 0, "建离线账户失败：{stderr}");
    }

    // 前置：这条版本的要求确实没人满足（否则「注定被拦」这个前提不成立，直接判红）。
    let (code, stdout, stderr) = run_cli(&[
        "--json",
        "--data-dir",
        &dir.arg(),
        "launch",
        VERSION,
        "--dry-run",
        "--account",
        "alpha",
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let envelope = parse_json(&stdout);
    assert_eq!(
        envelope["data"]["java"]["mismatch"]["code"], "java_too_old",
        "前置：本机探测到的 Java 必须都不满足 Java 99：{envelope}"
    );

    // 真启动（不指定账户）：两个账户 ⇒ 解析身份会报 account_ambiguous，但 D4 拦得更早。
    let (code, stdout, _stderr) = run_cli(&["--json", "--data-dir", &dir.arg(), "launch", VERSION]);
    assert_eq!(code, 1);
    let envelope = parse_json(&stdout);
    assert_eq!(
        envelope["error"]["code"], "java_mismatch",
        "D4 拦截必须排在身份解析之前（否则注定被拦的启动也先刷了一遍令牌）：{envelope}"
    );

    // 对照：dry-run 不拦（D4 只管真启动）⇒ 它会走到身份解析，报的是账户歧义。
    let (code, stdout, _stderr) = run_cli(&[
        "--json",
        "--data-dir",
        &dir.arg(),
        "launch",
        VERSION,
        "--dry-run",
    ]);
    assert_eq!(code, 1);
    let envelope = parse_json(&stdout);
    assert_eq!(
        envelope["error"]["code"], "account_ambiguous",
        "dry-run 不该被 D4 拦住（它只提示），应当走到身份解析：{envelope}"
    );
}
