//! 系统摘要与启动自检的 **CLI 呈现层**（D19 探测 + D21 Win10+ 单基线）。
//!
//! 事实采集与等级判定全在 `unlucky_core::platform::os_info`（M20）：
//! - [`os_info::detect`] 真探一次（会起子进程，别在热路径反复调用）；
//! - [`os_info::current`] 进程内缓存（CLI 每次执行只真探一次）；
//! - [`os_info::check`] 启动自检（等级 + 双语文案 + 系统信息）。
//!
//! 本模块只做 CLI 关心的事：一行摘要、`--json` 对象、拒绝运行时的文案。
//! 支持级别只有两档（D21 用户拍板「直接 win10 起步」）：[`SupportLevel::One`] 与
//! [`SupportLevel::Unsupported`]——Win8.1/8/7 不承诺 GUI 也不承诺 CLI，
//! 识别出旧版本正是为了明确报「不支持」。

use serde_json::{json, Value};
use unlucky_core::platform::os_info::{self, OsInfo, SupportLevel};

/// 取进程内缓存的系统信息（一次进程只真探一次）。
#[must_use]
pub fn detect() -> OsInfo {
    os_info::current().clone()
}

/// 启动自检（等级 + 双语文案 + 系统信息），供拒绝运行时直接打印。
#[must_use]
pub fn check() -> os_info::SupportCheck {
    os_info::check()
}

/// 一行摘要（`ull --version` 附带）。
#[must_use]
pub fn summary_line(info: &OsInfo) -> String {
    let version = if info.version_string.is_empty() {
        "版本未知"
    } else {
        info.version_string.as_str()
    };
    format!(
        "{} · {}（{}）· {}",
        info.product,
        version,
        info.arch,
        support_label(info.support_level)
    )
}

/// `--json` 里的系统对象。
#[must_use]
pub fn to_json(info: &OsInfo) -> Value {
    json!({
        "family": info.family.manifest_name(),
        "product": info.product,
        "version": if info.version_string.is_empty() { None } else { Some(&info.version_string) },
        "major_minor": info.major_minor.map(|(major, minor)| format!("{major}.{minor}")),
        "build": info.build,
        "arch": info.arch,
        "session": info.session.as_str(),
        "support": info.support_level.as_str(),
        "baseline": BASELINE,
    })
}

/// 支持基线的一句话（`--json` 与帮助文案共用）。
pub const BASELINE: &str = "Windows 10+ / macOS 12+ / Linux（内核 ≥ 3.2，glibc ≥ 2.17）";

/// 环境级提示：仅在低于基线时非空（一等基线不打岔）。
#[must_use]
pub fn notes(info: &OsInfo) -> Vec<String> {
    match info.support_level {
        SupportLevel::One => Vec::new(),
        SupportLevel::Unsupported => info.describe().zh.lines().map(str::to_string).collect(),
    }
}

fn support_label(level: SupportLevel) -> &'static str {
    match level {
        SupportLevel::One => "满足最低要求",
        SupportLevel::Unsupported => "不支持（低于最低要求）",
    }
}
