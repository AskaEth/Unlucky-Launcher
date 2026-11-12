//! 替换的坏路径：跨卷降级、复制到一半失败、截断复制、最后一步改名失败、同卷不多搬一个字节。
//!
//! # 为什么要注入而不是造真盘
//!
//! 「跨卷」在 `install_xdev_cases` 里是**真跨卷**（真盘、真 `EXDEV`），但那需要机器上有第二块卷；
//! 「复制写到一半失败」在真盘上根本没法确定性复现（要么让盘满，要么让杀软恰好在那一刻咬住）。
//! 这两条恰好是本次修复最关键的路径，所以这里用 [`ReplaceFs`] 的注入器把系统调用变成输入：
//!
//! * 注入器能改的只有「rename/copy 这一次成不成功」；
//! * 注入器**改不了**判据与顺序——`install_with` 与生产路径是同一份代码，只有 `&dyn` 后面那个
//!   实现不同。
//!
//! 抓得住：跨卷时是否真的降级到「复制到目标同目录的中转名」；复制半途失败后目标是否仍是旧字节、
//! 中转名是否被清掉；截断的复制是否被字节数判据拒掉；同卷是否一个字节都没多搬。
//! 抓不住：真实文件系统的掉电/写缓存语义（`fsync` 从未被调用过），以及「复制期间目标盘被拔掉」
//! 这类整卷消失的场景。

use std::cell::Cell;
use std::fs;
use std::io;
use std::path::Path;

use crate::update::manifest::ReleaseManifest;
use crate::update::replace_io::ReplaceFs;
use crate::update::{install::install_with, stage, DirSource, ReplacePolicy, StagedPackage};

use super::support::{artifact, manifest, names_in, release, TempDir};

const UPDATE: &[u8] = b"new build bytes that must land whole";
const OLD: &[u8] = b"old build bytes";

fn policy() -> ReplacePolicy {
    ReplacePolicy::new(1, std::time::Duration::from_millis(1))
}

fn staged(tmp: &TempDir) -> StagedPackage {
    tmp.write("ull-test.zip", UPDATE);
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact("win-x64", "portable-exe", "ull-test.zip", UPDATE)],
        )],
    );
    let parsed = ReleaseManifest::from_json(&json).expect("清单应可解析");
    stage(
        &DirSource::new(tmp.path()),
        &parsed.releases[0].artifacts[0],
        &tmp.join("staging"),
    )
    .expect("取包与校验应通过")
}

/// 复制这一步的表现。
#[derive(Clone, Copy)]
enum CopyMode {
    /// 老实复制。
    Normal,
    /// 只写 `n` 字节就报错（真盘上「拷到一半失败」留下的半截文件）。
    PartialThenError(usize),
    /// 只写 `n` 字节但**声称**成功（模拟「长度判据必须抓住」的那一类坏复制）。
    TruncateThenOk(usize),
    /// 一律拒绝（用来证明同卷路径上没有复制）。
    Forbidden,
}

/// 注入器：默认全权交给 `std::fs`，按开关制造跨卷与失败。
struct FakeFs {
    /// 「暂存包 → 中转名」这一步假装跨卷。
    staged_rename_crosses: bool,
    /// 中转名 → dest 这一步失败。
    fail_final_rename: bool,
    copy_mode: CopyMode,
    copies: Cell<usize>,
    renames: Cell<usize>,
}

impl FakeFs {
    fn new() -> Self {
        Self {
            staged_rename_crosses: false,
            fail_final_rename: false,
            copy_mode: CopyMode::Normal,
            copies: Cell::new(0),
            renames: Cell::new(0),
        }
    }

    fn across_volumes() -> Self {
        Self {
            staged_rename_crosses: true,
            ..Self::new()
        }
    }

    fn copies(&self) -> usize {
        self.copies.get()
    }

    fn renames(&self) -> usize {
        self.renames.get()
    }
}

fn is_transit(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().contains(".ullnew-"))
}

impl ReplaceFs for FakeFs {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.renames.set(self.renames.get() + 1);
        if self.staged_rename_crosses && is_transit(to) {
            return Err(io::Error::from(io::ErrorKind::CrossesDevices));
        }
        if self.fail_final_rename && is_transit(from) {
            return Err(io::Error::other("模拟：中转名改名到目标失败"));
        }
        fs::rename(from, to)
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        self.copies.set(self.copies.get() + 1);
        let bytes = fs::read(from)?;
        match self.copy_mode {
            CopyMode::Normal => fs::copy(from, to),
            CopyMode::PartialThenError(wrote) => {
                fs::write(to, &bytes[..wrote.min(bytes.len())])?;
                Err(io::Error::other("模拟：复制到一半失败"))
            }
            CopyMode::TruncateThenOk(wrote) => {
                let wrote = wrote.min(bytes.len());
                fs::write(to, &bytes[..wrote])?;
                Ok(wrote as u64)
            }
            CopyMode::Forbidden => Err(io::Error::other("这里不许复制")),
        }
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
}

/// 目标目录里只该有 `ull.exe`（+ 可能的一份备份），中转名一律不许留。
fn assert_no_transit(dir: &Path) {
    let names = names_in(dir);
    assert!(
        names.iter().all(|name| !name.contains(".ullnew-")),
        "中转名必须被清理：{names:?}"
    );
}

/// ① 跨卷：降级为「复制到目标同目录的中转名」，装上去的必须是完整字节。
#[test]
fn cross_volume_falls_back_to_a_copy_beside_the_target() {
    let fake = FakeFs::across_volumes();
    let tmp = TempDir::new("io-xdev-ok");
    let dest = tmp.write("app/ull.exe", OLD);
    let staged = staged(&tmp);
    let receipt = install_with(&fake, &staged, &dest, policy()).expect("跨卷必须成功");

    assert_eq!(fs::read(&dest).expect("目标应存在"), UPDATE);
    assert_eq!(receipt.bytes, UPDATE.len() as u64);
    assert_eq!(
        fake.copies(),
        1,
        "跨卷只该复制一次（降级路径唯一允许的复制）"
    );
    let backup = receipt.backup.clone().expect("旧版本应有备份");
    assert_eq!(fs::read(&backup).expect("备份"), OLD);
    assert_no_transit(dest.parent().expect("父目录"));
}

/// ② 复制写到一半失败：目标仍是旧字节、不留中转名、也不该动过备份。
#[test]
fn a_copy_that_dies_halfway_leaves_the_old_bytes_and_no_garbage() {
    let fake = FakeFs {
        copy_mode: CopyMode::PartialThenError(UPDATE.len() / 2),
        ..FakeFs::across_volumes()
    };
    let tmp = TempDir::new("io-xdev-partial");
    let dest = tmp.write("app/ull.exe", OLD);
    let staged = staged(&tmp);
    let failure = install_with(&fake, &staged, &dest, policy()).expect_err("半截复制必须失败");

    assert!(
        failure.rolled_back,
        "目标没被碰过，旧版本仍在原位：{failure}"
    );
    assert_eq!(
        fs::read(&dest).expect("旧版本"),
        OLD,
        "目标必须是完整的旧字节"
    );
    assert_no_transit(dest.parent().expect("父目录"));
    assert!(
        names_in(dest.parent().expect("父目录")) == ["ull.exe"],
        "连备份都不该建（失败发生在动目标之前）：{:?}",
        names_in(dest.parent().expect("父目录"))
    );
}

/// ②′ 复制「声称成功」但字节数不符：长度判据必须拒掉，目标不动。
#[test]
fn a_short_copy_is_rejected_by_the_byte_count() {
    let fake = FakeFs {
        copy_mode: CopyMode::TruncateThenOk(UPDATE.len() - 1),
        ..FakeFs::across_volumes()
    };
    let tmp = TempDir::new("io-xdev-short");
    let dest = tmp.write("app/ull.exe", OLD);
    let staged = staged(&tmp);
    let failure = install_with(&fake, &staged, &dest, policy()).expect_err("短了必须拒");

    assert!(failure.rolled_back, "{failure}");
    assert_eq!(fs::read(&dest).expect("旧版本"), OLD);
    assert!(
        failure.error.message.contains("跨卷复制只写出"),
        "失败原因要说清是字节数不符：{failure}"
    );
    assert_no_transit(dest.parent().expect("父目录"));
}

/// ②″ 最后一步改名失败：回滚到旧字节，中转名与备份都不留。
#[test]
fn a_failed_final_rename_rolls_back_and_cleans_up() {
    let fake = FakeFs {
        fail_final_rename: true,
        ..FakeFs::across_volumes()
    };
    let tmp = TempDir::new("io-final-rename");
    let dest = tmp.write("app/ull.exe", OLD);
    let staged = staged(&tmp);
    let failure = install_with(&fake, &staged, &dest, policy()).expect_err("改名失败必须报错");

    assert!(failure.rolled_back, "旧版本应回到原位：{failure}");
    assert_eq!(failure.backup_left, None);
    assert_eq!(fs::read(&dest).expect("旧版本"), OLD);
    assert_eq!(
        names_in(dest.parent().expect("父目录")),
        ["ull.exe"],
        "中转名与备份都必须被清掉"
    );
}

/// ③ 同卷：一个字节都不许复制（行为与修前一致）。
///
/// 注入器的 `copy` 一律报错——只要实现走了复制，这条就会红。
#[test]
fn the_same_volume_path_never_copies() {
    let fake = FakeFs {
        copy_mode: CopyMode::Forbidden,
        ..FakeFs::new()
    };
    let tmp = TempDir::new("io-same-volume");
    let dest = tmp.write("app/ull.exe", OLD);
    let staged = staged(&tmp);
    let receipt = install_with(&fake, &staged, &dest, policy()).expect("同卷替换必须成功");

    assert_eq!(fs::read(&dest).expect("目标"), UPDATE);
    assert_eq!(fake.copies(), 0, "同卷走改名，不许有复制");
    assert_eq!(
        fake.renames(),
        3,
        "同卷三步：暂存→中转名、目标→备份、中转名→目标（比修前多一次同卷改名、不多一个字节）"
    );
    assert_eq!(receipt.bytes, UPDATE.len() as u64);
    assert_no_transit(dest.parent().expect("父目录"));
}
