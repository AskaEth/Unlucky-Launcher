//! 入口。所有实际逻辑在 lib（`unlucky_app_iced`）里。
//!
//! 一个二进制两种用法（M82），分发在进 UI 之前完成：
//!
//! - 被子命令命中（`instances` / `install` / `launch` / `accounts` / `sessions` / `logs` /
//!   `diag` / `config` / `java` / `agent`，或 `--help` / `--version`）→ 直接转给
//!   `unlucky_cli::run`，**不初始化任何 UI**（不 probe 后端、不扫字体、不碰图形库）。
//!   子命令表不在这里抄一份：唯一真相源是 `crates/cli` 的解析器
//!   （`unlucky_cli::cli::looks_like_cli`，本文件只调用）。
//! - 其余参数（`--software-renderer` / `--renderer` / `--theme` / `--screenshot*` /
//!   `--diff` …）：开窗跑骨架，`--screenshot` 出图后退出；`--diff <a> <b>` 只读两张 PNG
//!   做逐像素对比（D30 的 GPU/CPU 几何一致性检查），不开窗。
//!
//! 零 UI 依赖的 `ull` 仍然保留（D18）：它是唯一连 fontconfig 都不需要的产物，
//! 给服务器 / CI / Termux 这类没有图形库的机器用。
//!
//! 退出码：`0` 成功 / `1` 截图有差异（diff 模式）或 CLI 用户错误 / `2` 参数或运行错误。
//!
//! ## 为什么这里是 WINDOWS GUI 子系统（M196）
//!
//! 下面那条 `#![cfg_attr(windows, windows_subsystem = "windows")]` 直接改的是**产物 PE 头**的
//! `Subsystem` 字段（CUI=3 → WINDOWS_GUI=2），这是「双击不再多一个常驻控制台宿主窗口」的
//! 唯一机制：PE 头写 3 时由系统在启动时分配控制台（那个黑框），写 2 时系统不分配。
//!
//! **为什么放这一层而不是 `build.rs`**：PE 子系统是**每个 bin 目标**的属性，而 `cfg_attr` 就是
//! 它的原生长相——rustc 读到它就把标志写进链接出来的 PE 里，没有任何别的路径能改到那一行。
//! `build.rs` 做不到这件事：它只能在编译**之前**跑（`cargo:rustc-link-arg` 之类确实能加链接参数，
//! 但那是「顺手改了链接命令行」的间接法，且在交叉/多 bin 时按 crate 而非按 target 生效），
//! 更麻烦的是它把「这条 bin 是 GUI」这条事实挪到一个**看不见 bin 声明**的地方——本 crate 的
//! `[[bin]]` 名是 `unlucky-launcher-iced`（不是 crate 名），读 `build.rs` 的人得先在 Cargo.toml
//! 里找回来。放 `main.rs` 顶上，事实与产物一一对应：改这个文件 = 改这个 bin。
//!
//! 代价与补救见 [`unlucky_app_iced::platform::win32::console`]：GUI 子系统进程**不继承**父进程
//! 的控制台，所以 `main()` 第一步就把输出接回父控制台（`AttachConsole`），CLI 子命令、
//! `--diff`、`--screenshot` 的日志在 cmd / 终端里照样看得见；双击时没有父控制台，附着失败、
//! 也绝不新建窗口。判据是**产物 PE 头的读数**，不是「代码看起来对」：
//! `packaging/check_pe_subsystem.py` 读 `Subsystem` 字段，非 2 即红（`release-build.yml` 里跑）。
//!
//! 历史：同样的修复曾做过一次，但打在已退役的 Slint 树（`37d3039`，只改了 `crates/app/src/main.rs`），
//! 而 `crates/app` 自 M118 起不在 workspace、不进发布矩阵——所以那一笔从未到这里。
// M196：PE 头的子系统 = WINDOWS_GUI(2)。没有它，双击产物会多一个控制台宿主窗口。
// 这一行只对 bin 目标有意义（lib 没有 PE 头），所以放在 main.rs 而不是 lib.rs。
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::ExitCode;

use unlucky_app_iced::platform::win32::console::attach_parent_console;
use unlucky_app_iced::shot;
use unlucky_app_iced::Options;

fn main() -> ExitCode {
    // M196 第一步：GUI 子系统进程默认没有控制台（stdout/stderr 是 0 号无效句柄），
    // 不接回来 CLI 输出与所有 eprintln 都会静默丢失。四种启动情形的实测表在
    // `platform/win32/console.rs` 顶部：重定向/管道两个句柄本来就可用（一个字节都不动），
    // cmd 里直接跑要 attach，双击时 attach 必失败且不会新建窗口。
    let console = attach_parent_console();
    if std::env::var_os("ULL_ACCEPTANCE").is_some() {
        eprintln!("[console] {}", console.describe());
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = run_cli_if_requested(&args) {
        return code;
    }

    // M97：把 iced_wgpu / wgpu 的 `log` 输出接到 stderr。`Selected: AdapterInfo{…}` 那行
    // 是「GPU 真生效」的实证——没有它，日志只能证明我们**请求过** GPU。
    // CLI 子命令在上面就返回了；`--diff` 虽然也装，但它不碰 wgpu，不会多出任何一行。
    unlucky_app_iced::renderer::log_bridge::install();

    match run_gui() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("[cli] {message}");
            eprintln!(
                "用法：unlucky-launcher-iced [--renderer=auto|gpu|cpu] [--software-renderer] \
                 [--theme=light|dark] [--screenshot <png>] [--screenshot-theme=light|dark] \
                 [--scale-factor <f32>] [--diff <a.png> <b.png>]"
            );
            // M98：验收开关写在用法里，不然「日志怎么变详细了」只能靠翻文档。
            eprintln!(
                "验收诊断：ULL_ACCEPTANCE=1（[console]/[env]/[backend]/[window]/[font]/[theme]/[perf]/[i18n]/[caps]）\
                 ，ULL_WINDOW_SIZE=1520x900 覆盖窗口尺寸"
            );
            // M201：设置页要出「带真值」的验收图，而截图通道默认强制 mock（mock 不发
            // `CoreEvent::SettingsChanged`）。这一条把**数据源**换成真 host，其余一字不改。
            eprintln!(
                "取证：ULL_SCREENSHOT_REAL_HOST=1 让 --screenshot 接真 host（默认 mock；\
                 出图将不再可复现，只用于验收真值屏）"
            );
            ExitCode::from(2)
        }
    }
}

/// 子命令命中 CLI 就跑 CLI 并以它的退出码收尾；返回 `None` 表示「这不是 CLI 调用，去开窗」。
///
/// 这条路径上**不构造窗口、不探后端、不扫字体、不依赖任何图形库**（这些库缺失时同一条命令照样
/// 跑完，实测见 packaging/README.md §7.4）。`unlucky-launcher-iced launch …` 的耗时因此与首帧无关——
/// 这句是结构性判断，**没有重量**（原因见 §10-8）。
///
/// 「这条命令行算不算 CLI 调用」的判据不在这里重复：`unlucky_cli::cli::looks_like_cli` 是唯一
/// 一处（M82 复审去重），两个 GUI 二进制都只调它，改语义不会漂。
fn run_cli_if_requested(args: &[String]) -> Option<ExitCode> {
    if !unlucky_cli::cli::looks_like_cli(args) {
        return None;
    }
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    let code = unlucky_cli::run(args, &mut stdout, &mut stderr);
    let _ = std::io::Write::flush(&mut stdout);
    let _ = std::io::Write::flush(&mut stderr);
    Some(ExitCode::from(u8::try_from(code).unwrap_or(2)))
}

/// 开窗路径：解析 GUI 参数 → （`--diff` 走比对）→ 起 UI。
fn run_gui() -> Result<ExitCode, String> {
    let options = Options::from_env()?;

    if let Some((left, right)) = options.diff.clone() {
        let scan = i32::try_from(options.diff_scan).unwrap_or(i32::MAX);
        return match shot::diff_files_with_scan(&left, &right, scan) {
            Ok(diff) => {
                eprintln!("{}", diff.describe());
                if diff.is_identical() {
                    Ok(ExitCode::SUCCESS)
                } else {
                    Ok(ExitCode::from(1))
                }
            }
            Err(message) => {
                eprintln!("[diff] {message}");
                Ok(ExitCode::from(2))
            }
        };
    }

    match unlucky_app_iced::run(options) {
        Ok(()) => Ok(ExitCode::SUCCESS),
        Err(error) => {
            eprintln!("[run] {error}");
            Ok(ExitCode::FAILURE)
        }
    }
}
