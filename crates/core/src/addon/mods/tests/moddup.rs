//! jar 这一层的用例：真建 zip、真读回来（M312 · MODDUP）。
//!
//! 覆盖三件事：①三种元数据文件**真的**能被读出来（`meta` 的文本级用例之外那一半）；
//! ②读不到 / 不是 jar / 压缩炸弹都不会 panic，且降级成「普通 jar」；③`inspect` 把
//! `ModRecord` 交给 `report` 这条管道是通的——**把它断掉，本文件的管道用例必红。**

use std::io::Write;
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::super::meta;
use super::super::*;
use crate::addon::temp::TempRoot;

/// 一个运行目录 + 空的 `mods/`。
fn run(tag: &str) -> TempRoot {
    let root = TempRoot::under(tag, "run");
    std::fs::create_dir_all(mods_dir(&root)).expect("建 mods");
    root
}

/// 按给定条目打一个 zip（`mods/` 下的 jar 就是这玩意儿）。
fn jar(path: &Path, entries: &[(&str, &[u8])], method: CompressionMethod) {
    let file = std::fs::File::create(path).expect("建 jar");
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(method);
    for &(name, body) in entries {
        writer.start_file(name, options).expect("写条目");
        writer.write_all(body).expect("写内容");
    }
    writer.finish().expect("收尾");
}

/// 一个最小的 `fabric.mod.json` jar（`deps` 是 `depends` 表里的原文）。
fn fabric_jar(dir: &Path, file: &str, id: &str, version: &str, deps: &str) {
    let body =
        format!(r#"{{"id":"{id}","version":"{version}","name":"{id}","depends":{{{deps}}}}}"#);
    jar(
        &dir.join(file),
        &[("fabric.mod.json", body.as_bytes())],
        CompressionMethod::Stored,
    );
}

/// 一个最小的 `META-INF/mods.toml` jar。
fn forge_jar(dir: &Path, file: &str, id: &str, version: &str, deps: &str) {
    let body =
        format!("[[mods]]\nmodId=\"{id}\"\nversion=\"{version}\"\ndisplayName=\"{id}\"\n{deps}");
    jar(
        &dir.join(file),
        &[("META-INF/mods.toml", body.as_bytes())],
        CompressionMethod::Stored,
    );
}

/// **管道判据**：`inspect` → `ModRecord` → `report`。
///
/// 变异证明（只改一处）：把 `inspect.rs` 的 `let report = report::report(&records);` 换成
/// `report::report(&[])`（= 断开 `ModRecord` 消费者）⇒ 本用例红在
/// 「重复组必须是 1 条」那两句断言上。
#[test]
fn inspect_reports_duplicates_and_missing_prerequisites_from_jars() {
    let root = run("moddup-pipeline");
    let dir = mods_dir(&root);

    // 同一个 mod 的两个版本（用户真实痛点：游戏崩）+ 一个声明了必需前置的 mod。
    fabric_jar(&dir, "sodium-0.5.8.jar", "sodium", "0.5.8", "");
    fabric_jar(&dir, "sodium-0.5.11.jar", "sodium", "0.5.11", "");
    fabric_jar(
        &dir,
        "iris-1.7.0.jar",
        "iris",
        "1.7.0",
        r#""fabric-api":">=0.92""#,
    );

    let scan = scan(&root).expect("列目录");
    let inspection = inspect(&dir, &scan.entries);

    assert_eq!(
        inspection.records.len(),
        3,
        "每个文件一条记录：{:?}",
        inspection.records
    );
    assert_eq!(
        inspection
            .records
            .iter()
            .find(|record| record.file.as_deref() == Some("sodium-0.5.11.jar"))
            .map(|record| record.version.as_deref()),
        Some(Some("0.5.11")),
        "版本要从 jar 里读出来（不是从文件名猜）"
    );

    assert_eq!(
        inspection.report.duplicates.len(),
        1,
        "重复组必须是 1 条：{:?}",
        inspection.report
    );
    assert_eq!(
        inspection.report.duplicates[0].keep, "sodium-0.5.11.jar",
        "建议保留版本新的那一份"
    );
    assert_eq!(
        inspection.report.missing.len(),
        1,
        "必需前置缺失必须是 1 条：{:?}",
        inspection.report
    );
    assert_eq!(inspection.report.missing[0].id, "fabric-api");
    assert_eq!(
        inspection.report.missing[0].required_by,
        vec!["iris-1.7.0.jar"]
    );
    assert_eq!(inspection.opaque, 0, "三个都是正常的 fabric jar");
}

/// Forge（`mods.toml`）与旧版（`mcmod.info`）两条路都要走通，且有装好的一份时不误报。
#[test]
fn inspect_reads_forge_and_legacy_metadata() {
    let root = run("moddup-formats");
    let dir = mods_dir(&root);

    forge_jar(
        &dir,
        "jei-15.2.0.27.jar",
        "jei",
        "15.2.0.27",
        "[[dependencies.jei]]\nmodId=\"cloth-config\"\nmandatory=false\nversionRange=\"[11,)\"\n\
         [[dependencies.jei]]\nmodId=\"architectury\"\nmandatory=true\nversionRange=\"[9,)\"\n",
    );
    let legacy = r#"[{"modid":"buildcraft","version":"7.99.24",
        "requiredMods":["codechickenlib"],"dependencies":["railcraft@1.0"]}]"#;
    jar(
        &dir.join("buildcraft-7.99.24.jar"),
        &[("mcmod.info", legacy.as_bytes())],
        CompressionMethod::Stored,
    );
    // 装上 architectury：必需前置满足，另一项是可选提示。
    forge_jar(
        &dir,
        "architectury-9.2.14.jar",
        "architectury",
        "9.2.14",
        "",
    );

    let scan = scan(&root).expect("列目录");
    let inspection = inspect(&dir, &scan.entries);

    assert_eq!(
        inspection.report.duplicates.len(),
        0,
        "{:?}",
        inspection.report
    );
    let ids = inspection
        .report
        .missing
        .iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["codechickenlib"], "{:?}", inspection.report.missing);
    let optional = inspection
        .report
        .optional
        .iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        optional,
        ["cloth-config", "railcraft"],
        "可选缺失只进提示桶"
    );
}

/// 不是 jar / 半截 jar / 没有元数据的合法 zip —— 全当普通 jar，不 panic、不算「可疑」。
#[test]
fn inspect_treats_unreadable_files_as_plain_jars() {
    let root = run("moddup-degrade");
    let dir = mods_dir(&root);

    std::fs::write(dir.join("notes.txt"), "这一行不是 zip").expect("写普通文件");
    std::fs::write(dir.join("broken.jar"), b"PK\x03\x04half").expect("写半截 jar");
    jar(
        &dir.join("plain.jar"),
        &[("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\n")],
        CompressionMethod::Stored,
    );

    let scan = scan(&root).expect("列目录");
    let inspection = inspect(&dir, &scan.entries);

    assert_eq!(inspection.records.len(), 3, "一个文件都不许丢");
    assert_eq!(inspection.opaque, 0, "「不是 jar」是正常现象，不算可疑");
    assert!(inspection.report.is_clean(), "{:?}", inspection.report);
}

/// `.old` 是旧版本残留：不进记录、不算重复。
#[test]
fn inspect_skips_old_leftovers() {
    let root = run("moddup-old");
    let dir = mods_dir(&root);

    fabric_jar(&dir, "sodium.jar", "sodium", "0.5.11", "");
    fabric_jar(&dir, "sodium.jar.old", "sodium", "0.5.8", "");

    let scan = scan(&root).expect("列目录");
    assert_eq!(scan.entries.len(), 2, "列表本身照样列 `.old`");
    let inspection = inspect(&dir, &scan.entries);

    assert_eq!(inspection.records.len(), 1, "`.old` 不读元数据");
    assert!(
        inspection.report.duplicates.is_empty(),
        "{:?}",
        inspection.report
    );
}

/// 体积闸门：单条目声明过大、压缩比过高都判 `TooBig`；`inspect` 记一个「可疑」但仍当普通 jar。
#[test]
fn oversized_and_bomb_like_metadata_is_refused() {
    let root = run("moddup-bombs");
    let dir = mods_dir(&root);

    // ① 声明 2 MiB + 1 字节的元数据（Stored：声明值与实际值一致）。
    let huge = vec![b'a'; (meta::MAX_META_BYTES + 1) as usize];
    jar(
        &dir.join("huge.jar"),
        &[("fabric.mod.json", huge.as_slice())],
        CompressionMethod::Stored,
    );
    // ② 压缩炸弹形状：1 MiB 的零字节 Deflated（压缩比远超 MAX_RATIO）。
    let zeros = vec![0_u8; 1024 * 1024];
    jar(
        &dir.join("bomb.jar"),
        &[("fabric.mod.json", zeros.as_slice())],
        CompressionMethod::Deflated,
    );

    for file in ["huge.jar", "bomb.jar"] {
        let gap = meta::read(&dir.join(file)).expect_err("必须被限额挡住");
        assert_eq!(gap, meta::MetaGap::TooBig, "{file} → {gap:?}");
        assert!(gap.is_suspicious());
    }

    let scan = scan(&root).expect("列目录");
    let inspection = inspect(&dir, &scan.entries);
    assert_eq!(inspection.opaque, 2, "两个都该记「本该读到却没读到」");
    // 降级之后它们是普通 jar：报告不出错、也不崩。
    assert!(inspection.report.is_clean());
}

/// 条目数闸门：中央目录条目超过上限直接拒（不遍历、不解压）。
#[test]
fn too_many_entries_is_refused() {
    let root = run("moddup-entries");
    let dir = mods_dir(&root);
    let file = std::fs::File::create(dir.join("many.jar")).expect("建 jar");
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for index in 0..=meta::MAX_ENTRIES {
        writer
            .start_file(format!("x/{index}.txt"), options)
            .expect("写条目");
    }
    writer
        .start_file("fabric.mod.json", options)
        .expect("写元数据");
    writer.write_all(br#"{"id":"x"}"#).expect("写内容");
    writer.finish().expect("收尾");

    let gap = meta::read(&dir.join("many.jar")).expect_err("必须被条目数挡住");
    assert_eq!(gap, meta::MetaGap::TooBig);
}
