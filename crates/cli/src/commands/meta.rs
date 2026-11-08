//! 元命令：`--version` / 无子命令 / `help`。

use serde_json::json;

use crate::cli::{help, program};
use crate::os;
use crate::output::Report;

/// 启动器名（`${launcher_name}` 与 `--version` 输出共用）。
pub const LAUNCHER_NAME: &str = "UnluckyLauncher";

/// 版本 + 系统摘要（`ull --version`，D19 §5-2 要求附带 os 摘要）。
///
/// 探测走 `core::platform::os_info::current()`（进程内缓存），未达基线时附带升级指引。
/// 自称的名字取 argv[0]（M82）：GUI 二进制调进来时是 `unlucky-launcher`，`ull` 自己跑时仍是 `ull`。
#[must_use]
pub fn version() -> Report {
    let info = os::detect();
    let invoked_as = program::name();
    let data = json!({
        "name": invoked_as,
        "package": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION"),
        "os": os::to_json(&info),
    });
    let mut human = vec![
        format!(
            "{invoked_as} {}（{}）",
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_NAME")
        ),
        format!("os: {}", os::summary_line(&info)),
    ];
    human.extend(os::notes(&info));
    Report::new(data, human)
}

/// 无子命令：版本摘要 + 帮助（用户原话「无子命令时 --version + --help」）。
#[must_use]
pub fn welcome() -> Report {
    let version_report = version();
    let mut human = version_report.human;
    let text = general_help();
    human.push(String::new());
    human.extend(text.lines().map(str::to_string));
    let mut data = version_report.data;
    data["help"] = json!(text);
    Report::new(data, human)
}

/// 帮助（`ull help <命令>` 给该命令，未知主题退回总帮助）。
#[must_use]
pub fn help(topic: Option<&str>) -> Report {
    let text = match topic.and_then(help::topic) {
        Some(text) => program::fill(text),
        None => general_help(),
    };
    let data = json!({
        "topic": topic,
        "text": text,
    });
    Report::new(data, text.lines().map(str::to_string).collect())
}

/// 总帮助，按被调用的二进制名自述（`ull` 自己跑时逐字不变）。
fn general_help() -> String {
    program::fill(help::general())
}
