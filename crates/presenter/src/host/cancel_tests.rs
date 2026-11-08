//! 取消安装的**实测装置**（M105）。
//!
//! ## 为什么这条测试要真起一次安装
//!
//! 「点取消没反应」有三种可能的断点，读码分不出来：
//! ① 按钮/消息链断了（不派 `AppCommand::CancelInstall`）；
//! ② host 收下取消后，**还在跑的任务**继续发进度事件，把面板又点亮（面板闪一下又回来）；
//! ③ 取消根本停不下传输（M105 之前 core 的 `execute` 没有取消入参，下载线程会跑到段尾）。
//!
//! ①② 能靠单测的假事件流抓；③ 只有真起一次安装、真取消、真量「取消之后还在发生什么」才抓得到。
//! 修完之后 ①②③ 都有守门：① 是 app 层的消息测试，② 是下面那条 CI 可跑的
//! [`progress_from_a_cancelled_job_is_dropped`]，③ 就是本用例（真网络）。
//!
//! 所以这一条是**真网络**用例：`#[ignore]` 守着（CI 不跑），人工跑：
//!
//! ```text
//! cargo test -p unlucky-presenter --test '' -- real_install --ignored --nocapture
//! # 或（等价，本用例在 lib 测试里）
//! cargo test -p unlucky-presenter real_install_cancel -- --ignored --nocapture
//! ```
//!
//! ## 它量什么（三样里能抓住的两样）
//!
//! | 现象 | 怎么量 | 抓住什么 / 抓不住什么 |
//! |---|---|---|
//! | 进度还在推进 | 取消后收到的 `InstallProgress(Some)` 条数与 `files_done` 增量 | 抓得住「UI 侧还在被事件喂」；抓不住事件之外的后台动作 |
//! | 断点文件 | `<game>/**/*.ullpart*` 的总字节：取消时 vs 取消后 20 s，**并且**绝对值必须是 0 | 抓得住「还在写」与「留了半截垃圾」；抓不住「线程还活着」（`.ullpart` 不再长也可能是卡在等响应） |
//! | 半成品变成品 | `<game>/versions/<id>/<id>.json`（版本清单）与 `<launcher_home>/config/profiles.json`（**实例登记的唯一真相源**，`LauncherLayout::profiles_config()`） | 抓得住「取消后仍然落了成品 / 登记了实例」；抓不住「用户看不到的临时目录」 |
//!
//! 线程与 socket 两样**抓不住**（没有可移植的查询口，见 `probe` 一节），所以这里不用它当判据。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::dto::{AppCommand, AppEvent, Backend, CoreEvent, InstallProgressView, InstallStage};

use super::job::HostTask;
use super::{env::HostEnv, CoreHost};

/// 实测用的数据根（放在固定位置而不是系统 temp：便于人工复跑时留着看）。
fn root() -> PathBuf {
    std::env::temp_dir().join("ull-m105-cancel")
}

/// 递归数 `.ullpart` 家族（中间文件 + 续传元数据）的总字节。
///
/// 目录很小（只有本次安装写的那几层），所以裸递归够用；读不动的一律当 0
/// （量的是趋势，不是精确值）。
fn part_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            total += part_bytes(&path);
        } else if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            // 中间文件 `<name>.ullpart[-<pid>-<seq>]` 与它的元数据 `.ullmeta.json`
            if name.contains(".ullpart") {
                total += meta.len();
            }
        }
    }
    total
}

/// 一次 tick 后抽干事件，返回本轮的全部事件。
fn pump(host: &mut CoreHost, wait: Duration) -> Vec<AppEvent> {
    host.tick(wait);
    host.drain_events()
}

/// 事件里的安装进度（别的变体一概忽略）。
fn progress(event: &AppEvent) -> Option<&crate::dto::InstallProgressView> {
    match event {
        AppEvent::Core(CoreEvent::InstallProgress(Some(progress))) => Some(progress),
        _ => None,
    }
}

/// **实测主用例**：真起一次安装 → 取消 → 量取消之后还在发生什么。
///
/// 版本用环境变量 `ULL_CANCEL_VERSION` 覆盖（默认 `1.20.1`：文件多、总量够大，
/// 取消点之后还有一大截没下完，于是「取消后还在跑」一眼可见）。
#[test]
#[ignore = "真网络：真起一次安装再取消（需要 ULL_CANCEL_VERSION 指定的版本可下载）"]
fn real_install_cancel_stops_progress_and_leaves_no_half_product() {
    let version = std::env::var("ULL_CANCEL_VERSION").unwrap_or_else(|_| "1.20.1".to_string());
    let root = root();
    std::fs::remove_dir_all(&root).ok();

    let env = HostEnv::at_root(&root);
    let game = env.game.root().to_path_buf();
    // 实例登记的真相源是 `<launcher_home>/config/profiles.json`（`LauncherLayout::profiles_config()`），
    // 不是 `<game>/instances` —— 后者根本不存在，探它等于什么都没探（复审 P2-3 第 2 点）。
    let launcher = env.layout.clone();
    let mut host = CoreHost::headless(env);

    // ---- 起点：取消之前 ----
    host.dispatch(AppCommand::InstallVersion(version.clone()));
    let started = Instant::now();
    let mut before_cancel_progress = 0_usize;
    let mut done_at_cancel = 0_u64;
    // 等到「真在下载」（files_done ≥ 2）再取消：取消得太早会落在 Planning/解析阶段，
    // 那种取消本来就只是阶段边界停，证明不了下载中取消的行为。
    while started.elapsed() < Duration::from_secs(180) {
        for event in pump(&mut host, Duration::from_millis(100)) {
            if let Some(progress) = progress(&event) {
                before_cancel_progress += 1;
                done_at_cancel = done_at_cancel.max(progress.files_done);
            }
        }
        if done_at_cancel >= 2 {
            break;
        }
    }
    assert!(
        done_at_cancel >= 2,
        "180 s 内没进到下载阶段（files_done={done_at_cancel}）：网络或版本号有问题，先让这条能跑起来"
    );
    let parts_at_cancel = part_bytes(&game);

    // ---- 取消 ----
    host.dispatch(AppCommand::CancelInstall);
    let cancelled = Instant::now();
    let mut after: Vec<(f32, u64, InstallStage, u64)> = Vec::new();
    for event in host.drain_events() {
        if let Some(progress) = progress(&event) {
            after.push((
                cancelled.elapsed().as_secs_f32(),
                progress.files_done,
                progress.stage,
                part_bytes(&game),
            ));
        }
    }

    // ---- 取消之后 20 s：还会不会被喂进度、断点文件还会不会长 ----
    //
    // 每秒采一次 `.ullpart` 总字节：判据不是「取消那一刻的瞬时值」，而是**它在 5 s 之后
    // 不再变高**——在飞的那个文件本来就要多活一个数据块（取消延迟的定义），
    // 5 s 是给它留的余量（远超一个数据块）。
    let settle = Duration::from_secs(5);
    let mut latest = None;
    let mut samples: Vec<(f32, u64)> = Vec::new();
    let mut next_sample = Duration::from_secs(1);
    while cancelled.elapsed() < Duration::from_secs(20) {
        for event in pump(&mut host, Duration::from_millis(100)) {
            if let Some(progress) = progress(&event) {
                latest = Some(progress.clone());
                after.push((
                    cancelled.elapsed().as_secs_f32(),
                    progress.files_done,
                    progress.stage,
                    part_bytes(&game),
                ));
            }
        }
        let elapsed = cancelled.elapsed();
        if elapsed >= next_sample {
            samples.push((elapsed.as_secs_f32(), part_bytes(&game)));
            next_sample += Duration::from_secs(1);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let parts_after = part_bytes(&game);
    let done_after = latest
        .as_ref()
        .map_or(done_at_cancel, |view| view.files_done);

    // ---- 取证：日志行 ----
    eprintln!("[cancel-test] root={}", root.display());
    eprintln!(
        "[cancel-test] 取消前：进度事件 {before_cancel_progress} 条，files_done={done_at_cancel}，.ullpart 合计 {parts_at_cancel} 字节"
    );
    eprintln!(
        "[cancel-test] 取消后 20 s：进度事件 {} 条，files_done={done_after}（+{} 个文件），.ullpart 合计 {parts_after} 字节（{:+}）",
        after.len(),
        done_after.saturating_sub(done_at_cancel),
        i64::try_from(parts_after).unwrap_or(i64::MAX) - i64::try_from(parts_at_cancel).unwrap_or(i64::MAX)
    );
    for (at, parts) in &samples {
        eprintln!("[cancel-test]   t={at:>5.1}s ullpart={parts}");
    }
    for (index, (at, done, stage, parts)) in after.iter().enumerate().take(12) {
        eprintln!(
            "[cancel-test]   #{index} +{at:.2}s files_done={done} stage={stage:?} ullpart={parts}"
        );
    }
    if after.len() > 12 {
        eprintln!("[cancel-test]   …（另有 {} 条）", after.len() - 12);
    }

    // ---- 断点文件 / 临时目录 / 成品三样 ----
    // 版本 JSON 只在「全量就绪」才落盘（`execute` 的 complete 判定）；
    // 实例登记在 `run()` 的最后一步，取消后不该登记（读 profiles.json 这份唯一真相源）。
    let version_json = game
        .join("versions")
        .join(&version)
        .join(format!("{version}.json"));
    let profiles = unlucky_core::game::install::instance::read_profiles(&launcher)
        .expect("profiles.json 应可读（不存在 = 空清单）");
    eprintln!(
        "[cancel-test] 版本清单{} / profiles.json 里 {} 条实例",
        if version_json.is_file() {
            "已落盘"
        } else {
            "未落盘"
        },
        profiles.profiles.len()
    );

    // ---- 判据（修好之后必须成立的三条） ----
    assert!(
        after.is_empty(),
        "取消之后还在被喂进度事件（{} 条）：面板会被重新点亮 = 用户看到的「点了没反应」",
        after.len()
    );
    let settled = samples
        .iter()
        .find(|(at, _)| Duration::from_secs_f32(*at) >= settle)
        .map(|(_, parts)| *parts)
        .unwrap_or(parts_at_cancel);
    let grew = samples
        .iter()
        .filter(|(at, _)| Duration::from_secs_f32(*at) >= settle)
        .map(|(_, parts)| *parts)
        .max()
        .map_or(0, |peak| peak.saturating_sub(settled));
    assert_eq!(
        grew, 0,
        "取消 {settle:?} 之后断点文件还在长（+{grew} 字节）：下载没有真的停"
    );
    assert_eq!(
        parts_after, 0,
        "取消之后还留着 {parts_after} 字节的 `.ullpart`：半截垃圾没清掉（D21 要的就是「清掉或标清楚」）"
    );
    assert!(
        profiles.profiles.is_empty(),
        "取消之后仍然登记了实例（profiles.json 有 {} 条）：半成品被记成成品",
        profiles.profiles.len()
    );
    assert!(
        !version_json.is_file(),
        "取消之后仍然落了版本清单：半成品被记成「已安装」"
    );

    std::fs::remove_dir_all(&root).ok();
}

// --------------------------------------------------------------- CI 可跑的守门

/// 一条安装进度快照（够画面板就行）。
fn progress_snapshot(files_done: u64) -> InstallProgressView {
    InstallProgressView {
        version: "1.20.1".to_string(),
        stage: InstallStage::Downloading,
        files_done,
        files_total: 512,
        ..InstallProgressView::new()
    }
}

/// **「点了取消没反应」的真根因**：取消之后旧任务的进度事件还在往回灌。
///
/// 现场：用户点「取消安装」→ host 收下取消、清 `install`、发 `InstallProgress(None)`（面板回弹），
/// 但**后台线程还在跑**（core 的下载段不是瞬停的），它每下完一个文件就发一条
/// `HostTask::Install`。没有代次的话这条事件照样被翻译成 `InstallProgress(Some(..))`，
/// 面板立刻又被点亮 —— 用户看到的就是「取消按钮点了没反应」。
///
/// 抓得住：代次不符的 `Install` / `InstallDone` 被丢掉（面板不复燃、也不清掉新一轮的取消句柄）。
/// 抓不住：真网络的取消延迟（那条在 `real_install_cancel_*`，`#[ignore]` 守着要手动跑）。
#[test]
fn progress_from_a_cancelled_job_is_dropped() {
    let dir = std::env::temp_dir().join(format!("ull-cancel-epoch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut host = CoreHost::headless(HostEnv::at_root(&dir));

    // 开工：拿当代次
    let epoch = host.next_install_epoch();
    host.handle(HostTask::Install {
        progress: Box::new(progress_snapshot(3)),
        epoch,
    });
    let live: Vec<AppEvent> = host.drain_events();
    assert!(
        live.iter()
            .any(|event| matches!(event, AppEvent::Core(CoreEvent::InstallProgress(Some(_))))),
        "当代次的事件必须照常点亮面板"
    );

    // 取消
    host.dispatch(AppCommand::CancelInstall);
    let cancelled: Vec<AppEvent> = host.drain_events();
    assert!(
        cancelled
            .iter()
            .any(|event| matches!(event, AppEvent::Core(CoreEvent::InstallProgress(None)))),
        "取消要立刻把面板收回（不等后台）"
    );
    assert!(
        !cancelled
            .iter()
            .any(|event| matches!(event, AppEvent::Core(CoreEvent::InstallProgress(Some(_))))),
        "取消本身不该重新点亮面板"
    );

    // 旧任务继续发进度（现场就是这样）——必须被丢掉
    host.handle(HostTask::Install {
        progress: Box::new(progress_snapshot(4)),
        epoch,
    });
    host.handle(HostTask::InstallDone {
        progress: Box::new(progress_snapshot(512)),
        result: Err(crate::dto::ContentFailure::new(
            crate::dto::ContentFailureKind::Other,
            "已取消",
        )),
        epoch,
    });
    let after: Vec<AppEvent> = host.drain_events();
    assert!(
        after.is_empty(),
        "取消之后旧任务的事件必须一条都不出（实际 {} 条）：出了就会把面板重新点亮",
        after.len()
    );

    // 新一轮安装的代次是新的：它的进度照常生效
    let fresh = host.next_install_epoch();
    assert_ne!(fresh, epoch, "新一轮开工必须换代次");
    host.handle(HostTask::Install {
        progress: Box::new(progress_snapshot(1)),
        epoch: fresh,
    });
    assert!(
        host.drain_events()
            .iter()
            .any(|event| matches!(event, AppEvent::Core(CoreEvent::InstallProgress(Some(_))))),
        "新一轮的进度要能点亮面板（不能把代次守卫写成「一律丢弃」）"
    );

    std::fs::remove_dir_all(&dir).ok();
}
