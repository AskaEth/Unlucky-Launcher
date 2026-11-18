//! 启动失败/收尾的**文案**用例（从 `app/view/tests.rs` 拆出：那边 400 行到线）。
//!
//! 这里的断言都钉**具体句子**（不是「字符串非空」）：磁贴与 Toast 必须说同一件事，
//! 「没起来」与「起来又崩」不许共用一句（M161 复审 P1-1）。

use super::*;

/// 失败后磁贴要留「重试」，且行/详情文案按「哪一步 + 原文」给。
#[test]
fn failed_instances_can_be_retried() {
    // 进程**没起来**：说「哪一步 + 原文」。
    let failed = instance(InstanceRuntime::Failed {
        cause: unlucky_presenter::dto::LaunchFailureCause::Stage {
            stage: unlucky_presenter::dto::LaunchFailureStage::Spawn,
            detail: "os error 5".to_string(),
        },
    });
    // 列表行是**单行**：只放「哪一步」（原文太长会被 `Wrapping::None` 裁成半截字）。
    assert_eq!(
        super::instances::runtime_status_row(&failed.runtime, Lang::En.catalog()).as_deref(),
        Some("Launch failed · Starting the game process")
    );
    // 详情页给全文（阶段 + 原文）。
    assert_eq!(
        super::instances::runtime_status(&failed.runtime, Lang::En.catalog()).as_deref(),
        Some("Launch failed · Starting the game process: os error 5")
    );
    assert!(
        detail_of(&failed, Lang::En.catalog()).can_launch,
        "失败后要能重试"
    );
}

/// **「没起来」与「起来又崩」不许说成同一句**（复审 P1-1 的文案面）。
///
/// 抓得住：崩溃/静默失败被塞回 `LaunchFailureStage::Spawn`（磁贴会写「启动游戏进程失败」，
/// 而进程明明起来了）；退出码漏成 `Some(1)` 这种 Debug 文本；静默失败丢掉
/// 「exit 0 却没跑起来」这条信息。断言的是**具体句子**，不是「字符串非空」。
#[test]
fn a_crash_after_start_is_not_worded_as_a_start_failure() {
    use unlucky_presenter::dto::{LaunchFailureCause, LaunchFailureStage};

    let line = |cause: LaunchFailureCause, lang: Lang| {
        let runtime = InstanceRuntime::Failed { cause };
        let row = super::instances::runtime_status_row(&runtime, lang.catalog()).expect("行文案");
        let detail = super::instances::runtime_status(&runtime, lang.catalog()).expect("详情文案");
        (row, detail)
    };

    // ① 没起来：说阶段。
    let (row, detail) = line(
        LaunchFailureCause::Stage {
            stage: LaunchFailureStage::Spawn,
            detail: "os error 5".to_string(),
        },
        Lang::En,
    );
    assert_eq!(row, "Launch failed · Starting the game process");
    assert_eq!(
        detail,
        "Launch failed · Starting the game process: os error 5"
    );

    // ② 起来又崩：说结局 + 数字退出码（不许出现 `Some(1)`）。
    let (row, detail) = line(LaunchFailureCause::Crashed { exit_code: Some(1) }, Lang::En);
    assert_eq!(row, "Launch failed · the game crashed (exit 1)");
    assert_eq!(detail, row, "结局类没有「原文」可加，行与详情同一句");
    assert!(!detail.contains("Some("), "退出码不许漏 Debug：{detail}");
    assert!(
        !detail.contains("Starting the game process"),
        "进程起来了，不许说「启动游戏进程」：{detail}"
    );

    // ③ 静默失败：exit 0 却没跑起来 —— 这条信息不能丢。
    let (row, zh) = line(
        LaunchFailureCause::SilentFailure { exit_code: Some(0) },
        Lang::ZhCn,
    );
    assert_eq!(row, "启动失败 · 退出码 0，没起来");
    assert_eq!(zh, row);
    assert!(row.contains("没起来"), "{row}");

    // ④ 平台没给退出码：只说崩溃，不编一个 0 出来。
    let (row, _) = line(LaunchFailureCause::Crashed { exit_code: None }, Lang::En);
    assert_eq!(row, "Launch failed · The game crashed");
}

///
/// 此前 `LaunchFinished.summary` 是 presenter 拼的中文，英文界面上弹的就是中文；
/// 现在 presenter 只给结构（结局 + 退出码 + 失败阶段），文案在这里按 key 现求。
///
/// 抓得住：①谁把中文串再写回 presenter（英文那一行会露中文）；②失败时丢掉 `detail`
/// （原文是取证信息，不许翻也不许删）；③「跑起来再崩」被说成「启动失败在某一步」。
#[test]
fn the_launch_toast_speaks_the_interface_language() {
    use unlucky_presenter::dto::{
        LaunchFailureStage, LaunchFailureView, LaunchFinished, LogOutcome,
    };

    let finished = |outcome, failure| LaunchFinished {
        id: "survival".to_string(),
        session: "20261031T120000-abcd12".to_string(),
        outcome,
        exit_code: Some(1),
        uptime_ms: 12_500,
        failure,
    };

    // 正常结束：只报结局 + 时长（`正常结束 · 12.5 秒` / `Finished normally · 12.5s`）。
    let ok = finished(LogOutcome::Ok, None);
    let en = super::instances::launch_toast(&ok, Lang::En);
    let zh = super::instances::launch_toast(&ok, Lang::ZhCn);
    assert_eq!(en, "Finished normally · 12.5s");
    assert_eq!(zh, "正常结束 · 12.5 秒");
    assert_ne!(en, zh, "切语言时提示语要变");

    // 装配阶段失败：说清**哪一步**，并把原文原样带出来。
    let failed = finished(
        LogOutcome::Crashed,
        Some(LaunchFailureView::new(
            LaunchFailureStage::InstanceConfig,
            "permission denied (os error 5)",
        )),
    );
    let en = super::instances::launch_toast(&failed, Lang::En);
    let zh = super::instances::launch_toast(&failed, Lang::ZhCn);
    assert!(
        en.starts_with("Launch failed · Reading the instance settings"),
        "{en}"
    );
    assert!(zh.starts_with("启动失败 · 读实例设置"), "{zh}");
    for text in [&en, &zh] {
        assert!(
            text.contains("permission denied (os error 5)"),
            "原文（取证信息）不许丢：{text}"
        );
    }
    // 英文界面里不许出现中文（反之亦然）——这条是本次债的核心。
    assert!(!en.chars().any(is_cjk), "英文提示里混了中文：{en}");
    assert!(
        !zh.contains("Launch failed"),
        "中文提示里混了英文前缀：{zh}"
    );
}

/// 有没有中文字符（上面那条「不许混语言」的判据）。
fn is_cjk(ch: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&ch)
}
