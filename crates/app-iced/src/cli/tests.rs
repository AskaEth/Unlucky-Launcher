//! `cli::Options` 的解析断言（M100 从 `cli.rs` 内联的 `mod tests` 搬出来：那边 394 行，
//! 加一个 `ForcedState::Rest` 就必然破 400）。
//!
//! 搬移**不改一行断言**：只去掉了 `mod tests { }` 那一层缩进，外加 M100 新增的
//! `screenshots_default_to_the_rest_state`（见文件末尾）。
//!
//! 站位：本文件是 `crate::cli::tests`，`use super::*;` 就够（`Options` / `ForcedState` /
//! `Backend` / `Screen` 都在 `cli` 里）。
use super::*;

fn parse(args: &[&str]) -> Result<Options, String> {
    Options::parse(args.iter().map(|arg| (*arg).to_owned()))
}

#[test]
fn defaults_are_auto_system_theme_and_shell() {
    let options = parse(&[]).expect("空参数要能解析");
    assert_eq!(options.backend, Backend::Auto);
    assert_eq!((options.theme, options.screenshot), (None, None));
    assert_eq!(options.screen, Screen::Shell);
}

#[test]
fn software_renderer_is_an_alias_of_cpu() {
    assert_eq!(
        parse(&["--software-renderer"]).unwrap().backend,
        Backend::Cpu
    );
    assert_eq!(parse(&["--renderer=cpu"]).unwrap().backend, Backend::Cpu);
    assert_eq!(parse(&["--renderer", "gpu"]).unwrap().backend, Backend::Gpu);
}

#[test]
fn both_argv_forms_are_accepted() {
    let spaced = parse(&["--screenshot", "out.png"]).unwrap();
    let inline = parse(&["--screenshot=out.png"]).unwrap();
    assert_eq!(spaced.screenshot, Some(PathBuf::from("out.png")));
    assert_eq!(spaced.screenshot, inline.screenshot);
    assert_eq!(
        parse(&["--diff", "a.png", "b.png"]).unwrap().diff,
        Some((PathBuf::from("a.png"), PathBuf::from("b.png")))
    );
    assert_eq!(
        parse(&["--screen", "gallery"]).unwrap().screen,
        Screen::Gallery
    );
    assert_eq!(parse(&["--diff-scan=2"]).unwrap().diff_scan, 2);
    assert_eq!(
        parse(&["--force-state", "hover"]).unwrap().force_state,
        Some(ForcedState::Hover)
    );
}

#[test]
fn screenshot_theme_wins_over_theme() {
    let o = parse(&["--theme=dark", "--screenshot-theme=light"]).unwrap();
    assert_eq!(
        (o.theme, o.shot_flavor()),
        (Some(SchemeFlavor::Dark), Some(SchemeFlavor::Light))
    );
}

#[test]
fn bad_values_are_reported_not_panicked() {
    for args in [
        &["--renderer=metal"][..],
        &["--theme=blue"],
        &["--scale-factor=0"],
        &["--scale-factor=abc"],
        &["--screenshot"],
        &["--diff", "only-one.png"],
        &["--screen=settings"],
        &["--force-state=focus"],
        &["--diff-scan=-1"],
        &["--nope"],
    ] {
        assert!(parse(args).is_err(), "{args:?} 应该报错");
    }
}

#[test]
fn data_dir_is_accepted_and_ignored_for_the_flip() {
    // 现役 CI 冒烟：--software-renderer --screenshot <png> --data-dir <dir>
    let options = parse(&[
        "--software-renderer",
        "--screenshot",
        "shot.png",
        "--data-dir",
        "/d/tmp/x",
    ])
    .expect("翻牌前必须兼容 --data-dir");
    assert_eq!(options.backend, Backend::Cpu);
    assert_eq!(options.screenshot, Some(PathBuf::from("shot.png")));
    assert_eq!(options.data_dir, Some(PathBuf::from("/d/tmp/x")));
}

/// **取证不变量（M100）**：截图通道默认 Rest，交互模式保持 `None`，显式要态仍然优先。
///
/// 抓得住：① 谁把截图默认值改回 `None`（系统指针 hover 会重新漏进图，M99 复审实测翻盘过一次）；
/// ② 谁顺手在交互模式也钉死状态（那会让真窗口永远看不到 hover）。
/// 抓不住：**产品页**上是否还有不消费 forced 的控件——那要出图比对，测试管不了。
#[test]
fn screenshots_default_to_the_rest_state() {
    assert_eq!(
        parse(&["--force-state=rest"]).unwrap().force_state,
        Some(ForcedState::Rest),
        "rest 是显式的合法取值"
    );
    assert_eq!(
        parse(&["--screenshot", "out.png"]).unwrap().force_state,
        Some(ForcedState::Rest),
        "截图通道必须默认 Rest"
    );
    assert_eq!(
        parse(&["--screenshot", "o.png", "--force-state=hover"])
            .unwrap()
            .force_state,
        Some(ForcedState::Hover),
        "显式要态时按用户给的画（状态矩阵图靠它）"
    );
    assert_eq!(
        parse(&["--screenshot", "o.png", "--force-state=pressed"])
            .unwrap()
            .force_state,
        Some(ForcedState::Pressed)
    );
    assert_eq!(
        parse(&[]).unwrap().force_state,
        None,
        "交互模式不许替用户钉死状态"
    );
    assert_eq!(parse(&["--theme=dark"]).unwrap().force_state, None);
}

/// **门禁依赖**：CI 的 runtime-smoke 不带主题，截图模式必须有确定主题。
/// 退化成 `None` 的话，xvfb 里的 runner 会挂到超时（不是红，更难查）。
#[test]
fn screenshot_without_theme_defaults_to_light() {
    let options = parse(&["--screenshot", "out.png"]).unwrap();
    assert_eq!(options.shot_flavor(), None, "用户没给主题");
    assert_eq!(
        options.capture_flavor(),
        Some(SchemeFlavor::Light),
        "截图模式必须给确定主题（CI 冒烟依赖这条）"
    );
    assert!(!options.wants_system_theme(), "截图模式不许等系统主题");
}

#[test]
fn explicit_themes_still_win_in_screenshot_mode() {
    let dark = parse(&["--screenshot", "a.png", "--theme=dark"]).unwrap();
    assert_eq!(dark.capture_flavor(), Some(SchemeFlavor::Dark));

    let both = parse(&[
        "--screenshot",
        "a.png",
        "--theme=dark",
        "--screenshot-theme=light",
    ]);
    assert_eq!(both.unwrap().capture_flavor(), Some(SchemeFlavor::Light));
}

#[test]
fn interactive_mode_still_follows_the_system_theme() {
    let interactive = parse(&[]).unwrap();
    assert_eq!(interactive.capture_flavor(), None);
    assert!(interactive.wants_system_theme(), "交互模式等系统主题");

    let pinned = parse(&["--theme=dark"]).unwrap();
    assert_eq!(pinned.capture_flavor(), Some(SchemeFlavor::Dark));
    assert!(!pinned.wants_system_theme());
}
