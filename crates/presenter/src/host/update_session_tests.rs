//! 会话状态的用例：拒绝后不再打扰、关掉自动检查后启动不派发、偏好跨重启、GUI/CLI 同一内核。
//!
//! 夹具在父模块，本文件 `use super::*`。

use super::*;

// ------------------------------------------------------------------ 会话状态

/// 忽略**某一个版本**之后：它自己不弹了，候选留着；**更高版本照弹**；手动检查清掉忽略。
///
/// 这条是复审 P3-1 的守门：`skipped` 曾经是会话级 bool，于是「忽略 0.2.3」把同一次运行里
/// 后来发布的 0.2.4 也一起压住了——而文案写的是「忽略这个版本」。
#[test]
fn skipping_a_version_does_not_silence_the_next_one() {
    fn available(version: &str) -> Report {
        Report::Checked(UpdateOutcome::Available(Box::new(
            crate::dto::UpdateCandidateView {
                version: version.to_string(),
                size: 12,
                notes: String::new(),
                forced: false,
            },
        )))
    }

    let tmp = TempDir::new("dismiss");
    let mut runtime = Runtime::with_factory(&HostEnv::at_root(tmp.join("root")), factory(&tmp));
    runtime.adopt(available("0.9.0"));
    assert!(matches!(
        runtime.view().banner(),
        Some(UpdateBanner::Available(_))
    ));

    runtime.dismiss();
    assert_eq!(
        runtime.view().skipped.as_deref(),
        Some("0.9.0"),
        "忽略要记下**那一个**版本号"
    );
    assert!(runtime.view().banner().is_none(), "被忽略的版本不再打扰");
    assert!(
        runtime.view().outcome.candidate().is_some(),
        "候选不能丢：设置页还要能手动装"
    );

    // 静默检查回来的是**同一个**版本：仍然压住（用户刚说过忽略它）。
    let _ = runtime.begin_check(false).expect("静默检查派发");
    runtime.adopt(available("0.9.0"));
    assert!(
        runtime.view().banner().is_none(),
        "同一版本不许因为一次静默检查又弹出来"
    );

    // 静默检查回来的是**更高**版本：照弹（忽略的是 0.9.0，不是「以后别再问」）。
    let _ = runtime.begin_check(false).expect("静默检查派发");
    runtime.adopt(available("0.9.1"));
    assert!(
        matches!(runtime.view().banner(), Some(UpdateBanner::Available(c)) if c.version == "0.9.1"),
        "0.9.0 被忽略不该压住 0.9.1"
    );

    // 手动检查 ⇒ 清掉忽略标记（用户要看真结果），重新查。
    let _ = runtime.begin_check(true).expect("手动检查派发");
    assert!(
        runtime.view().skipped.is_none(),
        "手动检查要清掉「忽略这个版本」"
    );
    assert!(runtime.view().outcome.is_checking());
}

/// 没有候选时点「忽略」是空操作：不该写进一个「你别问」的状态。
#[test]
fn dismissing_without_a_candidate_does_nothing() {
    let tmp = TempDir::new("dismiss-none");
    let mut runtime = Runtime::with_factory(&HostEnv::at_root(tmp.join("root")), factory(&tmp));
    runtime.adopt(Report::Checked(UpdateOutcome::UpToDate {
        latest: Some("0.2.2".to_string()),
    }));
    runtime.dismiss();
    assert!(runtime.view().skipped.is_none());
}

/// 关掉「启动时检查」之后：状态上是 no-op、启动路径上根本不派发。
///
/// 开关的落点是**主配置**（`update.auto_check`），这里顺带把 M161 的迁移走一遍：
/// 旧 `cache/update/prefs.json` 里的 `false` 必须是新会话看到的 `false`（不是默认的 `true`）。
#[test]
fn auto_check_off_means_no_request_on_start() {
    let tmp = TempDir::new("autocheck");
    let env = HostEnv::at_root(tmp.join("root"));
    let prefs_path = env.layout.cache().join("update").join("prefs.json");
    std::fs::create_dir_all(prefs_path.parent().expect("父目录")).expect("建 cache");
    std::fs::write(&prefs_path, r#"{"auto_check": false}"#).expect("写旧偏好");

    let runtime = Runtime::with_factory(&env, factory(&tmp));
    assert!(!runtime.view().auto_check, "开关要读进快照");
    assert!(!runtime.auto_check(), "关掉的值一路迁进主配置");
    assert!(
        !runtime.should_check_on_start(),
        "关掉开关后启动不许派发检查"
    );
    // 迁移确实发生了：旧文件退役 + 收据落盘（收据可查）。
    assert!(!prefs_path.exists(), "旧文件必须退役");
    let receipt = runtime
        .migration()
        .receipt()
        .expect("收据路径")
        .to_path_buf();
    assert!(receipt.exists(), "收据要落盘：{}", receipt.display());

    // 结构性判据：`refresh_all` 里那次静默检查必须挂在 `should_check_on_start` 上，
    // 否则单测绿着而启动仍然联网（这条读源码，理由同 `instances/tests.rs` 的源码断言）。
    let host = include_str!("mod.rs");
    assert!(
        host.contains("if self.update.should_check_on_start()"),
        "refresh_all 的静默检查必须由 should_check_on_start 门控"
    );
}

/// 开关能写进主配置、能读回（换一份 Runtime 仍记得上次的选择）。
#[test]
fn the_auto_check_preference_survives_a_restart() {
    let tmp = TempDir::new("prefs");
    let env = HostEnv::at_root(tmp.join("root"));
    let mut runtime = Runtime::with_factory(&env, factory(&tmp));
    assert!(runtime.auto_check(), "缺省开");

    runtime.set_auto_check(false).expect("落盘");
    let restarted = Runtime::with_factory(&env, factory(&tmp));
    assert!(!restarted.auto_check(), "重启后仍记得关掉");
    assert!(!restarted.view().auto_check);

    // 落点是主配置的 schema 键（CLI 读到的就是这一份），不是更新域自己的文件。
    let text = std::fs::read_to_string(env.launcher_config()).expect("读主配置");
    assert!(text.contains("\"auto_check\": false"), "{text}");
    assert!(
        !env.layout
            .cache()
            .join("update")
            .join("prefs.json")
            .exists(),
        "新写入不许再落到旧文件"
    );
}

/// GUI 与 CLI 共用**同一个** `Job::apply`（mission：不许两套逻辑）。
///
/// 判据是源码锚点：CLI 的 `update apply` 必须调 `Job::apply`，而不是自己拼一遍
/// 「下载 + 校验 + 替换」。
#[test]
fn the_gui_and_the_cli_share_one_kernel_path() {
    let cli = include_str!("../../../cli/src/commands/update.rs");
    assert!(
        cli.contains("Job::") && cli.contains(".apply("),
        "CLI 必须复用 presenter 的 Job::apply（同一内核），实际没有调用"
    );
    // 反向：check 也只许走 Job::check
    assert!(
        cli.contains(".check("),
        "CLI 的 update check 必须走 Job::check"
    );
}

/// 辅助：装配自证用的是哪份源与哪个替换目标（日志/诊断用）。
#[test]
fn the_factory_names_its_source_and_target() {
    let tmp = TempDir::new("source-id");
    let factory = factory(&tmp);
    assert_eq!(factory.dir(), Some(tmp.join("dist").as_path()));
    assert_eq!(factory.dest(), tmp.join("bin/unlucky-launcher.exe"));
}

/// 渠道来自主配置的 `update.channel`（M161：不再是编译期常量）。
///
/// 抓得住：把渠道写回硬编码常量而键里是别的值；也抓得住「认不出的渠道 panic 而不是回出厂值」。
#[test]
fn the_channel_is_read_from_the_schema_key() {
    let tmp = TempDir::new("channel-key");
    let env = HostEnv::at_root(tmp.join("root"));
    let runtime = Runtime::with_factory(&env, factory(&tmp));
    assert_eq!(
        runtime.channel().as_str(),
        unlucky_core::setting::update_prefs::DEFAULT_CHANNEL,
        "缺键 ⇒ 出厂渠道"
    );
    assert_eq!(runtime.view().channel, "stable", "快照里的渠道同源");

    // 手改成一个 schema 不允许的值（越过 `config set` 的校验）⇒ 回出厂渠道，不许 panic。
    std::fs::create_dir_all(env.layout.config()).expect("建 config");
    std::fs::write(
        env.launcher_config(),
        r#"{"$schema":"https://schemas.unlucky-launcher.dev/ull/launcher/1.6.0","schemaVersion":1,
            "update":{"channel":"hand-edited"}}"#,
    )
    .expect("写配置");
    let runtime = Runtime::with_factory(&env, factory(&tmp));
    assert_eq!(runtime.channel().as_str(), "stable", "认不出的渠道回出厂值");
}

/// CLI 与 GUI 用的是**同一个** core Job：源码锚点，防止有人另起一条流程。
#[test]
fn the_core_job_is_the_single_kernel() {
    let job = include_str!("../../../core/src/update/job.rs");
    assert!(
        job.contains("pub fn apply(") && job.contains("pub fn check("),
        "内核必须同时提供 check / apply"
    );
    assert!(
        job.contains("install_staged(") && job.contains("stage("),
        "替换与校验必须在内核里做（壳层不许自己拼）"
    );
}
