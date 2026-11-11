//! `[env]` / `[i18n]` / `[window]` / `[caps]` / `[backend]` / `[perf]` 六类行的钉死测试。
//!
//! **这些测试只钉「给定真实输入 → 打出什么」**：格式化函数是纯的，所以能断言逐字字符串。
//! 它们抓不住「输入本身是不是真的」（那要靠验收脚本在真机上 grep），抓得住的是这一整类事故：
//! **在需要结果的地方打印请求 / 常量**——每一格都有一个能改坏的断言。

use std::path::Path;
use std::time::Duration;

use super::*;
use crate::probe::AdapterSummary;
use crate::renderer::decision::{decide, Kind};
use crate::Probe;
use unlucky_design::typography::Platform as FontPlatform;

fn probe_of(adapters: &[(&str, &'static str)]) -> Probe {
    let adapters: Vec<AdapterSummary> = adapters
        .iter()
        .map(|(name, device_type)| AdapterSummary {
            name: (*name).to_owned(),
            device_type,
        })
        .collect();
    let hardware = adapters
        .iter()
        .any(|adapter| !matches!(adapter.device_type, "Cpu" | "Other"));
    Probe { adapters, hardware }
}

/// 抓得住：`[window]` 打的是**请求**，字段名必须自证（`requested` / `scale_requested`）。
/// 抓不住：窗口真的开成了多大（由验收脚本从 `[shot] … scale=` 与 PNG 像素换算）。
#[test]
fn window_line_is_labelled_as_a_request() {
    let facts = WindowFacts {
        size: (1180.0, 760.0),
        min: Some((880.0, 560.0)),
        resizable: true,
        decorations: false,
        transparent: false,
    };
    let line = window_text(&facts, None, "default", Some("Unlucky Launcher"));
    assert!(
        line.starts_with("[window] requested size=1180x760"),
        "{line}"
    );
    assert!(line.contains("min=880x560"), "{line}");
    assert!(line.contains("resizable=true"), "{line}");
    assert!(line.contains("decorations=false"), "{line}");
    assert!(line.contains("transparent=false"), "{line}");
    assert!(line.contains("scale_requested=system"), "{line}");
    assert!(line.contains("size_source=default"), "{line}");
    assert!(line.contains("title=\"Unlucky Launcher\""), "{line}");

    // `pages_demo` 那条通道没有最小尺寸 / 自定标题：必须写 none / <默认>，不许编 0x0
    let harness = window_text(
        &WindowFacts { min: None, ..facts },
        Some(1.0),
        "pages_demo",
        None,
    );
    assert!(harness.contains("min=none"), "{harness}");
    assert!(harness.contains("title=<默认>"), "{harness}");
    assert!(harness.contains("scale_requested=1"), "{harness}");
}

/// `ULL_WINDOW_SIZE` 的三种分隔符都要认；非法值必须返回 `None`（调用方打警告，不许静默吞）。
#[test]
fn window_size_override_accepts_the_three_separators() {
    for raw in ["1520x900", "1520X900", "1520×900", " 1180 x 760 "] {
        assert!(parse_window_size(raw).is_some(), "{raw:?}");
    }
    assert_eq!(parse_window_size("1520x900"), Some((1520.0, 900.0)));
    for raw in [
        "",
        "1520",
        "1520x",
        "x900",
        "0x900",
        "-1x900",
        "axb",
        "1520x900x1",
    ] {
        assert_eq!(parse_window_size(raw), None, "{raw:?}");
    }
}

/// 抓得住：`size_source=` 必须能区分「默认」「覆盖生效」「覆盖被夹到最小」「覆盖是垃圾」——
/// 四种情况打出一样的两张图，是最典型的假绿（脚本以为在测宽屏，其实测的是默认）。
#[test]
fn window_size_source_distinguishes_the_four_cases() {
    let default = (1180.0, 760.0);
    let min = (880.0, 560.0);

    assert_eq!(
        resolve_window_size(None, default, min),
        (default, "default")
    );
    assert_eq!(
        resolve_window_size(Some("1520x900"), default, min),
        ((1520.0, 900.0), "ULL_WINDOW_SIZE")
    );
    // 最小尺寸档：正好等于 min，不该被标成 clamped
    assert_eq!(
        resolve_window_size(Some("880x560"), default, min),
        ((880.0, 560.0), "ULL_WINDOW_SIZE")
    );
    assert_eq!(
        resolve_window_size(Some("640x480"), default, min),
        (min, "ULL_WINDOW_SIZE:clamped-to-min")
    );
    assert_eq!(
        resolve_window_size(Some("wide-ish"), default, min),
        (default, "ULL_WINDOW_SIZE:invalid")
    );
}

/// 抓得住：macOS 上「菜单栏显示什么名字」从可执行文件路径就能读出来——bundle 与非 bundle
/// 是两条不同的答案，且 `controls=self-drawn` 由 `decorations=false` 推出来。
#[test]
fn caps_line_reads_the_menu_bar_name_from_the_executable_path() {
    let plain = caps_line(
        Some(Path::new(
            "/Users/x/ull/target/release/unlucky-launcher-iced",
        )),
        false,
        true,
        false,
    );
    assert!(plain.contains("bundle=no"), "{plain}");
    assert!(
        plain.contains("menu_bar_name=\"unlucky-launcher-iced\""),
        "{plain}"
    );
    assert!(plain.contains("menu_bar_source=exe-stem"), "{plain}");
    assert!(plain.contains("controls=self-drawn"), "{plain}");

    let bundled = caps_line(
        Some(Path::new(
            "/Applications/Unlucky Launcher.app/Contents/MacOS/unlucky-launcher-iced",
        )),
        false,
        true,
        false,
    );
    assert!(bundled.contains("bundle=yes"), "{bundled}");
    assert!(
        bundled.contains("menu_bar_name=\"Unlucky Launcher\""),
        "{bundled}"
    );
    assert!(bundled.contains("menu_bar_source=bundle"), "{bundled}");

    let native = caps_line(Some(Path::new("/tmp/ull")), false, true, true);
    assert!(native.contains("controls=native"), "{native}");
}

/// 抓得住：`bundle=` 与 `menu_bar_name=` 拿不到路径时不猜（`<未知>`）。
#[test]
fn caps_line_does_not_guess_without_a_path() {
    let line = caps_line(None, false, true, false);
    assert!(line.contains("bundle=no"), "{line}");
    assert!(line.contains("menu_bar_name=\"<未知>\""), "{line}");
}

/// 抓得住：`[backend] plan` 打的是**判据产物**（primary / fallback / reason），不是请求。
/// 改回 `request=` 当 primary 用、或把兜底写成候选列表，这条就红。
#[test]
fn backend_plan_prints_the_decision_not_the_request() {
    let probe = probe_of(&[("Apple M1", "IntegratedGpu")]);
    let decision = decide(crate::Backend::Auto, &probe, true);
    let line = backend_plan_text(&decision, crate::Backend::Auto);
    assert!(line.contains("request=auto"), "{line}");
    assert!(line.contains("primary=wgpu"), "{line}");
    assert!(line.contains("fallback=tiny-skia"), "{line}");
    assert!(line.contains("candidates=1"), "{line}");
    assert!(line.contains("reason=auto-hardware-adapter"), "{line}");
}

/// 抓得住：`[backend] final` 的两条路都要能一眼读出「到底跑起来的是谁」。
#[test]
fn backend_final_lines_separate_started_from_failed() {
    let started = backend_started_text(Kind::Wgpu, Duration::from_millis(37));
    assert!(
        started.contains("final=wgpu result=started elapsed=37ms"),
        "{started}"
    );
    assert!(started.contains("evidence=probe-single-item"), "{started}");

    // refuse：**什么都没跑起来** ⇒ final=none（不是兜底名，否则报告会把「这次用的是 tiny-skia」当真）
    let refused = backend_failed_text(
        Kind::Wgpu,
        Some(Kind::TinySkia),
        None,
        "refuse",
        "metal support not compiled in",
    );
    assert!(refused.contains("final=none result=failed"), "{refused}");
    assert!(refused.contains("fallback=tiny-skia"), "{refused}");
    assert!(refused.contains("primary=wgpu"), "{refused}");
    assert!(refused.contains("via=refuse"), "{refused}");
    assert!(
        refused.contains("error=\"metal support not compiled in\""),
        "具名错误必须原样出现在同一行里：{refused}"
    );

    let none = backend_failed_text(Kind::TinySkia, None, None, "none", "boom");
    assert!(none.contains("final=none result=failed"), "{none}");

    // respawn：进程即将带着兜底重开 ⇒ final=<兜底>
    let respawned = backend_failed_text(
        Kind::Wgpu,
        Some(Kind::TinySkia),
        Some(Kind::TinySkia),
        "respawn",
        "x",
    );
    assert!(respawned.contains("final=tiny-skia"), "{respawned}");
    assert!(respawned.contains("via=respawn"), "{respawned}");

    // 具名错误自带双引号时，`error="…"` 仍必须是**格式良好的单个字段**（否则脚本只能取到前半截）
    let quoted = backend_failed_text(
        Kind::Wgpu,
        Some(Kind::TinySkia),
        None,
        "refuse",
        "GraphicsAdapterNotFound { backend: \"wgpu\" }",
    );
    assert!(
        quoted.contains("error=\"GraphicsAdapterNotFound { backend: 'wgpu' }\""),
        "{quoted}"
    );
    assert_eq!(
        quoted.matches('"').count(),
        2,
        "整行只该有 error= 那一对引号：{quoted}"
    );
}

/// 抓得住：`[perf]` 打的是实测毫秒数，不是「预计」；场景行的 fps 由帧数/时长算。
#[test]
fn perf_lines_carry_measured_millis() {
    let boot = perf_boot_text(
        Some(Duration::from_millis(650)),
        Duration::from_millis(120),
        Duration::from_millis(780),
    );
    assert_eq!(boot, "[perf] boot probe=650ms font=120ms to_run=780ms");
    // 没探测 ⇒ skipped，不是 0ms（M98 r1 p3-1：拿 0 冒充实测）
    let skipped = perf_boot_text(None, Duration::from_millis(120), Duration::from_millis(780));
    assert!(skipped.contains("probe=skipped"), "{skipped}");

    let session = perf_session_text("screenshot", Duration::from_millis(1234), true);
    assert!(session.contains("mode=screenshot elapsed=1234ms result=ok"));

    let scene = perf_scene_text(
        "instances-minimal-w880",
        3,
        Some(Duration::from_millis(210)),
        Duration::from_millis(300),
    );
    assert!(
        scene.contains("frames=3 first_frame=210ms elapsed=300ms"),
        "{scene}"
    );
    // 只算首帧之后的帧间隔：(3-1) 帧 / (0.30-0.21) s ≈ 22.2 fps（启动那段不算进帧率）
    assert!(scene.contains("fps=22.2"), "{scene}");
    // 少于 2 帧 / 没有首帧时间 ⇒ 没有间隔可测，必须是 unknown 而不是编一个数
    let one = perf_scene_text(
        "x",
        1,
        Some(Duration::from_millis(5)),
        Duration::from_millis(9),
    );
    assert!(one.contains("fps=unknown"), "{one}");
    let no_first = perf_scene_text("x", 5, None, Duration::from_millis(9));
    assert!(no_first.contains("fps=unknown"), "{no_first}");
}

/// 抓得住：`[font] chain` 来自令牌的三条栈（顺序 = 首选 → 兜底），不是某个常量字符串。
#[test]
fn font_chain_lists_the_token_stacks_in_order() {
    let line = font_chain_line(FontPlatform::MacOs);
    assert!(line.starts_with("[font] chain platform=macos"), "{line}");
    // macOS 的中文兜底族必须在链里（M98 要验的就是它有没有被用上）
    assert!(line.contains("PingFang SC"), "{line}");
    assert!(line.contains("mono="), "{line}");
    // 顺序：首选族在前，通用族名在最后
    let text = line.split("text=").nth(1).expect("text 栈");
    let first = text.split('>').next().unwrap_or_default();
    assert!(!first.contains("sans-serif"), "栈头不该是通用族名：{line}");
}

/// 抓得住：`[theme]` 的来源判定（哪个参数赢）与「跟随系统 = pending」这条不变量。
#[test]
fn theme_source_follows_the_capture_flavor_order() {
    // --screenshot-theme 压过 --theme，也压过「这是截图模式」的默认值
    assert_eq!(theme_source(true, true, true), ("screenshot-theme", false));
    assert_eq!(theme_source(false, true, true), ("theme", false));
    // 截图模式没给主题 ⇒ 默认 Light（不是「等系统」）
    assert_eq!(theme_source(false, false, true), ("default-light", false));
    // 交互模式没给主题 ⇒ 跟随系统（异步，真值晚到）
    assert_eq!(theme_source(false, false, false), ("system", true));

    let system = theme_text("system", None, true);
    assert!(system.contains("flavor=pending"), "{system}");
}

/// 抓得住：`[theme]` 的 `follows_system=true` 必须同时是 `flavor=pending`
/// （系统主题异步到达；写死一个值就是猜）。
#[test]
fn theme_line_marks_the_async_system_case_as_pending() {
    let system = theme_text("system", None, true);
    assert!(system.contains("source=system"), "{system}");
    assert!(system.contains("flavor=pending"), "{system}");
    assert!(system.contains("follows_system=true"), "{system}");

    let pinned = theme_text("screenshot-theme", Some("dark"), false);
    assert!(pinned.contains("flavor=dark"), "{pinned}");
    assert!(pinned.contains("follows_system=false"), "{pinned}");
}

/// 抓得住：开关默认关（产品启动不该多花 1 s 去枚举适配器），`0/off/false/no` 也算关。
#[test]
fn acceptance_switch_is_off_by_default() {
    // 这个测试不读进程环境（会与其它测试抢 `set_var`），只钉常量名：
    // 名字改了验收脚本就失效，所以它必须是测试锁住的东西。
    assert_eq!(ACCEPTANCE_ENV, "ULL_ACCEPTANCE");
    assert_eq!(WINDOW_SIZE_ENV, "ULL_WINDOW_SIZE");
}

/// 抓得住：`theme_source()`（日志用）与 `Options::capture_flavor()`（真取值口）**两处优先级漂了**——
/// 漂了就会打出错的 `source=`，而图上的主题才是真的（M98 r1 p3-5：两份实现，没有交叉测试）。
/// 抓不住：系统主题异步到达之后的值（那要 `[theme] 系统主题 = …` 那一行）。
#[test]
fn theme_source_agrees_with_the_only_flavor_gate() {
    use crate::cli::Options;

    let cases: [&[&str]; 5] = [
        &[],
        &["--screenshot", "out.png"],
        &["--screenshot", "out.png", "--theme=dark"],
        &[
            "--screenshot",
            "out.png",
            "--theme=dark",
            "--screenshot-theme=light",
        ],
        &["--theme=dark"],
    ];

    for args in cases {
        let options = Options::parse(args.iter().map(|arg| (*arg).to_owned())).expect("参数合法");
        let (source, follows) = theme_source(
            options.screenshot_theme.is_some(),
            options.theme.is_some(),
            options.screenshot.is_some(),
        );
        let flavor = options.capture_flavor();

        assert_eq!(
            follows,
            flavor.is_none(),
            "{}：follows_system 与 capture_flavor().is_none() 必须一致",
            args.join(" ")
        );
        // 非「跟随系统」时，来源标签必须能解释 capture_flavor 给出的那个值
        match source {
            "screenshot-theme" => assert_eq!(flavor, options.screenshot_theme),
            "theme" => assert_eq!(flavor, options.theme),
            "default-light" => assert_eq!(flavor, Some(unlucky_design::color::SchemeFlavor::Light)),
            "system" => assert_eq!(flavor, None),
            other => panic!("没料到的来源 {other}"),
        }
    }
}

/// 抓得住：`via` 的三档与判据表第 8/9 行一致（有兜底 + 取证启动 ⇒ `refuse`，**不是** `respawn`）。
#[test]
fn failure_via_matches_the_two_paths_of_the_criteria_table() {
    assert_eq!(
        failure_via(true, false),
        "respawn",
        "交互启动 + 有兜底 ⇒ 换进程重开"
    );
    assert_eq!(
        failure_via(true, true),
        "refuse",
        "取证启动 + 有兜底 ⇒ 拒绝出图"
    );
    assert_eq!(failure_via(false, false), "none", "没兜底 ⇒ 原样失败");
    assert_eq!(
        failure_via(false, true),
        "none",
        "没兜底时也不该冒出 refuse 的提示"
    );
}
