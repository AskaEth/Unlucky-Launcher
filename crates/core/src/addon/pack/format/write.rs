//! 把模型写成 zip：**声明尺寸预检 → 写清单 → 逐条搬覆盖内容**（`format.rs` 只留 trait 与注册表）。
//!
//! # 为什么预检必须在读第一个字节之前
//!
//! 一条写死的边界（research/18-p2 §8.6，A 级 `export_mrpack.rs:305-311`）：**不写 ZIP64 包**。
//! Modrinth 官方导出撞到阈值直接报错，HMCL 那边 `(int)` 截断只打 warning 是反面教材。
//!
//! 早先的实现是「读一个文件 → 累加 → 比较」：真要导出一个 4 GiB 覆盖文件的实例，
//! `std::fs::read` 先把它整份读进内存，闸门**永远够不着**——用户看到的是内存打爆或 IO 错，
//! 而不是"包太大，把这个文件移出实例"。所以现在先跑一遍 `metadata().len()` 累加
//! （`preflight`），超阈值在任何内容被读进内存**之前**就报 `pack_limits`。
//!
//! 写循环里那份增量检查保留，当第二道：预检与写入之间文件可能变大（实例目录不是快照）。
//! **第二道自己从清单长度起算**，绝不拿预检的返回值当基数——两者合起来会重复累计，
//! 于是真实总量只要过半阈值（~2.15 GiB）就被误拒，而且拒在内容已经读出来之后（P2-1）。
//!
//! # 内容按块搬运，不整份读进内存
//!
//! 覆盖文件用 1 MiB 缓冲流式写进 zip（`COPY_CHUNK`）。这不是优化癖：`std::fs::read` 会把
//! 一份 2.5 GiB 的 mod 整份摊在内存里，恰恰破坏"闸在内容进内存之前"这条性质——即使判据
//! 后来修对了，误拒也发生在把几个 GiB 读进内存之后。流式之后第二道闸看的是**已写字节数**，
//! 与内存占用无关。
//!
//! # 阈值与声明口径（**与导入读取同一份常量**）
//!
//! 判据是**未压缩声明的总量**（清单 JSON + 每个覆盖文件的字节数）**与单条目体积**，两个数
//! 都来自 `pack::archive::limits`（[`WriteLimits::production`]）：以前写侧直接拿
//! `zip::ZIP64_BYTES_THR`（≈4 GiB），导入读取只允许 512 MiB ⇒「导得出来、导不回去」。现在
//! 只此一份口径，关系是
//! `PACK_MAX_ENTRY_BYTES ≤ PACK_MAX_UNCOMPRESSED_BYTES < PACK_ZIP64_BYTES_THR`，
//! 所以 ZIP64 阈值永远不会被触碰（`pack/tests/limits.rs` 钉住这条不等式）。
//! zip 条目头与中央目录的几十字节开销不计——这个常数本身就是近似边界，官方实现同样按这个
//! 粗粒度判。

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::super::error::{PackError, PackResult};
use super::super::manifest::PackManifest;
use super::super::path::{normalize_dir, normalize_file};
use super::payload::PackPayload;

/// 搬覆盖内容时的缓冲大小（流式，不把文件整份读进内存）。
const COPY_CHUNK: usize = 1024 * 1024;

/// 与目标同目录的临时文件路径（同卷 `rename` 才能原子替换）。
pub(super) fn temporary_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().map_or_else(
        || std::ffi::OsString::from("pack"),
        std::ffi::OsStr::to_os_string,
    );
    name.push(format!(".{}.part", std::process::id()));
    target.with_file_name(name)
}

/// 真正写 zip 的那一段（失败了由调用方删临时文件）。
pub(super) fn container(
    format_id: &str,
    entry: &'static str,
    json: &str,
    manifest: &PackManifest,
    payload: PackPayload<'_>,
    temp: &Path,
    limits: WriteLimits,
) -> PackResult<()> {
    // 先判总量再碰磁盘：超限时临时文件都不建，更不会读进一份几百 MiB 的内容。
    // **返回值只属于预检**：写循环有自己的计数器（`Written`），拿它当基数会把全量声明尺寸
    // 再累加一遍 ⇒ 过拒窗口（P2-1）。
    let _ = preflight(json.len() as u64, manifest, payload, limits)?;

    let file = File::create(temp)
        .map_err(|error| PackError::io(format!("创建 {} 失败：{error}", temp.display())))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    writer
        .start_file(entry, options)
        .map_err(|error| PackError::io(format!("写 {entry} 失败：{error}")))?;
    writer
        .write_all(json.as_bytes())
        .map_err(|error| PackError::io(format!("写 {entry} 失败：{error}")))?;

    // 第二道闸的计数器：**从清单长度重新起算**，与预检的结果无关（见 `Written`）。
    let mut written = Written::new(json.len() as u64, limits.total);
    let mut buffer = vec![0_u8; COPY_CHUNK];
    for group in &manifest.overrides {
        let dir = normalize_dir(&group.dir)?;
        for override_file in &group.files {
            let path = normalize_file(&override_file.path)?;
            let name = format!("{dir}/{path}");
            let source = source_for(payload, &dir, &path)?;
            let mut source_file = File::open(&source).map_err(|error| {
                PackError::io(format!("读覆盖文件 {} 失败：{error}", source.display()))
            })?;
            writer
                .start_file(name.clone(), options)
                .map_err(|error| PackError::io(format!("写 {name} 失败：{error}")))?;
            loop {
                let read = source_file.read(&mut buffer).map_err(|error| {
                    PackError::io(format!("读覆盖文件 {} 失败：{error}", source.display()))
                })?;
                if read == 0 {
                    break;
                }
                writer
                    .write_all(&buffer[..read])
                    .map_err(|error| PackError::io(format!("写 {name} 失败：{error}")))?;
                written.spend(read as u64)?;
            }
        }
    }
    writer
        .finish()
        .map_err(|error| PackError::io(format!("收尾 zip 失败：{error}（格式 {format_id}）")))?;
    Ok(())
}

/// 写盘前的**声明尺寸预检**：只量 `metadata().len()`，不读内容。
///
/// 返回值是"清单 + 全部覆盖文件的声明总量"，**只给测试与诊断看**。写循环有自己的计数器
/// （从清单长度起算）——曾经把它直接当累加基数，结果每个文件的字节被算两遍，真实总量过半
/// 阈值就被误拒（P2-1，已有端到端用例钉住这个窗口）。
///
/// 判据顺序是**先总量、后单条**：总量超限时给出的那句"包太大"更贴用户的实际处境
/// （"把这个文件移出实例"只对单条超限才有意义）。
fn preflight(
    json_len: u64,
    manifest: &PackManifest,
    payload: PackPayload<'_>,
    limits: WriteLimits,
) -> PackResult<u64> {
    // **条目数闸**（读侧同一条：`PACK_MAX_ENTRIES`）。放在读盘之前：超 2 万条时连一次
    // `metadata()` 都不必做，也顺手证明"闸在碰内容之前"。清单条目自己也占一条。
    let declared_entries = 1 + manifest
        .overrides
        .iter()
        .map(|group| group.files.len())
        .fold(0_usize, usize::saturating_add);
    guard_entries(declared_entries, limits.entries)?;

    let mut total = json_len;
    let mut entries: Vec<(String, u64)> = Vec::new();
    for group in &manifest.overrides {
        let dir = normalize_dir(&group.dir)?;
        for override_file in &group.files {
            let path = normalize_file(&override_file.path)?;
            let source = source_for(payload, &dir, &path)?;
            // 缺文件 / 读不到元数据在这里就报（与"读内容失败"同一个 `pack_io` 口径：
            // 目标路径上不会留下半个包，临时文件由调用方删）。
            let length = std::fs::metadata(&source)
                .map_err(|error| {
                    PackError::io(format!("读覆盖文件 {} 失败：{error}", source.display()))
                })?
                .len();
            total += length;
            entries.push((format!("{dir}/{path}"), length));
        }
    }
    guard_with(total, limits.total)?;
    for (name, length) in entries {
        guard_entry(&name, length, limits.entry)?;
    }
    Ok(total)
}

/// 写侧的限额口径（**默认值直接取读侧那三个常量**，见 `archive::limits` 的模块文档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WriteLimits {
    /// 未压缩总量上限。
    pub(super) total: u64,
    /// 单条目上限。
    pub(super) entry: u64,
    /// 条目数上限（**与读侧同一个数**：清单条目 + 每个覆盖文件）。
    pub(super) entries: usize,
}

impl WriteLimits {
    /// 生产口径：与 [`PackLimits::default`](crate::addon::pack::PackLimits) 同一个数。
    pub(super) const fn production() -> Self {
        Self {
            total: super::super::archive::limits::PACK_MAX_UNCOMPRESSED_BYTES,
            entry: super::super::archive::limits::PACK_MAX_ENTRY_BYTES,
            entries: super::super::archive::limits::PACK_MAX_ENTRIES,
        }
    }

    /// 显式给值（测试用：把边界缩小到可测尺寸）。
    #[cfg(test)]
    pub(super) const fn new(total: u64, entry: u64, entries: usize) -> Self {
        Self {
            total,
            entry,
            entries,
        }
    }
}

/// 条目数闸：与读侧的 `PACK_MAX_ENTRIES` **同一份口径**。
///
/// 读侧在 `archive::open` 里对"中央目录条数"判 `>= limit` 就拒；写侧产出的是"清单 + N 个覆盖
/// 文件"，比读侧少掉目录条目，所以这里按同一侧的口径判（`count > limit` 拒，`== limit` 许可，
/// 与读侧能读回的最大值一致）。没有这道闸时，一个内容目录超过 2 万条的实例**导得出来、
/// 自己的导入器拒读**——正是"M152 遗留的限额不衔接"那条。
fn guard_entries(count: usize, entries_limit: usize) -> PackResult<()> {
    if count > entries_limit {
        return Err(PackError::limits(format!(
            "包内条目数 {count} 超过统一上限 {entries_limit}（导入读取用的也是这个数）"
        ))
        .with_hint("把内容目录里多余的文件移出实例后再导出；单文件与总量上限也是同一份口径"));
    }
    Ok(())
}

/// 单条目闸：与读侧的 `PACK_MAX_ENTRY_BYTES` **同一份口径**。
///
/// 没有这道闸时，"导出 100 MiB 的覆盖文件 → 自己的导入器拒读"是能真实发生的（读侧把一条
/// 内容整块读进内存，所以有单条上限）。写侧照着读侧的上限拒，才不会产出自己读不回来的包。
fn guard_entry(name: &str, length: u64, entry_limit: u64) -> PackResult<()> {
    if length > entry_limit {
        return Err(PackError::limits(format!(
            "覆盖条目 {name} 有 {length} 字节，超过单条目统一上限 {entry_limit}（导入读取用的也是这个数）"
        ))
        .with_hint("把该文件移出实例（或换一个不需要内联它的内容目录）后再导出"));
    }
    Ok(())
}

/// 单条覆盖文件的来源：来源表优先（跨格式导出用），否则按 `<root>/<dir>/<path>` 拼。
fn source_for(payload: PackPayload<'_>, dir: &str, path: &str) -> PackResult<PathBuf> {
    let name = format!("{dir}/{path}");
    if let Some(source) = payload.source_for(&name) {
        return Ok(source.to_path_buf());
    }
    let Some(root) = payload.root() else {
        return Err(PackError::unsupported(format!(
            "导出需要覆盖内容（{name}），但既没有来源映射也没有 payload 根目录"
        )));
    };
    Ok(root.join(dir).join(path))
}

/// 写循环的第二道闸（**已写出**的未压缩字节数）。
///
/// 起点是**清单 JSON 的长度**，与 `preflight` 的返回值无关：两者混用会把每个文件的字节算
/// 两遍，真实总量过半阈值（~2.15 GiB）就被误拒（P2-1）。类型上也没法混用——`Written`
/// 由 `u64` 构造，而预检的返回值是"全量声明尺寸"，只有显式取值才拿得到。
struct Written {
    total: u64,
    limit: u64,
}

impl Written {
    const fn new(json_len: u64, limit: u64) -> Self {
        Self {
            total: json_len,
            limit,
        }
    }

    /// 记一笔实际写出的字节，并在**每一块之后**过闸（大文件因此能在中途被拦下）。
    fn spend(&mut self, bytes: u64) -> PackResult<()> {
        self.total = self.total.saturating_add(bytes);
        guard_with(self.total, self.limit)
    }
}

/// 总量闸（`pack_limits`）：`total` **达到**上限就拒（`>=` 不是 `>`，撞上就不写）。
///
/// 生产路径的 `limit` 来自 [`WriteLimits::production`]（= 读侧 `PACK_MAX_UNCOMPRESSED_BYTES`）；
/// 把它做成参数是为了能用几十字节的小数值钉住"边界在 `>=` 上"和"写循环不许重复累计"这两件事。
fn guard_with(total: u64, limit: u64) -> PackResult<()> {
    if total >= limit {
        return Err(PackError::limits(format!(
            "包体未压缩总量超过统一上限 {limit} 字节（ZIP64 阈值 {} 远在其上，所以永远不需要写 ZIP64）",
            super::super::archive::limits::PACK_ZIP64_BYTES_THR
        ))
        .with_hint("把超大的覆盖文件移出实例后再导出；导入读取用的是同一个上限"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
