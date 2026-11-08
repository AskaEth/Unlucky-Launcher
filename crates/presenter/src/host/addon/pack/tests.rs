//! M113 的证据（都跑真管线 / 真 host，不用假数据）：
//!
//! | 用例 | 钉住什么 |
//! |---|---|
//! | [`bad_packs_get_a_named_diagnosis`] | 坏包必须有具名失败，不是静默、不是 panic |
//! | [`preview_counts_match_the_written_report`] | 预检的 N/M 与最终报告**逐项相等** |
//! | [`a_real_pack_runs_through_the_pipeline_end_to_end`] | 正常包能走完管线并落盘 |
//! | [`chinese_instance_names_get_distinct_directories`] | 中文实例名不会塌成同一个目录 |
//! | [`a_failed_new_instance_directory_can_be_reused_for_retry`] | 失败残留的空目录不挡重试 |
//! | [`pack_epoch_is_per_host_and_is_its_own_counter`] | 代次按 host 分开，不再跨 host 串扰 |
//! | [`a_failed_import_releases_the_in_flight_flag_and_keeps_install_cancel_intact`] | 早退路径放回「在跑」计数，取消安装仍是安装的取消 |
//!
//! 夹具（临时目录、写文件、泵 host）在 [`harness`]。

mod harness;
mod scan;

use harness::{pack_events, pump, pump_until, temp_dir, write};

use unlucky_core::addon::export::{
    export_instance, scan_instance, ExportRequest, InstanceSnapshot, OfflineCatalog,
    DEFAULT_CONTENT_ROOTS,
};

use crate::dto::{ConflictDecisionsView, ExportClassView, ExportRequestView, PackFormatView};

#[test]
fn bad_packs_get_a_named_diagnosis() {
    let dir = temp_dir("badpack");

    // ① 把文本文件改名成 .mrpack：连 zip 都不是
    let fake = dir.join("fake.mrpack");
    write(&fake, b"this is not a zip");
    let probe = super::diagnose::of(&fake).expect("打不开的包必须有诊断");
    assert!(
        !probe.detail.is_empty(),
        "诊断要给一句具体原因（不是空字符串）"
    );
    assert!(
        probe.detail.to_lowercase().contains("archive") || probe.detail.contains("zip"),
        "诊断要说清是哪一步失败：{}",
        probe.detail
    );

    // ② 目录形态：有 overrides/、没有 manifest.json ⇒ 「像 CF，缺 manifest.json」
    let dir_pack = dir.join("half");
    write(&dir_pack.join("overrides").join("options.txt"), b"fov=90\n");
    if let Some(probe) = super::diagnose::of(&dir_pack) {
        assert!(
            !probe.saw.is_empty(),
            "看到了 overrides/ 就该说出来（saw 不能为空）"
        );
        if probe.looks_like.is_some() {
            assert!(
                !probe.missing.is_empty(),
                "说「像某个格式」就必须说出缺哪个标记文件"
            );
        }
    }
}

#[test]
fn preview_counts_match_the_written_report() {
    let dir = temp_dir("export-eq");
    let root = dir.join("instance");
    write(&root.join("mods").join("sodium.jar"), &[0x11; 4096]);
    write(&root.join("mods").join("lithium.jar"), &[0x22; 2048]);
    write(&root.join("mods").join("old.jar.disabled"), &[0x33; 512]);
    write(&root.join("config").join("options.txt"), b"fov=90\n");
    // 白名单之外的目录不该被扫（否则「将内联 N」会虚高）
    write(&root.join("logs").join("latest.log"), b"noise\n");

    let request = ExportRequestView {
        instance: "survival".to_string(),
        format: PackFormatView::Modrinth,
        name: "Survival".to_string(),
        version: "1.0.0".to_string(),
        target: Some(dir.join("out.mrpack")),
    };
    let env_root = dir.join("data");
    std::fs::create_dir_all(&env_root).expect("env");
    let env = crate::host::HostEnv::at_root(&env_root);

    // 预检（不写盘）
    let preview = super::export::preview_of(&root, &env, &request);

    // 真写盘：用同一套判据（离线目录 + 默认内容目录 + 直接给的快照）
    let snapshot = InstanceSnapshot {
        game_version: "1.20.1".to_string(),
        loaders: Vec::new(),
    };
    let target = dir.join("out.mrpack");
    let core_request = ExportRequest::new(
        &root,
        &target,
        &unlucky_core::addon::pack::modrinth::MODRINTH,
    )
    .with_name("Survival")
    .with_version("1.0.0")
    .with_catalog(&OfflineCatalog)
    .with_snapshot(&snapshot);
    let report = export_instance(&core_request).expect("导出应当成功");

    // 三个计数逐项相等（塔台要求：不是「大致对」）
    assert_eq!(
        preview.inline_count(),
        report.inline_count(),
        "内联条数不一致：预检 {} vs 报告 {}",
        preview.inline_count(),
        report.inline_count()
    );
    assert_eq!(
        preview.skipped_count(),
        report.skipped_count(),
        "无法导出条数不一致"
    );
    assert_eq!(
        preview.remote_count(),
        report.remote_count(),
        "远程引用条数不一致"
    );

    // 逐条：同一个路径在两边必须是同一个类
    for file in &preview.files {
        let written = report
            .files
            .iter()
            .find(|candidate| candidate.path == file.path)
            .unwrap_or_else(|| panic!("预检多出一条：{}", file.path));
        let expected = match file.class {
            ExportClassView::Remote => unlucky_core::addon::export::FileClass::Remote,
            ExportClassView::Inline => unlucky_core::addon::export::FileClass::Inline,
            ExportClassView::Skipped => unlucky_core::addon::export::FileClass::Skipped,
        };
        assert_eq!(written.class, expected, "{} 的归类不一致", file.path);
    }

    // 白名单外的 logs/ 不许进包（防「扫太宽」把预检数字弄虚）
    assert!(
        !report
            .files
            .iter()
            .any(|file| file.path.starts_with("logs/")),
        "logs/ 不该被扫进整合包"
    );

    // 同时钉一条：`scan_instance` 的口径就是预览用的那一套
    let roots: Vec<String> = DEFAULT_CONTENT_ROOTS
        .iter()
        .map(|entry| (*entry).to_string())
        .collect();
    let (files, skipped) = scan_instance(&root, &roots);
    assert_eq!(files.len(), preview.inline_count());
    assert_eq!(skipped.len(), preview.skipped_count());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_real_pack_runs_through_the_pipeline_end_to_end() {
    // 真管线端到端：造一个 MultiMC 实例目录当「包」（目录形态不需要 zip 写入器），
    // 走 `import::run` 的**真**路径，看事件流里有没有「进度 + 结果」、文件有没有落地。
    let dir = temp_dir("import-e2e");
    let source = dir.join("Better MC");
    write(
        &source.join("mmc-pack.json"),
        br#"{"formatVersion": 1, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"},
            {"uid": "net.fabricmc.fabric-loader", "version": "0.15.11"}]}"#,
    );
    write(&source.join(".minecraft/mods/sodium.jar"), &[0x11; 1024]);
    write(&source.join(".minecraft/config/options.txt"), b"fov=90\n");

    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("data root");
    let env = crate::host::HostEnv::at_root(&data);

    let (tx, rx) = crate::host::job::channel();
    let request = crate::dto::ImportRequestView {
        pack: source,
        instance: None,
        instance_name: Some("Imported Pack".to_string()),
        decisions: ConflictDecisionsView::default(),
    };
    super::import::run(
        &env,
        &tx,
        &request,
        7,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    drop(tx);

    let mut progress = 0usize;
    let mut report = None;
    for task in rx.iter() {
        match task {
            crate::host::job::HostTask::Pack { event, epoch } => {
                assert_eq!(epoch, 7, "代次必须原样带回（取消判定靠它）");
                match *event {
                    crate::dto::PackEvent::Progress(Some(_)) => progress += 1,
                    crate::dto::PackEvent::Report(view) => report = Some(*view),
                    other => panic!("意料之外的事件：{other:?}"),
                }
            }
            other => panic!("意料之外的任务：{other:?}"),
        }
    }

    assert!(progress > 0, "至少要有一条进度事件");
    let report = report.expect("必须有结果事件");
    assert_ne!(
        report.status,
        crate::dto::ImportStatusView::Failed,
        "正常包不该失败：{:?}",
        report.error
    );
    assert_eq!(
        report.format,
        Some(crate::dto::PackFormatView::Multimc),
        "应当认成 MultiMC 格式"
    );
    assert!(
        report.written() > 0,
        "报告里要至少有写盘的文件：{:?}",
        report.files
    );
    // 落盘位置以**报告里的 target** 为准（那是 host 真算出来的目录，界面显示的也是它）
    let landed = &report.target;
    assert!(
        landed.join("mods/sodium.jar").is_file(),
        "文件没有落到实例目录：{}",
        landed.display()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn chinese_instance_names_get_distinct_directories() {
    // 复审 r1 P2-8：slug() 只留 ASCII，中文名会全塌成 `pack`，第二个就撞目录
    let first = super::convert::dir_name("我的整合包");
    let second = super::convert::dir_name("另一个整合包");
    assert_ne!(first, second, "两个不同的中文名必须得到不同目录");
    assert_eq!(
        first,
        super::convert::dir_name("我的整合包"),
        "同一个名字要稳定（不然重试会漂到新目录）"
    );
    assert!(first.starts_with("pack-"), "退化时要留可读前缀：{first}");

    // 判据与注释对齐（复审 r2 P2-2）：只含 ASCII 字母数字/空格/`-`/`_` ⇒ 保持原样
    assert_eq!(super::convert::dir_name("Better-MC"), "Better-MC");
    assert_eq!(
        super::convert::dir_name("Better MC"),
        "Better-MC",
        "空格折成 `-`、**不加**哈希"
    );
    assert_eq!(
        super::convert::dir_name("pack"),
        "pack",
        "名字就叫 pack 也保留"
    );

    // 折过**别的**字符（这里是 `.`）就必须补哈希，否则不同名字会撞同一个目录
    let versioned = super::convert::dir_name("Survival 1.20.1");
    assert!(versioned.starts_with("Survival-1-20-1-"), "{versioned}");
    assert_ne!(versioned, "Survival-1-20-1", "折过 `.` 就要补哈希");
}

#[test]
fn a_failed_new_instance_directory_can_be_reused_for_retry() {
    // 复审 r1 P2-9：失败回滚留下空目录时会挡住重试
    let dir = temp_dir("retry");
    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("data root");
    let env = crate::host::HostEnv::at_root(&data);

    let request = crate::dto::ImportRequestView {
        pack: dir.join("whatever.mrpack"),
        instance: None,
        instance_name: Some("Retry Me".to_string()),
        decisions: ConflictDecisionsView::default(),
    };

    // 第一次：`target_of` 只算路径（预检要能只读地用它），真要写盘前才 `prepare_target`。
    let (first, register) = super::import::target_of(&env, &request).expect("首次应当算得出目录");
    assert!(register);
    assert!(
        !first.exists(),
        "`target_of` 不许建目录（M160：预检是只读的）"
    );
    super::import::prepare_target(&first).expect("写盘前建目录");
    assert!(first.is_dir());

    // 第二次（模拟上一次失败留下的空目录）⇒ 复用，而不是报「已存在」
    let (second, _) = super::import::target_of(&env, &request).expect("空目录应当可复用");
    assert_eq!(first, second);

    // 目录里已经有东西 ⇒ 拒绝（不能往别人的实例里倒）
    std::fs::write(second.join("marker.txt"), b"x").expect("write");
    assert!(
        super::import::target_of(&env, &request).is_err(),
        "非空目录必须拒绝"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pack_epoch_is_per_host_and_is_its_own_counter() {
    // 复审 r2 P3-6：代次**按 host** 分开（不再是跨 host 共享的裸静态量）。
    // 两个 host（两个临时根）各 bump 一次，互不影响 —— 这正是并行测试串扰的根因。
    let one = temp_dir("epoch-one");
    let two = temp_dir("epoch-two");
    let env_one = crate::host::HostEnv::at_root(one.join("data"));
    let env_two = crate::host::HostEnv::at_root(two.join("data"));

    let before_one = super::pack_epoch(&env_one);
    let before_two = super::pack_epoch(&env_two);
    super::bump_pack_epoch(&env_one);
    assert_eq!(
        super::pack_epoch(&env_one),
        before_one.wrapping_add(1),
        "自己这个 host 的代次 +1"
    );
    assert_eq!(
        super::pack_epoch(&env_two),
        before_two,
        "**另一个** host 的代次不受影响（P3-6 的根因）"
    );
    assert_eq!(super::in_flight(&env_two), 0, "在跑计数也是每 host 一份");

    let _ = std::fs::remove_dir_all(&one);
    let _ = std::fs::remove_dir_all(&two);
}

#[test]
fn a_failed_import_releases_the_in_flight_flag_and_keeps_install_cancel_intact() {
    // 复审 r2 P2-1 的护栏：早退路径必须把「在跑」计数放回去，否则之后**下载页**的
    // 「取消安装」会被当成导入取消（不发 `InstallCancelled`，还给内容库画一句假话）。
    use crate::dto::{
        AppCommand, AppEvent, Backend, CoreEvent, ImportRequestView, PackCommand, PackEvent,
    };

    let dir = temp_dir("inflight-guard");
    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("data root");
    let host_env = crate::host::HostEnv::at_root(&data);
    let mut host = crate::host::CoreHost::headless(host_env.clone());

    // ① 派一次**必然早退**的导入：包文件不存在 ⇒ 探测当场失败，根本不进管线
    host.dispatch(AppCommand::Pack(Box::new(PackCommand::Import(Box::new(
        ImportRequestView {
            pack: dir.join("missing.mrpack"),
            instance: None,
            instance_name: Some("Guess".to_string()),
            decisions: ConflictDecisionsView::default(),
        },
    )))));
    let events = pump_until(&mut host, |all| {
        pack_events(all)
            .iter()
            .any(|event| matches!(event, PackEvent::Probe(_)))
    });
    assert!(
        pack_events(&events)
            .iter()
            .any(|event| matches!(event, PackEvent::Probe(_))),
        "坏包必须给出探测诊断（而不是静默）；实际收到 {events:?}"
    );
    assert_eq!(
        super::in_flight(&host_env),
        0,
        "早退之后「在跑」计数必须归零（RAII guard 负责，不靠手工 finished）"
    );

    // ② 此后下载页的「取消安装」必须照常发 `InstallCancelled`，且**不能**冒出 pack 的取消
    host.dispatch(AppCommand::CancelInstall);
    let events = pump(&mut host);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AppEvent::Core(CoreEvent::InstallCancelled))),
        "没有导入在跑时，取消安装必须照常给回执（否则用户点了取消毫无反应）"
    );
    assert!(
        !pack_events(&events)
            .iter()
            .any(|event| matches!(event, PackEvent::Cancelled)),
        "此刻没有导入在跑，不该给内容库发「导入还在后台跑」"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
