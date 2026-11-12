//! `fonts` 的单元测试（拆出来只为「src 文件 ≤400 行」的门禁）。

use super::cjk::CjkReport;
use super::*;
use unlucky_design::typography::WeightCoverage;
use unlucky_i18n::font::Strategy;
use unlucky_i18n::lang::Detection;

fn only(installed: &'static [&'static str]) -> impl Fn(&str) -> bool {
    move |family| installed.contains(&family)
}

#[test]
fn windows_stack_falls_back_to_classic_segoe_when_variable_is_absent() {
    // Win10 没有 Segoe UI Variable Text
    let family = pick(
        Platform::Windows,
        FontFamily::Text,
        &only(&["Segoe UI", "Microsoft YaHei UI"]),
    );
    assert_eq!(family, "Segoe UI");
}

#[test]
fn windows_stack_prefers_the_variable_face_on_win11() {
    let family = pick(
        Platform::Windows,
        FontFamily::Text,
        &only(&["Segoe UI Variable Text", "Segoe UI"]),
    );
    assert_eq!(family, "Segoe UI Variable Text");
}

#[test]
fn generic_family_names_are_never_reported_as_the_resolved_family() {
    // 一个都没装：返回栈头，且栈头不是通用族名
    let family = pick(Platform::Linux, FontFamily::Text, &only(&[]));
    assert_eq!(family, "Roboto");
    assert!(!is_generic(family));
}

#[test]
fn missing_cjk_fonts_do_not_change_the_latin_pick() {
    // 装了苹方但没装 SF：macOS 栈应该选 Helvetica Neue 而不是中文族
    let family = pick(
        Platform::MacOs,
        FontFamily::Text,
        &only(&["PingFang SC", "Helvetica Neue"]),
    );
    assert_eq!(family, "Helvetica Neue");
}

#[test]
fn weight_reports_use_the_token_degradation_path() {
    // 没有系统字面表时回落到令牌纯函数：Windows 栈里 Segoe UI Variable Text 是可变 300..700
    let medium = weight_report(
        Platform::Windows,
        "Segoe UI Variable Text",
        Weight::Medium,
        None,
    );
    assert_eq!(medium.requested, 500);
    assert_eq!(medium.quality, WeightMatch::Native);

    // 手工构造一个「只有 400/700」的族（= 经典 Segoe UI），500 应该落到最近档
    let faces = [
        Face {
            family: "Segoe UI".to_owned(),
            weight: 400,
        },
        Face {
            family: "Segoe UI".to_owned(),
            weight: 700,
        },
    ];
    let medium = weight_report(Platform::Windows, "Segoe UI", Weight::Medium, Some(&faces));
    assert_eq!(medium.available, 400, "400 比 700 更近");
    assert_eq!(medium.quality, WeightMatch::Nearest);

    // 只有 400 却要 700 → 合成加粗
    let only_regular = [Face {
        family: "Narrow".to_owned(),
        weight: 400,
    }];
    let bold = weight_report(
        Platform::Windows,
        "Narrow",
        Weight::Bold,
        Some(&only_regular),
    );
    assert_eq!(bold.quality, WeightMatch::Synthetic);

    // 原生有 500 就用 500
    let has_medium = [Face {
        family: "Noto".to_owned(),
        weight: 500,
    }];
    assert_eq!(
        weight_report(Platform::Linux, "Noto", Weight::Medium, Some(&has_medium)).quality,
        WeightMatch::Native
    );

    let narrow = WeightCoverage::Static(&[400]);
    assert_eq!(narrow.nearest(700), 400);
}

#[test]
fn describe_is_one_line_and_mentions_all_three_stacks() {
    let resolved = Resolved {
        platform: Platform::Windows,
        families: [
            FamilyReport {
                stack: "text",
                family: "Segoe UI",
                installed: true,
            },
            FamilyReport {
                stack: "display",
                family: "Segoe UI Variable Display",
                installed: false,
            },
            FamilyReport {
                stack: "mono",
                family: "Cascadia Mono",
                installed: true,
            },
        ],
        weights: [
            WeightReport {
                requested: 400,
                available: 400,
                quality: WeightMatch::Native,
            },
            WeightReport {
                requested: 500,
                available: 400,
                quality: WeightMatch::Nearest,
            },
            WeightReport {
                requested: 700,
                available: 400,
                quality: WeightMatch::Synthetic,
            },
        ],
        cjk: CjkReport {
            detection: Detection {
                lang: unlucky_i18n::Lang::En,
                raw: None,
                origin: unlucky_i18n::lang::ORIGIN_DEFAULT,
            },
            strategy: Strategy::NotNeeded,
        },
    };

    let line = resolved.describe();
    assert_eq!(line.lines().count(), 1);
    assert!(line.contains("text=\"Segoe UI\""), "{line}");
    assert!(line.contains("未装机"), "{line}");
    assert!(line.contains("w700=400(synthetic)"), "{line}");
    assert!(line.contains("cjk=not-needed"), "英文环境要进日志：{line}");
    assert!(resolved.has_synthetic());
    assert_eq!(resolved.families().text, "Segoe UI");
}

#[test]
fn type_roles_pick_the_display_stack_for_big_text() {
    use unlucky_design::typography::TypeRole;

    assert_eq!(
        TypeRole::TitleLarge.style().family,
        FontFamily::Display,
        "22px 标题走 Display 栈"
    );
    assert_eq!(TypeRole::BodyMedium.style().family, FontFamily::Text);
}

/// **求值字体不许发起下载**（复审 r3 P2-2：上一轮的修复没有任何用例守着）。
///
/// 用 `resolve_with_faces` 而不是 `resolve_with`：`system_faces()` 不可注入，拿不到「一台没有
/// 中文字体的机器」。这里给一张只有拉丁族的脸表 ⇒ 中文环境判出 `NeedsDownload`。
///
/// ## 为什么这条现在要在**自己的进程**里量（M262，ubuntu CI 真红）
///
/// `install::status()` 读的是**进程级**的静态槽：`install.rs:146` 的
/// `static STATE: OnceLock<Mutex<CjkInstall>>`。而同一个 test binary（`unlucky-app-iced --lib`）
/// 里还有别的用例会**写**它，路径如下：
///
/// ```text
/// app/update/page_messages.rs:206  the_language_message_switches_the_catalog_and_rebuilds_the_view
///   → App::update(Message::Language(Fixed(ZhCn)))
///   → app/update/page_messages.rs:146 set_language → ui/lang.rs:135 log_cjk
///   → fonts.rs:266 cjk_for(Some("zh_cn")) → install.rs:311 after_probe
///   → install.rs:317-318 Step::Start → install/worker.rs:74 update(Downloading{bytes})
/// ```
///
/// 触发条件：那个用例用 `Options::default()`（`screenshot: None` ⇒ `construct.rs:106`
/// `set_offline(false)`）构造 App，而机器上**没有**中文字体时 `zh_cn` 判出 `NeedsDownload`。
/// ubuntu runner 正是这一档（Windows 开发机装了中文字体，策略停在 `System`，所以本地看不出来）。
/// 之后那条后台线程会真去拉 16.9 MiB（`install/worker.rs:88`），槽在下载期间一直是
/// `Downloading`。它和本用例**并行**跑（默认多线程 harness），于是本用例读到的 `after`
/// 完全可能是**别人**在飞的那条下载。
///
/// 上一版直接比 `install::status()` 的前后快照 = 拿一把全进程共享的尺子量，谁并行谁赢。
/// 修法不是放宽断言，而是**把尺子放进只有本用例的进程**：`current_exe()` + `--exact`
/// 重新起一次本 test binary、只跑这一条（`ULL_FONT_PURITY_CHILD=1` 防递归）。子进程里槽的初值
/// 就是 `Unknown`，没有谁跟它抢，判据因此是确定性的。
///
/// 抓得住：
/// - 把 `install::after_probe(cjk_report.strategy)` 加回求值路径（r2 判为 bug 的写法）——
///   子进程里槽从 `Unknown` 变成 `Downloading { bytes: 17772300 }`，`!after.is_downloading()`
///   当场红（实测，见报告「变异证明」）；
/// - 更宽的回归：求值路径**任何**写槽的动作（连 `Unknown → NotNeeded` 这种「无害改写」也算），
///   由「前后必须逐值相等」那条守；
/// - 判据本身失效（这张脸表不再判出 `NeedsDownload`）—— 单独一条断言，免得上面两条变成假绿。
///
/// 抓不住：
/// - 别的用例会不会写槽（本用例主动换进程，**不再**替全进程背书）；
/// - `App` 构造之后 `log_cjk` 那一拍会不会触发（那是 `cjk_for` 的职责，且它本来就该触发）；
/// - 起进程本身失败（`current_exe` 拿不到、禁 fork 的环境）——那时是**红**，不是软过；
/// - 子进程里跑了「几条」这件事只由 stdout 的 `1 passed` 兜着（改名会让它红，而不是变 0 条假绿）。
#[test]
fn resolving_fonts_never_starts_a_download() {
    if std::env::var_os(CHILD_ENV).is_some() {
        evaluation_starts_no_download();
        return;
    }
    rerun_in_own_process();
}

/// 子进程标记（防递归：父进程起子进程，子进程直接跑判据）。
const CHILD_ENV: &str = "ULL_FONT_PURITY_CHILD";

/// 判据本体（只在**没有别的用例抢全局槽**的进程里跑）。
fn evaluation_starts_no_download() {
    use unlucky_i18n::font::Strategy;

    use crate::fonts::install;

    // 只有拉丁族 ⇒ 中文环境一定判 needs-download，于是这条用例对「触发」是敏感的。
    let latin: Vec<Face> = ["Segoe UI", "Consolas"]
        .iter()
        .map(|family| Face {
            family: (*family).to_owned(),
            weight: 400,
        })
        .collect();

    let before = install::status();
    let resolved = resolve_with_faces(Some("zh_cn"), Some(&latin));
    let after = install::status();

    assert_eq!(
        before,
        install::CjkInstall::Unknown,
        "隔离失效：这个进程里槽的初值已被谁改过（{before:?}），下面的前后比较就没有意义"
    );
    assert_eq!(
        resolved.cjk.strategy,
        Strategy::NeedsDownload(&unlucky_i18n::font::NOTO_SANS_SC),
        "这张脸表必须判出「需要下载」，否则后面的断言证明不了什么"
    );
    assert!(
        !after.is_downloading(),
        "求值字体**不许**发起下载（状态从 {before:?} 变成了 {after:?}）"
    );
    assert_eq!(
        after, before,
        "求值字体一次都不许写状态槽（比「不再是下载中」更强：任何改写都算越界）"
    );
}

/// 用 `current_exe()` 把本 test binary 再起一次，只跑 [`resolving_fonts_never_starts_a_download`]。
///
/// 子进程经 `unlucky_core::platform::child::quiet` 起（M272）：仓规是全树硬约束，测试代码不是
/// 例外——这条 test binary 本身是控制台程序，父进程将来若换到 GUI 侧（或 CI 用别的方式直接跑
/// 二进制）也照样不该新建窗口。`quiet` 只动创建 flag，`.output()` 的管道语义一个字不变。
fn rerun_in_own_process() {
    let exe = std::env::current_exe().expect("取本 test binary 的路径");
    let name = "fonts::tests::resolving_fonts_never_starts_a_download";
    let output = unlucky_core::platform::child::quiet(&exe)
        .args(["--exact", "--nocapture", "--test-threads=1", name])
        .env(CHILD_ENV, "1")
        // 截图通道的钉子会直接替换 `status()` 的返回值（`install.rs:157` 先看它），
        // 留着就等于把尺子换成别人的常数。
        .env_remove(crate::fonts::install::CAPTURE_ENV)
        .output()
        .unwrap_or_else(|error| panic!("起不了隔离子进程 {}：{error}", exe.display()));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "隔离子进程里的判据红了（退出码 {:?}）\n--- stdout ---\n{stdout}\n--- stderr ---\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("1 passed"),
        "子进程里必须**真**跑了一条用例（`--exact` 名字对不上会 0 passed 退出 0，那是假绿）：\n{stdout}"
    );
}
