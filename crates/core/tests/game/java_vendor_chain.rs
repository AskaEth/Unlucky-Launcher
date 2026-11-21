//! D23 发行版回退链：地区默认序、用户优先级覆盖、能力过滤、mirrors v3 三张新表。
//!
//! 全部离线：只跑纯函数与清单解析，不发一个请求。
//! 真机验收（TUNA 装 17 / 华为云装 Oracle 21 / `%APPDATA%` 落点）见 mission 报告。

use unlucky_core::download::MirrorTable;
use unlucky_core::game::java::install::chain_order;
use unlucky_core::game::java::release::{parse_sha256_text, JdkArchive, JdkRelease};
use unlucky_core::game::java::vendor::{ids, order, parse_priority, JdkVendor, VENDOR_IDS};
use unlucky_core::game::java::InstallRequest;

fn vendors(items: &[&str]) -> Vec<JdkVendor> {
    parse_priority(
        &items
            .iter()
            .map(|item| (*item).to_string())
            .collect::<Vec<_>>(),
    )
}

#[test]
fn domestic_default_is_temurin_oracle_microsoft_zulu() {
    assert_eq!(
        ids(&order(&[], true)),
        vec!["temurin", "oracle", "microsoft", "zulu"],
        "国内：Oracle 国内镜像 11.5MB/s vs MS 410KB/s（D23 实测修正）"
    );
    assert_eq!(
        ids(&order(&[], false)),
        vec!["temurin", "microsoft", "oracle", "zulu"],
        "国外：维持用户原话 Temurin→MS→Oracle"
    );
}

#[test]
fn setting_priority_overrides_the_region_default_but_never_drops_a_vendor() {
    let priority = vendors(&["microsoft"]);
    assert_eq!(
        ids(&order(&priority, true)),
        vec!["microsoft", "temurin", "oracle", "zulu"]
    );
    // 写全四家也等于顺序覆盖。
    let full = vendors(&["zulu", "microsoft", "oracle", "temurin"]);
    assert_eq!(
        ids(&order(&full, false)),
        vec!["zulu", "microsoft", "oracle", "temurin"]
    );
    // 未知值被忽略（老配置里可能残留 graalvm 之类）。
    let messy = vendors(&["graalvm", "oracle"]);
    assert_eq!(
        ids(&order(&messy, true)),
        vec!["oracle", "temurin", "microsoft", "zulu"]
    );
}

#[test]
fn capability_filter_drops_microsoft_and_oracle_for_java_8() {
    // Java 8：MS 已停发、Oracle archive 从 9 起 → 链里只剩 Temurin 与 Zulu。
    assert_eq!(
        ids(&chain_order(&InstallRequest::new(8), true)),
        vec!["temurin", "zulu"]
    );
    // Java 21：四家都发，顺序完整。
    assert_eq!(
        ids(&chain_order(&InstallRequest::new(21), true)),
        vec!["temurin", "oracle", "microsoft", "zulu"]
    );
    // 钉死一家：链里只放它，即使它不支持这个大版本（由 install 报明确原因，不偷偷换家）。
    let pinned = InstallRequest::with_vendor(8, JdkVendor::Microsoft);
    assert_eq!(
        ids(&chain_order(&pinned, true)),
        vec!["microsoft"],
        "--vendor 不做能力过滤：用户点名的那家要显式失败"
    );
}

#[test]
fn pinned_vendor_wins_over_the_user_priority() {
    let request =
        InstallRequest::with_vendor(17, JdkVendor::Oracle).with_priority(vendors(&["zulu"]));
    assert_eq!(ids(&request.order(true)), vec!["oracle"]);
    let chained = InstallRequest::new(17).with_priority(vendors(&["zulu"]));
    assert_eq!(
        ids(&chained.order(true)),
        vec!["zulu", "temurin", "oracle", "microsoft"]
    );
}

#[test]
fn mirror_table_v3_registers_all_four_source_families() {
    let table = MirrorTable::builtin();
    assert_eq!(table.version, 3);
    // adoptium 收敛：只有 TUNA 启用，其余六家登记为 disabled。
    assert_eq!(
        table
            .adoptium
            .iter()
            .filter(|mirror| mirror.enabled)
            .map(|mirror| mirror.id.as_str())
            .collect::<Vec<_>>(),
        vec!["tuna"]
    );
    assert_eq!(table.adoptium.len(), 7, "TUNA + USTC/SJTU/BFSU/NJU/ZJU/163");
    // 华为云的假阳性不再登记成 adoptium 源（它没有 Adoptium 镜像）。
    assert!(table
        .adoptium
        .iter()
        .all(|mirror| mirror.id != "huaweicloud"));
    // Oracle：华为云在前、南大在后（顺序即默认尝试顺序）。
    assert_eq!(
        table
            .oracle_mirrors()
            .map(|mirror| mirror.id.as_str())
            .collect::<Vec<_>>(),
        vec!["huawei", "nju"]
    );
    // MS：模板 + 无国内镜像 + 四个大版本。
    let microsoft = table.microsoft_openjdk().expect("MS 表");
    assert!(microsoft.domestic_mirrors.is_empty());
    assert_eq!(microsoft.majors, vec![25, 21, 17, 11]);
    // Zulu：size 不可信，只信 sha256。
    assert!(!table.zulu().expect("Zulu 表").size_field_reliable);
}

#[test]
fn every_vendor_release_shape_carries_its_own_version_semantics() {
    // 四家的版本串并列：目录名、戳记、--json 都直接用 vendor 原生串。
    let samples = [
        (JdkVendor::Temurin, "21.0.13+9"),
        (JdkVendor::Oracle, "21.0.2"),
        (JdkVendor::Microsoft, "21.0.13.0.2"),
        (JdkVendor::Zulu, "21.0.13+3"),
    ];
    for (vendor, version) in samples {
        let release = JdkRelease {
            vendor,
            major: 21,
            version: version.to_string(),
            semver: None,
            release_name: version.to_string(),
            archive: JdkArchive {
                name: format!("{}-{version}.zip", vendor.id()),
                url: format!("https://example.invalid/{}-{version}.zip", vendor.id()),
                sha256: "a".repeat(64),
                size: 0,
            },
        };
        assert!(release.matches(vendor, 21));
        assert!(release.summary().contains(version), "{}", release.summary());
        assert_eq!(release.archive_json()["vendor"], vendor.id());
        assert_eq!(
            unlucky_core::game::java::sanitize_version(&release.version),
            version,
            "版本串要能直接当目录名"
        );
    }
}

#[test]
fn sha256_texts_are_parsed_the_same_way_for_every_upstream() {
    let hash = "b6c17e747ae78cdd6de4d7532b3164b277daee97c007d3eaa2b39cca99882664";
    // 华为云 .sha256：纯哈希。
    assert_eq!(parse_sha256_text(hash).as_deref(), Some(hash));
    // MS .sha256sum.txt：`<hash>  <filename>`（大写 + CRLF）。
    assert_eq!(
        parse_sha256_text(&format!(
            "{}  microsoft-jdk-21.0.13-windows-x64.zip\r\n",
            hash.to_uppercase()
        ))
        .as_deref(),
        Some(hash)
    );
    assert_eq!(parse_sha256_text("404 not found"), None);
}

#[test]
fn vendor_ids_are_the_setting_whitelist() {
    assert_eq!(
        VENDOR_IDS,
        &["temurin", "oracle", "microsoft", "zulu", "termux"]
    );
    for id in VENDOR_IDS {
        assert_eq!(JdkVendor::parse(id).map(|vendor| vendor.id()), Some(*id));
    }
    assert!(JdkVendor::parse("graalvm").is_none());
}
