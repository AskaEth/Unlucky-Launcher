//! 本地夹具：CurseForge / Modrinth / MultiMC 三种格式各一个最小但**形状真实**的包。
//!
//! # 来源声明（不许把"看起来像"当真实）
//!
//! | 夹具 | 来源 | 说明 |
//! |------|------|------|
//! | [`curseforge`] | **手造** | 字段取自 analyse/05 §2.2（HMCL `CurseManifest` / `CurseManifestFile`）；内容与数值是编的 |
//! | [`modrinth`] | **手造** | 字段取自 analyse/05 §2.3（HMCL `ModrinthManifest`）；`hashes` 里的摘要是**形状正确的占位值**，不对应任何真实文件 |
//! | [`multimc`] | **手造** | 字段取自 analyse/05 §2.6（HMCL `MultiMCModpackProvider`）；组件表比真实 Prism 实例小得多 |
//!
//! **没有一个来自真实导出**：本任务环境离线、仓库里也没有真实整合包样本。真实导出物的字段
//! 偏移风险只能等 M111/M115 拿到真包后回收（报告里已登记这条欠账）。
//!
//! # 刻意造的坏文件（导入器不许崩、也不许悄悄丢掉）
//!
//! - CF：`overrides/config/broken.toml` —— 截断的 TOML（引号未闭合、段落未收尾）；
//! - mrpack：`overrides/mods/half.jar` —— 只有半个 zip 头；
//! - MultiMC：`.minecraft/mods/broken.jar` —— 零字节。
//!
//! 三种损坏都是**覆盖目录里的文件**：它们没有清单声明的摘要可比，读包时只能如实记下实际
//! SHA-1/大小（三方合并的 base 侧就是这么来的），谁想"校验并修掉"必须另做判断。

use std::path::PathBuf;

use super::{write_file, zip_dir, TempRoot};

/// 一个既有源目录（可当导出 payload）又有打好的 zip 的夹具。
///
/// `_root` 是临时根的**守卫**：夹具活多久，目录就活多久；夹具 drop（含测试 panic 展开）时
/// 整个临时根被删。没有它，这些手造包会一直堆在 `%TEMP%` 里（M160 r2：实测堆到 110 GB）。
pub(crate) struct Fixture {
    /// 源目录（zip 的内容来源，也是 `PackPayload::from_root` 的根）。
    pub dir: PathBuf,
    /// 打好的包。
    pub zip: PathBuf,
    /// 临时根守卫（只为了在夹具存活期间保住 `dir` / `zip`）。
    _root: TempRoot,
}

/// CF 夹具的包名。
pub(crate) const CF_NAME: &str = "M110 手造 CurseForge 包";
/// CF 夹具里那个坏文件的内容。
pub(crate) const CF_BROKEN: &[u8] = b"broken = \"unterminated\n[section\n";
/// mrpack 夹具的包名。
pub(crate) const MR_NAME: &str = "M110 手造 Modrinth 包";
/// mrpack 夹具里那个半截 jar 的内容。
pub(crate) const MR_HALF_JAR: &[u8] = b"PK\x03\x04half";
/// MultiMC 夹具的实例名（写在 `instance.cfg`）。
pub(crate) const MMC_NAME: &str = "M110 手造 MultiMC 实例";

/// 手造的 SHA-1 占位值（40 位十六进制）。
const FAKE_SHA1: &str = "0123456789abcdef0123456789abcdef01234567";
/// 手造的 SHA-256 占位值（64 位十六进制）。
const FAKE_SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
/// 手造的 SHA-512 占位值（128 位十六进制）。
const FAKE_SHA512: &str = concat!(
    "0123456789abcdef",
    "0123456789abcdef",
    "0123456789abcdef",
    "0123456789abcdef",
    "0123456789abcdef",
    "0123456789abcdef",
    "0123456789abcdef",
    "0123456789abcdef"
);

const CF_MANIFEST: &str = r#"{
  "manifestType": "minecraftModpack",
  "manifestVersion": 1,
  "name": "M110 手造 CurseForge 包",
  "version": "1.0.0",
  "author": "AskaEth",
  "overrides": "overrides",
  "minecraft": {
    "version": "1.20.1",
    "modLoaders": [{"id": "forge-47.2.0", "primary": true}],
    "recommendedRam": 4096
  },
  "files": [
    {"projectID": 238222, "fileID": 5102025, "required": true},
    {"projectID": 306612, "fileID": 4587454, "required": false}
  ],
  "customCommunityField": {"since": 2, "note": "社区新增字段，必须原样回写"}
}
"#;

fn mrpack_index() -> String {
    format!(
        r#"{{
  "game": "minecraft",
  "formatVersion": 1,
  "versionId": "2.0.0",
  "name": "{MR_NAME}",
  "summary": "手造夹具：只验证形状与边界，不是真实导出",
  "files": [
    {{
      "path": "mods/jei.jar",
      "hashes": {{"sha1": "{FAKE_SHA1}", "sha256": "{FAKE_SHA256}", "sha512": "{FAKE_SHA512}"}},
      "env": {{"client": "required", "server": "unsupported"}},
      "downloads": ["https://cdn.modrinth.com/data/AANobbMI/versions/abc/jei.jar"],
      "fileSize": 1234567,
      "customFileField": "社区新增字段"
    }},
    {{
      "path": "mods/optional.jar",
      "hashes": {{"sha1": "{FAKE_SHA1}", "sha512": "{FAKE_SHA512}"}},
      "env": {{"client": "optional", "server": "unsupported"}},
      "downloads": ["https://cdn.modrinth.com/data/opt/versions/x/optional.jar"],
      "fileSize": 11
    }}
  ],
  "dependencies": {{
    "minecraft": "1.20.1",
    "fabric-loader": "0.15.11",
    "parchment": "2023.09.03"
  }},
  "customPackField": [1, 2, 3]
}}
"#
    )
}

const MMC_PACK: &str = r#"{
  "formatVersion": 1,
  "components": [
    {"uid": "net.minecraft", "version": "1.20.1", "important": true, "cachedName": "1.20.1"},
    {"uid": "net.fabricmc.fabric-loader", "version": "0.15.11"},
    {"uid": "org.lwjgl3", "version": "3.3.2"}
  ],
  "customComponentGraphField": true
}
"#;

const MMC_INSTANCE: &str = "InstanceType=OneSix\nname=M110 手造 MultiMC 实例\niconKey=default\n";

/// CurseForge 夹具（`manifest.json` + `overrides/`，含一个截断的 TOML）。
pub(crate) fn curseforge(tag: &str) -> Fixture {
    let root = TempRoot::new(tag);
    let dir = root.join("src");
    write_file(&dir.join("manifest.json"), CF_MANIFEST.as_bytes());
    write_file(
        &dir.join("overrides/mods/example.jar"),
        b"PK\x03\x04 hand-made jar stub",
    );
    write_file(
        &dir.join("overrides/config/known.toml"),
        b"key = \"value\"\n",
    );
    write_file(&dir.join("overrides/config/broken.toml"), CF_BROKEN);
    let zip = root.join("pack.zip");
    zip_dir(&dir, &zip);
    Fixture {
        dir,
        zip,
        _root: root,
    }
}

/// Modrinth 夹具（`modrinth.index.json` + 三个覆盖目录，含一个半截 jar）。
pub(crate) fn modrinth(tag: &str) -> Fixture {
    let root = TempRoot::new(tag);
    let dir = root.join("src");
    write_file(&dir.join("modrinth.index.json"), mrpack_index().as_bytes());
    write_file(&dir.join("overrides/mods/half.jar"), MR_HALF_JAR);
    write_file(&dir.join("overrides/config/known.toml"), b"shared = true\n");
    write_file(
        &dir.join("client-overrides/config/client-only.toml"),
        b"client = true\n",
    );
    write_file(
        &dir.join("server-overrides/config/server.properties"),
        b"online-mode=true\n",
    );
    let zip = root.join("pack.mrpack");
    zip_dir(&dir, &zip);
    Fixture {
        dir,
        zip,
        _root: root,
    }
}

/// MultiMC 夹具：**目录形式**（真实形态就是实例目录，不是 zip），含一个零字节 jar。
///
/// 返回 `TempRoot`（`Deref<Target = Path>` 指向 `<root>/instance`）：调用点照旧当路径用，
/// 而临时根由返回值持有——用到哪、活到哪、drop 就删。
pub(crate) fn multimc(tag: &str) -> TempRoot {
    let root = TempRoot::under(tag, "instance");
    write_file(&root.join("mmc-pack.json"), MMC_PACK.as_bytes());
    write_file(&root.join("instance.cfg"), MMC_INSTANCE.as_bytes());
    write_file(
        &root.join(".minecraft/mods/example.jar"),
        b"PK\x03\x04 hand-made jar stub",
    );
    write_file(&root.join(".minecraft/mods/broken.jar"), b"");
    write_file(
        &root.join(".minecraft/config/known.toml"),
        b"key = \"value\"\n",
    );
    root
}
