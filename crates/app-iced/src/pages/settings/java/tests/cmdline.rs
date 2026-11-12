//! `tests.rs` 的后半：真命令行 / 落盘读回 / 版本门槛 / 截图种子。
//!
//! 拆出来的理由：`tests.rs` 加上守卫之后 434 行，越过 400 行硬线；这一半是「真值走完整条生产链」
//! 的一族（`launcher.json` → 组装链 / 重启后读回），与前半的「输入校验」不是一件事。
//! 公共的临时目录与假 JDK 构造在父模块（`super::root` / `super::fake_jdk`）。

use std::path::Path;

use super::super::state::{Fit, Layer, Notice, Requirement, RuntimeRow, State};
use super::super::store;
use super::{config, fake_jdk, root};

// --------------------------------------------------------------- 守卫④：内存真进命令行

/// 写进 `java.default_memory_mb` 的值必须出现在**真组装出来的 JVM 参数**里。
///
/// 走的是生产代码那条链：`launcher.json` → `GlobalLaunchDefaults` → `launch_overrides::resolve`
/// → `launch::overrides::apply` → `launch::args_table::assemble`（`ull launch --dry-run` 用的是同一条）。
/// 不是读配置回显：断言的是 `-Xmx<新值>m` 这个 argv 元素。
///
/// 变异证明：把 `store::save` 里写 `java.default_memory_mb` 的那一行删掉（或写死 2048），
/// 这条立刻红（argv 里还是 `-Xmx2048m`）。
#[test]
fn the_saved_memory_reaches_the_real_command_line() {
    let dir = root("cmdline");
    let file = config(&dir);
    let state = State {
        storage: true,
        memory: "6144".to_string(),
        jvm: "-Dfile.encoding=UTF-8".to_string(),
        ..State::default()
    };
    assert!(state.validate().is_ok());
    let saved = store::save(&file, &state).expect("保存应成功");
    assert_eq!(saved.memory_mb, 6144, "落盘后读回的值");
    assert_eq!(saved.jvm, "-Dfile.encoding=UTF-8");

    let args = real_argv(&file);
    assert!(
        args.iter().any(|arg| arg == "-Xmx6144m"),
        "内存字段必须长成 -Xmx6144m：{args:?}"
    );
    assert!(
        args.iter().any(|arg| arg == "-Dfile.encoding=UTF-8"),
        "java.extra_args 必须进命令行：{args:?}"
    );
    assert!(
        !args.iter().any(|arg| arg == "-Xmx2048m"),
        "出厂默认值不许还在里面：{args:?}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 用生产链组装一次命令行（`launch.json` 之外什么都不读）。
fn real_argv(config: &Path) -> Vec<String> {
    use std::path::PathBuf;

    use unlucky_core::game::launch_overrides::{resolve, GlobalLaunchDefaults};
    use unlucky_core::launch::args_table::assemble;
    use unlucky_core::launch::overrides;
    use unlucky_core::launch::platform::{OsFamily, PlatformCtx};
    use unlucky_core::launch::request::{
        InstanceInfo, JavaSelection, LaunchDirs, ManifestArgs, MemoryOptions,
    };
    use unlucky_core::launch::request::{
        LaunchRequest, LauncherInfo, Log4jOptions, OptimizationOptions,
    };

    let snapshot = store::load(config);
    let resolved = resolve(
        None,
        &GlobalLaunchDefaults {
            jvm_args: Some(snapshot.jvm.clone()),
            memory_mb: Some(snapshot.memory_mb),
            ..GlobalLaunchDefaults::default()
        },
    );
    let mut request = LaunchRequest {
        instance: InstanceInfo {
            id: "inst-1".to_string(),
            name: "1.20.1".to_string(),
            version: unlucky_core::launch::game_version::GameVersion::parse("1.20.1"),
            version_type: "release".to_string(),
        },
        launcher: LauncherInfo {
            name: "UnluckyLauncher".to_string(),
            version: "0.2.3".to_string(),
        },
        java: JavaSelection::new("C:/java/bin/java.exe", 21, 64),
        memory: MemoryOptions {
            min_mb: None,
            max_mb: None,
        },
        manifest: ManifestArgs {
            main_class: Some("net.minecraft.client.main.Main".to_string()),
            ..ManifestArgs::default()
        },
        dirs: LaunchDirs {
            game_dir: PathBuf::from("C:/mc"),
            primary_jar: PathBuf::from("C:/mc/versions/x/x.jar"),
            ..LaunchDirs::default()
        },
        classpath: vec![PathBuf::from("C:/mc/libraries/a.jar")],
        log4j: Log4jOptions {
            enabled: false,
            ..Log4jOptions::default()
        },
        optimization: OptimizationOptions {
            enabled: false,
            physical_memory_mb: None,
        },
        language: "zh-CN".to_string(),
        ..LaunchRequest::default()
    };
    overrides::apply(&mut request, &resolved);
    assemble(&request, &PlatformCtx::new(OsFamily::Windows, "x86_64"))
        .expect("组装应成功")
        .args
}

// --------------------------------------------------------------- 落盘 / 读回 / 门槛

/// 三键都能落盘、能在「重启后」读回来（新的 `State` + `hydrate` 就是重启的等价物）。
#[test]
fn the_settings_survive_a_restart() {
    let dir = root("restart");
    let file = config(&dir);
    let state = State {
        storage: true,
        memory: "8192".to_string(),
        jvm: "-XX:+UseG1GC".to_string(),
        ..State::default()
    };
    store::save(&file, &state).expect("保存");
    let selected = store::set_selected(&file, "C:/jdk-17/bin/java.exe").expect("选默认");

    // 「重启」：全新的 State 从盘上灌一次。
    let mut reborn = State::default();
    assert!(reborn.hydrate(store::load(&file)));
    assert_eq!(reborn.memory, "8192");
    assert_eq!(reborn.saved_memory, 8192);
    assert_eq!(reborn.jvm, "-XX:+UseG1GC");
    assert_eq!(reborn.selected, "C:/jdk-17/bin/java.exe");
    assert_eq!(selected.selected, "C:/jdk-17/bin/java.exe");

    // 手动名单也在（与设置同一个 config/ 目录下的纯文本）。
    let jdk = fake_jdk(&dir, "keep-me");
    std::fs::write(store::manual_file(&file), format!("{}\n", jdk.display())).expect("写名单");
    assert_eq!(store::load_manual(&file).len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

/// 「为什么选它」那一行：版本门槛来自实例，选中的运行时够不够要能算出来。
#[test]
fn the_requirement_explains_why_a_runtime_fits_or_not() {
    let mut state = State {
        scanned: true,
        runtimes: vec![
            RuntimeRow {
                binary: "C:/jdk-8/bin/java.exe".to_string(),
                version: String::new(),
                major: Some(8),
                vendor: None,
                source: "system",
            },
            RuntimeRow {
                binary: "C:/jdk-21/bin/java.exe".to_string(),
                version: String::new(),
                major: Some(21),
                vendor: None,
                source: "system",
            },
        ],
        selected: "C:/jdk-8/bin/java.exe".to_string(),
        ..State::default()
    };
    assert_eq!(state.fit(Some(8)), None, "没有要求时不下结论");

    state.context.required = Some(Requirement {
        instance: "生存服".to_string(),
        version: "1.20.1".to_string(),
        minimum: 17,
        maximum: None,
    });
    assert_eq!(
        state.fit(Some(8)),
        Some(Fit::TooOld {
            required: 17,
            actual: 8
        }),
        "太低要说「需要 17」"
    );
    assert_eq!(state.fit(Some(21)), Some(Fit::Ok));
    assert_eq!(
        state.default_row().map(|row| row.binary.clone()),
        Some("C:/jdk-8/bin/java.exe".to_string()),
        "默认那一行按 selected 找出来"
    );

    state.context.layer = Some(Layer::Instance {
        instance: "生存服".to_string(),
        mb: 8192,
    });
    assert!(matches!(state.layer(), Layer::Instance { mb: 8192, .. }));
}

/// 截图种子：三档都得给得出来（`--screenshot` 的取证开关）。
#[test]
fn the_screenshot_seeds_cover_the_three_states() {
    let empty = State::screenshot_seed("empty").expect("empty 档");
    assert!(empty.runtimes.is_empty() && empty.scanned);

    let two = State::screenshot_seed("two").expect("two 档");
    assert_eq!(two.runtimes.len(), 2);
    assert!(two.default_row().is_some(), "两个 JDK 里要选中其一");

    let editing = State::screenshot_seed("editing").expect("editing 档");
    assert!(editing.dirty, "编辑态要有未保存改动");
    assert!(matches!(editing.notice, Some(Notice::Refused(_))));
    assert!(State::screenshot_seed("nonsense").is_none());
}

/// M204 r1：扫描的三档（扫描中 / 还没扫描过 / 正在取消）——视觉复审 p2-1 要的正是这三格。
///
/// 每档的判据都落在**界面真的会读的那两个字段**上：`State.scanning`（状态行画什么）与
/// `State.scan.report().running`（心跳下一拍会不会把 `scanning` 推开）。
///
/// 变异证明：把 `"scanning"` 档里的 `scan.seed_report(...)` 换成不调 ⇒ `scanning` 在心跳第一拍
/// 就被推回 `None`（`apply_scan` 用槽里的快照重算）⇒ 第二段红。这也正是这两档必须写槽、
/// 不能只改 `State` 的理由。
#[test]
fn the_scan_seeds_cover_the_running_states() {
    let scanning = State::screenshot_seed("scanning").expect("scanning 档");
    assert_eq!(
        scanning.scanning,
        Some(super::super::state::Progress { done: 3, total: 7 }),
        "状态行要画「正在扫描… 3/7」"
    );
    assert!(
        scanning.scan.report().running,
        "槽里的快照也得是「在扫」，否则下一拍就把 scanning 推开"
    );
    assert!(!scanning.scanned, "还没扫完过");
    assert!(!scanning.cancelled, "没按过取消 ⇒「取消」这颗按钮可点");
    assert!(scanning.runtimes.is_empty(), "扫描中列表还没有结论");

    let cancelling = State::screenshot_seed("cancelling").expect("cancelling 档");
    assert!(cancelling.cancelled && cancelling.scan.report().running);
    assert!(
        cancelling.scanning.is_some(),
        "线程还没退出 ⇒ 状态行仍是活动态（画的是「正在取消…」）"
    );

    let unscanned = State::screenshot_seed("unscanned").expect("unscanned 档");
    assert!(unscanned.scanning.is_none() && !unscanned.scanned);
    assert!(!unscanned.scan.report().running, "这一档没有扫描在跑");
}

/// M197 的三档：版本选择列表 / 下载中 / 下载失败——出图靠它们，所以每档都要有**可断言的特征**。
///
/// 变异证明：把 `seed::build` 里 `"downloading"` 那一档的 `seed_report` 换成 `Report::default()`
/// ⇒ 第 2 段红（作业没了，图上就只剩目录）。
#[test]
fn the_download_seeds_cover_the_three_states() {
    use super::super::downloads::Phase;

    let list = State::screenshot_seed("download").expect("download 档");
    assert!(
        list.prefs.saved_auto_install && !list.prefs.saved_priority.is_empty(),
        "目录那一档要把三键摆成真值（自动准备开着 + 自己排过顺序）"
    );
    assert!(list.downloads.report().job.is_none(), "选择列表不是下载中");

    let running = State::screenshot_seed("downloading").expect("downloading 档");
    let job = running.downloads.report().job.expect("下载中要有作业");
    assert_eq!(job.phase, Phase::Downloading);
    assert_eq!(job.percent(), Some(45), "96.4 MiB / 210.7 MiB ≈ 45%");
    assert_eq!(job.source.as_deref(), Some("tuna"), "要说得出当前源");
    assert!(
        job.last.is_some(),
        "退避那一行也要有（图上要看见「失败 / 重试」）"
    );

    let failed = State::screenshot_seed("download-failed").expect("download-failed 档");
    let report = failed.downloads.report();
    let failure = report.failure.expect("失败档要有失败");
    assert_eq!(failure.major, 21);
    assert_eq!(failure.source.as_deref(), Some("github"));
    assert_eq!(
        report.tries.len(),
        2,
        "失败态要列出「试过哪些源」：tuna + github"
    );
}

/// 自动扫描的判据：首屏扫一次，扫过就不重扫，截图通道永不扫。
///
/// 变异证明：把 `auto_scan_due` 改成恒 `true`，第二、三档立刻红（每次心跳都发起扫描）；
/// 改成恒 `false`，第一档红（用户实测的头一条抱怨就是「设置页从来自动检测」）。
#[test]
fn the_auto_scan_predicate_runs_once_and_never_in_screenshots() {
    let fresh = store::ScanReport::default();
    assert!(
        store::auto_scan_due(&fresh, false),
        "首屏（没扫过、没在扫）应该自动扫一次"
    );
    assert!(
        !store::auto_scan_due(&fresh, true),
        "截图通道不许读本机装了什么"
    );

    let running = store::ScanReport {
        running: true,
        ..store::ScanReport::default()
    };
    assert!(!store::auto_scan_due(&running, false), "正在扫就不重入");

    let finished = store::ScanReport {
        finished: true,
        ..store::ScanReport::default()
    };
    assert!(
        !store::auto_scan_due(&finished, false),
        "扫过一次就不再自动扫（手动「立即扫描」仍可再扫）"
    );

    // 被取消过的扫描：`finished` 仍是 false（结论作废），但**不许**自动重来——否则
    // `launcher.json` 每变一次，心跳就会把用户刚做的取消无声撤销（r2 复审 P2-2）。
    let cancelled = store::ScanReport {
        cancelled: true,
        ..store::ScanReport::default()
    };
    assert!(
        !store::auto_scan_due(&cancelled, false),
        "取消过一次之后，本进程不再自动扫描"
    );
    // 手动「立即扫描」走的是 `ScanSlot::start`，不看这个判据 ⇒ 取消不是永久锁死。
    assert!(
        store::auto_scan_due(&store::ScanReport::default(), false),
        "全新进程（没扫过、没取消）仍然自动扫一次"
    );
}
