//! 扩展点判据：加一个假格式（**一个文件、一条注册**）就能被探测与读写，
//! 且不碰模型、不碰 CurseForge / Modrinth / MultiMC 任何一行。
//!
//! 这里刻意不写"文档说它可扩展"，而是真的实现一个 `toy.json` 格式走完整条链：
//! 注册 → 探测 → 读 → 渲染 → 写 → 再读。

use serde_json::{json, Value};

use super::{write_file, write_zip, TempRoot};
use crate::addon::pack::archive::PackArchive;
use crate::addon::pack::curseforge::CURSEFORGE;
use crate::addon::pack::error::{PackError, PackErrorKind, PackResult};
use crate::addon::pack::format::{PackFormat, PackPayload, PackRegistry};
use crate::addon::pack::manifest::{PackFile, PackManifest};

/// 测试专用假格式：`toy.json` = `{"name": …, "minecraft": …, "mods": ["mods/a.jar"]}`。
///
/// 全部实现加起来不到 40 行——这就是"社区加一个新格式"的代价上限。
#[derive(Debug)]
struct ToyFormat;

/// 注册表要的是 `&'static dyn PackFormat`，所以用 `static`。
static TOY: ToyFormat = ToyFormat;

impl PackFormat for ToyFormat {
    fn id(&self) -> &'static str {
        "toy"
    }

    fn label(&self) -> &'static str {
        "Toy（测试专用假格式）"
    }

    fn metadata_entry(&self) -> Option<&'static str> {
        Some("toy.json")
    }

    fn detect(&self, pack: &PackArchive) -> PackResult<Option<&'static str>> {
        let Some(value) = pack.read_json("toy.json")? else {
            return Ok(None);
        };
        Ok(value.is_object().then_some("toy.json"))
    }

    fn read(&self, pack: &PackArchive) -> PackResult<PackManifest> {
        let value = pack
            .read_json("toy.json")?
            .ok_or_else(|| PackError::metadata("没有 toy.json"))?;
        let files = value
            .get("mods")
            .and_then(Value::as_array)
            .map(|mods| {
                mods.iter()
                    .filter_map(Value::as_str)
                    .map(PackFile::with_path)
                    .collect()
            })
            .unwrap_or_default();
        Ok(PackManifest {
            name: value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            minecraft: value
                .get("minecraft")
                .and_then(Value::as_str)
                .map(str::to_string),
            files,
            ..PackManifest::default()
        })
    }

    fn render(&self, manifest: &PackManifest) -> PackResult<String> {
        let mods: Vec<&str> = manifest
            .files
            .iter()
            .filter_map(|file| file.path.as_deref())
            .collect();
        Ok(json!({
            "name": manifest.name,
            "minecraft": manifest.minecraft,
            "mods": mods,
        })
        .to_string())
    }
}

/// 造一个 toy 包（源目录形式，顺带能当导出 payload）。
fn toy_pack(tag: &str) -> TempRoot {
    let dir = TempRoot::under(tag, "toy");
    write_file(
        &dir.join("toy.json"),
        br#"{"name":"Toy Pack","minecraft":"1.21","mods":["mods/a.jar","mods/b.jar"]}"#,
    );
    dir
}

#[test]
fn a_new_format_needs_one_registration_line() {
    let dir = toy_pack("extend-register");
    let pack = PackArchive::open_default(&dir).expect("打开 toy 包");

    // ① 没注册时：内置三格式都不认它（证明它不是"顺手"被认出来的）。
    let error = PackRegistry::builtin()
        .detect(&pack)
        .expect_err("内置格式不该认领 toy 包");
    assert_eq!(error.kind(), PackErrorKind::Unsupported);

    // ② 加一行注册。
    let registry = PackRegistry::builtin().with(&TOY);

    // ③ 探测得到。
    let matched = registry.detect(&pack).expect("注册后就该探测到");
    assert_eq!(matched.format().id(), "toy");
    assert!(!matched.is_ambiguous());

    // ④ 读得到。
    let manifest = matched.format().read(&pack).expect("读 toy 清单");
    assert_eq!(manifest.name, "Toy Pack");
    assert_eq!(manifest.minecraft.as_deref(), Some("1.21"));
    assert_eq!(manifest.files.len(), 2);

    // ⑤ 渲染 + 写（默认 write 实现直接用 metadata_entry + render，不用重写一遍 zip 逻辑）。
    let rendered = matched.format().render(&manifest).expect("渲染 toy 清单");
    assert!(rendered.contains("Toy Pack"));

    let target = dir.with_file_name("toy.zip");
    matched
        .format()
        .write(&manifest, PackPayload::metadata_only(), &target)
        .expect("导出 toy 包");

    // ⑥ 再读回来必须一样。
    let written = PackArchive::open_default(&target).expect("打开导出的 toy 包");
    let reread = matched.format().read(&written).expect("再读 toy 清单");
    assert_eq!(reread, manifest);
}

#[test]
fn appending_a_format_never_steals_existing_packs() {
    // 同一份包同时有 CF 清单与 toy 清单。
    let root = TempRoot::new("extend-priority");
    let target = root.join("both.zip");
    write_zip(
        &target,
        &[
            (
                "manifest.json",
                &br#"{"manifestType":"minecraftModpack","manifestVersion":1,
                     "name":"real cf pack","minecraft":{"version":"1.20.1"},"files":[]}"#[..],
            ),
            (
                "toy.json",
                &br#"{"name":"Toy shadow","minecraft":"1.21","mods":[]}"#[..],
            ),
        ],
    );
    let pack = PackArchive::open_default(&target).expect("打开包");

    // 追加到末尾 = 最低优先级：老包的行为不变，但歧义要看得见。
    let appended = PackRegistry::builtin().with(&TOY);
    let matched = appended.detect(&pack).expect("探测");
    assert_eq!(matched.format().id(), "curseforge");
    assert_eq!(matched.also_matched().to_vec(), vec!["toy"]);

    // 反过来排到最前，就该 toy 赢——注册顺序就是优先级。
    let front = PackRegistry::empty().with(&TOY).with(&CURSEFORGE);
    let matched = front.detect(&pack).expect("探测");
    assert_eq!(matched.format().id(), "toy");
    assert_eq!(matched.also_matched().to_vec(), vec!["curseforge"]);
}
