//! presenter 的**真令牌**接线（M195 复审 P2-2）：取消标记必须真的交给 core。
//!
//! # 为什么判据只能是盘上的事实
//!
//! 只断言"没收到 `PackEvent::Report`"是**两条路都成立**的伪证明：`import::run` 的早退条件是
//! `report.status == Cancelled || cancel.load()`，后者会把"根本没接线"完全遮住——core 照旧跑完
//! 并登记实例，界面只看到面板收回。那正是本 mission 要消灭的形态，所以本用例盯的是**目标目录
//! 与 `profiles.json`**：标记一进来就是 true ⇒ 目录里一个文件都不该有、实例一个都不该登记。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::dto::{ConflictDecisionsView, ImportRequestView};

use super::harness::{temp_dir, write};

/// 一个能走完管线的最小 MultiMC 包（目录形态：不需要 zip 写入器）。
fn multimc_pack(dir: &std::path::Path) -> std::path::PathBuf {
    let source = dir.join("Better MC");
    write(
        &source.join("mmc-pack.json"),
        br#"{"formatVersion": 1, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"}]}"#,
    );
    write(&source.join(".minecraft/mods/sodium.jar"), &[0x11; 1024]);
    write(&source.join(".minecraft/config/options.txt"), b"fov=90\n");
    source
}

#[test]
fn a_pre_cancelled_flag_really_stops_the_core_pipeline() {
    let dir = temp_dir("cancel-wired");
    let source = multimc_pack(&dir);
    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("data root");
    let env = crate::host::HostEnv::at_root(&data);
    let request = ImportRequestView {
        pack: source,
        instance: None,
        instance_name: Some("Cancelled Pack".to_string()),
        decisions: ConflictDecisionsView::default(),
    };
    // 目标目录与"是否登记"都按 host 的真口径算（判据不能自己另算一份）。
    let (target, register) = super::super::import::target_of(&env, &request).expect("目标目录");
    assert!(register, "走新实例这条路才覆盖「登记」这一环");

    let (tx, _rx) = crate::host::job::channel();
    let cancel = Arc::new(AtomicBool::new(true));
    super::super::import::run(&env, &tx, &request, 11, Arc::clone(&cancel));
    drop(tx);
    assert!(cancel.load(Ordering::SeqCst), "用例前提：标记一直是 true");

    assert!(
        !target.exists(),
        "取消后目标目录不该存在（core 在入口检查点就停手，暂存目录也删了）：{}",
        target.display()
    );
    let profiles =
        unlucky_core::game::install::instance::read_profiles(&env.layout).expect("读档案");
    assert!(
        profiles.profiles.is_empty(),
        "取消后不许登记实例：{:?}",
        profiles.profiles
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// **路径 A：取消来得太晚**（M195 r3 的核心用例）。
///
/// 三段拼起来，全部确定性、没有 sleep：
/// 1. **core 真管线**跑一次合并态导入，把取消置在**最后一条** `ImportStage::Commit` 进度上——
///    之后没有任何检查点，于是管线一路跑完（用户的 `mods/a.jar` 被覆盖），报告是
///    `Success` + `rolled_back == false`（core 一直是诚实的）；
/// 2. `cancel_pack` 把代次 +1（这就是"取消"），并记下被作废的那个代次；
/// 3. `finish` 照发 `PackEvent::Report`，`handle` 让它**穿透代次**——界面因此拿到「已完成 /
///    未回滚 / 取消太晚」，而不是旧行为里那句无条件的「已停手并回滚」。
///
/// 变异：把终态通道去掉（`handle` 恢复"代次不符一律丢"）⇒ 本用例立刻红（报告到不了界面）。
#[test]
fn a_late_cancel_still_delivers_the_real_outcome_to_the_ui() {
    use std::sync::atomic::AtomicUsize;

    use unlucky_core::addon::import::{
        import_pack, ConflictPolicy, ImportCancel, ImportProgress, ImportProgressSink,
        ImportRequest, ImportStage, ImportStatus,
    };
    use unlucky_core::download::blocking::block_on;

    use crate::dto::{AppEvent, Backend, CoreEvent, PackEvent};

    let dir = temp_dir("cancel-late");
    // 包里有 `mods/a.jar`，目标目录里也有（内容不同）⇒ 合并态"被覆盖"是真的会发生。
    let source = dir.join("Path A Pack");
    write(
        &source.join("mmc-pack.json"),
        br#"{"formatVersion": 1, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"}]}"#,
    );
    write(&source.join(".minecraft/mods/a.jar"), b"pack a");
    let target = dir.join("instance");
    write(&target.join("mods/a.jar"), b"user a");

    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("data root");
    let env = crate::host::HostEnv::at_root(&data);

    // ① core：取消置在最后一条逐文件进度上 ⇒ 之后没有检查点 ⇒ 管线跑完。
    let token = ImportCancel::new();
    let sink_token = token.clone();
    let seen = Arc::new(AtomicUsize::new(0));
    let seen_by_sink = Arc::clone(&seen);
    let sink: ImportProgressSink = Arc::new(move |progress: &ImportProgress| {
        if progress.stage != ImportStage::Commit || progress.total == 0 {
            return;
        }
        seen_by_sink.fetch_add(1, Ordering::SeqCst);
        if progress.done + 1 == progress.total {
            sink_token.cancel();
        }
    });
    let report = block_on(import_pack(
        &ImportRequest::new(&source, &target)
            .with_cancel(&token)
            // 覆盖档：让用户的 `mods/a.jar` 真的被覆盖（缺省是"保留用户那一侧"）。
            .with_conflict_policy(ConflictPolicy::Overwrite)
            .with_progress(sink),
    ));
    assert!(token.is_cancelled(), "用例前提：取消确实置位了");
    assert!(
        seen.load(Ordering::SeqCst) > 1,
        "确实逐文件走到了底（合并态）"
    );
    assert_eq!(
        report.status,
        ImportStatus::Success,
        "取消置在最后一条之后 ⇒ 管线已经跑完：{:?}",
        report.error
    );
    assert!(
        !report.rolled_back,
        "没有回滚就别说回滚过（core 的机器位是诚实的）"
    );
    assert_eq!(
        std::fs::read(target.join("mods/a.jar")).expect("读被覆盖的文件"),
        b"pack a",
        "路径 A 的前提：用户的同名文件**已经被覆盖**"
    );

    // ② 用户点取消：代次 +1，被作废的那个代次进了终态通道。
    let epoch = super::super::pack_epoch(&env);
    let _in_flight = super::super::InFlight::begin(&env);
    assert!(
        super::super::cancel_pack(&env),
        "取消应当看到一条在跑的导入"
    );
    assert_ne!(
        super::super::pack_epoch(&env),
        epoch,
        "取消会作废本次代次（这正是旧实现丢报告的成因）"
    );

    // ③ 收尾照发报告；同代次上的**进度**事件仍按代次丢掉。
    let mut host = crate::host::CoreHost::headless(env.clone());
    let (tx, rx) = crate::host::job::channel();
    super::super::import::finish(&tx, &target, false, epoch, true, &report);
    tx.send(crate::host::job::HostTask::Pack {
        event: Box::new(PackEvent::Progress(None)),
        epoch,
    })
    .expect("送一条同代次的进度事件");
    drop(tx);
    for task in rx.iter() {
        host.handle(task);
    }
    let events = host.drain_events();

    let delivered: Vec<&crate::dto::ImportReportView> = events
        .iter()
        .filter_map(|event| match event {
            AppEvent::Core(CoreEvent::Pack(inner)) => match inner.as_ref() {
                PackEvent::Report(view) => Some(view.as_ref()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let view = delivered
        .first()
        .expect("取消太晚时，真实结果必须到达界面（丢掉就是路径 A 的假话）");
    assert_eq!(
        view.status,
        crate::dto::ImportStatusView::Success,
        "界面拿到的结局是「已完成」"
    );
    assert!(
        !view.rolled_back,
        "界面拿到的结局是「未回滚」，不是「已回滚」"
    );
    assert_eq!(
        view.cancel,
        crate::dto::ImportCancelView::TooLate,
        "必须标成「取消太晚」，不能标成「已回滚」"
    );
    assert!(
        !events.iter().any(|event| match event {
            AppEvent::Core(CoreEvent::Pack(inner)) =>
                matches!(inner.as_ref(), PackEvent::Progress(_)),
            _ => false,
        }),
        "终态通道只放行报告：取消后的进度事件仍按代次丢掉"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
