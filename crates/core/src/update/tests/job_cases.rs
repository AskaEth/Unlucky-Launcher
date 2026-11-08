//! `Job`（检查/应用的一条龙）的用例：包形态闸门 + 一条成功的自替换。
//!
//! 为什么这层要单独测：`Release::artifact_for` 只比 `target`、不看 `format`，所以「清单给
//! win-x64 挂了一个 msi/zip」是完全可能的输入（复审 P3-6）。以前 `apply` 会把它直接 rename
//! 成本进程的 exe——D50 明说「不做 MSI」，而 zip 当 exe 更是跑不起来。闸门在本模块，
//! 用例就在这里钉住（presenter / CLI 的测试覆盖不到「形态」这一维）。

use crate::platform::os_info::OsFamily;
use crate::update::job::self_replaceable;
use crate::update::{
    Applied, Channel, DirSource, Job, PackageFormat, Target, UpdateErrorKind, Version,
};

use super::support::{artifact, manifest, release, TempDir};

const PAYLOAD: &[u8] = b"job payload";

fn target() -> Target {
    Target::new(OsFamily::Windows, crate::update::Arch::X64)
}

/// 造一个本地源 + 一个替换目标；返回的 `TempDir` 要活到用例结束。
fn fixture(format: &str, version: &str) -> (TempDir, Job) {
    let dir = TempDir::new("job");
    dir.write(
        "stable.json",
        manifest(
            "stable",
            &[release(
                version,
                "",
                &[artifact("win-x64", format, "asset.bin", PAYLOAD)],
            )],
        )
        .as_bytes(),
    );
    dir.write("asset.bin", PAYLOAD);
    let dest = dir.join("bin/unlucky-launcher.exe");
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).expect("建目标目录");
    }
    std::fs::write(&dest, b"old-bytes").expect("旧版本");
    let staging = dir.join("staging");
    let job = Job::new(
        Box::new(DirSource::new(dir.path())),
        target(),
        staging,
        dest,
    );
    (dir, job)
}

/// 只有便携单文件 exe 能原地变成启动器本体。
///
/// 抓得住：把闸门写成 `!needs_installer()`（那样 `portable-zip` 会漏过去，zip 会被 rename 成
/// exe，得到一个跑不起来的启动器）。
#[test]
fn only_portable_exe_is_self_replaceable() {
    assert!(self_replaceable(PackageFormat::PortableExe));
    for format in [
        PackageFormat::Msi,
        PackageFormat::Dmg,
        PackageFormat::Deb,
        PackageFormat::Rpm,
        PackageFormat::AppImage,
        PackageFormat::PortableZip,
    ] {
        assert!(
            !self_replaceable(format),
            "{} 不该被当成可原地自替换的形态",
            format.as_str()
        );
    }
}

/// 安装器形态被拒绝，且**目标文件一个字节都没动、暂存目录没建**（闸门在取包之前）。
#[test]
fn an_installer_artifact_is_refused_before_any_download() {
    for format in ["msi", "portable-zip"] {
        let (dir, job) = fixture(format, "9.9.9");
        let error = job
            .apply(&Version::parse("0.2.2").expect("版本"), Channel::Stable)
            .expect_err("不该装上");
        assert_eq!(
            error.kind,
            UpdateErrorKind::InvalidManifest,
            "{format}：清单挂错形态属发布侧问题（换源/重试都没用），实际 {error:?}"
        );
        assert_eq!(
            std::fs::read(job.dest()).expect("读目标"),
            b"old-bytes",
            "{format}：拒绝之后目标文件不许被动过"
        );
        assert!(
            !job.staging().exists(),
            "{format}：闸门在取包之前，暂存目录都不该被建出来"
        );
        drop(dir);
    }
}

/// 便携 exe 形态走完整条路：新字节就位、旧字节进备份。
#[test]
fn a_portable_exe_runs_the_whole_chain() {
    let (_dir, job) = fixture("portable-exe", "9.9.9");
    let applied = job
        .apply(&Version::parse("0.2.2").expect("版本"), Channel::Stable)
        .expect("应当装上");
    let Applied::Installed { receipt, version } = applied else {
        panic!("应当装上");
    };
    assert_eq!(version, "9.9.9");
    assert_eq!(std::fs::read(job.dest()).expect("读目标"), PAYLOAD);
    let backup = receipt.backup.expect("旧版本应留备份");
    assert_eq!(std::fs::read(backup).expect("读备份"), b"old-bytes");
}

/// 发布里挂的是「没有可装更新」时 `apply` 返回 `Nothing`，不碰任何文件。
#[test]
fn nothing_to_apply_does_not_touch_the_target() {
    let dir = TempDir::new("job-none");
    dir.write(
        "stable.json",
        manifest(
            "stable",
            &[release(
                "0.2.2",
                "",
                &[artifact("win-x64", "portable-exe", "asset.bin", PAYLOAD)],
            )],
        )
        .as_bytes(),
    );
    let dest = dir.join("bin/unlucky-launcher.exe");
    std::fs::create_dir_all(dest.parent().expect("父目录")).expect("建目录");
    std::fs::write(&dest, b"old-bytes").expect("旧版本");
    let job = Job::new(
        Box::new(DirSource::new(dir.path())) as Box<dyn crate::update::UpdateSource>,
        target(),
        dir.join("staging"),
        &dest,
    );
    let applied = job
        .apply(&Version::parse("0.2.2").expect("版本"), Channel::Stable)
        .expect("不是错误");
    assert!(matches!(applied, Applied::Nothing(_)));
    assert_eq!(std::fs::read(&dest).expect("读目标"), b"old-bytes");
}

/// 调试构建认 `ULL_UPDATE_DIR`、发布构建不认——判据是靠 `cfg!(debug_assertions)` 折叠掉的，
/// 所以这里用源码锚点把「闸门还在」钉住（release 分支在测试里跑不到，只能这样守）。
#[test]
fn the_local_dir_override_is_debug_only() {
    let source = include_str!("../job.rs");
    assert!(
        source.contains("if !cfg!(debug_assertions) {"),
        "`ULL_UPDATE_DIR` 的闸门必须是 debug_assertions（发布构建里整段被折叠掉）"
    );
}
