//! `fonts::install` 的守卫（M168）：**三条能变红的断言**，每条都写明「抓得住什么」。
//!
//! 这些用例全部脱机：下载器是假的（记调用次数），注册器也是假的（记收到的字节）。真下载、真注册
//! 进 iced 字体系统这两件事**不在**本文件的能力范围内（真注册要 iced 的全局字体系统，见报告）。

use std::cell::RefCell;

use unlucky_i18n::font::{CjkFont, FetchError, Fetcher, Strategy, NOTO_SANS_SC};

use super::*;

/// 假下载器：记调用次数与拿到的字体族名，返回预置结果。
struct FakeFetcher {
    calls: RefCell<usize>,
    seen: RefCell<Option<&'static str>>,
    result: Result<Vec<u8>, FetchError>,
}

impl FakeFetcher {
    fn new(result: Result<Vec<u8>, FetchError>) -> Self {
        Self {
            calls: RefCell::new(0),
            seen: RefCell::new(None),
            result,
        }
    }

    fn calls(&self) -> usize {
        *self.calls.borrow()
    }
}

impl Fetcher for FakeFetcher {
    fn fetch(&self, font: &'static CjkFont) -> Result<Vec<u8>, FetchError> {
        *self.calls.borrow_mut() += 1;
        *self.seen.borrow_mut() = Some(font.family);
        self.result.clone()
    }
}

/// 假注册器收到的字节长度（只关心「注册了几次、每次多大」）。
type Registered = std::rc::Rc<RefCell<Vec<usize>>>;

/// 假注册器：记收到的字节数，返回一个族名。
fn recorder() -> (Registered, impl Fn(Vec<u8>) -> Result<String, String>) {
    let seen = std::rc::Rc::new(RefCell::new(Vec::new()));
    let recorder = std::rc::Rc::clone(&seen);
    let register = move |bytes: Vec<u8>| {
        recorder.borrow_mut().push(bytes.len());
        Ok("Noto Sans SC".to_owned())
    };
    (seen, register)
}

/// 守卫 ①：**英文环境 / 系统已有字体 / 探测不出 ⇒ 一次都不联网**。
///
/// 抓得住：把 `ensure_for` 的判据换成「先试着下一下看看」、或 `after_probe` 里对这三种策略也走
/// `start()`——本用例在 `fetcher.calls() == 0` 处当场红（这是新路径，不是 i18n 那条既有用例）。
/// 抓不住：`after_probe` 起线程之后的真实网络行为（那由 `core::download::font` 的守卫与
/// `CdnFetcher` 的缓存判定承担）。
#[test]
fn the_three_idle_strategies_never_touch_the_fetcher() {
    let (registered, register) = recorder();

    let idle = FakeFetcher::new(Ok(vec![1, 2, 3]));
    assert_eq!(
        ensure_for(Strategy::NotNeeded, &idle, &register),
        CjkInstall::NotNeeded
    );
    assert_eq!(
        ensure_for(Strategy::System { family: "SimSun" }, &idle, &register),
        CjkInstall::System {
            family: "SimSun".to_owned()
        }
    );
    assert_eq!(
        ensure_for(Strategy::Unprobeable, &idle, &register),
        CjkInstall::Unprobeable
    );
    assert_eq!(
        idle.calls(),
        0,
        "英文 / 系统已有 / 探测不出：一次都不许调下载器"
    );
    assert!(registered.borrow().is_empty(), "这三路不许注册任何字体");

    // 反方向：真需要时才调一次（否则上面那三个 0 可能是「永远不调」的假绿）。
    let working = FakeFetcher::new(Ok(vec![9; 4096]));
    assert_eq!(
        ensure_for(Strategy::NeedsDownload(&NOTO_SANS_SC), &working, &register),
        CjkInstall::Ready {
            family: "Noto Sans SC".to_owned(),
            from_cache: false
        }
    );
    assert_eq!(working.calls(), 1);
    assert_eq!(working.seen.borrow().as_deref(), Some("Noto Sans SC"));
    assert_eq!(*registered.borrow(), vec![4096]);
}

/// 守卫 ②：**校验失败 ⇒ 状态 `Failed(integrity)`，且一个字节都不注册**。
///
/// 抓得住：把 `ensure_for` 的错误分支写成「失败也注册回落的字节」、或把类别压成 `network`
/// （设置页就会说「网络不通」，而真相是镜像给了不对的东西）。
/// 抓不住：sha256 到底比得对不对（那是 `core::download::font` 的守卫）。
#[test]
fn an_integrity_failure_registers_nothing_and_keeps_the_class() {
    let (registered, register) = recorder();
    let broken = FakeFetcher::new(Err(FetchError::Integrity("sha256 不符".to_owned())));

    let state = ensure_for(Strategy::NeedsDownload(&NOTO_SANS_SC), &broken, &register);
    assert_eq!(
        state,
        CjkInstall::Failed {
            class: FetchClass::Integrity,
            message: "sha256 不符".to_owned()
        }
    );
    assert!(registered.borrow().is_empty(), "校验失败不许注册");
    assert!(!state.is_downloading(), "失败是终态：不再转圈、不阻塞界面");
}

/// 三类失败各归各的类别（设置页按类别选词，串了就等于对用户说错话）。
#[test]
fn every_failure_class_survives_the_trip() {
    let (_, register) = recorder();
    for (error, class) in [
        (FetchError::Network("超时".to_owned()), FetchClass::Network),
        (
            FetchError::Integrity("哈希不符".to_owned()),
            FetchClass::Integrity,
        ),
        (FetchError::Disk("磁盘满".to_owned()), FetchClass::Disk),
    ] {
        let fetcher = FakeFetcher::new(Err(error));
        let state = ensure_for(Strategy::NeedsDownload(&NOTO_SANS_SC), &fetcher, &register);
        match state {
            CjkInstall::Failed { class: got, .. } => assert_eq!(got, class),
            other => panic!("应当是 Failed：{other:?}"),
        }
    }

    // 注册本身失败（字体系统锁毒掉 / 字体库不认）也要落到失败的磁盘档，而不是 Ok。
    let fetcher = FakeFetcher::new(Ok(vec![7; 8]));
    let state = ensure_for(
        Strategy::NeedsDownload(&NOTO_SANS_SC),
        &fetcher,
        &|_bytes| Err("字体系统锁被毒掉".to_owned()),
    );
    assert!(matches!(
        state,
        CjkInstall::Failed {
            class: FetchClass::Disk,
            ..
        }
    ));
}

/// 幂等闸（M168 复审 P2-4：这条以前是**假守卫**，现在打真判据）。
///
/// 复审的原话：上一版的循环体只断言「一个从没被传进任何函数的假下载器没被调用」，把
/// `after_probe` 里的 `is_downloading() || Ready` 整段删掉它**依然全绿**。
///
/// 现在直接打 [`plan`]：删掉 `Skip` 那一臂（或把它改成 `Start`），本用例在
/// `assert_eq!(plan(...), Step::Skip)` 处当场红。为什么这条闸不是可有可无：去掉之后每切一次
/// 语言就 spawn 一条线程，线程会命中缓存（不联网）但会**再注册一遍 17 MiB 字体**
/// （`load_font(Cow::Owned)` 不做去重），内存随切换次数累积。
///
/// 抓得住：幂等判据被删/被改反（下载中、已就绪两种状态各自一行）。
/// 抓不住：`after_probe` 真的 spawn 几条线程（那要真线程 + 真网络，见报告「未验」）。
#[test]
fn a_started_or_finished_install_is_not_restarted() {
    let downloading = CjkInstall::Downloading { bytes: 17_772_300 };
    let ready = CjkInstall::Ready {
        family: "Noto Sans SC".to_owned(),
        from_cache: true,
    };

    for current in [downloading, ready] {
        assert_eq!(
            plan_with_offline(Strategy::NeedsDownload(&NOTO_SANS_SC), &current, false),
            Step::Skip,
            "{current:?}：已经在下载 / 已就绪时再问一次，必须什么都不做"
        );
    }

    // 反方向（否则上面两条可能是「永远 Skip」的假绿）：其余三种当前状态都要真的发起。
    for current in [
        CjkInstall::Unknown,
        CjkInstall::NotNeeded,
        CjkInstall::Failed {
            class: FetchClass::Network,
            message: "超时".to_owned(),
        },
    ] {
        assert_eq!(
            plan_with_offline(Strategy::NeedsDownload(&NOTO_SANS_SC), &current, false),
            Step::Start(&NOTO_SANS_SC),
            "{current:?}：需要下载且还没成就该发起（失败后的重试也只能走这条）"
        );
    }
}

/// 三种「不需要下载」的策略在判据表里只产生 `Apply`（**零网络**），且状态文案对得上。
///
/// 这里显式给 `offline = false`（`plan_with_offline`）而不是用 `plan`：`plan` 会读进程级的
/// `set_offline` 开关，而同进程里 `headless(screenshot_options())` 那 20+ 处会把它置 true 且
/// 从不还原（复审 r3 P2-4 ①）——用 `plan` 的话这三条的结果取决于执行顺序。
///
/// 抓得住：谁把 `NotNeeded` / `System` / `Unprobeable` 的臂改成 `Start`（那就会替英文用户偷偷
/// 下一次 17 MiB）；也抓得住「系统已有字体」被记成「需要下载」这种状态映射错。
/// 抓不住：真线程行为（`after_probe` 的执行部分）。
#[test]
fn the_idle_strategies_only_apply_state() {
    assert_eq!(
        plan_with_offline(Strategy::NotNeeded, &CjkInstall::Unknown, false),
        Step::Apply(CjkInstall::NotNeeded)
    );
    assert_eq!(
        plan_with_offline(
            Strategy::System { family: "SimSun" },
            &CjkInstall::Unknown,
            false
        ),
        Step::Apply(CjkInstall::System {
            family: "SimSun".to_owned()
        })
    );
    // 复审 r2 P2-2 ② 改判：**下载中一律 Skip**，即使策略说「探测不出」。
    // 上一版在这里断言 `Apply(Unprobeable)`，理由是「否则界面会停在正在下载」——那个理由只对
    // 僵死成立（线程 panic），而在飞是常见情形：线程还在下，状态却被改成「无法探测」，下完又
    // 翻回「已就绪」，状态行在几十秒里来回翻。「僵死」那一半按构建档分两说（worker 模块文档）：
    // unwind 档（dev/test/fast）由 `catch_unwind` 兜底；发布档 `panic = "abort"` 是进程退出，
    // 不存在停在 Downloading 的状态。
    assert_eq!(
        plan_with_offline(
            Strategy::Unprobeable,
            &CjkInstall::Downloading { bytes: 1 },
            false
        ),
        Step::Skip,
        "在飞的下载不许被别的策略改写状态"
    );
}

/// **下载中一律 `Skip`**（复审 r2 P2-2 ②）：四种策略挨个打一遍。
///
/// 抓得住：把 `is_downloading()` 那道闸挪回 `NeedsDownload` 分支里（上一版就是这样）——
/// `Unprobeable` / `NotNeeded` / `System` 三行立刻红。
/// 抓不住：worker 真的永不僵死（那要制造 panic，见 `worker` 的 `catch_unwind` 与报告的「没验」）。
#[test]
fn every_strategy_skips_while_a_download_is_in_flight() {
    let downloading = CjkInstall::Downloading { bytes: 17_772_300 };
    for strategy in [
        Strategy::NeedsDownload(&NOTO_SANS_SC),
        Strategy::NotNeeded,
        Strategy::System { family: "SimSun" },
        Strategy::Unprobeable,
    ] {
        assert_eq!(
            plan_with_offline(strategy, &downloading, false),
            Step::Skip,
            "{strategy:?}：下载中不许改状态、不许起第二条线程"
        );
    }
}

/// 出图通道（`offline`）**永不发起下载**（D30：同一份代码出同一张图）。
///
/// 抓得住：删掉 `plan_with_offline` 里 `NeedsDownload(_) if offline => Skip` 那一臂 ⇒ 截图/演示
/// 通道会在 zh-locale 且无中文字体的机器上真下 17 MiB，用例在 `Step::Skip` 处红。
/// 抓不住：真出图时有没有产生流量（那要真机跑 `--screenshot`，见报告「没验」）。
#[test]
fn the_screenshot_channel_never_starts_a_download() {
    assert_eq!(
        plan_with_offline(
            Strategy::NeedsDownload(&NOTO_SANS_SC),
            &CjkInstall::Unknown,
            true
        ),
        Step::Skip,
        "出图通道不许联网"
    );
    assert_eq!(
        plan_with_offline(
            Strategy::NeedsDownload(&NOTO_SANS_SC),
            &CjkInstall::Unknown,
            false
        ),
        Step::Start(&NOTO_SANS_SC),
        "正常通道该发起（否则上面那条可能是「永远 Skip」的假绿）"
    );
    // 只改状态的三种策略在两条通道里表现一致（它们本来就不联网）。
    for strategy in [
        Strategy::NotNeeded,
        Strategy::System { family: "SimSun" },
        Strategy::Unprobeable,
    ] {
        assert_eq!(
            plan_with_offline(strategy, &CjkInstall::Unknown, false),
            plan_with_offline(strategy, &CjkInstall::Unknown, true),
            "{strategy:?}：不联网的策略与通道无关"
        );
    }
}

/// 截图钉子（D30）：每个 `ULL_FONT_STATUS` 取值都能解析成一档状态，坏值不猜。
///
/// 抓得住：出图夹具写了一个不存在的取值却静默出「未知」状态的图（报告就会撒谎）。
#[test]
fn the_capture_switch_parses_every_state() {
    assert_eq!(parse_capture("not-needed"), Some(CjkInstall::NotNeeded));
    assert_eq!(parse_capture("unprobeable"), Some(CjkInstall::Unprobeable));
    assert!(matches!(
        parse_capture("system"),
        Some(CjkInstall::System { .. })
    ));
    assert!(matches!(
        parse_capture("downloading"),
        Some(CjkInstall::Downloading { .. })
    ));
    assert!(matches!(
        parse_capture("ready"),
        Some(CjkInstall::Ready {
            from_cache: true,
            ..
        })
    ));
    for (raw, class) in [
        ("failed", FetchClass::Network),
        ("failed:network", FetchClass::Network),
        ("failed:integrity", FetchClass::Integrity),
        ("failed:disk", FetchClass::Disk),
    ] {
        match parse_capture(raw) {
            Some(CjkInstall::Failed { class: got, .. }) => assert_eq!(got, class, "{raw}"),
            other => panic!("{raw} 应解析成 Failed：{other:?}"),
        }
    }
    assert_eq!(parse_capture("bogus"), None);
    assert_eq!(parse_capture(""), None);
}

/// 一行日志片段：每种状态都能被 grep 到，且**不带换行**（启动日志按行解析）。
#[test]
fn every_state_describes_itself_on_one_line() {
    let states = [
        CjkInstall::Unknown,
        CjkInstall::NotNeeded,
        CjkInstall::System {
            family: "SimSun".to_owned(),
        },
        CjkInstall::Unprobeable,
        CjkInstall::Downloading { bytes: 17_772_300 },
        CjkInstall::Ready {
            family: "Noto Sans SC".to_owned(),
            from_cache: false,
        },
        CjkInstall::Failed {
            class: FetchClass::Integrity,
            message: "sha256 不符".to_owned(),
        },
    ];
    for state in states {
        let line = state.describe();
        assert_eq!(line.lines().count(), 1, "{line}");
        assert!(line.starts_with("cjk-install="), "{line}");
    }
}
