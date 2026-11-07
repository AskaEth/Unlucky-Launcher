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
//! # 阈值与声明口径
//!
//! 判据是**未压缩声明的总量**（清单 JSON + 每个覆盖文件的字节数）≥ `zip::ZIP64_BYTES_THR`
//! （≈4 GiB）。zip 条目头与中央目录的几十字节开销不计——这个常数本身就是近似边界，
//! 官方实现同样按这个粗粒度判。

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
    limit: u64,
) -> PackResult<()> {
    // 先判总量再碰磁盘：超限时临时文件都不建，更不会读进一份 4 GiB 的内容。
    // **返回值只属于预检**：写循环有自己的计数器（`Written`），拿它当基数会把全量声明尺寸
    // 再累加一遍 ⇒ 过拒窗口（P2-1）。
    let _ = preflight(json.len() as u64, manifest, payload, limit)?;

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
    let mut written = Written::new(json.len() as u64, limit);
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
/// 阈值（~2.15 GiB）就被误拒（P2-1，已有 2.5 GB 的端到端用例钉住这个窗口）。
fn preflight(
    json_len: u64,
    manifest: &PackManifest,
    payload: PackPayload<'_>,
    limit: u64,
) -> PackResult<u64> {
    let mut total = json_len;
    guard_with(total, limit)?;
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
            guard_with(total, limit)?;
        }
    }
    Ok(total)
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

/// ZIP64 阈值闸（`pack_limits`）：`total` **达到**阈值就拒（`>=` 不是 `>`，官方实现也是
/// "撞上就不写"）。
///
/// 生产路径的 `limit` 恒为 `zip::ZIP64_BYTES_THR`（由 `format.rs` 传下来）；把它做成参数是为了
/// 能用几十字节的小数值钉住"边界在 `>=` 上"和"写循环不许重复累计"这两件事——真造 4 GiB 的
/// 包要几十秒 CPU 与一大片内存/页面文件，本机资源纪律下不经默认套件跑（塔台 2026-11-07）。
fn guard_with(total: u64, limit: u64) -> PackResult<()> {
    if total >= limit {
        return Err(PackError::limits(format!(
            "包体未压缩总量已超过 {limit} 字节（ZIP64 阈值），拒绝写出一个需要 ZIP64 的整合包"
        ))
        .with_hint("把超大的覆盖文件移出实例后再导出；mrpack/CF 都不接受 ZIP64"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addon::pack::manifest::{OverrideFile, PackOverrides, PackSide};

    #[test]
    fn the_threshold_is_reached_not_exceeded() {
        // `>=` 而不是 `>`：恰好等于阈值就已经需要 ZIP64 了。
        assert!(guard_with(99, 100).is_ok());
        let error = guard_with(100, 100).expect_err("等于阈值必须拒");
        assert_eq!(
            error.kind(),
            super::super::super::error::PackErrorKind::Limits
        );
        assert!(error.message().contains("100"), "{error}");
        assert!(guard_with(101, 100).is_err());
    }

    #[test]
    fn the_second_gate_counts_from_the_manifest_length_not_the_preflight_total() {
        // P2-1 的窗口：真实总量在 [阈值一半, 阈值) 之间的包**不许**被拒。这一条不真写
        // 2.5 GB（那要 ~70 秒 deflate 与一大片内存/页面文件），而是直接把窗口里的字节数
        // 喂给第二道闸的计数器——它的起点是清单长度，所以只有"把预检的全量 total 当基数"
        // 那种写法才会在这里红（红法是 `spend` 返回 Limits）。
        let half = zip::ZIP64_BYTES_THR / 2;
        let mut written = Written::new(28, zip::ZIP64_BYTES_THR);
        written
            .spend(2_500_000_000)
            .expect("2.5 GB 没到 4 GiB，第二道闸不许拒");
        assert!(2_500_000_000 > half, "样本要落在过拒窗口里");

        // 对照：撞上阈值必须拒，且正好在阈值处（不是超一点点才拒）。
        let mut written = Written::new(28, zip::ZIP64_BYTES_THR);
        let error = written
            .spend(zip::ZIP64_BYTES_THR)
            .expect_err("撞阈值必须拒");
        assert_eq!(
            error.kind(),
            super::super::super::error::PackErrorKind::Limits
        );
    }

    #[test]
    fn the_container_does_not_double_count_the_preflight_total() {
        // 容器层的窗口回归钉（P2-1），**用几十字节的 limit 把窗口缩小到可测尺寸**：
        // `json` 30 字节 + 覆盖文件 40 字节 = 声明总量 70。
        //  - 正确实现：第二道闸从 30 起算，写到 70 也不到 100 ⇒ 导出成功；
        //  - P2-1 的写法（把预检返回的全量 70 当基数）：写循环再累加 40 ⇒ 110 ≥ 100 ⇒ 误拒。
        // 真造 2.5 GB 才能暴露同一个 bug，本机资源纪律下不跑（那条留成 `zip64.rs` 里的
        // `#[ignore]` 用例）。
        let root = std::env::temp_dir().join(format!("ull-double-count-{}", std::process::id()));
        std::fs::create_dir_all(root.join("overrides/mods")).expect("建目录");
        std::fs::write(root.join("overrides/mods/a.bin"), [0_u8; 40]).expect("写覆盖文件");
        // 清单很短，limit 取"声明总量 + 20 字节"的余量：正确实现只写到 70，重复累计会写到 110。
        let json = "{\"name\":\"0123456789012345\"}";
        let declared = json.len() as u64 + 40;
        let limit = declared + 20;
        let mut manifest = PackManifest::new("double count");
        manifest.overrides.push(PackOverrides {
            dir: "overrides".to_string(),
            side: PackSide::Both,
            files: vec![OverrideFile {
                path: "mods/a.bin".to_string(),
                sha1: String::new(),
                size: 40,
            }],
        });
        let target = root.join("out.zip");

        container(
            "curseforge",
            "manifest.json",
            json,
            &manifest,
            PackPayload::from_root(&root),
            &target,
            limit,
        )
        .expect("声明总量没到 limit：写循环不许把预检的 total 重复累计");

        // 对照：limit 降到声明总量本身 ⇒ 预检就该拒，且临时文件都没建。
        let refused = root.join("refused.zip");
        let error = container(
            "curseforge",
            "manifest.json",
            json,
            &manifest,
            PackPayload::from_root(&root),
            &refused,
            declared,
        )
        .expect_err("声明总量达到 limit 必须拒");
        assert_eq!(
            error.kind(),
            super::super::super::error::PackErrorKind::Limits
        );
        assert!(!refused.exists(), "预检失败时目标路径不许出现");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_preflight_measures_declared_sizes_not_content() {
        // 预检必须只量 `metadata().len()`：这一条用"小文件 + 3 GiB 稀疏文件"钉住，
        // 真读内容的话要么慢要么直接吃 3 GiB 内存。
        let root = std::env::temp_dir().join(format!("ull-preflight-{}", std::process::id()));
        std::fs::create_dir_all(root.join("overrides/mods")).expect("建目录");
        std::fs::write(root.join("overrides/mods/small.jar"), b"0123456789").expect("写小文件");
        let huge = root.join("overrides/mods/huge.jar");
        let file = std::fs::File::create(&huge).expect("建稀疏文件");
        file.set_len(3 * 1024 * 1024 * 1024).expect("拉长（稀疏）");
        drop(file);

        let mut manifest = PackManifest::new("preflight");
        manifest.overrides.push(PackOverrides {
            dir: "overrides".to_string(),
            side: PackSide::Both,
            files: vec![
                OverrideFile {
                    path: "mods/small.jar".to_string(),
                    sha1: String::new(),
                    size: 10,
                },
                OverrideFile {
                    path: "mods/huge.jar".to_string(),
                    sha1: String::new(),
                    size: 0,
                },
            ],
        });

        let total = preflight(
            7,
            &manifest,
            PackPayload::from_root(&root),
            zip::ZIP64_BYTES_THR,
        )
        .expect("3 GiB 总量仍在阈值之下，预检不许误伤");
        assert_eq!(
            total,
            7 + 10 + 3 * 1024 * 1024 * 1024,
            "清单 + 每个覆盖文件的声明尺寸"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
