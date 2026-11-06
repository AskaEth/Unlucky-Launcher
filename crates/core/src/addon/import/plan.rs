//! 清单 → **导入计划**：把三种格式的差异在进解压器之前全部拍平。
//!
//! 计划是纯计算（只额外读一次 `instance.cfg`），产出四张表：覆盖条目、远端条目、
//! 加载器计划输入、被拒绝的 component。落盘与合并都不在这里。
//!
//! # 三种格式的语义在这层定死
//!
//! | 事 | CurseForge | Modrinth | MultiMC / Prism |
//! |----|-----------|----------|-----------------|
//! | 覆盖目录 | 清单点名的**一个**目录（任意名字） | `overrides/` + `client-overrides/`（`server-overrides/` **不解压**） | `.minecraft/` |
//! | 同名冲突 | 不存在（单目录） | `client-overrides` **胜** `overrides`（Modrinth 官方客户端语义；与 HMCL 相反） | 不存在 |
//! | 远端条目 | `projectID/fileID`，**无路径无地址** ⇒ 必须补全 | 自带 `path`/`downloads`/`hashes` | 没有远端条目（内容就是目录） |
//! | 加载器 | `modLoaders[].id`（`neoforge-1.20.1-47.1.106` 的版本段**带 MC 前缀**） | `dependencies` 的键 | `components[].uid` |
//! # 三处"不许自创"的边界
//!
//! 1. **覆盖目录的优先级只按侧**：`overrides/` 先、`client-overrides/` 后（后者胜）。同一
//!    目录内的条目顺序**不重要**——M110 的 [`crate::addon::pack::PackOverrides::files`] 按路径
//!    排序，zip 条目顺序没有保留；而同一目录内路径唯一，所以"按 zip 条目顺序落盘"与"按路径
//!    排序落盘"在无重复时等价。有重复（跨目录）时由侧优先级决定，不靠条目顺序。
//! 2. **`.disabled` 只认路径后缀**（官方 Modrinth 表达），`.old` **只识别与保留**（不清不删
//!    不改名）。`env.client=optional` **不是**禁用（"可选"＝安装时可取消勾选）。判据在
//!    [`paths`]。
//! 3. **MultiMC 的非加载器 component 逐项进报告**（`org.lwjgl3` / `net.fabricmc.intermediary`
//!    / 社区自定义 uid）：静默丢它们等于"看起来装好了、其实少了补丁层"。

pub mod paths;

pub use paths::{curse_class_dir, strip_mc_prefix, suffix_note, suffix_variant, SuffixVariant};

use super::super::pack::{
    curseforge, multimc, EnvSupport, PackArchive, PackError, PackFile, PackLoader, PackManifest,
    PackOverrides, PackSide,
};
use super::error::ImportError;
use super::report::{keys, FileSource, ImportIssue, LoaderTarget, RejectedComponent};

/// 一条要落地的覆盖条目。
#[derive(Debug, Clone)]
pub struct PlanOverride {
    /// 包内目录前缀（读包时拼 `<dir>/<path>`）。
    pub dir: String,
    /// 实例内相对路径。
    pub path: String,
    /// 来源侧。
    pub source: FileSource,
    /// 包内实际 SHA-1（M110 读包时现算）。
    pub sha1: String,
    /// 字节数。
    pub size: u64,
    /// 是否禁用态（路径后缀）。
    pub disabled: bool,
    /// 可选注记（`.disabled` / `.old`）。
    pub note: Option<ImportIssue>,
}

/// 一条要补全/下载的远端条目。
#[derive(Debug, Clone)]
pub struct PlanRemote {
    /// 原清单条目（摘要表 / downloads / env / project/file id 都在里面）。
    pub file: PackFile,
    /// 已知的实例内相对路径（mrpack 有；CF 需要补全才有）。
    pub path: Option<String>,
    /// 来源。
    pub source: FileSource,
    /// 是否禁用态（路径后缀）。
    pub disabled: bool,
}

/// 一次导入的计划。
#[derive(Debug, Clone)]
pub struct ImportPlan {
    /// 命中的格式 id。
    pub format: String,
    /// 原始模型（`extra` 等诊断信息留着）。
    pub manifest: PackManifest,
    /// 游戏版本。
    pub mc_version: String,
    /// 加载器安装计划输入（`None` = 纯原版）。
    pub loader: Option<LoaderTarget>,
    /// 实例绑定版本 id。
    pub version_id: String,
    /// 不支持的 component（逐项）。
    pub rejected: Vec<RejectedComponent>,
    /// 注记。
    pub notes: Vec<ImportIssue>,
    /// 覆盖条目（已按侧优先级排好、已剔除 `server-overrides`）。
    pub overrides: Vec<PlanOverride>,
    /// 远端条目。
    pub remote: Vec<PlanRemote>,
    /// `instance.cfg` 里受开关门控的内存上限（MB）。
    pub max_memory_mb: Option<u64>,
}

impl ImportPlan {
    /// 组计划。
    ///
    /// # Errors
    ///
    /// 清单缺游戏版本、加载器条目缺版本号、MultiMC 的 `formatVersion != 1` 等"清单自身非法"
    /// 的情形返回 [`ImportError`]（调用方据此整包回滚）。"某条文件缺地址"**不是**错误。
    pub fn build(
        pack: &PackArchive,
        format_id: &str,
        manifest: PackManifest,
    ) -> Result<Self, ImportError> {
        let mut notes: Vec<ImportIssue> = Vec::new();
        let mut rejected: Vec<RejectedComponent> = Vec::new();

        // Prism 的规矩（A 级来源 `PackProfile.cpp:207-210`）：`formatVersion` 必须恰为 1，
        // 否则**整份丢弃**——不是降级解析。M110 的读者把缺省值当 1（老包不写这一行），
        // 因此这里只拒"写了非 1 的值"。
        if format_id == multimc::ID && manifest.format_version != Some(1) {
            return Err(ImportError::from(PackError::metadata(format!(
                "{} 的 formatVersion 是 {:?}，必须恰为 1（Prism 自己也是整份丢弃，不降级解析）",
                multimc::METADATA_ENTRY,
                manifest.format_version
            ))));
        }

        let mc_version = manifest
            .minecraft
            .as_deref()
            .map(str::trim)
            .filter(|version| !version.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                ImportError::from(PackError::metadata(
                    "清单里没有游戏版本（CF minecraft.version / mrpack dependencies.minecraft / MultiMC net.minecraft 组件）",
                ))
            })?;

        let loader = select_loader(&manifest, &mc_version, format_id, &mut notes, &mut rejected)?;
        let version_id = loader
            .as_ref()
            .map_or_else(|| mc_version.clone(), |target| target.version_id.clone());

        let mut overrides: Vec<PlanOverride> = Vec::new();
        let mut skipped_server_files = 0_usize;
        for group in ordered_overrides(&manifest) {
            if group.side == PackSide::Server {
                // Modrinth 官方客户端安装器就不解压 server-overrides/（A 级来源：
                // install_mrpack.rs 的 strip_prefix 链只有 overrides/ 与 client-overrides/）。
                skipped_server_files += group.files.len();
                continue;
            }
            for file in &group.files {
                let variant = suffix_variant(&file.path);
                overrides.push(PlanOverride {
                    dir: group.dir.clone(),
                    path: file.path.clone(),
                    source: FileSource::from_side(group.side),
                    sha1: file.sha1.clone(),
                    size: file.size,
                    disabled: variant.is_disabled(),
                    note: suffix_note(variant, &file.path),
                });
            }
        }
        if skipped_server_files > 0 {
            notes.push(ImportIssue::new(
                keys::NOTE_SERVER_OVERRIDES_SKIPPED,
                format!(
                    "server-overrides/ 里的 {skipped_server_files} 个文件是服务端内容，客户端导入不解压（Modrinth 官方语义）"
                ),
            ));
        }

        let mut remote: Vec<PlanRemote> = Vec::new();
        let source = if format_id == curseforge::ID {
            FileSource::CurseForge
        } else {
            FileSource::Modrinth
        };
        for file in &manifest.files {
            if file.client == EnvSupport::Unsupported {
                notes.push(
                    ImportIssue::new(
                        keys::NOTE_ENV_UNSUPPORTED,
                        "env.client=unsupported：客户端不需要这个文件，跳过".to_string(),
                    )
                    .with_path(file.path.clone().unwrap_or_else(|| file.key())),
                );
                continue;
            }
            let variant = file
                .path
                .as_deref()
                .map_or(SuffixVariant::Plain, suffix_variant);
            if file.client == EnvSupport::Optional {
                notes.push(
                    ImportIssue::new(
                        keys::NOTE_OPTIONAL_FILE,
                        "env.client=optional：可选条目照常安装（「可选」不等于「禁用」）"
                            .to_string(),
                    )
                    .with_path(file.path.clone().unwrap_or_else(|| file.key())),
                );
            }
            if let Some(note) = file
                .path
                .as_deref()
                .and_then(|path| suffix_note(variant, path))
            {
                notes.push(note);
            }
            remote.push(PlanRemote {
                path: file.path.clone(),
                source,
                disabled: variant.is_disabled(),
                file: file.clone(),
            });
        }

        let max_memory_mb = if format_id == multimc::ID {
            instance_memory(pack, &mut notes)?
        } else {
            None
        };

        Ok(Self {
            format: format_id.to_string(),
            manifest,
            mc_version,
            loader,
            version_id,
            rejected,
            notes,
            overrides,
            remote,
            max_memory_mb,
        })
    }
}

/// 覆盖目录按侧排序：`Both`（`overrides/`）先、`Client`（`client-overrides/`）后 = 后者胜。
fn ordered_overrides(manifest: &PackManifest) -> Vec<&PackOverrides> {
    let mut groups: Vec<&PackOverrides> = manifest.overrides.iter().collect();
    groups.sort_by_key(|group| match group.side {
        PackSide::Both => 0_u8,
        PackSide::Client => 1,
        PackSide::Server => 2,
    });
    groups
}

/// 选一个加载器并翻译成 [`LoaderTarget`]。
fn select_loader(
    manifest: &PackManifest,
    mc_version: &str,
    format_id: &str,
    notes: &mut Vec<ImportIssue>,
    rejected: &mut Vec<RejectedComponent>,
) -> Result<Option<LoaderTarget>, ImportError> {
    let recognized: Vec<&PackLoader> = manifest
        .loaders
        .iter()
        .filter(|loader| loader.kind.is_some())
        .collect();

    // 认不出的条目：MultiMC 是"不支持的 component"（逐项列），其它格式是"不认识的依赖键"。
    for loader in manifest
        .loaders
        .iter()
        .filter(|loader| loader.kind.is_none())
    {
        if format_id == multimc::ID {
            rejected.push(RejectedComponent {
                uid: loader.raw.clone(),
                version: loader.version.clone(),
                message_key: keys::REJECTED_COMPONENT,
            });
        } else {
            notes.push(ImportIssue::new(
                keys::NOTE_UNKNOWN_DEPENDENCY,
                format!("认不出的加载器/依赖键 {:?}：本启动器不安装它", loader.raw),
            ));
        }
    }

    if recognized.len() > 1 {
        notes.push(ImportIssue::new(
            keys::NOTE_LOADER_MULTIPLE,
            format!(
                "清单里有 {} 个可识别的加载器，只按 primary/第一条组计划：{}",
                recognized.len(),
                recognized
                    .iter()
                    .map(|loader| loader.raw.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }

    let Some(loader) = recognized
        .iter()
        .find(|loader| loader.primary)
        .or_else(|| recognized.first())
        .copied()
    else {
        return Ok(None);
    };
    let Some(kind) = loader.kind else {
        return Ok(None);
    };
    let raw_version = loader
        .version
        .as_deref()
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .ok_or_else(|| {
            ImportError::loader(format!(
                "加载器条目 {:?} 没有版本号，组不出安装计划（清单自身非法）",
                loader.raw
            ))
        })?;
    let loader_version = strip_mc_prefix(mc_version, raw_version);
    Ok(Some(LoaderTarget {
        kind,
        mc_version: mc_version.to_string(),
        version_id: kind.version_id(mc_version, &loader_version),
        loader_version,
        raw: loader.raw.clone(),
    }))
}

/// 从 `instance.cfg` 取受开关门控的内存上限（MB）。
fn instance_memory(
    pack: &PackArchive,
    notes: &mut Vec<ImportIssue>,
) -> Result<Option<u64>, ImportError> {
    let Some(text) = pack.read_text(multimc::INSTANCE_ENTRY)? else {
        return Ok(None);
    };
    let mut values: Vec<(&str, &str)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        // `[General]` 这类小节标记直接跳过：instance.cfg 平铺，真出现也不该把它们当键。
        if line.starts_with('[') {
            continue;
        }
        // 值里含 `:` 与 `\`（Windows 路径）是常态，所以只按**第一个** `=` 切。
        if let Some((key, value)) = line.split_once('=') {
            values.push((key.trim(), value.trim()));
        }
    }
    let get = |name: &str| -> Option<&str> {
        values
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| *value)
    };
    let Some(max) = get("MaxMemAlloc").and_then(|value| value.parse::<u64>().ok()) else {
        return Ok(None);
    };
    // Prism 的规矩：这些值只在开关为 true 时生效（只看值不看开关会误判）。
    let enabled = get("OverrideMemory").is_some_and(|value| value.eq_ignore_ascii_case("true"));
    if !enabled {
        notes.push(ImportIssue::new(
            keys::NOTE_INSTANCE_CFG_MEMORY,
            format!("instance.cfg 写了 MaxMemAlloc={max}，但 OverrideMemory 未开：不采用"),
        ));
        return Ok(None);
    }
    notes.push(ImportIssue::new(
        keys::NOTE_INSTANCE_CFG_MEMORY,
        format!("从 instance.cfg 取到内存上限 {max} MB（OverrideMemory=true）"),
    ));
    Ok(Some(max))
}
