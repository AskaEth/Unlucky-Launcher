//! Modrinth 格式：`modrinth.index.json` + `overrides/` `client-overrides/` `server-overrides/`。
//!
//! # 形状（A 级来源：HMCL `ModrinthManifest.java`，见 analyse/05 §2.3）
//!
//! ```jsonc
//! {
//!   "game": "minecraft", "formatVersion": 1, "versionId": "1.0.0",
//!   "name": "…", "summary": "…",
//!   "files": [{
//!     "path": "mods/jei.jar",
//!     "hashes": {"sha1": "…", "sha512": "…"},
//!     "env": {"client": "required", "server": "unsupported"},
//!     "downloads": ["https://cdn.modrinth.com/data/…/jei.jar"],
//!     "fileSize": 123456
//!   }],
//!   "dependencies": {"minecraft": "1.20.1", "fabric-loader": "0.15.11"}
//! }
//! ```
//!
//! 比 CF 强的地方（也是模型主路径为什么按 mrpack 设计）：**有路径、有多源 URL、有摘要表、
//! 有客户端/服务端分侧语义**。这三样正是「能否被远程替代」判据（D10 §2 导出侧）需要的输入。
//!
//! # 读取时坚持的三条校验（与 HMCL `File.validate()` 同源）
//!
//! 1. `path` 必须是不越界的相对路径（[`normalize_file`] 七条规则，第二检在 `archive`）；
//! 2. `hashes.sha512` 必须存在且是 128 位十六进制——没有它，"远程替代"无从判断；
//! 3. `env` 出现时 `client` 必须存在（`env.server` 缺省按 `required`）。
//!
//! 另有两条事实（M115 调研 §字段缺口 1/4）：`fileSize` 在规范里是**必填**（模型里是
//! `Option`，未知就不写，由 M112 补全），`server-overrides` 客户端**不解压**（模型照记，
//! 落地判断留给 M111）。
//!
//! 这三条是**读**的硬门槛；写（导出）另有四条：
//!
//! 1. 条目缺 `path` 或缺 `downloads` 一律报 `PackErrorKind::Unsupported`（CF 的裸
//!    `projectID/fileID` 条目转不成 mrpack，必须先补全——这就是 M111/M112 的分工边界）；
//! 2. 禁用位（[`PackFile::disabled`]）落成 **`path` 结尾的 `.disabled`**，不是
//!    `env.client=optional`（tower 定案；后者跨启动器不保真，见 [`DISABLED_SUFFIX`]）；
//! 3. `fileSize` 按规范的 u32 校验，超了报 `pack_limits`（HMCL 那边 `(int)` 截断只打 warning
//!    是反面教材）；
//! 4. 整包未压缩总量撞 ZIP64 阈值时由 `format::write` 拒绝（`pack_limits`），不写半成品。
//!
//! `dependencies` 里的 `neoforge` 与 `neo-forge` 两种拼法都认（HMCL PR #5170 的坑），
//! 认不出的键（社区新加载器如 `parchment`）**原样保留**成一条 [`PackLoader`]。

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::archive::PackArchive;
use super::error::{PackError, PackResult};
use super::format::PackFormat;
use super::manifest::{
    merge_unknown, EnvSupport, PackFile, PackLoader, PackManifest, PackOverrides, PackSide,
};
use super::path::normalize_file;

/// 格式 id。
pub const ID: &str = "modrinth";
/// 清单入口名。
pub const METADATA_ENTRY: &str = "modrinth.index.json";
/// `game` 的固定值。
pub const GAME: &str = "minecraft";
/// 依赖表里代表游戏版本的那个键。
pub const MINECRAFT_KEY: &str = "minecraft";
/// 禁用文件的路径后缀（Modrinth 官方导出用它表达"装进来但禁用"）。
pub const DISABLED_SUFFIX: &str = ".disabled";

/// 单例（进注册表用）。
pub static MODRINTH: Modrinth = Modrinth;

/// Modrinth 格式的值类型实现。
#[derive(Debug, Clone, Copy, Default)]
pub struct Modrinth;

/// `modrinth.index.json` 的原始形状（未知字段落 `extra`）。
#[derive(Debug, Default, Deserialize)]
struct RawIndex {
    #[serde(default)]
    game: Option<String>,
    #[serde(rename = "formatVersion", default)]
    format_version: Option<i64>,
    #[serde(rename = "versionId", default)]
    version_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    files: Vec<RawFile>,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// `files[]` 的一条。
#[derive(Debug, Default, Deserialize)]
struct RawFile {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    hashes: BTreeMap<String, String>,
    #[serde(default)]
    env: Option<RawEnv>,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(rename = "fileSize", default)]
    file_size: Option<u64>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// `files[].env`。
#[derive(Debug, Default, Deserialize)]
struct RawEnv {
    #[serde(default)]
    client: Option<String>,
    #[serde(default)]
    server: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl PackFormat for Modrinth {
    fn id(&self) -> &'static str {
        ID
    }

    fn label(&self) -> &'static str {
        "Modrinth"
    }

    fn metadata_entry(&self) -> Option<&'static str> {
        Some(METADATA_ENTRY)
    }

    fn detect(&self, pack: &PackArchive) -> PackResult<Option<&'static str>> {
        let Some(value) = pack.read_json(METADATA_ENTRY)? else {
            return Ok(None);
        };
        let object = value
            .as_object()
            .ok_or_else(|| PackError::metadata(format!("{METADATA_ENTRY} 存在但不是 JSON 对象")))?;
        // 有 formatVersion 数字即可认领；没有则要求 game + files 的形状对得上。
        let matched = object.get("formatVersion").is_some_and(Value::is_number)
            || (object.get("game").and_then(Value::as_str) == Some(GAME)
                && object.get("files").is_some_and(Value::is_array));
        Ok(matched.then_some(METADATA_ENTRY))
    }

    fn check_overrides(&self, manifest: &PackManifest) -> PackResult<()> {
        // mrpack 的覆盖目录名由 side 决定，且每侧最多一个：不符合就别写（r1 p2-1）。
        let mut seen: Vec<PackSide> = Vec::new();
        for group in &manifest.overrides {
            let expected = group.side.mrpack_dir();
            if group.dir != expected {
                return Err(PackError::unsupported(format!(
                    "Modrinth 的覆盖目录名由侧决定：{} 侧必须是 {expected:?}，模型里是 {:?}",
                    group.side.as_str(),
                    group.dir
                ))
                .with_hint(
                    "按 side 分派到 overrides / client-overrides / server-overrides 属 M112；\
                     不同格式的实例目录（如 MultiMC 的 .minecraft）不能直接搬过来",
                ));
            }
            if seen.contains(&group.side) {
                return Err(PackError::unsupported(format!(
                    "Modrinth 每个侧只能有一个覆盖目录，{} 侧出现了多次",
                    group.side.as_str()
                )));
            }
            seen.push(group.side);
        }
        Ok(())
    }

    fn read(&self, pack: &PackArchive) -> PackResult<PackManifest> {
        let value = pack
            .read_json(METADATA_ENTRY)?
            .ok_or_else(|| PackError::metadata(format!("包里没有 {METADATA_ENTRY}")))?;
        let raw: RawIndex = serde_json::from_value(value)
            .map_err(|error| PackError::metadata(format!("{METADATA_ENTRY} 形状不符：{error}")))?;
        let name = raw
            .name
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| PackError::metadata(format!("{METADATA_ENTRY} 缺 name")))?;

        let mut minecraft = None;
        let mut loaders: Vec<PackLoader> = Vec::new();
        for (key, version) in &raw.dependencies {
            if key == MINECRAFT_KEY {
                minecraft = Some(version.clone());
                continue;
            }
            let mut loader = PackLoader::parse_dependency(key, version, false);
            // 第一条认得出来的加载器当主加载器（mrpack 没有 primary 概念）。
            if loader.kind.is_some() && !loaders.iter().any(|seen| seen.kind.is_some()) {
                loader.primary = true;
            }
            loaders.push(loader);
        }

        let mut files = Vec::with_capacity(raw.files.len());
        for file in raw.files {
            let path = file.path.as_deref().ok_or_else(|| {
                PackError::metadata(format!("{METADATA_ENTRY} 有一条 files[] 没有 path"))
            })?;
            let path = normalize_file(path)?;
            check_sha512(&path, &file.hashes)?;
            let (client, server, env_extra) = match file.env {
                None => (EnvSupport::Required, EnvSupport::Required, Map::new()),
                Some(env) => {
                    let client_raw = env.client.as_deref().ok_or_else(|| {
                        PackError::metadata(format!(
                            "files[] 条目 {path} 有 env 但没有 env.client（Modrinth 规范要求）"
                        ))
                    })?;
                    let client = parse_env(&path, "client", client_raw)?;
                    let server = match env.server.as_deref() {
                        Some(raw) => parse_env(&path, "server", raw)?,
                        None => EnvSupport::Required,
                    };
                    (client, server, env.extra)
                }
            };
            files.push(PackFile {
                path: Some(path),
                hashes: file.hashes,
                downloads: file.downloads,
                size: file.file_size,
                client,
                server,
                extra: file.extra,
                env_extra,
                ..PackFile::default()
            });
        }

        let mut overrides = Vec::new();
        for side in [PackSide::Both, PackSide::Client, PackSide::Server] {
            let dir = side.mrpack_dir();
            let files = pack.override_files(dir)?;
            if pack.has_entry(dir) || !files.is_empty() {
                overrides.push(PackOverrides {
                    dir: dir.to_string(),
                    side,
                    files,
                });
            }
        }

        Ok(PackManifest {
            name,
            version: raw.version_id,
            author: None,
            summary: raw.summary,
            format_version: raw.format_version,
            game: raw.game.or_else(|| Some(GAME.to_string())),
            minecraft,
            loaders,
            files,
            overrides,
            extra: raw.extra,
            minecraft_extra: Map::new(),
        })
    }

    fn render(&self, manifest: &PackManifest) -> PackResult<String> {
        let version_id = manifest.version.as_deref().ok_or_else(|| {
            PackError::metadata("Modrinth 清单需要 versionId（包版本），模型里没有")
        })?;
        let mut root = Map::new();
        root.insert(
            "game".into(),
            json!(manifest.game.as_deref().unwrap_or(GAME)),
        );
        root.insert(
            "formatVersion".into(),
            json!(manifest.format_version.unwrap_or(1)),
        );
        root.insert("versionId".into(), json!(version_id));
        root.insert("name".into(), json!(manifest.name));
        if let Some(summary) = &manifest.summary {
            root.insert("summary".into(), json!(summary));
        }

        let mut dependencies = Map::new();
        if let Some(minecraft) = &manifest.minecraft {
            dependencies.insert(MINECRAFT_KEY.into(), json!(minecraft));
        }
        for loader in &manifest.loaders {
            let key = loader.dependency_key();
            let Some(version) = loader.version.as_deref() else {
                return Err(PackError::unsupported(format!(
                    "Modrinth dependencies 需要版本号，这条加载器没有：raw={:?}",
                    loader.raw
                )));
            };
            if key.is_empty() {
                return Err(PackError::unsupported(format!(
                    "Modrinth dependencies 需要键名，这条加载器没有：version={version:?}"
                )));
            }
            dependencies.insert(key, json!(version));
        }
        root.insert("dependencies".into(), Value::Object(dependencies));

        let mut files = Vec::with_capacity(manifest.files.len());
        for file in &manifest.files {
            let path = file.path.as_deref().ok_or_else(|| {
                PackError::unsupported(format!(
                    "Modrinth 的 files[] 需要 path，这条没有（projectID={:?}）",
                    file.project_id
                ))
                .with_hint("先用 CurseForge/Modrinth API 补全条目再导出")
            })?;
            let path = normalize_file(path)?;
            file.sha512().ok_or_else(|| {
                PackError::unsupported(format!("Modrinth 的 files[] 需要 hashes.sha512：{path}"))
            })?;
            if file.downloads.iter().all(|url| url.trim().is_empty()) {
                return Err(PackError::unsupported(format!(
                    "Modrinth 的 files[] 需要至少一个 downloads URL：{path}"
                ))
                .with_hint("内联覆盖内容无法用 mrpack 表达（见 M112 的内联兜底方案）"));
            }
            let mut hashes = Map::new();
            for (algorithm, digest) in &file.hashes {
                hashes.insert(algorithm.clone(), json!(digest));
            }
            let mut env = Map::new();
            merge_unknown(&mut env, &file.env_extra);
            env.insert("client".into(), json!(file.client.as_str()));
            env.insert("server".into(), json!(file.server.as_str()));
            // 禁用位用**路径后缀**表达（Modrinth 官方导出同款，`export_mrpack.rs:653-658`），
            // 不用 `env.client=optional`：后者只是"安装时可取消勾选"，装回来默认仍然启用，
            // 跨启动器不保真（tower 定案，D10 §2 的旧措辞已作废）。
            let path = if file.disabled && !path.ends_with(DISABLED_SUFFIX) {
                format!("{path}{DISABLED_SUFFIX}")
            } else {
                path
            };

            let mut object = Map::new();
            object.insert("path".into(), json!(path));
            object.insert("hashes".into(), Value::Object(hashes));
            object.insert("env".into(), Value::Object(env));
            object.insert("downloads".into(), json!(file.downloads));
            if let Some(size) = file.size {
                // 规范里 `fileSize` 是 u32（≈4 GiB）：超了要具名报错，不静默截断。
                let size = u32::try_from(size).map_err(|_| {
                    PackError::limits(format!(
                        "文件 {path} 有 {size} 字节，超过 mrpack 的 fileSize 上限（u32，4294967295）"
                    ))
                    .with_hint("这种尺寸只能靠 overrides/ 内联，而内联又会撞 ZIP64 上限；先缩减实例")
                })?;
                object.insert("fileSize".into(), json!(size));
            }
            merge_unknown(&mut object, &file.extra);
            files.push(Value::Object(object));
        }
        root.insert("files".into(), Value::Array(files));

        merge_unknown(&mut root, &manifest.extra);
        serde_json::to_string_pretty(&Value::Object(root)).map_err(Into::into)
    }
}

/// `hashes.sha512` 必须是 128 位十六进制。
fn check_sha512(path: &str, hashes: &BTreeMap<String, String>) -> PackResult<()> {
    let Some(digest) = hashes.get("sha512") else {
        return Err(PackError::metadata(format!(
            "files[] 条目 {path} 缺 hashes.sha512（Modrinth 规范要求）"
        )));
    };
    if digest.len() != 128 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PackError::metadata(format!(
            "files[] 条目 {path} 的 sha512 不是 128 位十六进制"
        )));
    }
    Ok(())
}

/// 解析 `env.client` / `env.server`；不认识的取值直接拒绝（不静默降级成全都要）。
fn parse_env(path: &str, field: &str, raw: &str) -> PackResult<EnvSupport> {
    EnvSupport::parse(raw).ok_or_else(|| {
        PackError::metadata(format!(
            "files[] 条目 {path} 的 env.{field} 是 {raw:?}，只认 required/optional/unsupported"
        ))
    })
}
