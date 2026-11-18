//! `import → export → import` 三轮一致性（M160）。
//!
//! # 这条用例补的是哪笔账
//!
//! M111/M112 的往返各自只有半条链：导入侧测"包 → 实例"，导出侧测"实例 → 包"（`export_roundtrip`
//! 里明确登记了"M111 合并后要补一条 `import_pack → export → import_pack`"）。本文件就是那条，
//! 而且是**三轮**：包① → 实例 A → 包② → 实例 B → 包③ → 实例 C，三个实例的内容逐字节相等。
//!
//! 夹具带 **overrides 叠层**（`client-overrides/` 与 `overrides/` 同名不同内容），并且第二轮
//! 的实例**预置了一个与包冲突的文件**：用户选了「覆盖」，于是三轮之后仍然一致——这条同时证明
//! "决策真的生效"与"决策之后的一致性仍然成立"。
//!
//! # 为什么比的是"内容目录里的文件"而不是整棵树
//!
//! 实例里还有两样东西**不属于整合包内容**：`versions/<id>/<id>.json`（导出要读它，但导出器
//! 不会把它打进包）与 `.ull/pack-baseline.json`（本仓库的三方合并基线）。两者都按设计不进包，
//! 所以比较口径是 `mods/` `config/` 等 8 个内容目录。
//!
//! # 没验的部分
//!
//! 真实社区包（CurseForge / Modrinth 在线下载）不在本轮：远端条目一律走 `StubFetcher`，
//! 导出一律走离线目录（`OfflineCatalog` ⇒ 全部内联）。这条欠账在完成报告里同样登记。

use std::collections::BTreeMap;
use std::path::Path;

use super::support::{sha1, StubFetcher};
use super::{write_file, zip_dir, TempRoot};
use crate::addon::export::{export_instance, ExportRequest};
use crate::addon::import::{
    import_pack, ConflictChoice, ConflictDecisions, ImportRequest, ImportStatus,
};
use crate::addon::pack::format::PackFormat;
use crate::addon::pack::modrinth::MODRINTH;
use crate::addon::pack::PackArchive;
use crate::download::blocking::block_on;

/// 内容目录白名单（与 `export::scan::DEFAULT_CONTENT_ROOTS` 同一条口径）。
const CONTENT_ROOTS: [&str; 4] = ["mods", "config", "resourcepacks", "scripts"];

/// 夹具里 `client-overrides/` 胜出的那份内容。
const KNOWN_CLIENT: &[u8] = b"shared = client\n";
/// 夹具里 `overrides/` 侧（被遮蔽）。
const KNOWN_OVERRIDES: &[u8] = b"shared = overrides\n";
/// 仅 overrides 声明的文件。
const LAYER: &[u8] = b"layer from overrides\n";
/// 远端条目的内容（走 stub 下载）。
const REMOTE: &[u8] = b"remote jar bytes";
/// 第二轮实例里用户自己那份（与包冲突）。
const USER_KNOWN: &[u8] = b"shared = mine\n";

/// 带叠层 + 一条远端条目的 mrpack。
fn layered_pack(tag: &str) -> (TempRoot, StubFetcher) {
    let root = TempRoot::at(tag, "pack.mrpack");
    let dir = root.parent().expect("临时根").join("src");
    let index = format!(
        r#"{{"game":"minecraft","formatVersion":1,"versionId":"1.0.0","name":"M160 三轮夹具",
             "files":[{{"path":"mods/remote.jar",
                        "hashes":{{"sha1":"{}","sha512":"{}"}},
                        "downloads":["stub://mods/remote.jar"],"fileSize":{}}}],
             "dependencies":{{"minecraft":"1.20.1"}}}}"#,
        sha1(REMOTE),
        "0".repeat(128),
        REMOTE.len()
    );
    write_file(&dir.join("modrinth.index.json"), index.as_bytes());
    write_file(&dir.join("overrides/config/known.toml"), KNOWN_OVERRIDES);
    write_file(
        &dir.join("client-overrides/config/known.toml"),
        KNOWN_CLIENT,
    );
    write_file(&dir.join("overrides/mods/layer.jar"), LAYER);
    write_file(&dir.join("overrides/scripts/start.js"), b"// script\n");
    zip_dir(&dir, &root);
    let fetcher = StubFetcher::new().with_file("stub://mods/remote.jar", REMOTE);
    (root, fetcher)
}

/// 内容目录里的文件（`/` 分隔、排序）→ 字节。
fn content_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    for dir in CONTENT_ROOTS {
        let base = root.join(dir);
        if !base.is_dir() {
            continue;
        }
        let mut pending = vec![base];
        while let Some(current) = pending.pop() {
            for entry in std::fs::read_dir(&current).expect("读内容目录").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                let relative = path
                    .strip_prefix(root)
                    .expect("在实例内")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(relative, std::fs::read(&path).expect("读内容文件"));
            }
        }
    }
    out
}

/// 给实例补一份版本 JSON（导出要读它拿游戏版本；导入**不装**游戏，所以得手工放）。
fn write_version(root: &Path, id: &str, game: &str) {
    write_file(
        &root.join("versions").join(id).join(format!("{id}.json")),
        format!(r#"{{"id":"{game}"}}"#).as_bytes(),
    );
}

/// 实例 → 包（走产品路径的导出器，离线目录 ⇒ 全部内联）。
fn export(root: &Path, target: &Path) {
    let request = ExportRequest::new(root, target, &MODRINTH).with_version_id("1.20.1");
    let report = export_instance(&request).expect("导出");
    assert!(
        report.is_complete(),
        "夹具里每个文件都该能内联进包：{:?}",
        report.summary()
    );
    assert!(target.exists(), "导出产物不存在");
}

/// 导出的包里覆盖条目的 `路径 → sha1`（三轮之间要比这一份）。
fn pack_overrides(pack: &Path) -> BTreeMap<String, String> {
    let archive = PackArchive::open_default(pack).expect("打开导出的包");
    let (format, manifest) = crate::addon::pack::PackRegistry::builtin()
        .read(&archive)
        .expect("读清单");
    assert_eq!(format.id(), MODRINTH.id(), "导出目标是 mrpack");
    manifest
        .override_files()
        .map(|(_, file)| (file.path.clone(), file.sha1.clone()))
        .collect()
}

#[test]
fn import_export_import_three_rounds_keep_the_same_file_set() {
    let (pack1, fetcher) = layered_pack("rt3");
    let base = TempRoot::new("rt3");
    let (dir_a, dir_b, dir_c) = (base.join("a"), base.join("b"), base.join("c"));

    // —— 第 1 轮：包① → 实例 A（干净目标）——
    let first = block_on(import_pack(
        &ImportRequest::new(&pack1, &dir_a).with_fetcher(&fetcher),
    ));
    assert_eq!(first.status, ImportStatus::Success, "{:?}", first.error);
    assert_eq!(
        content_files(&dir_a).keys().cloned().collect::<Vec<_>>(),
        vec![
            "config/known.toml",
            "mods/layer.jar",
            "mods/remote.jar",
            "scripts/start.js"
        ],
        "叠层的胜者是 client-overrides（不是 overrides）；实际树：{:?}",
        super::list_tree(&dir_a)
    );
    write_version(&dir_a, "1.20.1", "1.20.1");
    let pack2 = base.join("round2.mrpack");
    export(&dir_a, &pack2);

    // —— 第 2 轮：包② → 实例 B（**预置冲突**，用户选「覆盖」）——
    write_file(&dir_b.join("config/known.toml"), USER_KNOWN);
    let report = block_on(import_pack(
        &ImportRequest::new(&pack2, &dir_b)
            .with_fetcher(&fetcher)
            .with_decisions(
                ConflictDecisions::keep_all().with("config/known.toml", ConflictChoice::Overwrite),
            ),
    ));
    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    let conflict = report
        .conflicts
        .iter()
        .find(|conflict| conflict.path == "config/known.toml")
        .expect("第二轮必须报出那个冲突");
    assert_eq!(conflict.choice, ConflictChoice::Overwrite);
    write_version(&dir_b, "1.20.1", "1.20.1");
    let pack3 = base.join("round3.mrpack");
    export(&dir_b, &pack3);

    // —— 第 3 轮：包③ → 实例 C（干净目标）——
    let third = block_on(import_pack(
        &ImportRequest::new(&pack3, &dir_c).with_fetcher(&fetcher),
    ));
    assert_eq!(third.status, ImportStatus::Success, "{:?}", third.error);

    // 三个实例的内容逐字节相等。
    let a = content_files(&dir_a);
    let b = content_files(&dir_b);
    let c = content_files(&dir_c);
    assert_eq!(a, b, "第 1 轮与第 2 轮的文件集必须一致");
    assert_eq!(a, c, "第 1 轮与第 3 轮的文件集必须一致");
    assert_eq!(
        a.get("config/known.toml").map(Vec::as_slice),
        Some(KNOWN_CLIENT),
        "覆盖之后留下的是包内内容"
    );

    // 包②与包③的覆盖条目（路径 + 哈希）也必须一致。
    assert_eq!(
        pack_overrides(&pack2),
        pack_overrides(&pack3),
        "两轮导出的覆盖条目必须一样"
    );
    // 导出的是"内联"形态（离线目录没有远端命中），所以覆盖条目就是那四条内容文件。
    assert_eq!(
        pack_overrides(&pack2).keys().cloned().collect::<Vec<_>>(),
        vec![
            "config/known.toml",
            "mods/layer.jar",
            "mods/remote.jar",
            "scripts/start.js"
        ]
    );
}

#[test]
fn keeping_the_user_file_is_what_makes_the_rounds_diverge() {
    // 对照：同一个冲突若选「保留」，第二轮结果**按设计**与第一轮不同——这正是"决策真的进了管线"
    // 的反向证据（否则 `per_item` 那条就只是走了个过场）。
    let (pack1, fetcher) = layered_pack("rt3-keep");
    let base = TempRoot::new("rt3-keep");
    let (dir_a, dir_b) = (base.join("a"), base.join("b"));
    block_on(import_pack(
        &ImportRequest::new(&pack1, &dir_a).with_fetcher(&fetcher),
    ));
    write_version(&dir_a, "1.20.1", "1.20.1");
    let pack2 = base.join("round2.mrpack");
    export(&dir_a, &pack2);

    write_file(&dir_b.join("config/known.toml"), USER_KNOWN);
    let report = block_on(import_pack(
        &ImportRequest::new(&pack2, &dir_b).with_fetcher(&fetcher),
    ));
    // 保留了一条冲突 ⇒ `Partial`（实例可用，但与包内容不全一致）。
    assert_eq!(report.status, ImportStatus::Partial, "{:?}", report.error);
    assert_eq!(
        std::fs::read(dir_b.join("config/known.toml")).expect("读"),
        USER_KNOWN,
        "缺省决策下用户那份必须原样留着"
    );
    let a = content_files(&dir_a);
    let b = content_files(&dir_b);
    assert_ne!(a, b, "选了保留就该与第一轮不同（这条证明决策真的生效）");
    let mut differing: Vec<&str> = Vec::new();
    for (path, bytes) in &a {
        if b.get(path) != Some(bytes) {
            differing.push(path.as_str());
        }
    }
    assert_eq!(differing, vec!["config/known.toml"], "只有那一处不同");
}
