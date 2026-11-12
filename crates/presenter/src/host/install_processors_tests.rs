//! host/install_processors.rs 的单测（拆文件守 400 行上限）。
//!
//! 夹具在 [`super::fixture`]：这里只断言「计划怎么组」「找不到 Java 怎么说」
//! 「哪一步挂了怎么报」「进度文案长什么样」。

use std::path::PathBuf;

use unlucky_core::addon::loader::forge::InstallProfile;
use unlucky_core::addon::loader::processors::{ProcessorError, StepStatus};
use unlucky_core::addon::loader::LoaderKind as CoreLoaderKind;
use unlucky_core::download::DownloadErrorKind;
use unlucky_core::game::install::{FileFailure, FileOutcome, FileRole, FileStatus, InstallReport};
use unlucky_core::game::java::JavaRequirement;

use super::fixture::{input, installer, outcome, profile, Fake, Harness};
use super::{diagnostic, java_missing, libraries_failed, stage, step_line, StageInput};

#[test]
fn step_line_names_the_step_and_marks_the_outcome() {
    let executed = step_line(&outcome(StepStatus::Executed), 9);
    assert_eq!(
        executed,
        "installer 处理器 3/9：net.minecraftforge:installertools:1.3.0（1234 ms）"
    );
    let skipped = step_line(&outcome(StepStatus::Skipped), 9);
    assert!(skipped.contains("产物已就绪"), "{skipped}");
}

#[test]
fn diagnostic_keeps_the_stable_code_and_the_hint() {
    let failed = ProcessorError::failed("net.minecraftforge:installertools:1.3.0", "1", "boom")
        .with_hint("确认 Java 版本与磁盘权限");
    let text = diagnostic(&failed);
    assert!(text.contains("processor_failed"), "{text}");
    assert!(text.contains("退出码 1"), "{text}");
    assert!(text.contains("boom"), "stderr 尾部要留：{text}");
    assert!(text.contains("建议：确认 Java 版本"), "{text}");

    let plan = diagnostic(&ProcessorError::plan("install profile 缺 `jar`"));
    assert!(plan.contains("processor_plan"), "{plan}");
}

#[test]
fn java_missing_points_at_the_java_page() {
    let managed = PathBuf::from("/tmp/.ULL/java");
    let text = java_missing(&JavaRequirement::table(21, None), &managed);
    assert!(text.contains("Java 21+"), "{text}");
    assert!(text.contains("Java」页"), "{text}");
    assert!(text.contains("JAVA_HOME"), "{text}");
    assert!(text.contains(&managed.display().to_string()), "{text}");
}

#[test]
fn libraries_failed_lists_at_most_three() {
    let files: Vec<FileOutcome> = (0..4)
        .map(|index| FileOutcome {
            role: FileRole::Library,
            name: format!("lib{index}.jar"),
            dest: PathBuf::from(format!("lib{index}.jar")),
            status: FileStatus::Failed,
            bytes: 0,
            source: None,
            failure: Some(FileFailure {
                kind: DownloadErrorKind::Network,
                message: "连接超时".to_string(),
            }),
        })
        .collect();
    let report = InstallReport {
        files,
        version_json: None,
        manifest: None,
        warnings: Vec::new(),
    };
    let text = libraries_failed(&report);
    assert!(text.contains("4 个失败，共 4 个"), "{text}");
    assert!(text.contains("lib0.jar"), "{text}");
    assert!(!text.contains("lib3.jar"), "最多列三条：{text}");
}

#[test]
fn stage_skips_when_the_version_json_is_already_there() {
    let h = Harness::new();
    let installer = installer();
    let profile = profile();
    let mut input = input(&profile, &installer);
    input.installed = true;
    let fake = Fake::new();
    let result = stage(&h.env, &h.engine, &input, &fake, &h.reporter(), &h.cancel);
    assert!(matches!(result, Ok(None)));
    assert!(fake.calls().is_empty(), "已装完就不该再动网络/Java");
    assert!(h.log().contains("跳过 installer processors"), "{}", h.log());
}

#[test]
fn stage_skips_an_empty_processor_list() {
    let h = Harness::new();
    let installer = installer();
    let profile = InstallProfile::default();
    let input = input(&profile, &installer);
    let fake = Fake::new();
    let result = stage(&h.env, &h.engine, &input, &fake, &h.reporter(), &h.cancel);
    assert!(matches!(result, Ok(None)), "没有 processor 就没这段");
    assert!(fake.calls().is_empty());
}

#[test]
fn stage_reports_missing_installer_metadata() {
    let h = Harness::new();
    let input = StageInput {
        kind: CoreLoaderKind::Forge,
        mc_version: "1.20.1",
        version_id: "1.20.1-forge-47.2.0",
        profile: None,
        installer: None,
        installed: false,
    };
    let fake = Fake::new();
    let err = stage(&h.env, &h.engine, &input, &fake, &h.reporter(), &h.cancel).unwrap_err();
    assert!(err.contains("install profile"), "{err}");
    assert!(h.log().contains("失败："), "{}", h.log());
}

#[test]
fn stage_builds_the_command_line_from_the_install_profile() {
    let h = Harness::new();
    let installer = installer();
    let profile = profile();
    let input = input(&profile, &installer);
    let fake = Fake::new();
    let run = stage(&h.env, &h.engine, &input, &fake, &h.reporter(), &h.cancel)
        .expect("stage 成功")
        .expect("应当真的跑了 processors");
    assert_eq!(run.executed(), 1);
    let steps = fake.steps();
    assert_eq!(steps.len(), 1);
    let (index, label, args, classpath) = &steps[0];
    assert_eq!(*index, 1);
    assert_eq!(label, "net.minecraftforge:installertools:1.3.0");
    assert_eq!(args, &vec!["--task".to_string(), "MCP_DATA".to_string()]);
    assert_eq!(*classpath, 1, "jar 自己不在 -cp 里（拼命令行时才补）");
    assert_eq!(fake.calls(), vec!["libraries", "java", "run"]);
    let messages = h.messages();
    assert!(
        messages.iter().any(|m| m.contains("installer 处理器 1/1")),
        "面板要看到当前处理器：{messages:?}"
    );
    assert!(
        messages
            .last()
            .is_some_and(|m| m.contains("正在写入版本清单")),
        "{messages:?}"
    );
    assert!(
        h.log().contains("processors 完成：执行 1 步"),
        "{}",
        h.log()
    );
}

#[test]
fn stage_fails_loudly_without_a_usable_java() {
    let h = Harness::new();
    let installer = installer();
    let profile = profile();
    let input = input(&profile, &installer);
    let mut fake = Fake::new();
    fake.java = None;
    let err = stage(&h.env, &h.engine, &input, &fake, &h.reporter(), &h.cancel).unwrap_err();
    assert!(err.contains("Java"), "{err}");
    assert!(err.contains("托管目录"), "{err}");
    assert_eq!(
        fake.calls(),
        vec!["libraries", "java"],
        "没 Java 就不该起进程"
    );
    assert!(h.log().contains("失败："), "{}", h.log());
}

#[test]
fn stage_fails_loudly_when_a_processor_exits_nonzero() {
    let h = Harness::new();
    let installer = installer();
    let profile = profile();
    let input = input(&profile, &installer);
    let mut fake = Fake::new();
    fake.run_error = Some(ProcessorError::failed(
        "net.minecraftforge:installertools:1.3.0",
        "1",
        "java.lang.NoSuchMethodError",
    ));
    let err = stage(&h.env, &h.engine, &input, &fake, &h.reporter(), &h.cancel).unwrap_err();
    assert!(err.contains("processor_failed"), "{err}");
    assert!(err.contains("NoSuchMethodError"), "{err}");
    let log = h.log();
    assert!(log.contains("失败："), "{log}");
    assert!(log.contains("NoSuchMethodError"), "日志要能回溯：{log}");
}

#[test]
fn stage_reports_incomplete_library_downloads() {
    let h = Harness::new();
    let installer = installer();
    let profile = profile();
    let input = input(&profile, &installer);
    let mut fake = Fake::new();
    fake.libraries_ok = false;
    let err = stage(&h.env, &h.engine, &input, &fake, &h.reporter(), &h.cancel).unwrap_err();
    assert!(err.contains("依赖库没下全"), "{err}");
    assert!(err.contains("连接超时"), "{err}");
    assert_eq!(fake.calls(), vec!["libraries"], "库没下全就别去选 Java");
    let progress = h.meter.lock().expect("进度锁").progress.clone();
    assert!(progress.files_total >= 2, "依赖库要计入文件总数");
}
