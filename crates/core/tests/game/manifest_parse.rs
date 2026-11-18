//! 版本 JSON 解析：宽松解析、未知字段保真、旧版字段并存。

use unlucky_core::game::arguments::{Argument, ArgumentValue, RuleAction};
use unlucky_core::game::manifest::VersionJson;
use unlucky_core::game::version::ReleaseType;

use crate::helpers::parse;

const VANILLA_1_20_1: &str = r#"{
  "id": "1.20.1",
  "type": "release",
  "time": "2023-06-12T13:25:51+00:00",
  "releaseTime": "2023-06-12T13:25:51+00:00",
  "complianceLevel": 1,
  "minimumLauncherVersion": 21,
  "mainClass": "net.minecraft.client.main.Main",
  "assets": "5",
  "assetIndex": {
    "id": "5",
    "sha1": "a3b3b1b0e0d8f2a8f0fbe1a4b0a8f6a1c6c6a7a2",
    "size": 404297,
    "totalSize": 598385820,
    "url": "https://piston-meta.mojang.com/v1/packages/sha/5.json"
  },
  "javaVersion": { "component": "java-runtime-gamma", "majorVersion": 17 },
  "downloads": {
    "client": { "sha1": "abc", "size": 100, "url": "https://example.invalid/client.jar" },
    "client_mappings": { "sha1": "def", "size": 50, "url": "https://example.invalid/mappings.txt" }
  },
  "arguments": {
    "game": [
      "--username",
      "${auth_player_name}",
      { "rules": [{ "action": "allow", "features": { "has_custom_resolution": true } }],
        "value": ["--width", "${resolution_width}"] }
    ],
    "jvm": [
      { "rules": [{ "action": "allow", "os": { "name": "windows", "version": "^10\\." } }],
        "value": "-Dos.name=Windows 10" }
    ]
  },
  "libraries": [
    { "name": "com.mojang:logging:1.1.1",
      "downloads": { "artifact": { "path": "com/mojang/logging/1.1.1/logging-1.1.1.jar",
                                   "sha1": "aaa", "size": 10,
                                   "url": "https://example.invalid/logging.jar" } } }
  ],
  "logging": {
    "client": { "argument": "-Dlog4j.configurationFile=${path}",
                "file": { "id": "client-1.12.xml", "sha1": "bbb", "size": 20,
                          "url": "https://example.invalid/log4j.xml" },
                "type": "log4j2-xml" }
  },
  "tlauncherVersion": 1,
  "MMC-hint": "local"
}"#;

#[test]
fn parses_full_version_json() {
    let version = parse(VANILLA_1_20_1);

    assert_eq!(version.id.as_deref(), Some("1.20.1"));
    assert_eq!(version.version_type, Some(ReleaseType::Release));
    assert_eq!(
        version.main_class.as_deref(),
        Some("net.minecraft.client.main.Main")
    );
    assert_eq!(version.compliance_level, Some(1));
    assert_eq!(version.minimum_launcher_version, Some(21));
    assert_eq!(version.assets_id(), "5");

    let asset_index = version.asset_index.as_ref().expect("assetIndex 应解析");
    assert_eq!(asset_index.id, "5");
    assert_eq!(asset_index.total_size, Some(598_385_820));

    let java = version.java_version.as_ref().expect("javaVersion 应解析");
    assert_eq!(java.component.as_deref(), Some("java-runtime-gamma"));
    assert_eq!(java.major_version, Some(17));

    let downloads = version.downloads.as_ref().expect("downloads 应解析");
    assert_eq!(downloads.len(), 2, "未知键也要保留在映射里");
    assert_eq!(
        downloads.client().and_then(|c| c.url.as_deref()),
        Some("https://example.invalid/client.jar")
    );
    assert!(downloads.get("client_mappings").is_some());
}

#[test]
fn keeps_unknown_fields_verbatim() {
    let version = parse(VANILLA_1_20_1);
    // 第三方启动器私有字段（rawJson 保真，analyse/03 §4.2）。
    assert_eq!(
        version.extra.get("tlauncherVersion"),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        version.extra.get("MMC-hint"),
        Some(&serde_json::json!("local"))
    );

    let roundtrip = version.to_json_string().expect("应可序列化");
    let reparsed = VersionJson::from_json_str(&roundtrip).expect("应可再解析");
    assert_eq!(reparsed.extra, version.extra);
}

#[test]
fn parses_arguments_with_rules() {
    let version = parse(VANILLA_1_20_1);
    let arguments = version.arguments.as_ref().expect("arguments 应解析");

    assert_eq!(arguments.game.len(), 3);
    assert_eq!(arguments.jvm.len(), 1);
    assert!(matches!(&arguments.game[0], Argument::Plain(value) if value == "--username"));

    let conditional = match &arguments.game[2] {
        Argument::Conditional(cond) => cond,
        Argument::Plain(_) => panic!("第三项应是条件参数"),
    };
    assert_eq!(conditional.rules.len(), 1);
    assert_eq!(conditional.rules[0].action, RuleAction::Allow);
    assert_eq!(
        conditional.rules[0]
            .features
            .as_ref()
            .and_then(|f| f.get("has_custom_resolution")),
        Some(&true)
    );
    assert_eq!(
        conditional.value,
        ArgumentValue::Many(vec!["--width".into(), "${resolution_width}".into()])
    );

    // jvm 里的 os 限制带正则版本模式。
    let jvm = match &arguments.jvm[0] {
        Argument::Conditional(cond) => cond,
        Argument::Plain(_) => panic!("jvm 第一项应是条件参数"),
    };
    let os = jvm.rules[0].os.as_ref().expect("应有 os 限制");
    assert_eq!(os.name.as_deref(), Some("windows"));
    assert_eq!(os.version.as_deref(), Some("^10\\."));
}

#[test]
fn parses_libraries_and_logging() {
    let version = parse(VANILLA_1_20_1);
    let libraries = version.libraries.as_ref().expect("libraries 应解析");
    assert_eq!(libraries.len(), 1);
    let artifact = libraries[0].artifact().expect("应有 artifact");
    assert_eq!(
        artifact.path.as_deref(),
        Some("com/mojang/logging/1.1.1/logging-1.1.1.jar")
    );

    let logging = version.logging.as_ref().expect("logging 应解析");
    let client = logging.client().expect("应有 client 日志配置");
    assert_eq!(client.kind.as_deref(), Some("log4j2-xml"));
    assert_eq!(client.file.id.as_deref(), Some("client-1.12.xml"));
}

#[test]
fn tolerant_defaults_for_minimal_json() {
    let version = parse(r#"{ "id": "b1.7.3" }"#);
    assert_eq!(version.id.as_deref(), Some("b1.7.3"));
    assert_eq!(version.version_type, None, "未知 type 不报错");
    assert_eq!(version.client_jar(), None, "老 JSON 没有 downloads.client");
    assert_eq!(version.assets_id(), "legacy", "assets 缺失按 legacy");
    assert!(!version.is_resolved());
}

#[test]
fn unknown_type_value_falls_back_to_unknown() {
    let version = parse(r#"{ "id": "x", "type": "some_new_kind" }"#);
    assert_eq!(version.version_type, Some(ReleaseType::Unknown));
}

#[test]
fn missing_id_is_reported() {
    let version = parse(r#"{ "mainClass": "a.B" }"#);
    let error = version.require_id().expect_err("缺 id 应报错");
    assert_eq!(
        error.to_string(),
        "版本 JSON 缺少 id",
        "错误文案要给用户看得懂"
    );
}
