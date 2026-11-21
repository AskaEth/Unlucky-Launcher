//! M321：`pack import` 的人读输出要印出**处理器链**与**告警**。
//!
//! M324 追加：进度回调记下的阶段名在这份输出里要印**目录文案**，不是 core 的内部 id
//! （`阶段：probe → loader` ⇒ `阶段：读取清单 → 安装加载器`），判据见最后那一条用例。
//!
//! 从 `pack/tests.rs` 分出来（那边贴着 400 行硬线），判据只有一条：
//! **数据层有的，人读输出里就得看得见，而且不是占位符**。M318 真包盘查的 F3 是
//! 「`report.warnings` 里明明有 6 条告警，`pack.rs` 全文没有 `warnings`」。

use super::super::outcome;
use super::{ctx_at, TempDir};

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use unlucky_core::addon::import::{
    keys, ImportIssue, ImportProgress, ImportReport, ImportStage, ProcessorStep, ProcessorSummary,
};
use unlucky_core::game::java::JavaOrigin;

use crate::cli::program::PLACEHOLDER;
use crate::cli::PackImportArgs;

fn args() -> PackImportArgs {
    PackImportArgs {
        path: PathBuf::from("pack.mrpack"),
        target: None,
        name: None,
        overwrite: false,
        no_register: false,
    }
}

fn summary() -> ProcessorSummary {
    ProcessorSummary {
        java: "C:/fake/java/bin/java.exe".to_string(),
        java_origin: JavaOrigin::Managed,
        java_major: Some(17),
        executed: 2,
        skipped: 0,
        steps: vec![
            ProcessorStep {
                index: 1,
                label: "net.minecraftforge:installertools:1.3.0".to_string(),
                status: "executed",
                outputs: 2,
                elapsed_ms: 1234,
            },
            ProcessorStep {
                index: 2,
                label: "net.minecraftforge:jarsplitter:1.1.4".to_string(),
                status: "executed",
                outputs: 1,
                elapsed_ms: 88,
            },
        ],
        warnings: Vec::new(),
    }
}

/// 告警 / 注记**分成两段**，各算各的条数，且**渲染的是数据层那份内容**（不是一句占位）。
///
/// # 这条用例在 M339 之后验什么
///
/// M321 时它验的是「有一个标题写着 `告警/注记 N 条` 的混装段」——因为那时 core 里**没有级别**
/// 这一维，CLI 只能把两种混在一个标题下。M339 把级别补进 `ImportIssue::level` 之后，**那条
/// 断言本身就是要消灭的东西**（用户得看行尾才能分辨哪条要紧），所以改成按级别验：
///
/// 1. 三条 issue（两条 `Warning` + 一条 `Note`）落在**两个**标题下：`要处理：2 条` / `只是告知：1 条`；
/// 2. 告警档那句原文在「要处理」段、注记档那句在「只是告知」段（**顺序也是判据**：
///    注记不许跑到「要处理」前头）；
/// 3. `--json` 里每条都带 `level`（机器通道拿得到级别）。
///
/// # M342：那两个标题**读目录**，不是手写
///
/// 标题由 `CATALOG.render(Key::PackReportWarnings|PackReportNotes, count)` 出，与 GUI 的报告、
/// 预检两屏同两个 key。改目录里那两句 ⇒ 本用例（CLI）与 GUI 的
/// `the_report_splits_issues_by_level_and_counts_each` 一起红；改前 CLI 是手写字面量，只红 GUI。
///
/// 抓得住什么（变异判据）：
/// - 把 `outcome` 里 `human.extend(warnings_lines(report))` 那一行删掉 ⇒ 红（一条都找不到）；
/// - 把 `warnings_lines` 的分档去掉（两档一个标题）⇒ 红（`要处理：2 条` / `只是告知：1 条` 找不着，
///   只会看到 `要处理：3 条`）；
/// - 把 `ImportIssue::note` 那句改成 `warning`（或反过来）⇒ 两个标题的条数都变 ⇒ 红；
/// - 把稳定 key（`pack.import.note.…`）印回人读行 ⇒ 红——那是内部标识，只许留在 `--json`。
/// - 段标题改回手写字面量（不读目录）⇒ 目录改字时本条不红 ⇒ 抄回了 M342 修的那条漂移通道。
#[test]
fn the_human_output_renders_every_warning_from_the_report() {
    let dir = TempDir::new("m321-warnings");
    let ctx = ctx_at(dir.path());
    let mut report = ImportReport::empty(PathBuf::from("C:/tmp/instance"));
    report.warnings.push(ImportIssue::warning(
        keys::NOTE_PROCESSOR_WARNING,
        "产物摘要不符：forge-1.20.1-47.1.39-client.jar（本机 JDK 的 zlib 变体）",
    ));
    report.warnings.push(ImportIssue::warning(
        keys::NOTE_LOADER_WARNING,
        "loader 库缺下载地址，已降级",
    ));
    report.warnings.push(ImportIssue::note(
        keys::NOTE_REGISTERED,
        "已登记实例 \"Survival\"（fabric-loader-0.15.11）",
    ));

    let outcome = outcome(
        &ctx,
        &report,
        &PathBuf::from("C:/tmp/instance"),
        &[],
        &args(),
        None,
    );
    let human = &outcome.report.human;
    let index_of = |needle: &str| {
        human
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("人读输出里没有「{needle}」：{human:?}"))
    };
    // ① 两段、各算各的条数（`要处理：2 条` 不是 `要处理：3 条`，也不是 M321 那个混装标题）。
    //    排版取自目录（`PackReportWarnings` 那两句），M342：以前是 CLI 自排的 `要处理 2 条：`。
    let warnings_at = index_of("要处理：2 条");
    let notes_at = index_of("只是告知：1 条");
    assert!(
        warnings_at < notes_at,
        "要处理那一段要排在只是告知前头：{human:?}"
    );
    // ② 原文按档落位。
    let zlib = index_of("zlib 变体");
    let registered = index_of("已登记实例");
    assert!(
        (warnings_at..notes_at).contains(&zlib),
        "告警档那条要落在「要处理」段里：{human:?}"
    );
    assert!(
        registered > notes_at,
        "注记档那条不许混进「要处理」段：{human:?}"
    );
    // ③ 机器通道：每条都带级别（`--json` 的 `warnings[].level`）。
    for (index, expected) in ["warning", "warning", "note"].iter().enumerate() {
        assert_eq!(
            outcome.report.data["warnings"][index]["level"], *expected,
            "--json 的第 {index} 条要带级别（机器通道靠它分档）"
        );
    }
    // **M323 改了这条判据的正负号**：稳定 key（`pack.import.note.…`）以前也印在人读行里，
    // 那是内部标识，用户看不懂、GUI 也不显示；稳定 key 仍在 `--json` 的 `warnings[].message_key`
    // 里给脚本用。变异：把 `warnings_lines` 改回 `（{message_key}）` ⇒ 本条红。
    assert!(
        human
            .iter()
            .all(|line| !line.contains(keys::NOTE_PROCESSOR_WARNING)),
        "人读输出不许再印内部 key：{human:?}"
    );
    assert_eq!(
        outcome.report.data["warnings"][0]["message_key"],
        keys::NOTE_PROCESSOR_WARNING,
        "脚本要的稳定 key 在 --json 里，不许一起删掉"
    );
}

/// 处理器链那一段（人读）+ `--json` 的 `processors` 段（数据层）。
///
/// 同一个 `report.processors`，两个出口：人读里逐步骤一行，JSON 里一份结构化结果。
/// 抓得住：`processor_lines(report)` 那一行被删掉 ⇒ 人读里找不到"处理器 1/2"；报告里那一份
/// 不序列化 ⇒ JSON 断言红。
#[test]
fn the_human_output_shows_every_processor_step_it_ran() {
    let dir = TempDir::new("m321-processors");
    let ctx = ctx_at(dir.path());
    let mut report = ImportReport::empty(PathBuf::from("C:/tmp/instance"));
    report.processors = Some(summary());

    let outcome = outcome(
        &ctx,
        &report,
        &PathBuf::from("C:/tmp/instance"),
        &[],
        &args(),
        None,
    );
    let human = &outcome.report.human;
    assert!(
        human
            .iter()
            .any(|line| line.contains("处理器：执行 2 步 / 跳过 0 步")),
        "要有一行汇总：{human:?}"
    );
    assert!(
        human
            .iter()
            .any(|line| line.contains("处理器 1/2") && line.contains("installertools")),
        "每一步都要有一行（含 jar 坐标）：{human:?}"
    );
    assert!(
        human
            .iter()
            .any(|line| line.contains("处理器 2/2") && line.contains("jarsplitter")),
        "{human:?}"
    );
    // 整份人读输出都不许留 `%prog%` 占位符（M319 的守门条款同样适用于本轮新增的段）。
    assert!(
        human.iter().all(|line| !line.contains(PLACEHOLDER)),
        "{human:?}"
    );
    // `--json` 那一段：结构化字段齐全，脚本不看人读输出也拿得到。
    let data = &outcome.report.data;
    assert_eq!(data["processors"]["executed"], 2, "{data}");
    assert_eq!(
        data["processors"]["steps"][1]["label"],
        "net.minecraftforge:jarsplitter:1.1.4"
    );
    assert_eq!(data["processors"]["java_origin"], "managed", "{data}");
}

// ------------------------------- M321 r1 P2-1：装载器段的逐文件行（离线，不靠真包）

/// 装载器段的逐文件进度必须真的走到那个回调，并且只印装载器段。
///
/// 抓得住什么（变异判据）：
/// - `pack.rs` 不再用 `lines::progress_sink`（退回只记阶段名的旧闭包）⇒ 记录器一行都收不到；
/// - `progress_sink` 里 `out(&line)` 那一句被删 ⇒ 同上；
/// - `loader_file_line` 不再看 `stage` ⇒ 「非装载器阶段不许印」那条红。
#[test]
fn the_loader_file_progress_really_reaches_the_sink_output() {
    let stages: Arc<Mutex<Vec<ImportStage>>> = Arc::new(Mutex::new(Vec::new()));
    let printed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&printed);
    let out: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |line: &str| {
        recorder.lock().expect("锁").push(line.to_string());
    });
    let sink = super::super::lines::progress_sink(Arc::clone(&stages), out);

    let progress = |stage: ImportStage, done: usize, total: usize, current: &str| ImportProgress {
        stage,
        done,
        total,
        current: Some(current.to_string()),
    };
    // ①节流的第一行必印；②别的阶段一律不印；③最后一行（done == total）必印。
    sink(&progress(ImportStage::Loader, 1, 2, "libraries/a.jar"));
    sink(&progress(ImportStage::Probe, 1, 2, "mods/a.jar"));
    sink(&progress(ImportStage::Loader, 2, 2, "libraries/b.jar"));

    let printed = printed.lock().expect("锁").clone();
    assert_eq!(printed.len(), 2, "{printed:?}");
    assert!(
        printed[0].contains("装载器 1/2 libraries/a.jar"),
        "{printed:?}"
    );
    assert!(
        printed[1].contains("装载器 2/2 libraries/b.jar"),
        "收尾那一行不能丢：{printed:?}"
    );
    assert!(
        printed.iter().all(|line| !line.contains("mods/a.jar")),
        "非装载器阶段不许冒充装载器进度：{printed:?}"
    );
    // 阶段照旧按**发射顺序、去重**记录（人读的 `阶段：…` 行与 `--json` 的 `stages` 都靠它）。
    assert_eq!(
        stages.lock().expect("锁").as_slice(),
        &[ImportStage::Loader, ImportStage::Probe, ImportStage::Loader]
    );
}

// -------------------- M324：阶段名印目录文案，不印 core 的内部 id

/// 喂一条 `ImportStage::Loader` 的进度事件 ⇒ 人读那段不许露出 `loader`，要露出目录里的中文。
///
/// 抓得住什么（变异判据）：
/// - `stages_line` / `progress_sink` 改回印 `stage.as_str()` ⇒ 断言①红（`loader` 露出来）；
/// - `stage_key` 里任一条指错 key（比如六个都取 `PackStageProbe`）⇒ 断言②逐字对账红；
/// - `lines.rs` 里删掉 `stage_key` 的某个臂 ⇒ 编译不过（穷尽匹配，这是比字符串针更硬的守卫）。
#[test]
fn the_stage_line_speaks_the_catalog_not_core_ids() {
    let stages: Arc<Mutex<Vec<ImportStage>>> = Arc::new(Mutex::new(Vec::new()));
    // 出口用来喂真 sink；这一条测的是收尾时把记录渲染成一行人读，不看逐文件行。
    let out: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(|_line: &str| {});
    let sink = super::super::lines::progress_sink(Arc::clone(&stages), out);
    sink(&ImportProgress {
        stage: ImportStage::Loader,
        done: 1,
        total: 2,
        current: Some("libraries/b.jar".to_string()),
    });

    let seen = stages.lock().expect("锁").clone();
    assert_eq!(seen, vec![ImportStage::Loader], "阶段要记进 `seen`");
    let line = super::super::lines::stages_line(&seen).expect("走过了阶段就得出这一行");
    // ① 人读行里不许出现 core 的内部 id（目录里的中文名不含任何 ASCII 字母）。
    assert!(
        !line.contains(|c: char| c.is_ascii_alphabetic()),
        "阶段行印了 core 的内部 id：{line}"
    );
    // ①' 逐 id 断言（r1 建议）：ASCII 哨兵是超集守卫（能兜住 core 将来新增的 id），这组更精确
    // ——若目录文案将来合法包含拉丁字母（专名之类），把上面那条哨兵删掉、只留这组即可；改动在
    // 编译期就能看见的测试里，不是线上缺陷。
    for stage in ImportStage::ALL {
        assert!(
            !line.contains(stage.as_str()),
            "阶段行印了 core 的内部 id `{}`：{line}（若目录文案将来合法包含拉丁字母，把上面的 ASCII 哨兵改成这组逐 id 断言）",
            stage.as_str()
        );
    }
    assert!(
        line.contains("安装加载器"),
        "目录里 `PackStageLoader` 那句没出现：{line}"
    );
    // ② 六档逐个对账（文案的真相源是 i18n 的金表；这里钉「CLI 取的是哪一条」与顺序）。
    let all = super::super::lines::stages_line(&ImportStage::ALL).expect("六个阶段");
    assert_eq!(
        all,
        "阶段：读取清单 → 解包覆盖内容 → 补下载文件 → 安装加载器 → 落地写盘 → 登记实例"
    );
    // ③ 机器位不动：`--json` 的 `stages` 仍是稳定 id（`pack_test.rs` 断言过 `probe`/`commit`）。
    assert_eq!(
        super::super::lines::stage_ids(&ImportStage::ALL),
        vec!["probe", "extract", "fetch", "loader", "commit", "register"]
    );
}

/// **穿过 `outcome()`** 的那条判据（r1 F1）：人读输出里印的是目录文案而不是 core 的内部 id，
/// 且**一个阶段都没走到**时不许印出一行光秃秃的「阶段：」。
///
/// # 为什么必须穿过 `outcome()`
///
/// 只测 `stages_line` 的返回值时，`outcome()` 里那句 `if let Some(line) = stages_line(stages)
/// { human.push(line) }` 无人值守：把它改回 `format!("阶段：{}", stage_ids(stages).join(" → "))`
/// ——也就是本 mission 要根除的那个 bug——**整个 `unlucky-cli` 套件仍绿**（复审 r1 的变异 C/D
/// 实证）。判据要钉的字符串在 `human` 里，所以得从 `human` 里读。
///
/// 抓得住什么（变异判据）：
/// - `outcome()` 的人读行改回印 `as_str()` ⇒ 断言①红；
/// - 删掉 `stages_line` 的 `is_empty` 守卫 ⇒ 断言②红（多出一行「阶段：」）。
#[test]
fn the_stage_line_crosses_outcome_and_skips_an_empty_run() {
    let dir = TempDir::new("m324-stage-outcome");
    let ctx = ctx_at(dir.path());
    let report = ImportReport::empty(PathBuf::from("C:/tmp/instance"));
    let target = PathBuf::from("C:/tmp/instance");

    // ① 走过三个阶段：那一行说目录文案（顺序 = 发射顺序）。
    let heard = outcome(
        &ctx,
        &report,
        &target,
        &[ImportStage::Probe, ImportStage::Loader, ImportStage::Commit],
        &args(),
        None,
    );
    let human = &heard.report.human;
    let line = human
        .iter()
        .find(|line| line.starts_with("阶段："))
        .unwrap_or_else(|| panic!("人读输出里没有阶段行：{human:?}"));
    assert_eq!(line, "阶段：读取清单 → 安装加载器 → 落地写盘");
    // 超集守卫（任何内部 id 露头都红；目录文案将来合法引入拉丁专名时删这条、留下面那组）。
    assert!(
        !line.contains(|c: char| c.is_ascii_alphabetic()),
        "阶段行印了 core 的内部 id：{line}"
    );
    for stage in ImportStage::ALL {
        assert!(
            !line.contains(stage.as_str()),
            "阶段行印了 core 的内部 id `{}`：{line}",
            stage.as_str()
        );
    }

    // ② 一个阶段都没走到（预检就失败的导入）：不许有「阶段：」这一行，光秃秃的也不行。
    let silent = outcome(&ctx, &report, &target, &[], &args(), None);
    assert!(
        silent
            .report
            .human
            .iter()
            .all(|line| !line.contains("阶段：")),
        "没走到阶段却印了一行阶段：{:?}",
        silent.report.human
    );
}

/// M325 的两条用例（处理器行的 Java 来源说人话 / 缺文件行的「离线」只说一次）。
///
/// 挂在这里而不是 `tests.rs`：那边已是 399 行（400 行红线），再加一条 `#[path]` 就超限。
/// 做法照 `commands::agent::probe::tests` 挂 `tests_guards.rs` 的先例（同级测试文件挂子测试文件）。
#[path = "m325.rs"]
mod m325;
