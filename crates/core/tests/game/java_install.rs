//! `core::game::java` 的安装侧集成测试：归档解压（幂等 / 顶层剥离 / 越界拒绝）、
//! 托管目录命名、下载请求装配、平台归档形态。
//!
//! 镜像选源见 `java_mirrors.rs`，MC↔Java 映射见 `java_mapping.rs`。

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use flate2::write::GzEncoder;
use flate2::Compression;
use unlucky_core::download::ResourceKind;
use unlucky_core::game::java::{
    self, extract, install, probe, AdoptiumTarget, JdkArchive, JdkRelease,
};

/// 64 位十六进制（解压戳记只当字符串比对，不校验内容）。
const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
/// 另一个「新版本」的摘要。
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ull-java-{tag}-{}-{nanos}-{seq}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建临时目录");
    dir
}
/// 造一个 Temurin 形状的 zip（顶层 `jdk-21.0.13+9/`）。
fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = File::create(path).expect("建 zip");
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, body) in entries {
        writer.start_file(*name, options).expect("写 zip 条目");
        writer.write_all(body).expect("写 zip 内容");
    }
    writer.finish().expect("收尾 zip");
}

/// 造一个 Temurin 形状的 tar.gz。
fn write_tar_gz(path: &Path, entries: &[(&str, &[u8])]) {
    let file = File::create(path).expect("建 tar.gz");
    let encoder = GzEncoder::new(file, Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    for (name, body) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o755);
        header.set_entry_type(tar::EntryType::Regular);
        builder
            .append_data(&mut header, name, *body)
            .expect("写 tar 条目");
    }
    let encoder = builder.into_inner().expect("收尾 tar");
    encoder.finish().expect("收尾 gzip");
}

fn java_bin(root: &Path) -> PathBuf {
    root.join("bin").join(java::java_binary_name())
}

#[test]
fn zip_archive_loses_the_temurin_top_level_directory() {
    let dir = temp_dir("zip");
    let archive = dir.join("jdk.zip");
    let top = "jdk-21.0.13+9";
    write_zip(
        &archive,
        &[
            (
                &format!("{top}/bin/{}", java::java_binary_name()),
                b"#!/bin/true\n",
            ),
            (&format!("{top}/release"), b"JAVA_VERSION=\"21.0.13\"\n"),
        ],
    );
    let dest = dir.join("jdk-21-temurin");
    let report = extract::extract_archive(&archive, SHA_A, 21, &dest).expect("解压应成功");

    assert!(!report.skipped);
    assert_eq!(report.files, 2);
    assert!(java_bin(&dest).is_file(), "bin/java 应在顶层");
    assert!(!dest.join(top).exists(), "顶层目录必须被剥掉");
    let stamp = extract::read_stamp(&dest).expect("应写戳记");
    assert_eq!(stamp.major, 21);
    assert_eq!(stamp.sha256, SHA_A);
    assert_eq!(stamp.files, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn second_extract_with_same_sha_touches_nothing() {
    let dir = temp_dir("idem");
    let archive = dir.join("jdk.zip");
    write_zip(
        &archive,
        &[(
            &format!("jdk-21.0.13+9/bin/{}", java::java_binary_name()),
            b"bin\n",
        )],
    );
    let dest = dir.join("jdk-21-temurin");
    extract::extract_archive(&archive, SHA_A, 21, &dest).expect("首解");

    // 用户手工动过的痕迹：幂等命中时不能被抹掉。
    let marker = dest.join("安装后手工放的.txt");
    std::fs::write(&marker, b"keep").expect("写标记");

    let again = extract::extract_archive(&archive, SHA_A, 21, &dest).expect("重解");
    assert!(again.skipped, "同一 sha256 应整体跳过");
    assert_eq!(again.files, 0);
    assert!(marker.is_file(), "幂等命中时不应重写目录");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn new_sha_replaces_the_managed_directory() {
    let dir = temp_dir("upgrade");
    let archive = dir.join("jdk.zip");
    write_zip(
        &archive,
        &[(
            &format!("jdk-21.0.13+9/bin/{}", java::java_binary_name()),
            b"v1\n",
        )],
    );
    let dest = dir.join("jdk-21-temurin");
    extract::extract_archive(&archive, SHA_A, 21, &dest).expect("首解");
    let marker = dest.join("旧版残留");
    std::fs::write(&marker, b"old").expect("写标记");

    let report = extract::extract_archive(&archive, SHA_B, 21, &dest).expect("新摘要重解");
    assert!(!report.skipped);
    assert!(!marker.exists(), "官方发新版后旧目录应被替换");
    assert_eq!(
        extract::read_stamp(&dest).map(|stamp| stamp.sha256),
        Some(SHA_B.to_string())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tar_gz_archive_is_supported_for_linux_and_mac() {
    let dir = temp_dir("targz");
    let archive = dir.join("OpenJDK21U-jdk_x64_linux_hotspot_21.0.13_9.tar.gz");
    let top = "jdk-21.0.13+9";
    write_tar_gz(
        &archive,
        &[
            (&format!("{top}/bin/{}", java::java_binary_name()), b"bin\n"),
            (&format!("{top}/lib/modules"), b"modules\n"),
        ],
    );
    let dest = dir.join("jdk-21-temurin");
    let report = extract::extract_archive(&archive, SHA_A, 21, &dest).expect("解压 tar.gz");
    assert_eq!(report.files, 2);
    assert!(dest.join("bin").join(java::java_binary_name()).is_file());
    assert!(dest.join("lib").join("modules").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn archive_without_a_single_top_directory_is_promoted_as_is() {
    // 有的魔改发行版直接把 bin/ 放在根上：不能硬剥一层。
    let dir = temp_dir("flat");
    let archive = dir.join("flat.zip");
    write_zip(
        &archive,
        &[
            (
                &format!("bin/{}", java::java_binary_name()),
                b"bin\n".as_slice(),
            ),
            ("release", b"x\n".as_slice()),
        ],
    );
    let dest = dir.join("jdk-17-temurin");
    extract::extract_archive(&archive, SHA_A, 17, &dest).expect("解压");
    assert!(dest.join("bin").join(java::java_binary_name()).is_file());
    assert!(dest.join("release").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn foreign_directory_is_never_clobbered() {
    let dir = temp_dir("foreign");
    let archive = dir.join("jdk.zip");
    write_zip(
        &archive,
        &[(
            &format!("jdk-21.0.13+9/bin/{}", java::java_binary_name()),
            b"bin\n",
        )],
    );
    let dest = dir.join("jdk-21-temurin");
    std::fs::create_dir_all(&dest).expect("建目标目录");
    std::fs::write(dest.join("用户的文件.txt"), b"mine").expect("写用户文件");

    let error = extract::extract_archive(&archive, SHA_A, 21, &dest).expect_err("应拒绝覆盖");
    assert_eq!(error.kind(), java::JavaErrorKind::Io);
    assert!(dest.join("用户的文件.txt").is_file(), "用户文件必须还在");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn zip_slip_entries_do_not_escape_the_target_directory() {
    let dir = temp_dir("slip");
    let archive = dir.join("evil.zip");
    write_zip(
        &archive,
        &[
            (
                &format!("jdk-21.0.13+9/bin/{}", java::java_binary_name()),
                b"bin\n",
            ),
            ("../../escaped.txt", b"nope"),
        ],
    );
    let dest = dir.join("jdk-21-temurin");
    let report = extract::extract_archive(&archive, SHA_A, 21, &dest).expect("越界条目只跳过");
    assert_eq!(report.files, 1, "越界条目不应被写盘");
    assert!(!dir.join("escaped.txt").exists());
    assert!(!dir.parent().unwrap_or(&dir).join("escaped.txt").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn managed_layout_names_carry_major_and_vendor() {
    let root = Path::new("/tmp/java-managed");
    let dest = install::managed_root_dir(root, 21, java::JDK_VENDOR);
    assert_eq!(dest, root.join("jdk-21-temurin"));
    assert_eq!(
        probe::parse_managed_dir("jdk-21-temurin"),
        Some((21, "temurin"))
    );
    assert_eq!(probe::managed_dir_name(8, "temurin"), "jdk-8-temurin");
}

#[test]
fn archive_request_carries_sha256_and_the_java_runtime_kind() {
    let release = JdkRelease {
        vendor: java::JdkVendor::Temurin,
        major: 21,
        version: "21.0.13+9".to_string(),
        semver: Some("21.0.13+9.0.LTS".to_string()),
        release_name: "jdk-21.0.13+9".to_string(),
        archive: JdkArchive {
            name: "OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip".to_string(),
            url: "https://github.com/adoptium/temurin21-binaries/releases/download/x.zip"
                .to_string(),
            sha256: SHA_A.to_string(),
            size: 1024,
        },
    };
    let dest = Path::new("/tmp/x.zip");
    let request = install::archive_request(&release, dest.to_path_buf());
    assert_eq!(request.kind, ResourceKind::JavaRuntime);
    assert_eq!(request.expected_size, Some(1024));
    assert_eq!(
        request
            .integrity
            .as_ref()
            .map(|integrity| integrity.hex.as_str()),
        Some(SHA_A)
    );
    assert!(request.validate().is_ok());
    assert_eq!(
        install::archive_path(Path::new("/tmp/java-managed"), &release),
        Path::new("/tmp/java-managed/.cache/OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip")
    );
}

#[test]
fn host_target_picks_the_platform_archive_shape() {
    let target = AdoptiumTarget::host().expect("宿主应受支持");
    assert_eq!(target.is_zip(), cfg!(windows));
    assert_eq!(
        target.archive_ext(),
        if cfg!(windows) { "zip" } else { "tar.gz" }
    );
}
