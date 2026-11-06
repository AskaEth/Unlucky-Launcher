//! 入口。所有实际逻辑在 lib（`unlucky_app_iced`）里：
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

use std::process::ExitCode;

use unlucky_app_iced::shot;
use unlucky_app_iced::Options;

fn main() -> ExitCode {
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
                "验收诊断：ULL_ACCEPTANCE=1（[env]/[backend]/[window]/[font]/[theme]/[perf]/[i18n]/[caps]）\
                 ，ULL_WINDOW_SIZE=1520x900 覆盖窗口尺寸"
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
