//! 归档形态的产物 → 启动器本体（M308）：`unpack` 那一跳的用例。
//!
//! 为什么这一层必须单独测：M307 盘查抓到的真缺陷是「Linux/macOS 查得到更新、按钮画得出来、
//! 点下去必失败」——发布矩阵里那两平台的启动器是 `.tar.gz`（清单记作 `portable-zip`），而判据只认
//! 裸 exe。所以这里既要有**整条链能跑通**的正例（归档 → 双检 → 拆本体 → 原子替换），也要有
//! 三条**各自具名**的失败（找不到本体 / 本体不是本平台的可执行文件 / 本体大小不合规）——
//! 三种错因归到一个 `InvalidManifest` 就等于没归因。
//!
//! 归档全部在内存里现造（`flate2` / `tar` / `zip` 都是 core 的常规依赖，零新面）；
//! 「路径穿越」那条不是靠断言错误分类证明的，而是**结构上不可能**：本模块只往自己挑的路径写
//! 一个文件，条目名从不参与落盘（见 `unpack` 模块文档），用例顺带证明畸形条目连候选都不算。

use flate2::write::GzEncoder;
use flate2::Compression;

use crate::platform::os_info::OsFamily;
use crate::update::job::self_replaceable;
use crate::update::{
    Applied, Arch, Channel, DirSource, Job, PackageFormat, Target, UpdateErrorKind, UpdateSource,
    Version,
};

use super::support::{artifact, manifest, release, TempDir};

/// 三个平台的可执行文件魔数（与 `packaging/make_artifacts.py::image_arch` 同一套）。
const ELF: &[u8] = b"\x7fELF\x02\x01\x01\x00 new launcher payload";
const MACHO: &[u8] = b"\xcf\xfa\xed\xfe new launcher payload";
const PE: &[u8] = b"MZ\x90\x00 new launcher payload";

const STAGE: &str = "unlucky-launcher_9.9.9_linux_x64";

/// 一条归档条目：`(名字, 字节, unix 权限位)`。
type Entry<'a> = (&'a str, &'a [u8], u32);

/// 造一个 tar.gz（mtime 归零，同字节 ⇒ 同摘要，与 `make_artifacts.py` 同一条口径）。
fn tar_gz(entries: &[Entry<'_>]) -> Vec<u8> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::best());
    {
        let mut builder = tar::Builder::new(&mut gz);
        for (name, bytes, mode) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_mode(*mode);
            header.set_mtime(0);
            header.set_size(bytes.len() as u64);
            builder
                .append_data(&mut header, name, *bytes)
                .expect("写 tar 条目");
        }
        builder.finish().expect("收尾 tar");
    }
    gz.finish().expect("收尾 gzip")
}

/// 造一个**畸形条目名**的 tar.gz：`tar::Builder::append_data` 会拒绝 `..` 路径，
/// 所以这里先把名字写成合法的、再直接把原始字节盖到 header 的 name 字段上（病毒式的包就是这么来的）。
fn tar_gz_with_raw_name(name: &str, bytes: &[u8], mode: u32) -> Vec<u8> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::best());
    {
        let mut builder = tar::Builder::new(&mut gz);
        let mut header = tar::Header::new_gnu();
        header.set_mode(mode);
        header.set_mtime(0);
        header.set_size(bytes.len() as u64);
        header.set_path("placeholder").expect("占位名");
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_cksum();
        builder.append(&header, bytes).expect("追加条目");
        builder.finish().expect("收尾 tar");
    }
    gz.finish().expect("收尾 gzip")
}

/// 一个 zip（只为证明「zip 被明确拒绝」，见下一条用例）。
fn zip_of() -> Vec<u8> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        use std::io::Write as _;
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        writer
            .start_file("unlucky-launcher", options)
            .expect("写 zip 条目");
        writer.write_all(ELF).expect("写 zip 内容");
        writer.finish().expect("收尾 zip");
    }
    buffer.into_inner()
}

/// 造一个本地源 + 一个替换目标（`payload` 是**归档字节**，清单的 size/sha256 按它算）。
fn fixture(target: Target, format: &str, payload: &[u8]) -> (TempDir, Job) {
    let dir = TempDir::new("unpack");
    dir.write_signed(
        "stable.json",
        &manifest(
            "stable",
            &[release(
                "9.9.9",
                "",
                &[artifact(target.as_str(), format, "asset.bin", payload)],
            )],
        ),
    );
    dir.write("asset.bin", payload);
    let dest = dir.join("bin/unlucky-launcher");
    std::fs::create_dir_all(dest.parent().expect("父目录")).expect("建目标目录");
    std::fs::write(&dest, b"old-bytes").expect("旧版本");
    let job = Job::new(
        Box::new(DirSource::new(dir.path())) as Box<dyn UpdateSource>,
        target,
        dir.join("staging"),
        dest,
    );
    (dir, job)
}

fn linux() -> Target {
    Target::new(OsFamily::Linux, Arch::X64)
}

fn mac() -> Target {
    Target::new(OsFamily::MacOs, Arch::Arm64)
}

/// 跑一次 `apply` 并要它失败，返回错误分类（顺带断言目标**一个字节都没动**）。
///
/// `before_download` 区分两种拒绝时机：闸门在取包之前（安装器形态）时暂存目录**都不该被建**；
/// 归档拆包失败时归档已经下过、暂存目录已建，但**不许留任何残渣**（归档与拆出来的临时文件
/// 都由 `StagedPackage::Drop` 清掉）。
fn refused(job: &Job, expected: UpdateErrorKind, before_download: bool) {
    let error = job
        .apply(&Version::parse("0.2.2").expect("版本"), Channel::Stable)
        .expect_err("不该装上");
    assert_eq!(
        error.kind, expected,
        "分类不对（归因是这条 mission 的一半）：{error:?}"
    );
    assert_eq!(
        std::fs::read(job.dest()).expect("读目标"),
        b"old-bytes",
        "拒绝之后目标文件不许被动过"
    );
    if before_download {
        assert!(
            !job.staging().exists(),
            "闸门在取包之前，暂存目录都不该被建出来"
        );
    } else {
        assert_eq!(
            super::support::names_in(job.staging()),
            Vec::<String>::new(),
            "暂存目录里不许留下半成品"
        );
    }
}

/// **Linux 那条真缺陷的正面**：`.tar.gz` 走完整条链 —— 归档字节双检过、本体拆出来、旧版进备份。
#[test]
fn a_linux_tar_gz_runs_the_whole_chain() {
    let archive = tar_gz(&[
        (&format!("{STAGE}/unlucky-launcher"), ELF, 0o755),
        (&format!("{STAGE}/LICENSE"), b"license text", 0o644),
    ]);
    let (_dir, job) = fixture(linux(), "portable-zip", &archive);
    let applied = job
        .apply(&Version::parse("0.2.2").expect("版本"), Channel::Stable)
        .expect("应当装上");
    let Applied::Installed { receipt, version } = applied else {
        panic!("应当装上");
    };
    assert_eq!(version, "9.9.9");
    assert_eq!(
        std::fs::read(job.dest()).expect("读目标"),
        ELF,
        "目标上必须是归档里那个**本体**，不是整个 .tar.gz"
    );
    assert_eq!(
        receipt.bytes,
        u64::try_from(ELF.len()).expect("长度"),
        "收据里的字节数说的是装上的本体"
    );
    let backup = receipt.backup.expect("旧版本应留备份");
    assert_eq!(std::fs::read(backup).expect("读备份"), b"old-bytes");
}

/// macOS（`mac-arm64`）同样是 `.tar.gz`：本体是 Mach-O，随包文本 0644 不算候选。
#[test]
fn a_macos_tar_gz_runs_the_whole_chain() {
    let archive = tar_gz(&[
        ("Ull/unlucky-launcher", MACHO, 0o755),
        ("Ull/NOTICE", b"notice text", 0o644),
    ]);
    let (_dir, job) = fixture(mac(), "portable-zip", &archive);
    let applied = job
        .apply(&Version::parse("0.2.2").expect("版本"), Channel::Stable)
        .expect("应当装上");
    assert!(matches!(applied, Applied::Installed { .. }));
    assert_eq!(std::fs::read(job.dest()).expect("读目标"), MACHO);
}

/// Windows 的名字兜底：归档里的 `unlucky-launcher.exe` 带 **0644**（没有可执行位）也认。
///
/// 为什么需要这一条：`is_launcher_candidate` 是两条判据取并集（可执行位 **或** `.exe` 名字）。
/// 发布矩阵里 Windows 发裸 `.exe`，可「清单给 win-* 挂一个归档」是合法输入，而 tar 里没有 unix
/// 权限位、`make_artifacts.py` 也只给启动器打 0o755——名字这条兜底不能是空转的。
///
/// 抓得住：把 `is_launcher_candidate` 里 `|| name.to_ascii_lowercase().ends_with(".exe")` 删掉，
/// 本用例立刻红（归档里一个候选都没有 ⇒ `LauncherMissing`）。
#[test]
fn a_windows_exe_without_the_exec_bit_is_still_the_launcher() {
    let stage = "unlucky-launcher_9.9.9_windows_x64";
    let archive = tar_gz(&[
        (&format!("{stage}/unlucky-launcher.exe"), PE, 0o644),
        (&format!("{stage}/LICENSE"), b"license text", 0o644),
    ]);
    let target = Target::new(OsFamily::Windows, Arch::X64);
    let (_dir, job) = fixture(target, "portable-zip", &archive);
    let applied = job
        .apply(&Version::parse("0.2.2").expect("版本"), Channel::Stable)
        .expect("应当装上（名字判据兜住没有可执行位的那份）");
    assert!(matches!(applied, Applied::Installed { .. }));
    assert_eq!(std::fs::read(job.dest()).expect("读目标"), PE);
}

/// 归档里没有本体（只有随包文本）⇒ `LauncherMissing`。抓得住「只认可执行位」这条判据被放松。
#[test]
fn an_archive_without_a_launcher_is_refused() {
    let archive = tar_gz(&[
        (&format!("{STAGE}/LICENSE"), b"license text", 0o644),
        (&format!("{STAGE}/README.md"), b"readme", 0o644),
    ]);
    let (_dir, job) = fixture(linux(), "portable-zip", &archive);
    refused(&job, UpdateErrorKind::LauncherMissing, false);
}

/// 归档里有两个像本体的条目 ⇒ `LauncherMissing`（**不猜**装哪个）。
#[test]
fn an_archive_with_two_launchers_is_refused() {
    let archive = tar_gz(&[
        (&format!("{STAGE}/unlucky-launcher"), ELF, 0o755),
        (&format!("{STAGE}/unlucky-launcher.exe"), PE, 0o755),
    ]);
    let (_dir, job) = fixture(linux(), "portable-zip", &archive);
    refused(&job, UpdateErrorKind::LauncherMissing, false);
}

/// 条目名带 `..` 的不算候选（本模块从不按条目名落盘，这条是「连候选都不给」）。
///
/// 判据写成 `LauncherMissing` 而不是「装上了」：唯一那个可执行条目被挡掉 ⇒ 归档里等于没有本体。
#[test]
fn a_dotdot_entry_is_never_a_candidate() {
    let archive = tar_gz_with_raw_name("../../unlucky-launcher", ELF, 0o755);
    let (_dir, job) = fixture(linux(), "portable-zip", &archive);
    refused(&job, UpdateErrorKind::LauncherMissing, false);
}

/// 本体是别的平台的产物（`win-x64` 的条目里躺着 ELF）⇒ `LauncherNotExecutable`。
///
/// 这是「清单写错」那一类：摘要与大小都对（我们就是对这份字节算的），但字节不是 `MZ`。
#[test]
fn a_launcher_for_another_platform_is_refused() {
    let archive = tar_gz(&[(&format!("{STAGE}/unlucky-launcher"), ELF, 0o755)]);
    let target = Target::new(OsFamily::Windows, Arch::X64);
    let (_dir, job) = fixture(target, "portable-zip", &archive);
    refused(&job, UpdateErrorKind::LauncherNotExecutable, false);
}

/// 本体 0 字节（打了可执行位但内容是空的）⇒ `LauncherSizeMismatch`。
#[test]
fn a_zero_byte_launcher_is_refused() {
    let archive = tar_gz(&[
        (&format!("{STAGE}/unlucky-launcher"), b"", 0o755),
        (&format!("{STAGE}/LICENSE"), b"license text", 0o644),
    ]);
    let (_dir, job) = fixture(linux(), "portable-zip", &archive);
    refused(&job, UpdateErrorKind::LauncherSizeMismatch, false);
}

/// 清单说归档、字节却不是任何已知容器 ⇒ `InvalidManifest`（清单与产物对不上）。
#[test]
fn bytes_that_are_not_an_archive_are_refused() {
    let (_dir, job) = fixture(linux(), "portable-zip", b"just some bytes, no magic");
    refused(&job, UpdateErrorKind::InvalidManifest, false);
}

/// zip 在 `portable-zip` 这一档下被**明确拒绝**（不是猜哪个条目是本体）。
///
/// 这条用例守的是一个决定：发布矩阵只出 `.tar.gz`，zip 没有可靠的「哪个条目可执行」判据，
/// 所以宁可拒绝并点名原因，也不装错文件。
#[test]
fn a_zip_is_refused_with_a_reason() {
    let (dir, job) = fixture(linux(), "portable-zip", &zip_of());
    refused(&job, UpdateErrorKind::InvalidManifest, false);
    drop(dir);
}

/// 判据表：便携 exe 与便携归档能自替换，安装器形态与 AppImage 不能。
///
/// 抓得住：把 `self_replaceable` 放宽成「全格式都算」（安装器形态会漏过去，`msi`/`deb` 会被
/// 改名成启动器）、或者收回成「只认裸 exe」（Linux/macOS 的归档重新变成「点了必失败」）。
#[test]
fn the_self_replaceable_table_is_two_formats() {
    assert!(self_replaceable(PackageFormat::PortableExe));
    assert!(
        self_replaceable(PackageFormat::PortableZip),
        "Linux/macOS 的 `.tar.gz` 就是这一档（M308 修的正是这里）"
    );
    for format in [
        PackageFormat::Msi,
        PackageFormat::Dmg,
        PackageFormat::Deb,
        PackageFormat::Rpm,
        PackageFormat::AppImage,
    ] {
        assert!(
            !self_replaceable(format),
            "{} 不该被当成能变成启动器本体的形态",
            format.as_str()
        );
    }
}
