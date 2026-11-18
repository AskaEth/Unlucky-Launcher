//! `import_decisions` 的夹具与断言小工具：**从 `import_decisions.rs` 拆出来守 400 行硬线**。
//!
//! 拆分口径与仓库其它拆分一致：**纯搬移**，内容一行不改。这些夹具本身也是"口径"的一部分——
//! 叠层（`client-overrides/` 胜 `overrides/`）、目标里已存在的用户内容、以及"内容与包内一致"
//! 的那一份，都是冲突判定的边界样本。

use super::support::{sha1, StubFetcher};
use super::{write_file, zip_dir, TempRoot};
use crate::addon::import::ImportScan;

/// 夹具里 `client-overrides/` 胜出的那份内容（`overrides/` 里同名文件是不同的内容）。
pub(super) const KNOWN_CLIENT: &[u8] = b"shared = client\n";
/// 夹具里被遮蔽的那份（`overrides/` 侧）。
pub(super) const KNOWN_OVERRIDES: &[u8] = b"shared = overrides\n";
/// 与目标内容**一致**的那个文件。
pub(super) const SAME: &[u8] = b"same = 1\n";
/// 只有 overrides 声明、目标里还有一个不同内容的同名文件。
pub(super) const LAYER: &[u8] = b"layer from overrides\n";
/// 目标里用户自己的内容（会被覆盖 / 被保留的那一侧）。
pub(super) const USER_KNOWN: &[u8] = b"shared = mine\n";
/// 目标里用户自己的 layer 内容。
pub(super) const USER_LAYER: &[u8] = b"my layer\n";

/// 一份带 **overrides 叠层** 的 mrpack（`client-overrides/` 与 `overrides/` 同名不同内容）+ 一条远端条目。
pub(super) fn layered_pack(tag: &str) -> (TempRoot, StubFetcher) {
    // 返回值 `Deref` 到 `pack.mrpack`，根由它持有（`src/` 在 `root.parent()` 下）。
    let root = TempRoot::at(tag, "pack.mrpack");
    let dir = root.parent().expect("临时根").join("src");
    let remote = b"remote jar bytes";
    let index = format!(
        r#"{{"game":"minecraft","formatVersion":1,"versionId":"1.0.0","name":"M160 夹具",
             "files":[{{"path":"mods/remote.jar",
                        "hashes":{{"sha1":"{}","sha512":"{}"}},
                        "downloads":["stub://mods/remote.jar"],"fileSize":{}}}],
             "dependencies":{{"minecraft":"1.20.1"}}}}"#,
        sha1(remote),
        "0".repeat(128),
        remote.len()
    );
    write_file(&dir.join("modrinth.index.json"), index.as_bytes());
    // 叠层：同一路径两份声明，client-overrides 胜（Modrinth 官方语义）。
    write_file(&dir.join("overrides/config/known.toml"), KNOWN_OVERRIDES);
    write_file(
        &dir.join("client-overrides/config/known.toml"),
        KNOWN_CLIENT,
    );
    // 内容与目标一致的那个（不在叠层里，来源 = overrides）。
    write_file(&dir.join("overrides/config/same.toml"), SAME);
    write_file(&dir.join("overrides/mods/layer.jar"), LAYER);
    zip_dir(&dir, &root);
    let fetcher = StubFetcher::new().with_file("stub://mods/remote.jar", remote);
    (root, fetcher)
}

/// 预置一个"用户已有文件"的实例目录。
pub(super) fn seeded(tag: &str) -> TempRoot {
    let target = TempRoot::under(tag, "instance");
    write_file(&target.join("config/known.toml"), USER_KNOWN);
    write_file(&target.join("config/same.toml"), SAME);
    write_file(&target.join("mods/layer.jar"), USER_LAYER);
    target
}

pub(super) fn paths(scan: &ImportScan) -> Vec<&str> {
    scan.conflicts
        .iter()
        .map(|conflict| conflict.path.as_str())
        .collect()
}
