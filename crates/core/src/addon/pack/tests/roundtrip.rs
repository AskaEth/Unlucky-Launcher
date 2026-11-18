//! 读回模型 + "导出后再读"的往返一致性：**未知字段不丢**就在这些断言里被钉住。

use serde_json::json;

use super::fixtures;
use crate::addon::pack::archive::PackArchive;
use crate::addon::pack::curseforge::CURSEFORGE;
use crate::addon::pack::error::PackErrorKind;
use crate::addon::pack::format::{PackFormat, PackPayload, PackRegistry};
use crate::addon::pack::manifest::{EnvSupport, PackManifest, PackSide};
use crate::addon::pack::modrinth::MODRINTH;
use crate::addon::pack::multimc::MULTIMC;
use crate::addon::LoaderKind;
use crate::download::{verify, HashAlgo};

fn sha1_hex(bytes: &[u8]) -> String {
    verify::digest_bytes(HashAlgo::Sha1, bytes)
}

#[test]
fn curseforge_reads_into_model() {
    let fixture = fixtures::curseforge("cf-read");
    let pack = PackArchive::open_default(&fixture.zip).expect("打开 CF 夹具");
    let manifest = CURSEFORGE.read(&pack).expect("读 CF 清单");

    assert_eq!(manifest.name, fixtures::CF_NAME);
    assert_eq!(manifest.version.as_deref(), Some("1.0.0"));
    assert_eq!(manifest.author.as_deref(), Some("AskaEth"));
    assert_eq!(manifest.summary, None, "CF 没有 summary 字段");
    assert_eq!(manifest.minecraft.as_deref(), Some("1.20.1"));
    assert_eq!(manifest.game, None, "CF 没有 game 字段");
    assert_eq!(manifest.format_version, Some(1));

    assert_eq!(manifest.loaders.len(), 1);
    let loader = &manifest.loaders[0];
    assert_eq!(loader.raw, "forge-47.2.0");
    assert_eq!(loader.kind, Some(LoaderKind::Forge));
    assert_eq!(loader.version.as_deref(), Some("47.2.0"));
    assert!(loader.primary, "primary=true 必须保住");

    // CF 官方 files[] 没有路径/URL/摘要：模型如实记成 None，不猜。
    assert_eq!(manifest.files.len(), 2);
    let required = &manifest.files[0];
    assert_eq!(required.project_id, Some(238222));
    assert_eq!(required.file_id, Some(5102025));
    assert_eq!(required.path, None);
    assert_eq!(required.client, EnvSupport::Required);
    assert_eq!(required.key(), "curseforge:238222:5102025");
    let optional = &manifest.files[1];
    assert_eq!(
        optional.client,
        EnvSupport::Optional,
        "required=false → Optional"
    );
    assert_eq!(optional.server, EnvSupport::Optional);

    // 覆盖目录：3 个文件，含那个刻意截断的 TOML（它没有清单摘要可比，只能记实测值）。
    assert_eq!(manifest.overrides.len(), 1);
    let group = &manifest.overrides[0];
    assert_eq!(group.dir, "overrides");
    assert_eq!(group.side, PackSide::Both);
    assert_eq!(group.files.len(), 3);
    let broken = group.find("config/broken.toml").expect("坏文件也要进清单");
    assert_eq!(broken.size, fixtures::CF_BROKEN.len() as u64);
    assert_eq!(broken.sha1, sha1_hex(fixtures::CF_BROKEN));

    // 未知字段：顶层与 `minecraft` 对象各一处。
    assert_eq!(manifest.extra["customCommunityField"]["since"], json!(2));
    assert_eq!(manifest.minecraft_extra["recommendedRam"], json!(4096));
}

#[test]
fn curseforge_export_round_trips_unknown_fields() {
    let fixture = fixtures::curseforge("cf-roundtrip");
    let pack = PackArchive::open_default(&fixture.zip).expect("打开 CF 夹具");
    let manifest = CURSEFORGE.read(&pack).expect("读 CF 清单");

    let target = fixture.zip.with_file_name("rewritten.zip");
    CURSEFORGE
        .write(&manifest, PackPayload::from_root(&fixture.dir), &target)
        .expect("导出 CF 包");

    let written = PackArchive::open_default(&target).expect("打开导出的 CF 包");
    let reread = CURSEFORGE.read(&written).expect("再读导出的 CF 包");
    assert_eq!(reread, manifest, "导出→再读必须逐字段相等（含未知字段）");
    assert_eq!(
        reread.extra["customCommunityField"]["note"],
        json!("社区新增字段，必须原样回写")
    );
    assert_eq!(reread.minecraft_extra["recommendedRam"], json!(4096));
}

#[test]
fn modrinth_reads_into_model() {
    let fixture = fixtures::modrinth("mr-read");
    let pack = PackArchive::open_default(&fixture.zip).expect("打开 mrpack 夹具");
    let manifest = MODRINTH.read(&pack).expect("读 mrpack 清单");

    assert_eq!(manifest.name, fixtures::MR_NAME);
    assert_eq!(manifest.version.as_deref(), Some("2.0.0"));
    assert_eq!(
        manifest.summary.as_deref(),
        Some("手造夹具：只验证形状与边界，不是真实导出")
    );
    assert_eq!(manifest.game.as_deref(), Some("minecraft"));
    assert_eq!(manifest.minecraft.as_deref(), Some("1.20.1"));
    assert_eq!(manifest.author, None, "mrpack 没有 author 字段");

    // `dependencies` 里非 minecraft 的每条都成一条加载器依赖（认不出的也保留）。
    assert_eq!(manifest.loaders.len(), 2);
    let fabric = manifest
        .loader(LoaderKind::Fabric)
        .expect("要认出 fabric-loader");
    assert_eq!(fabric.raw, "fabric-loader");
    assert_eq!(fabric.version.as_deref(), Some("0.15.11"));
    assert!(fabric.primary);
    let parchment = manifest
        .loaders
        .iter()
        .find(|loader| loader.raw == "parchment")
        .expect("社区键也要保留");
    assert_eq!(parchment.kind, None);
    assert_eq!(parchment.version.as_deref(), Some("2023.09.03"));

    assert_eq!(manifest.files.len(), 2);
    let jei = &manifest.files[0];
    assert_eq!(jei.path.as_deref(), Some("mods/jei.jar"));
    assert_eq!(jei.size, Some(1_234_567));
    assert_eq!(jei.client, EnvSupport::Required);
    assert_eq!(jei.server, EnvSupport::Unsupported);
    assert_eq!(jei.downloads.len(), 1);
    assert!(jei.hashes.contains_key("sha256"), "整张摘要表都要留着");
    assert!(jei.sha512().is_some());
    assert!(jei.key().starts_with("modrinth:"));
    assert_eq!(jei.extra["customFileField"], json!("社区新增字段"));
    let optional = &manifest.files[1];
    assert!(optional.client.is_optional(), "env.client=optional 要保住");

    // 三个覆盖目录各成一侧。
    assert_eq!(manifest.overrides.len(), 3);
    let both = manifest.overrides_for(PackSide::Both).expect("overrides");
    assert_eq!(both.files.len(), 2);
    let client = manifest
        .overrides_for(PackSide::Client)
        .expect("client-overrides");
    assert_eq!(client.files.len(), 1);
    let server = manifest
        .overrides_for(PackSide::Server)
        .expect("server-overrides");
    assert_eq!(server.files.len(), 1);
    let half = both.find("mods/half.jar").expect("半截 jar 也要进清单");
    assert_eq!(half.size, fixtures::MR_HALF_JAR.len() as u64);
    assert_eq!(half.sha1, sha1_hex(fixtures::MR_HALF_JAR));

    assert_eq!(manifest.extra["customPackField"], json!([1, 2, 3]));
}

#[test]
fn modrinth_export_round_trips_unknown_fields() {
    let fixture = fixtures::modrinth("mr-roundtrip");
    let pack = PackArchive::open_default(&fixture.zip).expect("打开 mrpack 夹具");
    let manifest = MODRINTH.read(&pack).expect("读 mrpack 清单");

    let target = fixture.zip.with_file_name("rewritten.mrpack");
    MODRINTH
        .write(&manifest, PackPayload::from_root(&fixture.dir), &target)
        .expect("导出 mrpack");

    let written = PackArchive::open_default(&target).expect("打开导出的 mrpack");
    let reread = MODRINTH.read(&written).expect("再读导出的 mrpack");
    assert_eq!(reread, manifest, "导出→再读必须逐字段相等（含未知字段）");
    assert_eq!(reread.extra["customPackField"], json!([1, 2, 3]));
}

#[test]
fn multimc_reads_instance_directory() {
    let root = fixtures::multimc("mmc-read");
    let pack = PackArchive::open_default(&root).expect("打开 MultiMC 实例目录");
    assert!(pack.is_dir_source());
    let manifest = MULTIMC.read(&pack).expect("读 MultiMC 清单");

    assert_eq!(manifest.name, fixtures::MMC_NAME, "包名来自 instance.cfg");
    assert_eq!(manifest.minecraft.as_deref(), Some("1.20.1"));
    assert_eq!(manifest.game.as_deref(), Some("minecraft"));
    assert_eq!(manifest.format_version, Some(1));
    assert_eq!(manifest.version, None, "MultiMC 的实例没有包版本");

    let fabric = manifest
        .loader(LoaderKind::Fabric)
        .expect("组件 uid 要翻译成加载器");
    assert_eq!(fabric.raw, "net.fabricmc.fabric-loader");
    assert_eq!(fabric.version.as_deref(), Some("0.15.11"));
    assert!(fabric.primary);
    assert!(
        manifest
            .loaders
            .iter()
            .any(|loader| loader.raw == "org.lwjgl3"),
        "认不出的组件也要保留"
    );

    assert!(manifest.files.is_empty(), "MultiMC 没有远端条目");
    let content = manifest
        .overrides_for(PackSide::Both)
        .expect(".minecraft 当覆盖目录");
    assert_eq!(content.dir, ".minecraft");
    let broken = content
        .find("mods/broken.jar")
        .expect("零字节 jar 也要进清单");
    assert_eq!(broken.size, 0);
    assert_eq!(broken.sha1, sha1_hex(b""));
    assert_eq!(content.files.len(), 3);
    assert_eq!(manifest.extra["customComponentGraphField"], json!(true));
}

#[test]
fn multimc_export_is_refused() {
    let root = super::TempRoot::new("mmc-write");
    let target = root.join("out.zip");
    let manifest = PackManifest::new("whatever");
    let error = MULTIMC
        .write(&manifest, PackPayload::metadata_only(), &target)
        .expect_err("MultiMC 格式不支持导出");
    assert_eq!(error.kind(), PackErrorKind::Unsupported);
    assert!(!target.exists(), "拒绝导出时不该留下半个文件");
}

#[test]
fn registry_read_reports_matched_format() {
    let fixture = fixtures::curseforge("registry-read");
    let pack = PackArchive::open_default(&fixture.zip).expect("打开 CF 夹具");
    let (format, manifest) = PackRegistry::builtin().read(&pack).expect("探测 + 读");
    assert_eq!(format.id(), "curseforge");
    assert_eq!(manifest.name, fixtures::CF_NAME);
}

#[test]
fn curseforge_without_name_is_a_metadata_error() {
    let root = super::TempRoot::new("cf-noname");
    let target = root.join("nope.zip");
    super::write_zip(
        &target,
        &[(
            "manifest.json",
            &br#"{"manifestType":"minecraftModpack","minecraft":{"version":"1.20.1"}}"#[..],
        )],
    );
    let pack = PackArchive::open_default(&target).expect("zip 本身可读");
    assert_eq!(
        CURSEFORGE.detect(&pack).expect("清单可读"),
        Some("manifest.json"),
        "形状对得上就该被探测到"
    );
    let error = CURSEFORGE.read(&pack).expect_err("缺 name 必须报错");
    assert_eq!(error.kind(), PackErrorKind::Metadata);
}

#[test]
fn disabled_entries_export_as_path_suffix_on_mrpack_and_required_false_on_curseforge() {
    // 禁用位（M115 调研 §字段缺口 2 / §8.1）在两边的落法不同，且都不该靠"猜"：
    // mrpack → `files[].path` 结尾 `.disabled`（官方导出的表达，**不是** env.client=optional：
    // 后者只是"安装时可取消勾选"，装回来默认仍然启用 ⇒ 跨启动器不保真）；
    // CF → 没有原生表达，格式层只能写 `required: false`（Prism 同款）；导出**管线**更严：
    // 禁用文件直接内联进 overrides/ 并保留后缀（见 export::pipeline 的 DisabledNotExpressible）。
    let cf = fixtures::curseforge("disabled-cf");
    let pack = PackArchive::open_default(&cf.zip).expect("打开 CF 夹具");
    let mut manifest = CURSEFORGE.read(&pack).expect("读 CF 清单");
    assert!(!manifest.files[0].disabled, "读包不会凭空造出禁用位");
    manifest.files[0].disabled = true;
    let target = cf.zip.with_file_name("disabled.zip");
    CURSEFORGE
        .write(&manifest, PackPayload::from_root(&cf.dir), &target)
        .expect("导出 CF");
    let written = PackArchive::open_default(&target).expect("打开导出的 CF 包");
    let reread = CURSEFORGE.read(&written).expect("再读 CF");
    assert_eq!(
        reread.files[0].client,
        EnvSupport::Optional,
        "CF 没有禁用表达，格式层只能落 required=false"
    );

    let mr = fixtures::modrinth("disabled-mr");
    let pack = PackArchive::open_default(&mr.zip).expect("打开 mrpack 夹具");
    let mut manifest = MODRINTH.read(&pack).expect("读 mrpack 清单");
    assert!(
        !manifest.files[1].disabled,
        "env.client=optional 是「可选」，不是「禁用」：两个事实不合并"
    );
    manifest.files[0].disabled = true;
    let target = mr.zip.with_file_name("disabled.mrpack");
    MODRINTH
        .write(&manifest, PackPayload::from_root(&mr.dir), &target)
        .expect("导出 mrpack");
    let written = PackArchive::open_default(&target).expect("打开导出的 mrpack");
    let reread = MODRINTH.read(&written).expect("再读 mrpack");
    assert_eq!(
        reread.files[0].path.as_deref(),
        Some("mods/jei.jar.disabled"),
        "禁用靠路径后缀表达（官方导出的做法，跨启动器保真）"
    );
    assert_eq!(
        reread.files[0].client,
        EnvSupport::Required,
        "禁用不再被翻译成 env.client=optional（tower 定案）"
    );
}
