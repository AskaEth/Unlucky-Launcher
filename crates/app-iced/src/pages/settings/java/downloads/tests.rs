//! 「下载一个 JDK」的作业与状态（M197）：纯事件 → 快照的映射守卫。
//!
//! 从 `downloads.rs` 拆出来守 400 行硬线（与 `tests/runtime.rs` 同一种拆法：断言跟着它守的
//! 东西走，模块文件只留实现）。这些用例**不联网**：喂的是引擎会吐的那几种 `ProgressEvent`。
use super::{Event, Failure, Job, Phase, Report, Slot};
use unlucky_core::download::types::DownloadId;
use unlucky_core::download::{DownloadErrorKind, ProgressEvent};
use unlucky_core::game::java::JavaErrorKind;

/// 一条下载事件链 → 界面能看到「百分比 + 命中哪个源」。
///
/// 变异证明：把 `apply` 里 `ProgressEvent::Progress` 那一臂的 `job.downloaded = downloaded`
/// 删掉 ⇒ 本用例红（百分比恒为 0）。
#[test]
fn progress_events_fill_the_percent_and_the_source() {
    let slot = Slot::default();
    slot.seed_report(Report {
        job: Some(Job::new(21)),
        ..Report::default()
    });
    let id = DownloadId::next();
    slot.apply(ProgressEvent::Started {
        id,
        kind: unlucky_core::download::ResourceKind::JavaRuntime,
        url: "https://example.invalid/jdk.zip".to_string(),
        dest: "jdk.zip".to_string(),
        candidates: 2,
        total_bytes: Some(200),
    });
    slot.apply(ProgressEvent::Progress {
        id,
        downloaded: 50,
        total_bytes: Some(200),
    });
    let job = slot.report().job.expect("作业还在");
    assert_eq!(job.phase, Phase::Downloading);
    assert_eq!(job.percent(), Some(25), "50/200 = 25%");
    slot.apply(ProgressEvent::Completed {
        id,
        bytes: 200,
        from_cache: false,
        source: "tuna".to_string(),
        elapsed_ms: 10,
    });
    let job = slot.report().job.expect("作业还在");
    assert_eq!(job.phase, Phase::Extracting, "下完就该进解压阶段");
    assert_eq!(job.source.as_deref(), Some("tuna"), "命中哪个源要说得出");
}

/// 分母未知时**不做假进度**（`percent()` 给 `None`）。
#[test]
fn an_unknown_total_shows_no_percent() {
    let slot = Slot::default();
    slot.seed_report(Report {
        job: Some(Job::new(17)),
        ..Report::default()
    });
    slot.apply(ProgressEvent::Progress {
        id: DownloadId::next(),
        downloaded: 1024,
        total_bytes: None,
    });
    assert_eq!(slot.report().job.expect("作业").percent(), None);
}

/// 退避与换源要留在快照里（界面那一行「A 失败；2 秒后重试第 2 次」/「换源」）。
///
/// 变异证明：把 `SourceFailed` 那一臂的 `job.last = Some(Event::Retry { .. })` 删掉
/// ⇒ 本用例红。
#[test]
fn retries_and_switches_are_kept_for_the_ui() {
    let slot = Slot::default();
    slot.seed_report(Report {
        job: Some(Job::new(8)),
        ..Report::default()
    });
    let id = DownloadId::next();
    slot.apply(ProgressEvent::SourceFailed {
        id,
        source: "tuna".to_string(),
        kind: DownloadErrorKind::ServerError,
        reason: "服务器 503".to_string(),
        attempt: 1,
        will_retry: true,
        retry_in_ms: 2_000,
    });
    assert_eq!(
        slot.report().job.expect("作业").last,
        Some(Event::Retry {
            source: "tuna".to_string(),
            kind: DownloadErrorKind::ServerError,
            attempt: 2,
            retry_in_ms: 2_000,
        })
    );
    slot.apply(ProgressEvent::SourceSwitched {
        id,
        from: "tuna".to_string(),
        to: "ustc".to_string(),
    });
    let job = slot.report().job.expect("作业");
    assert_eq!(job.source.as_deref(), Some("ustc"));
    assert_eq!(
        job.last,
        Some(Event::Switched {
            from: "tuna".to_string(),
            to: "ustc".to_string(),
        })
    );
}

/// 「该推荐哪一档」：`java.auto_install` 的真实消费者（界面上那行「为当前实例推荐这一档」）。
///
/// 变异证明：把 `recommend` 开头 `if !auto_install { return None; }` 删掉 ⇒ 关掉那一档红；
/// 把 `satisfied` 判据写成恒 `false` ⇒ 「已满足就不推荐」那一档红。
#[test]
fn auto_install_decides_whether_a_major_is_recommended() {
    use super::recommend;
    use crate::pages::settings::java::context::Requirement;
    use crate::pages::settings::java::row::RuntimeRow;

    fn row(major: u32) -> RuntimeRow {
        RuntimeRow {
            binary: format!("C:/jdk-{major}/bin/java.exe"),
            version: format!("openjdk version \"{major}.0.1\""),
            major: Some(major),
            vendor: Some("Temurin".to_string()),
            source: "system",
        }
    }

    let required = Requirement {
        instance: "生存服".to_string(),
        version: "1.20.5".to_string(),
        minimum: 21,
        maximum: None,
    };
    let none: &[RuntimeRow] = &[];
    assert_eq!(recommend(Some(&required), none, true), Some(21));
    assert_eq!(recommend(Some(&required), none, false), None);
    let satisfied = [row(21)];
    assert_eq!(recommend(Some(&required), &satisfied, true), None);
    let too_old = [row(17)];
    assert_eq!(
        recommend(Some(&required), &too_old, true),
        Some(21),
        "手里只有 17、实例要 21 时照样推荐 21"
    );
    assert_eq!(
        recommend(None, none, true),
        Some(17),
        "没有实例时推荐基准 17"
    );
    assert_eq!(recommend(None, &satisfied, true), None);
}

/// 真下载一次（`--ignored`，需要网络）：装到临时目录并确认 `bin/java` 存在。
///
/// 这是「真入口」的端到端证据：`Slot::start` → `engine_with`（镜像表 / 地区 / 健康度）→
/// `java::install`（查版本 → 回退链 → 下载 → 解压 → 探测）。默认不跑：它会下 ~180 MB。
#[test]
#[ignore = "真下载（网络 + 数百 MB），用 cargo test -- --ignored 手动跑"]
fn the_real_download_installs_a_usable_jdk() {
    use super::{Request, Slot};
    use unlucky_core::game::java::JavaInstallRoot;

    let dir = std::env::temp_dir().join(format!("ull-java-install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("cache")).expect("temp");
    let slot = Slot::default();
    assert!(slot.start(Request {
        major: 21,
        cache_root: dir.join("cache"),
        install_root: JavaInstallRoot::custom(dir.join("jdks")),
        priority: Vec::new(),
        gh_proxy: super::GhProxy::Builtin,
    }));
    // 失败/panic 也要清理：这条用例会真下 ~180 MB，留在系统 temp 里是最贵的垃圾。
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(900);
    while slot.busy() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert!(!slot.busy(), "作业应在 15 分钟内结束");
    let report = slot.report();
    if let Some(failure) = &report.failure {
        panic!("安装失败：{failure:?}");
    }
    let success = report.success.expect("应成功");
    assert!(
        std::path::Path::new(&success.binary).is_file(),
        "bin/java 应存在：{}",
        success.binary
    );
}

/// 失败要留档：分类 + 大版本 + 最后试到的源（界面靠这三样说清「为什么没装成」）。
///
/// 变异证明：把 `finish` 的 `Err` 分支里 `report.failure = Some(...)` 删掉 ⇒ 本用例红。
#[test]
fn a_failure_is_kept_with_its_kind_and_source() {
    let slot = Slot::default();
    slot.seed_report(Report {
        job: Some(Job {
            source: Some("tuna".to_string()),
            ..Job::new(21)
        }),
        ..Report::default()
    });
    slot.finish(
        Err(unlucky_core::game::java::JavaError::new(
            JavaErrorKind::Metadata,
            "没有装成".to_string(),
        )),
        21,
    );
    let report = slot.report();
    assert!(report.success.is_none());
    assert!(report.job.is_none(), "失败之后不该还挂着作业");
    let failure: Failure = report.failure.expect("失败要留档");
    assert_eq!(failure.kind, JavaErrorKind::Metadata);
    assert_eq!(failure.major, 21);
    assert_eq!(failure.source.as_deref(), Some("tuna"));
}

/// **占位符不许漏**：`Catalog::render` 缺参数时会把 `{name}` 留下 `(name)`，而那张图已经能看见
/// （M197 出图时实测到 `试过 (tried) 家发行版`——模板里有一个调用点没给的占位符）。
///
/// 这条对**本页自己渲染的每一条下载文案**做同一件事：拿真实参数渲一遍，断言输出里没有
/// 「`(` + 标识符 + `)`」这种残留。ASCII 括号是判据：本页这些模板里没有合法的 ASCII 括号短语
/// （中文文案用全角「（）」），所以命中即漏参。
///
/// 变异证明：把 `SettingsJavaDownloadFailed` 的模板改回「`(tried {n} vendors…)`」那种带
/// `{tried}` 的写法 ⇒ 本用例红（渲出 `(tried)`）。M197 就是这么发现它的。
#[test]
fn every_download_template_renders_without_leftover_placeholders() {
    use unlucky_i18n::{Key, Lang};

    let catalog = Lang::En.catalog();
    /// 一条用例：标签 / key / 渲染参数。
    type Case<'a> = (&'a str, Key, &'a [(&'a str, &'a str)]);

    let cases: [Case<'_>; 6] = [
        (
            "failed",
            Key::SettingsJavaDownloadFailed,
            &[("major", "21"), ("source", "tuna")],
        ),
        (
            "failed-no-source",
            Key::SettingsJavaDownloadFailedNoSource,
            &[("major", "21")],
        ),
        (
            "done",
            Key::SettingsJavaDownloadDone,
            &[
                ("major", "21"),
                ("vendor", "Eclipse Temurin"),
                ("root", "D:/jdks/21"),
                ("source", "tuna"),
            ],
        ),
        (
            "done-reused",
            Key::SettingsJavaDownloadDoneReused,
            &[
                ("major", "21"),
                ("vendor", "Eclipse Temurin"),
                ("root", "D:/jdks/21"),
            ],
        ),
        ("busy", Key::SettingsJavaDownloadBusy, &[("major", "21")]),
        (
            "retry",
            Key::SettingsJavaDownloadRetry,
            &[
                ("source", "tuna"),
                ("reason", "server error"),
                ("attempt", "2"),
                ("seconds", "2.0"),
            ],
        ),
    ];
    for (tag, key, args) in cases {
        let text = catalog.render(key, args);
        let mut rest = text.as_str();
        while let Some(open) = rest.find('(') {
            let after = &rest[open + 1..];
            let close = after.find(')');
            if let Some(close) = close {
                let inner = &after[..close];
                assert!(
                    inner.is_empty()
                        || !inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                    "{tag} 渲染后还留着占位符 `({inner})`：{text}"
                );
            }
            rest = after;
        }
    }
}

/// `download.gh_proxy` 的两条守卫（M197）：取值映射 + 真消费者。拆到子模块守 400 行硬线。
#[path = "tests/gh_proxy.rs"]
mod gh_proxy;
