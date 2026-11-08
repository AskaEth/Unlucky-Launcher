//! Modrinth `.mrpack` 端到端导入：三目录优先级、`.disabled` 后缀、依赖翻译。
//!
//! 成功路径吃 [`super::support::mrpack_stub`]（同一份 M110 夹具，只把 `sha1`/`downloads`
//! 换成真实值）；"声明与实际不符"那条用例用**原始**夹具，见 `import_failures.rs`。

use crate::addon::import::{import_pack, keys, ImportRequest, ImportStatus};
use crate::addon::loader::LoaderKind;
use crate::download::blocking::block_on;

use super::support::{mrpack_stub, JEI_BYTES, OPTIONAL_BYTES};
use super::{fixtures, list_tree, read_text, TempRoot};

#[test]
fn imports_mrpack_end_to_end() {
    let (fixture, fetcher) = mrpack_stub("mr-e2e");
    let root = TempRoot::new("mr-e2e-out");
    let target = root.join("instance");
    let request = ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher);
    let report = block_on(import_pack(&request));

    assert_eq!(
        report.status,
        ImportStatus::Success,
        "缺文件 {:?} 冲突 {:?} 错误 {:?}",
        report.missing,
        report.conflicts,
        report.error
    );
    assert_eq!(report.format.as_deref(), Some("modrinth"));
    assert_eq!(report.pack_name.as_deref(), Some(fixtures::MR_NAME));
    assert_eq!(report.pack_version.as_deref(), Some("2.0.0"));
    assert_eq!(report.game_version.as_deref(), Some("1.20.1"));

    let loader = report.loader.as_ref().expect("mrpack 带 fabric-loader");
    assert_eq!(loader.kind, LoaderKind::Fabric);
    assert_eq!(loader.loader_version, "0.15.11");
    assert_eq!(loader.version_id, "1.20.1-fabric-0.15.11");

    // 覆盖内容：`overrides/` + `client-overrides/` 落地，`server-overrides/` **不落地**。
    assert_eq!(
        std::fs::read(target.join("mods/half.jar")).expect("half.jar"),
        fixtures::MR_HALF_JAR
    );
    assert!(target.join("config/known.toml").is_file());
    assert!(target.join("config/client-only.toml").is_file());
    assert!(
        !target.join("config/server.properties").exists(),
        "server-overrides/ 客户端不许解压（Modrinth 官方语义）"
    );

    // 远端条目：路径与地址都在清单里，内容来自 stub。
    assert_eq!(
        std::fs::read(target.join("mods/jei.jar")).expect("jei.jar"),
        JEI_BYTES
    );
    assert_eq!(
        std::fs::read(target.join("mods/optional.jar")).expect("optional.jar"),
        OPTIONAL_BYTES
    );

    // 三条注记：服务端覆盖被跳过 / 不认识的依赖键 / 可选条目不等于禁用。
    let note_keys: Vec<&str> = report
        .warnings
        .iter()
        .map(|note| note.message_key)
        .collect();
    assert!(
        note_keys.contains(&keys::NOTE_SERVER_OVERRIDES_SKIPPED),
        "{note_keys:?}"
    );
    assert!(
        note_keys.contains(&keys::NOTE_UNKNOWN_DEPENDENCY),
        "parchment 这类键要如实告警：{note_keys:?}"
    );
    assert!(
        note_keys.contains(&keys::NOTE_OPTIONAL_FILE),
        "env.client=optional 要提示但照装：{note_keys:?}"
    );

    assert_eq!(
        list_tree(&target),
        vec![
            super::baseline_rel(),
            "config/client-only.toml".to_string(),
            "config/known.toml".to_string(),
            "mods/half.jar".to_string(),
            "mods/jei.jar".to_string(),
            "mods/optional.jar".to_string(),
        ]
    );
}

#[test]
fn engine_artifacts_never_reach_the_instance() {
    // 引擎在下载失败时故意留下 `<name>.ullpart-<session>-<seq>` 与 `<name>.ullmeta.json`
    // 供下次续传；它们躺在暂存目录里，提交前必须剔掉，否则会变成实例里的"内容"。
    let (fixture, fetcher) = mrpack_stub("mr-artifacts");
    let fetcher = fetcher.with_engine_artifacts();
    let root = TempRoot::new("mr-artifacts-out");
    let target = root.join("instance");
    let report = block_on(import_pack(
        &ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher),
    ));

    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    let tree = list_tree(&target);
    assert!(
        tree.iter()
            .all(|path| !path.contains(".ullpart") && !path.contains(".ullmeta")),
        "中间产物不许进实例：{tree:?}"
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|note| note.message_key == keys::NOTE_ENGINE_ARTIFACTS_DROPPED),
        "剔除了就该留一条注记：{:?}",
        report.warnings
    );
}

#[test]
fn disabled_suffix_is_preserved_verbatim() {
    // 四种 `.disabled` / `.old` 变体都按原后缀落地，不改名、不清理（tower 定案）。
    let (fixture, fetcher) = mrpack_stub("mr-disabled");
    super::write_file(
        &fixture.dir.join("overrides/mods/variant.jar.disabled"),
        b"disabled bytes",
    );
    super::write_file(
        &fixture.dir.join("overrides/mods/variant.jar.disabled.old"),
        b"disabled old bytes",
    );
    super::write_file(
        &fixture.dir.join("overrides/mods/variant.jar.old"),
        b"old bytes",
    );
    super::zip_dir(&fixture.dir, &fixture.zip);

    let root = TempRoot::new("mr-disabled-out");
    let target = root.join("instance");
    let request = ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher);
    let report = block_on(import_pack(&request));

    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    for name in [
        "mods/variant.jar.disabled",
        "mods/variant.jar.disabled.old",
        "mods/variant.jar.old",
    ] {
        assert!(target.join(name).is_file(), "{name} 应当按原后缀落地");
    }
    // 禁用位从后缀读出来，`.old` 不被当成禁用。
    let disabled: Vec<(&str, bool)> = report
        .files
        .iter()
        .filter(|file| file.path.starts_with("mods/variant"))
        .map(|file| (file.path.as_str(), file.disabled))
        .collect();
    assert_eq!(
        disabled,
        vec![
            ("mods/variant.jar.disabled", true),
            ("mods/variant.jar.disabled.old", true),
            ("mods/variant.jar.old", false),
        ]
    );
    let note_keys: Vec<&str> = report
        .files
        .iter()
        .filter(|file| file.path.starts_with("mods/variant"))
        .filter_map(|file| file.note.as_ref().map(|note| note.message_key))
        .collect();
    assert!(
        note_keys.contains(&keys::NOTE_DISABLED_SUFFIX),
        "{note_keys:?}"
    );
    assert!(note_keys.contains(&keys::NOTE_OLD_SUFFIX), "{note_keys:?}");
    // 报告里的内容仍是原文（没有"读出来再写一遍"导致的空白变化）。
    assert_eq!(read_text(&target.join("mods/variant.jar.old")), "old bytes");
}
