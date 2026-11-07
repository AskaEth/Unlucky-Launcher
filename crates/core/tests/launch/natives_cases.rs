//! natives 解压（`launch::natives`）的用例：解析正确性 / 排除项 / 幂等 / 时代参数 / 事件流。
//!
//! 假 jar 用 `zip` 的 Stored 模式现造（不依赖真下载），真机验收另有人工步骤。

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use unlucky_core::launch::natives::{self, NativeJar, NativesEra, NativesPlan, STAMP_FILE};

use crate::prelude::*;

/// 造一个 natives jar：`entries` 是（条目名, 内容）。
fn make_jar(path: &Path, entries: &[(&str, &[u8])]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建 jar 目录");
    }
    let file = File::create(path).expect("建 jar");
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, bytes) in entries {
        writer.start_file(*name, options).expect("写条目");
        writer.write_all(bytes).expect("写条目内容");
    }
    writer.finish().expect("收尾 jar");
}

/// 一份 1.16.5 风格的 natives jar 的条目集（原生库 + 该被排除的噪音）。
fn lwjgl_jar_entries() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("lwjgl.dll", b"DLL-BYTES".as_slice()),
        ("OpenAL.dll", b"OPENAL-BYTES".as_slice()),
        ("META-INF/MANIFEST.MF", b"manifest".as_slice()),
        ("META-INF/LWJGL.SF", b"signature".as_slice()),
        ("lwjgl.dll.sha1", b"deadbeef".as_slice()),
        ("README.txt", b"noise".as_slice()),
    ]
}

fn plan(jars: Vec<NativeJar>) -> NativesPlan {
    NativesPlan::new(jars)
}

#[test]
fn extracts_native_libraries_and_drops_noise() {
    let root = temp_dir("natives-extract");
    let jar = root.join("libraries/lwjgl-natives-windows.jar");
    make_jar(&jar, &lwjgl_jar_entries());
    let target = root.join("versions/1.16.5/natives");

    let report =
        natives::extract(&target, &plan(vec![NativeJar::new(&jar, "lwjgl")])).expect("解压应成功");

    assert_eq!(report.files, 2, "只该解出两个原生库：{report:?}");
    assert!(!report.up_to_date);
    assert_eq!(report.jars, 1);
    // META-INF/ 两条 + .sha1 + README.txt。
    assert_eq!(report.ignored, 4, "噪音条目数：{report:?}");
    assert_eq!(
        std::fs::read(target.join("lwjgl.dll")).expect("读 dll"),
        b"DLL-BYTES"
    );
    assert!(target.join("OpenAL.dll").is_file());
    assert!(!target.join("META-INF").exists(), "META-INF/ 不该落地");
    assert!(!target.join("lwjgl.dll.sha1").exists(), "签名文件不该落地");
    assert!(!target.join("README.txt").exists(), "非原生库不该落地");
    assert!(target.join(STAMP_FILE).is_file(), "应写戳记");
}

#[test]
fn second_run_is_idempotent_and_source_change_reextracts() {
    let root = temp_dir("natives-idempotent");
    let jar = root.join("natives-windows.jar");
    make_jar(&jar, &[("lwjgl.dll", b"V1".as_slice())]);
    let target = root.join("natives");
    let extract_plan = plan(vec![NativeJar::new(&jar, "lwjgl")]);

    let first = natives::extract(&target, &extract_plan).expect("首次解压");
    assert_eq!(first.files, 1);
    assert!(!first.up_to_date);

    // 第二次：戳记命中，零写盘（文件 mtime 不变）。
    let before = std::fs::metadata(target.join("lwjgl.dll"))
        .expect("stat")
        .modified()
        .expect("mtime");
    let second = natives::extract(&target, &extract_plan).expect("二次解压");
    assert!(second.up_to_date, "应判定目录已就绪：{second:?}");
    assert_eq!(second.files, 0);
    assert_eq!(second.skipped, 1);
    let after = std::fs::metadata(target.join("lwjgl.dll"))
        .expect("stat")
        .modified()
        .expect("mtime");
    assert_eq!(before, after, "幂等路径不该重写文件");

    // 源 jar 变了（内容 + 长度都变）：必须重解，且旧的原生库被清掉。
    make_jar(&jar, &[("lwjgl.dll", b"V2-LONGER".as_slice())]);
    let third = natives::extract(&target, &extract_plan).expect("三次解压");
    assert!(!third.up_to_date, "源变了必须重解：{third:?}");
    assert_eq!(
        std::fs::read(target.join("lwjgl.dll")).expect("读 dll"),
        b"V2-LONGER"
    );
}

#[test]
fn stale_native_libraries_are_cleaned_before_reextract() {
    let root = temp_dir("natives-clean");
    let jar = root.join("natives-windows.jar");
    make_jar(&jar, &[("lwjgl.dll", b"NEW".as_slice())]);
    let target = root.join("natives");
    std::fs::create_dir_all(&target).expect("建目录");
    // 上一代遗留（旧版本 / 旧 loader 解出来的），必须清掉，否则两代 dll 混在 java.library.path 里。
    std::fs::write(target.join("old-lwjgl.dll"), b"stale").expect("写旧 dll");
    std::fs::write(target.join("keep-me.txt"), b"user file").expect("写用户文件");

    natives::extract(&target, &plan(vec![NativeJar::new(&jar, "lwjgl")])).expect("解压");

    assert!(!target.join("old-lwjgl.dll").exists(), "旧原生库应被清理");
    assert!(target.join("keep-me.txt").is_file(), "非原生库文件不该被删");
    assert!(target.join("lwjgl.dll").is_file());
}

#[test]
fn empty_plan_still_creates_directory_for_1_19() {
    // 1.19+ 没有 natives 分类器：目录仍要存在（SharedLibraryExtractPath 指向它）。
    let root = temp_dir("natives-empty");
    let target = root.join("versions/1.21.4/natives");
    let report = natives::extract(&target, &NativesPlan::default()).expect("建目录");
    assert!(target.is_dir());
    assert!(report.up_to_date);
    assert_eq!(report.files, 0);
    assert_eq!(report.jars, 0);
    assert_eq!(
        report.era,
        NativesEra::ClasspathNative,
        "默认时代（无 jar）"
    );
}

#[test]
fn missing_directory_is_an_error_not_a_silent_pass() {
    let plan = NativesPlan::new(vec![NativeJar::new("x.jar", "x")]);
    let err = natives::extract(Path::new(""), &plan).expect_err("空目录必须报错");
    assert!(err.message().contains("natives 目录"), "{err}");
}

#[test]
fn broken_jar_reports_zip_error() {
    let root = temp_dir("natives-broken");
    let jar = root.join("natives-broken.jar");
    std::fs::write(&jar, b"not a zip at all").expect("写坏 jar");
    let target = root.join("natives");
    let err = natives::extract(&target, &plan(vec![NativeJar::new(&jar, "broken")]))
        .expect_err("坏 jar 必须报错");
    assert!(err.message().contains("zip"), "错误应点明 zip：{err}");
}

#[test]
fn era_detect_covers_three_generations() {
    assert_eq!(NativesEra::detect(0, 3), NativesEra::Legacy);
    assert_eq!(NativesEra::detect(9, 3), NativesEra::Lwjgl3);
    assert_eq!(NativesEra::detect(9, 0), NativesEra::ClasspathNative);
    assert_eq!(NativesEra::Legacy.as_str(), "legacy");
    assert_eq!(NativesEra::Lwjgl3.as_str(), "lwjgl3");
    assert_eq!(NativesEra::ClasspathNative.as_str(), "classpath_native");
}

/// 事件流：`natives_extracted` 落盘字段与接口约定一致（dir/files/skipped/custom）。
#[test]
fn natives_extracted_event_carries_report_fields() {
    let root = temp_dir("natives-event");
    let jar = root.join("natives-windows.jar");
    make_jar(&jar, &[("lwjgl.dll", b"DLL".as_slice())]);
    let target = root.join("natives");
    let report =
        natives::extract(&target, &plan(vec![NativeJar::new(&jar, "lwjgl")])).expect("解压");

    let sessions = temp_sessions_root("natives-event-sessions");
    let mut writer = SessionWriter::create(
        &sessions,
        SessionMeta::new("inst", "1.16.5", "1.16.5"),
        Redactor::new(),
    )
    .expect("建会话");
    writer.natives_extracted(&report).expect("写事件");
    let path = writer.path().to_path_buf();
    writer.finish(EndReason::Exit).expect("收尾");

    let events = read_events(&path);
    let event = events
        .iter()
        .find(|event| event["kind"] == "natives_extracted")
        .expect("应有 natives_extracted 事件");
    let payload = &event["payload"];
    assert_eq!(payload["files"], 1);
    assert_eq!(payload["skipped"], 0);
    assert_eq!(payload["custom"], false);
    assert_eq!(payload["jars"], 1);
    assert_eq!(payload["up_to_date"], false);
    assert_eq!(payload["era"], "lwjgl3");
    assert!(payload["dir"]
        .as_str()
        .expect("dir 是字符串")
        .ends_with("natives"));
}

/// `--dry-run` 也做环境准备：第二次跑要看到 up_to_date（真机验收的判定依据）。
#[test]
fn dry_run_prepares_natives_and_is_idempotent() {
    let root = temp_dir("natives-dry-run");
    let jar = root.join("natives-windows.jar");
    make_jar(&jar, &[("lwjgl.dll", b"DLL".as_slice())]);
    let natives_dir: PathBuf = root.join("versions/1.16.5/natives");

    let (mut request, _) = legacy_forge_request();
    request.dirs.natives_dir = natives_dir.clone();
    request.natives = NativesPlan::new(vec![NativeJar::new(&jar, "lwjgl")]);
    request.manifest.legacy_arguments = Some("--username ${auth_player_name}".to_string());

    let sessions = temp_sessions_root("natives-dry-run-sessions");
    let launcher = Launcher::new(LauncherConfig::new(&sessions));

    let first = launcher.dry_run(&request).expect("首次 dry-run");
    let report = first.natives.expect("应有 natives 报告");
    assert_eq!(report.files, 1, "{report:?}");
    assert!(!report.up_to_date);
    assert!(natives_dir.join("lwjgl.dll").is_file());

    let second = launcher.dry_run(&request).expect("二次 dry-run");
    let report = second.natives.expect("应有 natives 报告");
    assert!(report.up_to_date, "第二次应命中戳记：{report:?}");
    assert_eq!(report.skipped, 1);
}

/// 缺 natives jar：适配层当成组件缺失（组装前显式失败），而不是等到 UnsatisfiedLinkError。
#[test]
fn missing_native_jar_becomes_missing_component() {
    let (mut request, ctx) = legacy_forge_request();
    request.natives = NativesPlan::new(vec![NativeJar::new(
        "C:/mc/libraries/missing-natives-windows.jar",
        "org.lwjgl:lwjgl:3.2.2:natives-windows",
    )]);
    // 直接验证适配层：给一个不存在的 natives jar，严格模式应产出 MissingNativeJar 告警。
    let libraries: Vec<LibraryInput> = request
        .classpath
        .iter()
        .map(|path| LibraryInput::new(path.clone(), "x"))
        .collect();
    let natives = request.natives.jars.clone();
    let snapshot = SnapshotInput {
        version_id: "1.16.5",
        main_class: Some("net.minecraft.client.main.Main"),
        libraries: &libraries,
        natives: &natives,
        ..SnapshotInput::minimal("1.16.5", "net.minecraft.client.main.Main")
    };
    let options = AdapterOptions {
        java: JavaSelection::new("C:/java/bin/java.exe", 17, 64),
        ..AdapterOptions::strict()
    };
    let output = build_request(&snapshot, options);
    assert!(
        output
            .warnings
            .iter()
            .any(|warning| matches!(warning, AdapterWarning::MissingNativeJar { .. })),
        "应有 MissingNativeJar 告警：{:?}",
        output.warnings
    );
    assert_eq!(output.request.natives.era, NativesEra::Legacy);
    // 组装按既有语义直接失败（missing_components 非空）。
    assert!(matches!(
        assemble(&output.request, &ctx).err(),
        Some(LaunchError::Assembly(AssemblyError::MissingComponents(_)))
    ));
}
