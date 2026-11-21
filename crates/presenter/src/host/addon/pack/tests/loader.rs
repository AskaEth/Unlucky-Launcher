//! M311：presenter 这一层**真的把第六跳接上了**吗。
//!
//! # 为什么判据是"必然失败"而不是"装成功"
//!
//! 装载器安装要联网（官方 meta → 原版 → 库）。GUI 用例不能依赖网络，而"有网时该成功"这条
//! 已经被 core 的 `addon::import::loader::tests` 用注入假件钉死（那边断言"装载器真的被请求、
//! 版本 id 与清单一致、版本 JSON 读得回来"）。
//!
//! 这里要证明的是另一件事：**`import::run` 里的 `.with_loader_install(...)` 真的接上了**。
//! 判据选一个"离线（连不上）与在线（404）都会失败"的加载器版本——`fabric 999.0.0`，
//! 于是：
//!
//! - 接线**在** ⇒ core 走到第六跳 ⇒ 装不上 ⇒ 这次导入失败、实例**不登记**（本用例绿）；
//! - 接线**被删**（变异） ⇒ core 照旧把包内容落地并登记实例、状态 `Success` ⇒ 本用例红
//!   （`status` 不再是 `Failed`、`registered` 变成 `Some`、`profiles.json` 多出一条）。
//!
//! 另外它还证明"失败不会拖几百 MB 下载"：`resolve` 在抓 profile 时就失败了，`execute` 一步没走。

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::dto::{ConflictDecisionsView, ImportRequestView, ImportStatusView};

use super::harness::{temp_dir, write};

#[test]
fn an_uninstallable_loader_fails_the_import_instead_of_registering_a_broken_instance() {
    let dir = temp_dir("import-loader-fail");
    let source = dir.join("Bad Loader MC");
    // 加载器版本 999.0.0 在官方 meta 上不存在 ⇒ 第六跳必然失败（离线/在线同结论）。
    write(
        &source.join("mmc-pack.json"),
        br#"{"formatVersion": 1, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"},
            {"uid": "net.fabricmc.fabric-loader", "version": "999.0.0"}]}"#,
    );
    write(&source.join(".minecraft/mods/sodium.jar"), &[0x11; 1024]);

    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("data root");
    let env = crate::host::HostEnv::at_root(&data);

    let (tx, rx) = crate::host::job::channel();
    let request = ImportRequestView {
        pack: source,
        instance: None,
        instance_name: Some("Broken Loader".to_string()),
        decisions: ConflictDecisionsView::default(),
    };
    super::super::import::run(&env, &tx, &request, 3, Arc::new(AtomicBool::new(false)));
    drop(tx);

    let mut report = None;
    for task in rx.iter() {
        if let crate::host::job::HostTask::Pack { event, .. } = task {
            if let crate::dto::PackEvent::Report(view) = *event {
                report = Some(*view);
            }
        }
    }
    let report = report.expect("必须有结果事件");

    // ①这次导入**失败**（不是"部分成功"，更不是成功）——界面画的是红块。
    assert_eq!(
        report.status,
        ImportStatusView::Failed,
        "装载器装不上必须是失败：{:?}",
        report.error
    );
    let error = report.error.as_ref().expect("必须有具名错误");
    assert_eq!(error.code, "import_loader", "错误大类要是加载器那条");
    // ②原因要指到"哪个加载器的哪个版本"，不是一句"失败了"。
    assert!(
        error.message.contains("999.0.0"),
        "原因里要点名版本：{}",
        error.message
    );
    // ③恢复动作要在（失败要指向下一步）。
    assert!(error.hint.is_some(), "要给恢复动作：{:?}", error);
    // ④**实例不许被登记成"看起来能启动"的样子**：档案里一条都不许有。
    assert!(report.registered.is_none(), "装不上时不许登记实例");
    let profiles =
        unlucky_core::game::install::instance::read_profiles(&env.layout).expect("读档案");
    assert!(
        profiles.profiles.is_empty(),
        "profiles.json 里不许出现这条实例：{:?}",
        profiles.profiles
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_gui_step_order_is_the_same_as_the_core_emission_order() {
    // **顺序判据的另一半**（复审 r1 P2-1）：core 那边有
    // `addon::import::loader::tests::the_stages_really_fire_in_the_declared_order` 钉"实际发射
    // 顺序 == `ImportStage::ALL`"，这条钉**两处 `ALL` 同序**——界面步号按
    // `ImportStageView::ALL.position() + 1` 算，它与 core 的声明顺序一旦分叉，用户就会看到
    // "5/6 安装加载器"装完跳回"4/6 落地写盘"。
    //
    // 抓得住：把 `ImportStageView::ALL` 里 `Loader` / `Commit` 换回旧顺序（或只改 core 枚举）⇒ 本条红。
    use unlucky_core::addon::import::ImportStage;

    let core: Vec<&str> = ImportStage::ALL
        .iter()
        .map(|stage| stage.as_str())
        .collect();
    let gui: Vec<&str> = crate::dto::ImportStageView::ALL
        .iter()
        .map(|stage| stage.slug())
        .collect();
    assert_eq!(
        gui, core,
        "界面阶段顺序必须等于 core 的执行顺序（步号按它算）"
    );
}
