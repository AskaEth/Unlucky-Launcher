//! CLI 侧夹具：一个「别的启动器铺好了 `.minecraft`」的数据根。
//!
//! 快照工具**复用 core 测试那一份**（`crates/core/tests/adopt/snapshot.rs`，全节点：目录/文件/链接
//! 各记形状与 mtime）。以前 CLI 自己抄了一份 walk，两份都漏了目录节点——复审 r1 的 R5 正是这个洞，
//! 所以这里直接 `#[path]` 引用同一份源码：判据只有一处，不会再漂。

#[path = "../../../core/tests/adopt/snapshot.rs"]
mod snapshot;

pub use snapshot::{assert_unchanged, snapshot, snapshot_opt, Node};

use std::path::Path;

/// 造一份「别的启动器已经铺好的」数据根（`<root>/.minecraft` + `<root>/.ULL`）：
///
/// | 路径 | 用意 |
/// | --- | --- |
/// | `versions/1.20.1/` | 正常版本（清单 + jar），可登记、可 `launch --dry-run` |
/// | `versions/1.20.1-orphan/` | 缺父版本 ⇒ 不可登记 |
/// | `libraries/` `assets/` | 恒共享层（登记不该碰） |
/// | `mods/` `saves/` | 玩家数据（登记不该碰） |
/// | `launcher_profiles.json` | 别的启动器的元数据（连读都不该读，更不该写） |
pub fn foreign_data_root(root: &Path) {
    let minecraft = root.join(".minecraft");
    let versions = minecraft.join("versions");
    write_version(&versions, "1.20.1", MANIFEST_OK, true);
    write_version(&versions, "1.20.1-orphan", MANIFEST_ORPHAN, true);
    write_file(
        &minecraft.join("libraries/com/mojang/brigadier/1.0.18/brigadier-1.0.18.jar"),
        b"library-bytes",
    );
    write_file(
        &minecraft.join("assets/indexes/5.json"),
        b"{\"objects\":{}}",
    );
    write_file(&minecraft.join("mods/jei.jar"), b"mod-bytes");
    write_file(&minecraft.join("saves/World/level.dat"), b"save-bytes");
    write_file(
        &minecraft.join("launcher_profiles.json"),
        br#"{"profiles":{"1.20.1":{"name":"1.20.1","lastVersionId":"1.20.1"}}}"#,
    );
}

fn write_version(versions: &Path, id: &str, manifest: &str, jar: bool) {
    let dir = versions.join(id);
    std::fs::create_dir_all(&dir).expect("建版本目录");
    std::fs::write(dir.join(format!("{id}.json")), manifest.as_bytes()).expect("写清单");
    if jar {
        std::fs::write(dir.join(format!("{id}.jar")), b"junk").expect("写 jar");
    }
}

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建父目录");
    }
    std::fs::write(path, bytes).expect("写文件");
}

const MANIFEST_OK: &str = r#"{
  "id": "1.20.1",
  "type": "release",
  "mainClass": "net.minecraft.client.main.Main",
  "assets": "5",
  "assetIndex": { "id": "5" },
  "arguments": {
    "game": ["--username", "${auth_player_name}", "--version", "${version_name}",
             "--gameDir", "${game_directory}"],
    "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"]
  },
  "libraries": [],
  "downloads": { "client": { "size": 4, "url": "http://example.invalid/client.jar" } }
}"#;

const MANIFEST_ORPHAN: &str = r#"{
  "id": "1.20.1-orphan",
  "type": "release",
  "inheritsFrom": "1.20.1-not-on-disk",
  "mainClass": "net.minecraft.client.main.Main",
  "libraries": []
}"#;
