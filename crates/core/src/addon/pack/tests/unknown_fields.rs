//! 「未知字段不丢」的三处条目级 flatten（r1 p3-3）：`env_extra`、CF `files[].extra`、
//! `PackLoader::extra`。三处实现本来是对的，但此前一条断言都没有——而它们恰好是 M115 点名的
//! HMCL 回写 `fileName`/`url` 的位置。每条都走「读 → render → 写整包 → 再读逐字段相等」。

use serde_json::json;

use crate::addon::pack::archive::PackArchive;
use crate::addon::pack::curseforge::CURSEFORGE;
use crate::addon::pack::format::{PackFormat, PackPayload};
use crate::addon::pack::modrinth::MODRINTH;

#[test]
fn mrpack_env_unknown_keys_survive_read_write_reread() {
    // r1 p3-3 的一处：`files[].env` 里的未知键（`env_extra`）此前没有任何断言。
    let root = super::temp_root("env-extra");
    let dir = root.join("src");
    super::write_file(
        &dir.join("modrinth.index.json"),
        br#"{"game":"minecraft","formatVersion":1,"versionId":"1.0","name":"env extra keys",
             "files":[{"path":"mods/a.jar",
                       "hashes":{"sha512":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"},
                       "env":{"client":"required","server":"unsupported","customEnvKey":{"k":1}},
                       "downloads":["https://example.invalid/a.jar"]}],
             "dependencies":{"minecraft":"1.20.1"}}"#,
    );
    let zip = root.join("pack.mrpack");
    super::zip_dir(&dir, &zip);

    let pack = PackArchive::open_default(&zip).expect("打开包");
    let manifest = MODRINTH.read(&pack).expect("读 mrpack 清单");
    assert_eq!(manifest.files[0].env_extra["customEnvKey"], json!({"k": 1}));

    let target = root.join("out.mrpack");
    MODRINTH
        .write(&manifest, PackPayload::metadata_only(), &target)
        .expect("导出 mrpack");
    let reread = MODRINTH
        .read(&PackArchive::open_default(&target).expect("打开导出包"))
        .expect("再读");
    assert_eq!(reread, manifest, "env 里的未知键必须原样回写");
    assert_eq!(reread.files[0].env_extra["customEnvKey"], json!({"k": 1}));
}

#[test]
fn curseforge_file_unknown_keys_survive_read_write_reread() {
    // r1 p3-3 的另一处：CF `files[].fileName` / `url` 是 HMCL 补全后回写的扩展点。
    let root = super::temp_root("cf-file-extra");
    let dir = root.join("src");
    super::write_file(
        &dir.join("manifest.json"),
        br#"{"manifestType":"minecraftModpack","manifestVersion":1,"name":"files extra keys",
             "version":"1.0","minecraft":{"version":"1.20.1","modLoaders":[{"id":"forge-47.2.0","primary":true}]},
             "files":[{"projectID":238222,"fileID":5102025,"required":true,
                       "fileName":"jei-1.20.1-15.2.0.27.jar",
                       "url":"https://edge.forgecdn.net/files/5102/25/jei.jar"}]}"#,
    );
    let zip = root.join("pack.zip");
    super::zip_dir(&dir, &zip);

    let pack = PackArchive::open_default(&zip).expect("打开包");
    let manifest = CURSEFORGE.read(&pack).expect("读 CF 清单");
    assert_eq!(
        manifest.files[0].extra["fileName"],
        json!("jei-1.20.1-15.2.0.27.jar")
    );
    assert_eq!(
        manifest.files[0].extra["url"],
        json!("https://edge.forgecdn.net/files/5102/25/jei.jar")
    );

    let target = root.join("out.zip");
    CURSEFORGE
        .write(&manifest, PackPayload::metadata_only(), &target)
        .expect("导出 CF");
    let reread = CURSEFORGE
        .read(&PackArchive::open_default(&target).expect("打开导出包"))
        .expect("再读");
    assert_eq!(reread, manifest, "files[] 里的未知键必须原样回写");
}

#[test]
fn curseforge_loader_unknown_keys_survive_read_write_reread() {
    // r1 p3-3 的第三处：`modLoaders[]` 里的未知键（`PackLoader::extra`）。
    let root = super::temp_root("cf-loader-extra");
    let dir = root.join("src");
    super::write_file(
        &dir.join("manifest.json"),
        br#"{"manifestType":"minecraftModpack","manifestVersion":1,"name":"loader extra keys",
             "version":"1.0",
             "minecraft":{"version":"1.20.1",
                          "modLoaders":[{"id":"forge-47.2.0","primary":true,"cachedName":"Forge 47.2.0"}]},
             "files":[]}"#,
    );
    let zip = root.join("pack.zip");
    super::zip_dir(&dir, &zip);

    let pack = PackArchive::open_default(&zip).expect("打开包");
    let manifest = CURSEFORGE.read(&pack).expect("读 CF 清单");
    assert_eq!(
        manifest.loaders[0].extra["cachedName"],
        json!("Forge 47.2.0")
    );

    let target = root.join("out.zip");
    CURSEFORGE
        .write(&manifest, PackPayload::metadata_only(), &target)
        .expect("导出 CF");
    let reread = CURSEFORGE
        .read(&PackArchive::open_default(&target).expect("打开导出包"))
        .expect("再读");
    assert_eq!(reread, manifest, "modLoaders[] 里的未知键必须原样回写");
    assert_eq!(reread.loaders[0].extra["cachedName"], json!("Forge 47.2.0"));
}
