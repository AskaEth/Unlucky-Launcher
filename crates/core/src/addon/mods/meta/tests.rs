//! 三种元数据文件的解析用例（文本级：不建 jar，直接喂字符串）。
//!
//! jar 这一层（zip 限额、条目挑选、读不到时的降级）在 `mods/tests/moddup.rs`。

use super::*;

// 两个解析器都是 `meta` 的私有子模块：测试是它的子模块，所以能直接调（glob 之外再显式列一次，
// 免得将来有人把 `mod forge;` 挪走时这一行先红而不是用例先红）。
use super::{forge, json};

const FABRIC: &str = r#"{
  "schemaVersion": 1,
  "id": "sodium",
  "version": "0.5.11+mc1.20.1",
  "name": "Sodium",
  "depends": {
    "fabricloader": ">=0.14",
    "minecraft": "~1.20",
    "fabric-api": ">=0.92.0"
  },
  "recommends": {
    "sodium-extra": "*",
    "indium": [">=1.0.30", "<2.0"]
  },
  "suggests": { "modmenu": "*" },
  "breaks": { "optifabric": "*" }
}"#;

/// Fabric：`depends` = 必需、`recommends` = 可选，`suggests` / `breaks` 不收。
#[test]
fn fabric_reads_id_version_and_both_kinds_of_prerequisites() {
    let meta = json::fabric(FABRIC).expect("认得出来");
    assert_eq!(meta.id, "sodium");
    assert_eq!(meta.name.as_deref(), Some("Sodium"));
    assert_eq!(meta.version.as_deref(), Some("0.5.11+mc1.20.1"));
    assert_eq!(meta.source, MetaSource::Fabric);

    let dep = |id: &str| {
        meta.deps
            .iter()
            .find(|dep| dep.id == id)
            .unwrap_or_else(|| panic!("{id} 不在前置里：{:?}", meta.deps))
    };
    assert!(dep("fabric-api").mandatory, "`depends` 是必需");
    assert_eq!(dep("fabric-api").version_range.as_deref(), Some(">=0.92.0"));
    assert!(!dep("sodium-extra").mandatory, "`recommends` 是可选");
    assert_eq!(
        dep("indium").version_range.as_deref(),
        Some(">=1.0.30, <2.0"),
        "数组形态的范围原文要拼起来（只为人读）"
    );
    assert!(
        meta.deps.iter().all(|dep| dep.id != "modmenu"),
        "`suggests` 不收"
    );
    assert!(
        meta.deps.iter().all(|dep| dep.id != "optifabric"),
        "`breaks` 不是前置"
    );
}

/// Fabric：`id` 是身份，丢了就不认（不拿文件名凑）；构建期占位符版本算「不知道」。
#[test]
fn fabric_needs_an_id_and_rejects_unfilled_versions() {
    assert!(json::fabric(r#"{"version":"1.0"}"#).is_none());
    assert!(json::fabric(r#"{"id":"   ","version":"1.0"}"#).is_none());
    assert!(json::fabric("not json at all").is_none());
    let meta = json::fabric(r#"{"id":"x","version":"${file.jarVersion}"}"#).expect("id 有就行");
    assert_eq!(meta.version, None, "没被替换掉的占位符不是版本");
}

const MODS_TOML: &str = r#"
modLoader="javafml"
loaderVersion="[47,)"
license="MIT"
# 一个注释行，整行忽略

[[mods]]
modId="jei"
version="15.2.0.27"
displayName="Just Enough Items"
description='''多行描述里也可能出现 = 号与 # 号，但我们不解多行串'''

[[dependencies.jei]]
    modId="minecraft"
    mandatory=true
    versionRange="[1.20.1,1.21)"
    ordering="NONE"
    side="BOTH"

[[dependencies.jei]]
    modId="cloth-config"
    mandatory=false
    versionRange="[11,)"
    ordering="AFTER"

[[dependencies.jei]]
    modId="optifine"
    mandatory=true
    type="incompatible"

[[dependencies.othermod]]
    modId="not-ours"
    mandatory=true
"#;

/// Forge / NeoForge：`[[mods]]` 给身份，`[[dependencies.<自己>]]` 给前置，
/// `mandatory` 决定必需位，`incompatible` 直接丢掉。
#[test]
fn forge_reads_identity_and_mandatory_flags() {
    let meta = forge::mods_toml(MODS_TOML).expect("认得出来");
    assert_eq!(meta.id, "jei");
    assert_eq!(meta.name.as_deref(), Some("Just Enough Items"));
    assert_eq!(meta.version.as_deref(), Some("15.2.0.27"));

    let dep = |id: &str| {
        meta.deps
            .iter()
            .find(|dep| dep.id == id)
            .unwrap_or_else(|| panic!("{id} 不在前置里：{:?}", meta.deps))
    };
    assert!(dep("minecraft").mandatory);
    assert_eq!(
        dep("minecraft").version_range.as_deref(),
        Some("[1.20.1,1.21)")
    );
    assert!(!dep("cloth-config").mandatory, "`mandatory=false` 是可选");
    assert!(
        meta.deps.iter().all(|dep| dep.id != "optifine"),
        "`type=incompatible` 是冲突声明，不是前置"
    );
    assert!(
        meta.deps.iter().all(|dep| dep.id != "not-ours"),
        "别人那块 `[[dependencies.othermod]]` 不算我们的前置"
    );
}

/// `type` 也表达必需位（没有 `mandatory` 键时的兜底判据）。
#[test]
fn forge_falls_back_to_the_type_field() {
    let text = r#"
[[mods]]
modId="x"
[[dependencies.x]]
modId="lib"
type="required"
[[dependencies.x]]
modId="nice-to-have"
type="optional"
"#;
    let meta = forge::mods_toml(text).expect("认得出来");
    let dep = |id: &str| {
        meta.deps
            .iter()
            .find(|dep| dep.id == id)
            .unwrap_or_else(|| panic!("{id} 缺失"))
            .mandatory
    };
    assert!(dep("lib"));
    assert!(!dep("nice-to-have"));
}

/// 认不出的 TOML 不是错误：没有 `[[mods]]` / 没有 `modId` / 截断的文件都只给 `None`。
#[test]
fn forge_degrades_on_everything_it_does_not_understand() {
    assert!(forge::mods_toml("").is_none());
    assert!(forge::mods_toml("[[mods]]\ndisplayName=\"没有 id\"\n").is_none());
    // 多行串被截断（导入夹具里真出现过这种文件）：不能 panic；`modId` 也读不全 ⇒ 认不出。
    assert!(forge::mods_toml("[[mods]]\nmodId=\"x\nversion=\"").is_none());
    // 表里只有第二个 `[[mods]]` 的身份也不该被当作主 mod。
    let meta = forge::mods_toml("[[mods]]\nmodId=\"first\"\n[[mods]]\nmodId=\"second\"\n")
        .expect("第一个 [[mods]] 作数");
    assert_eq!(meta.id, "first");
}

const LEGACY: &str = r#"[{
  "modid": "buildcraft",
  "name": "BuildCraft",
  "version": "7.99.24",
  "requiredMods": ["forge@[14.23.5,)", "codechickenlib"],
  "dependencies": ["@requiredlib", "optionallib@1.0"]
}]"#;

/// 旧版 `mcmod.info`：`requiredMods` 必是必需，`dependencies` 里 `@` 前缀的也算必需。
#[test]
fn legacy_reads_required_and_dependency_entries() {
    let meta = json::legacy(LEGACY).expect("认得出来");
    assert_eq!(meta.id, "buildcraft");
    assert_eq!(meta.version.as_deref(), Some("7.99.24"));

    let dep = |id: &str| {
        meta.deps
            .iter()
            .find(|dep| dep.id == id)
            .unwrap_or_else(|| panic!("{id} 不在前置里：{:?}", meta.deps))
    };
    assert!(dep("forge").mandatory);
    assert_eq!(dep("forge").version_range.as_deref(), Some("[14.23.5,)"));
    assert!(dep("codechickenlib").mandatory, "`requiredMods` 都是必需");
    assert!(dep("requiredlib").mandatory, "`@` 前缀 = 其实必需");
    assert!(!dep("optionallib").mandatory);
    assert_eq!(dep("optionallib").version_range.as_deref(), Some("1.0"));
}

/// `{"modList": [...]}` 那种包装也要认（1.12 era 的另一种写法）。
#[test]
fn legacy_accepts_the_wrapped_shape() {
    let text = r#"{"modListVersion": 2, "modList": [{"modid": "x", "version": "1.0"}]}"#;
    assert_eq!(json::legacy(text).expect("认得出来").id, "x");
    assert!(json::legacy(r#"{"modList": []}"#).is_none());
    assert!(json::legacy("[]").is_none());
}

/// 「读不到」的分级：不是 zip / 没有元数据是**正常**的，另三种才可疑。
#[test]
fn only_real_failures_are_suspicious() {
    assert!(!MetaGap::NotAJar.is_suspicious());
    assert!(!MetaGap::NoMetadata.is_suspicious());
    for gap in [MetaGap::Unreadable, MetaGap::TooBig, MetaGap::Malformed] {
        assert!(gap.is_suspicious(), "{gap:?}");
        assert!(!gap.as_str().is_empty());
    }
    assert_eq!(MetaSource::Fabric.file(), FABRIC_META);
    assert_eq!(MetaSource::NeoForge.file(), NEOFORGE_META);
    assert_eq!(MetaSource::Forge.file(), FORGE_META);
    assert_eq!(MetaSource::Legacy.file(), LEGACY_META);
}
