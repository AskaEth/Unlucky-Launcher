//! `Job`（检查/应用的一条龙）的用例：包形态闸门、一条成功的自替换、测试通道的两档判据。
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
    dir.write_signed(
        "stable.json",
        &manifest(
            "stable",
            &[release(
                version,
                "",
                &[artifact("win-x64", format, "asset.bin", PAYLOAD)],
            )],
        ),
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
    dir.write_signed(
        "stable.json",
        &manifest(
            "stable",
            &[release(
                "0.2.2",
                "",
                &[artifact("win-x64", "portable-exe", "asset.bin", PAYLOAD)],
            )],
        ),
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

/// 测试通道的判据是**编译期开 + 两档全给**：`--features update-test-source`、`ULL_UPDATE_DIR`、
/// `ULL_UPDATE_TEST_SOURCE=1`。
///
/// 旧判据是 `cfg!(debug_assertions)`（debug 认、release 整段折叠掉），M169 换掉它是因为发布
/// 产物正是 release——拿 release 验不了这条链，整条更新通道就没有端到端证据。换判据必须补隔离：
/// 编译期 feature 保证**官方二进制里不存在这条路**，两档环境变量保证「开了 feature 的验收构建」
/// 也不会被继承来的单个环境变量悄悄改源。这条用例守后者（纯判据，不碰进程环境）。
#[cfg(feature = "update-test-source")]
#[test]
fn the_test_channel_needs_both_switches() {
    use std::path::PathBuf;

    use crate::update::job::test_source_root_from;

    assert_eq!(
        test_source_root_from(Some("/d/dist"), Some("1")),
        Some(PathBuf::from("/d/dist")),
        "两档全给才开"
    );
    assert_eq!(
        test_source_root_from(Some("  /d/dist \n"), Some(" 1 ")),
        Some(PathBuf::from("/d/dist")),
        "路径与承认值两侧空白忽略"
    );

    assert_eq!(
        test_source_root_from(Some("/d/dist"), None),
        None,
        "只给路径不许改源（旧的单变量口子）"
    );
    assert_eq!(
        test_source_root_from(None, Some("1")),
        None,
        "只给承认档但没有路径"
    );
    for ack in ["true", "yes", "on", "0", "", "  "] {
        assert_eq!(
            test_source_root_from(Some("/d/dist"), Some(ack)),
            None,
            "承认值 `{ack}` 不算（手误不该开闸）"
        );
    }
    assert_eq!(
        test_source_root_from(Some("   "), Some("1")),
        None,
        "空路径按没给处理（`ULL_UPDATE_DIR=` 不该被读成当前目录）"
    );
}

/// 环境变量 → [`default_source`] 的接线，**两种构建各断各的**：
///
/// * 带 `update-test-source`（验收构建）：两档全给才换成 `file:` 源；半开仍走 GitHub；去掉承认档
///   立刻回到 GitHub（判据不缓存）。
/// * 不带 feature（**官方发布构建**）：环境变量设满了也照样走 GitHub——这条分支根本不存在。
///
/// 后者是本 mission 的关键断言：README 承诺「release 构建里不存在这条路径」，判据是编译期的，
/// 所以只能在「不带 feature 的测试跑」时验证。
///
/// 用例动的是**进程环境变量**（全局状态），所以先用一把进程内锁串起来、退出时还原原值——
/// `update` 的单测在同一个测试进程里并行跑。
#[test]
fn the_test_channel_is_a_compile_time_opt_in() {
    use crate::download::RegionProfile;
    use crate::update::job::{UPDATE_DIR_ENV, UPDATE_TEST_SOURCE_ENV};
    use crate::update::{default_source, github};

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _serial = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    /// 记住两个变量的原值，`Drop` 时还原（用例 panic 也还原）。
    struct Restore(Option<std::ffi::OsString>, Option<std::ffi::OsString>);
    impl Drop for Restore {
        fn drop(&mut self) {
            for (key, value) in [
                (UPDATE_DIR_ENV, self.0.clone()),
                (UPDATE_TEST_SOURCE_ENV, self.1.clone()),
            ] {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    let tmp = TempDir::new("job-env");
    let _restore = Restore(
        std::env::var_os(UPDATE_DIR_ENV),
        std::env::var_os(UPDATE_TEST_SOURCE_ENV),
    );
    let github_id = format!("github:{}/{}", github::DEFAULT_OWNER, github::DEFAULT_REPO);
    let engine = || super::support::engine_for(&tmp, RegionProfile::direct(0));
    let root = tmp.join("dist");

    // 两档都给满（不带 feature 的构建里这是最强的「攻击面」输入）。
    std::env::set_var(UPDATE_DIR_ENV, &root);
    std::env::set_var(UPDATE_TEST_SOURCE_ENV, "1");

    #[cfg(not(feature = "update-test-source"))]
    {
        assert_eq!(
            default_source(engine()).source_id(),
            github_id,
            "不带 `update-test-source` 的构建（= 官方发布）里，环境变量设满了也不许改源：{root:?}"
        );
    }

    #[cfg(feature = "update-test-source")]
    {
        use crate::update::job::test_source_root;

        assert_eq!(
            default_source(engine()).source_id(),
            format!("file:{}", root.display()),
            "带 feature：两档全给才换成本地目录源"
        );
        assert_eq!(test_source_root(), Some(root.clone()));

        std::env::remove_var(UPDATE_TEST_SOURCE_ENV);
        assert_eq!(
            default_source(engine()).source_id(),
            github_id,
            "半开（只给路径）必须回到 GitHub——旧的单变量口子不再生效"
        );
        assert_eq!(test_source_root(), None);
    }
}
