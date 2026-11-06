//! CurseForge `manifest.json` 端到端导入（离线，stub fetcher 充当 CF API + CDN）。
//!
//! 夹具是 M110 的手造包（`fixtures::curseforge`）：`overrides/` 里有一个**刻意的截断 TOML**
//! （`config/broken.toml`）与一个假 jar。导入器不许因为坏内容崩，也不许悄悄丢掉它们。

use crate::addon::import::{import_pack, ImportRequest, ImportStatus};
use crate::addon::loader::LoaderKind;
use crate::download::blocking::block_on;

use super::support::{curse_meta, StubFetcher, CURSE_URL};
use super::{fixtures, list_tree, read_text, temp_root};

/// CF 夹具里那两个条目补全出来的内容。
const CURSE_BYTES: &[u8] = b"stub curse file bytes";

#[test]
fn imports_curseforge_pack_end_to_end() {
    let fixture = fixtures::curseforge("cf-e2e");
    let target = temp_root("cf-e2e-out").join("instance");
    let fetcher = StubFetcher::new()
        .with_curse(238222, 5102025, curse_meta("jei.jar", CURSE_BYTES))
        .with_curse(306612, 4587454, curse_meta("fabric-api.jar", CURSE_BYTES))
        .with_file(CURSE_URL, CURSE_BYTES);

    let request = ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher);
    let report = block_on(import_pack(&request));

    assert_eq!(
        report.status,
        ImportStatus::Success,
        "状态 {:?}，缺文件 {:?}，冲突 {:?}，错误 {:?}",
        report.status,
        report.missing,
        report.conflicts,
        report.error
    );
    assert_eq!(report.format.as_deref(), Some("curseforge"));
    assert_eq!(report.pack_name.as_deref(), Some(fixtures::CF_NAME));
    assert_eq!(report.pack_version.as_deref(), Some("1.0.0"));
    assert_eq!(report.game_version.as_deref(), Some("1.20.1"));
    assert!(!report.rolled_back);

    // 加载器被翻译成 addon::loader 的三元组（不另写一套安装逻辑）。
    let loader = report.loader.as_ref().expect("CF 夹具带 forge");
    assert_eq!(loader.kind, LoaderKind::Forge);
    assert_eq!(loader.loader_version, "47.2.0");
    assert_eq!(loader.version_id, "1.20.1-forge-47.2.0");
    assert_eq!(
        loader.resolve_args(),
        (LoaderKind::Forge, "1.20.1", "47.2.0")
    );
    assert_eq!(report.version_id.as_deref(), Some("1.20.1-forge-47.2.0"));

    // 覆盖内容（`overrides/` 的内容铺到实例根，不多一层目录）。
    assert!(read_text(&target.join("config/known.toml")).contains("key"));
    // 刻意的截断 TOML 必须原样落地：导入器不是校验器，不许因为内容坏就丢文件。
    assert_eq!(
        read_text(&target.join("config/broken.toml")),
        String::from_utf8_lossy(fixtures::CF_BROKEN)
    );
    // 两个 CF 条目都被补全 + 下载（文件名与目录都来自 CF 元数据）。
    assert_eq!(
        std::fs::read(target.join("mods/jei.jar")).expect("jei.jar"),
        CURSE_BYTES
    );
    assert_eq!(
        std::fs::read(target.join("mods/fabric-api.jar")).expect("fabric-api.jar"),
        CURSE_BYTES
    );
    assert_eq!(
        fetcher.calls(),
        vec![CURSE_URL.to_string(), CURSE_URL.to_string()]
    );

    // 基线写进去了（下次导入的 base 侧）。
    let baseline = read_text(&target.join(super::baseline_rel()));
    assert!(baseline.contains("mods/jei.jar"), "基线：{baseline}");
    assert!(baseline.contains("config/known.toml"), "基线：{baseline}");

    assert_eq!(
        list_tree(&target),
        vec![
            super::baseline_rel(),
            "config/broken.toml".to_string(),
            "config/known.toml".to_string(),
            "mods/example.jar".to_string(),
            "mods/fabric-api.jar".to_string(),
            "mods/jei.jar".to_string(),
        ]
    );
}

#[test]
fn offline_curseforge_import_is_partial_not_success() {
    // 没有 fetcher ⇒ CF 的 files[] 一条都补不出来。这**不是**失败，也**不许**记成成功。
    let fixture = fixtures::curseforge("cf-offline");
    let target = temp_root("cf-offline-out").join("instance");
    let request = ImportRequest::new(&fixture.zip, &target);
    let report = block_on(import_pack(&request));

    assert_eq!(report.status, ImportStatus::Partial);
    assert_eq!(report.missing.len(), 2, "{:?}", report.missing);
    assert!(report
        .missing
        .iter()
        .all(|missing| missing.message_key == crate::addon::import::keys::MISSING_OFFLINE));
    // 覆盖内容照装（离线不影响包内自带的内容）。
    assert!(target.join("config/known.toml").is_file());
    assert!(!target.join("mods/jei.jar").exists());
    // 3 = Partial 的退出码（CLI 契约）。
    assert_eq!(report.status.exit_code(), 3);
}

#[test]
fn curseforge_meta_without_download_url_is_missing_not_guessed() {
    // CF 作者可以关掉 API 分发（`downloadUrl` 为 null）：我们**不猜** URL，只如实记缺。
    let fixture = fixtures::curseforge("cf-nourl");
    let target = temp_root("cf-nourl-out").join("instance");
    let mut meta = curse_meta("jei.jar", CURSE_BYTES);
    meta.url = None;
    let fetcher = StubFetcher::new()
        .with_curse(238222, 5102025, meta)
        .with_file(CURSE_URL, CURSE_BYTES);
    let request = ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher);
    let report = block_on(import_pack(&request));

    assert_eq!(report.status, ImportStatus::Partial);
    assert!(report
        .missing
        .iter()
        .any(|missing| missing.message_key == crate::addon::import::keys::MISSING_NO_SOURCE));
    // 没有发出任何下载请求。
    assert!(fetcher.calls().is_empty(), "{:?}", fetcher.calls());
}
