//! 从 mod jar 里**只读元数据**（M312 · MODDUP）。
//!
//! # 三种加载器，四份文件
//!
//! | 加载器 | 文件 | 里面的 id / 版本 / 前置 |
//! |---|---|---|
//! | Fabric / Quilt | `fabric.mod.json` | `id` / `version` / `depends`（必需）+ `recommends`（可选） |
//! | NeoForge | `META-INF/neoforge.mods.toml` | `[[mods]]` 的 `modId` / `version`；`[[dependencies.<id>]]` 的 `mandatory` |
//! | Forge | `META-INF/mods.toml` | 同上（文件名不同） |
//! | 旧版 FML | `mcmod.info` | `modid` / `version` / `requiredMods` + `dependencies` |
//!
//! 四种都落成同一个 [`ModMeta`]（前置是 [`ModDependency`]，必需位只有一条判据），
//! 所以「重复」与「前置缺失」的判定不用管它是哪种加载器的 mod。
//!
//! # 读不到就**优雅降级**
//!
//! 这里没有「错误」这个概念，只有 [`MetaGap`]（为什么没读到）。调用方
//! （[`fn@super::inspect`]）拿它当「普通 jar」：文件名照列、启停照用，只是不参与前置判定。
//! 读不出来的原因**不弹给用户**——一个 `.jar` 里没有元数据是正常现象（coremod、资源包改后缀、
//! 手放的占位文件），报错只会制造噪音。
//!
//! # jar 是 zip，所以按**不可信输入**对待
//!
//! 与 `addon::pack::archive::limits` 同一套思路（那边是整合包，这边是 jar），但门槛更紧——
//! 这里的四个文件加起来通常不到 10 KiB：
//!
//! | 闸门 | 值 | 挡什么 |
//! |---|---|---|
//! | [`MAX_ENTRIES`] | 20000 | 条目数爆炸（扫名字本身也要遍历中央目录） |
//! | [`MAX_META_BYTES`] | 2 MiB | 单条元数据（真实最大的 `mods.toml` 也才几十 KiB） |
//! | [`MAX_META_TOTAL`] | 4 MiB | 一个 jar 里四份候选文件累计（候选全在时也够） |
//! | [`MAX_RATIO`] | 200 | 压缩炸弹：声明 1 KiB 解出 2 GiB 那种（只在 > [`RATIO_CHECK_BYTES`] 时判） |
//!
//! 另外两道「只读元数据」的纪律：
//!
//! 1. **只解候选条目**：先用 `file_names`（只读中央目录、一个字节都不解压）挑出候选，
//!    再对命中的那几个调 `by_name`。不遍历内容、不落盘。
//! 2. **按实际读出的字节封顶**（`Read::take(cap + 1)`）：中央目录声明的大小可以撒谎，
//!    所以封顶在**读**这一侧再做一次，与 `addon::pack::archive::limits::read_capped` 同一条。
//!
//! # 已知边界
//!
//! - **`mods.toml` 用的是手写的 TOML 子集**，不是完整 TOML。原因：本模块的 crate 清单
//!   （`crates/core/Cargo.toml`）不在 M312 的改动范围内，不能为这一件事引 `toml` 依赖。
//!   子集覆盖 `[[表]]` / `键 = "串"` / `键 = true|false` / `键 = 裸字面量`，够 `mods.toml` 用；
//!   认不出的行**跳过**（不猜、不 panic）。
//! - **一个 jar 只认第一个 mod**：`mods.toml` 里可以声明多个 `[[mods]]`（库 + 主 mod）。
//!   这里取第一个当身份，前置也只取它的——多 mod 的 jar 里第二个 id 被别的前置引用时，
//!   会报成「缺失」。宁可少报的那一侧不在这个边界上，所以本模块把第一个 mod 的 id 记全。
//! - **不解版本范围表达式**：只把原文带出去（[`ModDependency::version_range`]），
//!   判定在 [`mod@super::report`] 里只做「能确认的几种写法」，其余报「无法判定」。

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::diagnose::collectors::ModDependency;

mod forge;
mod json;

#[cfg(test)]
mod tests;

/// Fabric / Quilt 的元数据文件（jar 根目录）。
pub const FABRIC_META: &str = "fabric.mod.json";
/// NeoForge 的元数据文件。
pub const NEOFORGE_META: &str = "META-INF/neoforge.mods.toml";
/// Forge 的元数据文件。
pub const FORGE_META: &str = "META-INF/mods.toml";
/// 旧版 FML 的元数据文件。
pub const LEGACY_META: &str = "mcmod.info";

/// 中央目录条目数上限（一个 mod jar 通常几十条；20000 只挡病态包）。
pub const MAX_ENTRIES: usize = 20_000;
/// 单份元数据的上限（2 MiB）。
pub const MAX_META_BYTES: u64 = 2 * 1024 * 1024;
/// 一个 jar 里元数据累计上限（4 MiB）。
pub const MAX_META_TOTAL: u64 = 4 * 1024 * 1024;
/// 压缩比上限（解压 / 压缩）。
pub const MAX_RATIO: u64 = 200;
/// 只有在声明的解压体积超过这个数时才查压缩比（小文件比值没意义）。
pub const RATIO_CHECK_BYTES: u64 = 256 * 1024;

/// 元数据来自哪份文件（只用于证据与用例，不进 DTO）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaSource {
    /// `fabric.mod.json`
    Fabric,
    /// `META-INF/neoforge.mods.toml`
    NeoForge,
    /// `META-INF/mods.toml`
    Forge,
    /// `mcmod.info`
    Legacy,
}

impl MetaSource {
    /// 这份来源的文件名。
    #[must_use]
    pub const fn file(self) -> &'static str {
        match self {
            Self::Fabric => FABRIC_META,
            Self::NeoForge => NEOFORGE_META,
            Self::Forge => FORGE_META,
            Self::Legacy => LEGACY_META,
        }
    }

    /// 读同一份 jar 时的优先级（小 = 先读）。
    const fn rank(self) -> u8 {
        match self {
            Self::Fabric => 0,
            Self::NeoForge => 1,
            Self::Forge => 2,
            Self::Legacy => 3,
        }
    }
}

/// 一个 mod jar 的元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModMeta {
    /// modid（**小写**：匹配前置时大小写不敏感）。
    pub id: String,
    /// 展示名（`fabric.mod.json` 的 `name` / `mods.toml` 的 `displayName`）。
    pub name: Option<String>,
    /// 版本（`${…}` 这种没被构建脚本替换掉的占位符算「不知道」）。
    pub version: Option<String>,
    /// 声明的前置（含必需位）。
    pub deps: Vec<ModDependency>,
    /// 从哪份文件读来的。
    pub source: MetaSource,
}

/// 没读到元数据的原因（**不是错误**，调用方当普通 jar 处理）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaGap {
    /// 打不开 / 不是 zip（`notes.txt`、半截下载）。
    NotAJar,
    /// 是 zip，但读不出中央目录或条目。
    Unreadable,
    /// 触到 `MAX_ENTRIES` / `MAX_META_BYTES` / `MAX_META_TOTAL` / `MAX_RATIO`。
    TooBig,
    /// 四份候选文件一份都没有（普通 jar / coremod / 只装资源的包）。
    NoMetadata,
    /// 有候选文件，但内容解析不出来（截断的 JSON / 认不出的 TOML）。
    Malformed,
}

impl MetaGap {
    /// 一句话说明（只进日志与证据，不上屏）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotAJar => "不是 zip",
            Self::Unreadable => "读不出条目",
            Self::TooBig => "触碰体积限额",
            Self::NoMetadata => "没有元数据文件",
            Self::Malformed => "元数据解析不出来",
        }
    }

    /// 这个原因值不值得进「有疑点」计数。
    ///
    /// `NotAJar` / `NoMetadata` 是**正常**的（不是每个 jar 都有元数据、也不是每个文件都是 jar），
    /// 另三种才是「本该读到却没读到」。
    #[must_use]
    pub const fn is_suspicious(self) -> bool {
        matches!(self, Self::Unreadable | Self::TooBig | Self::Malformed)
    }
}

/// 读一个文件的元数据。
///
/// 打开失败、不是 zip、没有元数据、解析不出来**都不是错误**——返回值是「为什么没读到」，
/// 由调用方决定要不要把它当回事。
pub fn read(path: &Path) -> Result<ModMeta, MetaGap> {
    let file = File::open(path).map_err(|_| MetaGap::Unreadable)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| MetaGap::NotAJar)?;
    if archive.len() > MAX_ENTRIES {
        return Err(MetaGap::TooBig);
    }

    // 第一步：只扫中央目录里的名字（`file_names` 不解压任何条目），挑出候选。
    let mut candidates: Vec<(String, MetaSource)> = Vec::new();
    for name in archive.file_names() {
        let Ok(name) = name else { continue };
        let Some(source) = source_of(&name) else {
            continue;
        };
        candidates.push((name.to_string(), source));
    }
    if candidates.is_empty() {
        return Err(MetaGap::NoMetadata);
    }
    // 同一个 jar 里理论上只会有一份；真有多的就按固定优先级读（顺序不随中央目录抖动）。
    candidates.sort_by_key(|(name, source)| (source.rank(), name.clone()));

    let mut spent = 0_u64;
    let mut worst = MetaGap::NoMetadata;
    for (name, source) in candidates {
        let bytes = match read_entry(&mut archive, &name, &mut spent) {
            Ok(bytes) => bytes,
            Err(gap) => {
                worst = gap;
                continue;
            }
        };
        let text = String::from_utf8_lossy(&bytes);
        let parsed = match source {
            MetaSource::Fabric => json::fabric(&text),
            MetaSource::Legacy => json::legacy(&text),
            MetaSource::Forge | MetaSource::NeoForge => forge::mods_toml(&text),
        };
        if let Some(mut meta) = parsed {
            meta.source = source;
            return Ok(meta);
        }
        worst = MetaGap::Malformed;
    }
    Err(worst)
}

/// 条目名 → 是哪种元数据（大小写不敏感：Windows 上解出来的 jar 偶尔会变大小写）。
fn source_of(name: &str) -> Option<MetaSource> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "fabric.mod.json" => Some(MetaSource::Fabric),
        "meta-inf/neoforge.mods.toml" => Some(MetaSource::NeoForge),
        "meta-inf/mods.toml" => Some(MetaSource::Forge),
        "mcmod.info" => Some(MetaSource::Legacy),
        _ => None,
    }
}

/// 读一个条目，按**实际读出的字节**封顶（中央目录声明的大小可以撒谎）。
fn read_entry(
    archive: &mut zip::ZipArchive<File>,
    name: &str,
    spent: &mut u64,
) -> Result<Vec<u8>, MetaGap> {
    let entry = archive.by_name(name).map_err(|_| MetaGap::Unreadable)?;
    let declared = entry.size();
    let compressed = entry.compressed_size();
    if declared > MAX_META_BYTES {
        return Err(MetaGap::TooBig);
    }
    if declared > RATIO_CHECK_BYTES && compressed > 0 && declared / compressed > MAX_RATIO {
        return Err(MetaGap::TooBig);
    }
    if *spent >= MAX_META_TOTAL {
        return Err(MetaGap::TooBig);
    }
    let cap = MAX_META_BYTES.min(MAX_META_TOTAL - *spent);
    let mut bytes = Vec::new();
    entry
        .take(cap + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| MetaGap::Unreadable)?;
    let actual = bytes.len() as u64;
    if actual > cap {
        return Err(MetaGap::TooBig);
    }
    *spent += actual;
    Ok(bytes)
}

/// 去掉段首尾空白并把空串当「没有」。
pub(super) fn non_empty(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// 版本串的清洗：`${file.jarVersion}` 这类没被构建脚本替换的占位符算「不知道」。
pub(super) fn version(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.contains("${") || text == "*" {
        return None;
    }
    Some(text.to_string())
}
