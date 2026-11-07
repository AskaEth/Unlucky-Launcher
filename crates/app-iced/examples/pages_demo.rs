//! 页面骨架截图入口（M84 `PAGEFRAME` 的验收通道）。
//!
//! ```text
//! cargo run -p unlucky-app-iced --example pages_demo -- \
//!   --screen=instances --width=1180 --height=760 \
//!   --screenshot=out.png --screenshot-theme=light --software-renderer
//! ```
//!
//! 参数解析与 iced 接线都在 `unlucky_app_iced::pages::harness` 里：**本 example 只是入口**，
//! 它也刻意不接 `cli.rs`（那边归 M75）。详细口径见 `pages::harness` 的模块文档。

use std::process::ExitCode;

use unlucky_app_iced::pages::harness::{self, Request};

fn main() -> ExitCode {
    let request = match Request::parse(std::env::args().skip(1)) {
        Ok(request) => request,
        Err(message) => {
            eprintln!("[cli] {message}");
            eprintln!(
                "用法：pages_demo [--screen=instances|download|settings|logs] [--width <px>] \
                 [--height <px>] [--theme=light|dark] [--screenshot-theme=light|dark] \
                 [--screenshot <png>] [--renderer=auto|gpu|cpu] [--software-renderer]"
            );
            return ExitCode::from(2);
        }
    };

    match harness::run(request) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("[run] {message}");
            ExitCode::FAILURE
        }
    }
}
