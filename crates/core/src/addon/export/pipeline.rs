//! 导出管线：实例目录 → 三分类 → 清单 + `overrides/` → 包。
//!
//! 顺序是写死的，每一步都可单独测：
//!
//! ```text
//! ① 实例快照（snapshot）       versions/<id>/<id>.json 的继承链 + 库坐标 ⇒ game / dependencies
//! ② 扫描（scan）               内容目录 → 路径 + 字节数 + SHA-1 + 禁用位；读不了的进跳过表
//! ③ 逐文件判定（classify）     Modrinth SHA-1 → CF 指纹 → 目标格式能不能表达 ⇒ 远程 / 内联 / 跳过
//! ④ 组模型                    files[] = 远程；overrides = 内联（统一 `overrides/`）
//! ⑤ 写包                      走 M110 的 `PackFormat::write`（临时文件 + rename，失败不留半个包）
//! ```
//!
//! # 判定为什么要"目标格式能力"参与
//!
//! 「能被远程替代」不是文件自己的属性，是**文件 + 目标格式**的属性：同一个文件，CF 认得
//! `projectID/fileID` 就能写进 CF 的 `files[]`，但 mrpack 要 `sha512` 才写得出来；反过来
//! mrpack 认可 Modrinth 直链，CF 清单却只认两个整数 id。所以
//! [`crate::addon::export::report::FileReason`] 里 `CurseOnlyMatch` 与 `ModrinthOnlyMatch`
//! 是两个不同的事实，不能合并成"没找到"。
//!
//! # 一条禁令
//!
//! **不猜**：目录没给命中就内联；命中缺必需字段（`sha512` / `projectID`）也内联；只有
//! "两条硬要求都齐"才写进 `files[]`。这样导出的包在任何启动器里都能装出**同一份内容**，
//! 最多是"该远程的走了内联"（体积大一点，内容不会变）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Map;

use super::catalog::{OfflineCatalog, RemoteCatalog};
use super::classify::{classify, entry, remote_entry};
use super::report::{ExportFile, ExportReport, FileClass};
use super::scan::{scan_instance, DEFAULT_CONTENT_ROOTS, OVERRIDES_DIR};
use super::snapshot::{read_instance_snapshot, InstanceSnapshot};
use crate::addon::pack::curseforge;
use crate::addon::pack::error::{PackError, PackResult};
use crate::addon::pack::format::{PackFormat, PackPayload};
use crate::addon::pack::manifest::{
    OverrideFile, PackLoader, PackManifest, PackOverrides, PackSide,
};

/// 包版本缺省值（CF `version` / mrpack `versionId` 都必填，调用方没给就用它）。
pub const DEFAULT_VERSION: &str = "1.0.0";

/// 默认目录（离线）：所有文件内联。
static OFFLINE: OfflineCatalog = OfflineCatalog;

/// 一次导出的输入。
pub struct ExportRequest<'a> {
    root: &'a Path,
    target: &'a Path,
    format: &'static dyn PackFormat,
    name: Option<String>,
    version: Option<String>,
    version_id: Option<&'a str>,
    content_roots: Option<Vec<String>>,
    catalog: &'a dyn RemoteCatalog,
    snapshot: Option<&'a InstanceSnapshot>,
}

impl<'a> ExportRequest<'a> {
    /// 从 `root`（实例游戏目录）导出到 `target`，用 `format` 的清单形状。
    ///
    /// 默认：包名取目录名、版本 [`DEFAULT_VERSION`]、内容目录取 [`DEFAULT_CONTENT_ROOTS`]、
    /// 目录用 [`OfflineCatalog`]（= 全部内联）。
    #[must_use]
    pub fn new(root: &'a Path, target: &'a Path, format: &'static dyn PackFormat) -> Self {
        Self {
            root,
            target,
            format,
            name: None,
            version: None,
            version_id: None,
            content_roots: None,
            catalog: &OFFLINE,
            snapshot: None,
        }
    }

    /// 包名（写进清单 `name`）。
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// 包版本（写进 CF `version` / mrpack `versionId`）。
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// 实例绑定的版本 id；不给就要求 `versions/` 下只有一个版本。
    #[must_use]
    pub fn with_version_id(mut self, version_id: &'a str) -> Self {
        self.version_id = Some(version_id);
        self
    }

    /// 覆盖要扫的内容目录（默认 [`DEFAULT_CONTENT_ROOTS`]）。
    #[must_use]
    pub fn with_content_roots(mut self, roots: Vec<String>) -> Self {
        self.content_roots = Some(roots);
        self
    }

    /// 注入远端查表（默认离线：全部内联）。
    #[must_use]
    pub fn with_catalog(mut self, catalog: &'a dyn RemoteCatalog) -> Self {
        self.catalog = catalog;
        self
    }

    /// 直接给实例快照，跳过 `versions/<id>/<id>.json` 的读取。
    ///
    /// 用于两种实例：①**Prism / MultiMC 实例**（游戏目录里没有 `versions/`，快照只能从
    /// `mmc-pack.json` 的组件表来，调用方用 `pack::multimc` 读出来再传）；②调用方已经解析过
    /// 快照（UI 侧手里就有 profile + 加载器信息），不必再读一遍盘。
    #[must_use]
    pub fn with_snapshot(mut self, snapshot: &'a InstanceSnapshot) -> Self {
        self.snapshot = Some(snapshot);
        self
    }
}

/// 导出一个实例。
///
/// # Errors
///
/// - `pack_unsupported`：实例里找不到唯一的版本 JSON / 目标格式表达不了布局；
/// - `pack_metadata`：版本 JSON 坏了（继承成环、不是 JSON）；
/// - `pack_limits`：单文件 `fileSize` 超 u32、或整包撞 ZIP64 阈值；
/// - `pack_io`：读写盘失败（失败时目标路径不留半个包）。
pub fn export_instance(request: &ExportRequest<'_>) -> PackResult<ExportReport> {
    let snapshot = match request.snapshot {
        Some(snapshot) => snapshot.clone(),
        None => read_instance_snapshot(request.root, request.version_id)?,
    };
    let roots = request
        .content_roots
        .clone()
        .unwrap_or_else(|| DEFAULT_CONTENT_ROOTS.map(str::to_string).to_vec());
    let (locals, skipped) = scan_instance(request.root, &roots);

    // CF 清单只认 projectID/fileID；mrpack 要 path + sha512 + downloads。
    let target_is_curse = request.format.id() == curseforge::ID;
    let mut manifest = build_manifest(request, &snapshot);
    let mut entries: Vec<ExportFile> = Vec::with_capacity(locals.len() + skipped.len());
    let mut overrides: Vec<OverrideFile> = Vec::new();
    let mut sources: BTreeMap<String, PathBuf> = BTreeMap::new();

    for local in &locals {
        let (reason, hit) = classify(local, request.catalog, target_is_curse);
        match reason.class() {
            FileClass::Remote => {
                let Some(hit) = hit else {
                    // 判定逻辑的自我约束：Remote 必带命中。真走到这里说明分类函数被改坏了。
                    return Err(PackError::unsupported(format!(
                        "内部不一致：{} 被判为远程引用却没有命中",
                        local.path
                    )));
                };
                manifest.files.push(remote_entry(local, &hit));
                entries.push(entry(local, reason, Some(&hit)));
            }
            FileClass::Inline => {
                overrides.push(OverrideFile {
                    path: local.path.clone(),
                    sha1: local.sha1.clone(),
                    size: local.bytes,
                });
                sources.insert(
                    format!("{OVERRIDES_DIR}/{}", local.path),
                    local.disk.clone(),
                );
                entries.push(entry(local, reason, None));
            }
            FileClass::Skipped => unreachable!("扫描阶段不会产出可导出项"),
        }
    }
    // 跳过项照原样进报告（逐条给原因），不进包。
    for item in &skipped {
        entries.push(ExportFile {
            path: item.path.clone(),
            class: FileClass::Skipped,
            reason: item.reason,
            bytes: 0,
            downloads: Vec::new(),
            project_id: None,
            file_id: None,
            disabled: false,
        });
    }

    if !overrides.is_empty() {
        // 一律 `overrides/`（Modrinth 官方 + Prism 的写法）。实例目录不分侧，所以只有一个 Both 组。
        manifest.overrides.push(PackOverrides {
            dir: OVERRIDES_DIR.to_string(),
            side: PackSide::Both,
            files: overrides,
        });
    }

    request.format.write(
        &manifest,
        PackPayload::with_sources(request.root, &sources),
        request.target,
    )?;
    let bytes = std::fs::metadata(request.target)
        .map_err(|error| {
            PackError::io(format!(
                "量 {} 的字节数失败：{error}",
                request.target.display()
            ))
        })?
        .len();

    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let notes = notes(target_is_curse, &manifest);
    Ok(ExportReport {
        format: request.format.id(),
        label: request.format.label(),
        path: request.target.to_path_buf(),
        bytes,
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        minecraft: manifest.minecraft.clone(),
        loaders: manifest
            .loaders
            .iter()
            .map(|loader| {
                format!(
                    "{} {}",
                    loader.dependency_key(),
                    loader.version.clone().unwrap_or_default()
                )
            })
            .collect(),
        files: entries,
        notes,
    })
}

/// 组清单：`game` / `minecraft` / `loaders` 全部来自实例快照。
fn build_manifest(request: &ExportRequest<'_>, snapshot: &InstanceSnapshot) -> PackManifest {
    let name = request
        .name
        .clone()
        .or_else(|| {
            request
                .root
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "整合包".to_string());
    let mut manifest = PackManifest::new(name);
    manifest.version = Some(
        request
            .version
            .clone()
            .unwrap_or_else(|| DEFAULT_VERSION.to_string()),
    );
    manifest.format_version = Some(1);
    manifest.game = Some(crate::addon::pack::modrinth::GAME.to_string());
    manifest.minecraft = Some(snapshot.game_version.clone());
    manifest.loaders = snapshot
        .loaders
        .iter()
        .enumerate()
        .map(|(index, loader)| PackLoader {
            raw: String::new(),
            kind: Some(loader.kind),
            version: Some(loader.version.clone()),
            primary: index == 0,
            extra: Map::new(),
        })
        .collect();
    manifest
}

/// 格式层面表达不了的东西（已知损失）——必须显式列出来，别让用户以为往返无损。
fn notes(target_is_curse: bool, manifest: &PackManifest) -> Vec<String> {
    let mut notes = vec![
        "覆盖内容统一写进 `overrides/`（Modrinth 官方与 Prism 的写法）；实例目录不分侧，\
         所以不产生 client-overrides / server-overrides。"
            .to_string(),
        "禁用文件用**路径后缀 `.disabled`** 表达（不是 `env.client=optional`：后者装回来默认启用）。"
            .to_string(),
        "MultiMC / Prism 导出本轮不做（只有导入），这是有意省略，不是遗漏。".to_string(),
    ];
    if target_is_curse {
        notes.push("mrpack 的 `summary` 在 CF 清单里没有字段；CF 也只认 projectID/fileID，禁用文件只能内联。".to_string());
    } else {
        notes.push(
            "CF 清单的 `author` / `manifestVersion` 之外的扩展字段在 mrpack 里没有对应位置。"
                .to_string(),
        );
    }
    if !manifest.files.is_empty() {
        notes.push(format!(
            "{} 条写进 files[]（远程引用），其余在 overrides/ 里；每条的原因见 files[].reason。",
            manifest.files.len()
        ));
    }
    notes
}
