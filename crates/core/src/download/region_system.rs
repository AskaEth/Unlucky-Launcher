//! 地区判定的**系统级**信号补充：环境变量之外的那一半（D23「国内用户默认都走镜像」）。
//!
//! 为什么需要它：`Signals::from_env` 只读 `TZ` / `LANG` / `LC_*`。Windows 上这三个通常**全是空的**
//! ——中文版 Windows 既不设 `TZ` 也不设 `LANG`，于是「国内机器」被误判成国外，
//! D23 定下的国内默认序（Temurin→Oracle→MS）就拿不到了。本机实测就是这个现象。
//!
//! 补法（只在环境变量没给出答案时才做，且读不到就静默跳过）：
//!
//! | 平台 | 时区 | 语言 |
//! | --- | --- | --- |
//! | Windows | `tzutil /g` → `China Standard Time` | `reg query` 的 `LocaleName` → `zh-CN` |
//! | 其它 | 不补（Unix 一般都有 `LANG`，`/etc/localtime` 解析成本不划算） | 同左 |
//!
//! 两个纪律：①**不引依赖**——`tzutil` / `reg` 都是系统自带，`discover.rs` 早就有 `reg query`
//! 的先例；②**不做网络探测**——探测（google 可达性）仍由调用方决定开不开。

use std::process::Command;

use super::region::{Signals, GREATER_CHINA_TIMEZONES};

/// Windows 时区显示名里能判成「大中华区」的关键词（`tzutil /g` 的输出形态）。
const GREATER_CHINA_ZONE_HINTS: &[&str] = &[
    "China Standard Time",
    "Taipei Standard Time",
    "Hong Kong Standard Time",
    "Ulaanbaatar",
];

/// 系统级信号（环境变量之外；读不到就是空信号）。
#[must_use]
pub fn system_signals() -> Signals {
    let mut signals = Signals::default();
    if !cfg!(windows) {
        return signals;
    }
    if let Some(zone) = windows_timezone() {
        if zone_in_greater_china(&zone) {
            // 折成 IANA 名，和 `Signals::from_env` 的取值空间保持一致。
            signals.timezone = Some("Asia/Shanghai".to_string());
        }
    }
    if let Some(locale) = windows_locale() {
        signals.languages.push(locale);
    }
    signals
}

/// 环境变量信号 + 系统级信号（系统级只补空缺，不覆盖环境变量）。
#[must_use]
pub fn detect_signals() -> Signals {
    let mut signals = Signals::from_env();
    let system = system_signals();
    if signals.timezone.is_none() {
        signals.timezone = system.timezone;
    }
    for language in system.languages {
        if !signals.languages.contains(&language) {
            signals.languages.push(language);
        }
    }
    signals
}

/// `tzutil /g` 的输出（失败为 `None`）。
#[must_use]
pub fn windows_timezone() -> Option<String> {
    let output = Command::new("tzutil").arg("/g").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// 系统的 locale 名（`HKCU\Control Panel\International` 的 `LocaleName`）。
#[must_use]
pub fn windows_locale() -> Option<String> {
    let output = Command::new("reg")
        .args([
            "query",
            "HKCU\\Control Panel\\International",
            "/v",
            "LocaleName",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    parse_reg_locale_name(&stdout)
}

/// 从 `reg query` 的输出里取 `LocaleName` 的值（`    LocaleName    REG_SZ    zh-CN`）。
#[must_use]
pub fn parse_reg_locale_name(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with("LocaleName") {
            continue;
        }
        let value = line.rsplit("REG_SZ").next()?.trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

/// Windows 时区显示名是否落在大中华区。
#[must_use]
pub fn zone_in_greater_china(zone: &str) -> bool {
    let zone = zone.trim();
    GREATER_CHINA_ZONE_HINTS
        .iter()
        .any(|hint| zone.eq_ignore_ascii_case(hint))
        || GREATER_CHINA_TIMEZONES
            .iter()
            .any(|known| known.eq_ignore_ascii_case(zone))
}

#[cfg(test)]
mod tests {
    use super::{detect_signals, parse_reg_locale_name, zone_in_greater_china};

    #[test]
    fn windows_zone_names_map_to_the_greater_china_list() {
        assert!(zone_in_greater_china("China Standard Time"));
        assert!(zone_in_greater_china("Taipei Standard Time"));
        assert!(zone_in_greater_china("Asia/Shanghai"), "IANA 名同样要认");
        assert!(!zone_in_greater_china("Pacific Standard Time"));
        assert!(!zone_in_greater_china(""));
    }

    #[test]
    fn reg_output_yields_the_locale_name() {
        // 真机 `reg query "HKCU\Control Panel\International" /v LocaleName` 的形态。
        let stdout = "\r\nHKEY_CURRENT_USER\\Control Panel\\International\r\n    \
                      LocaleName    REG_SZ    zh-CN\r\n\r\n";
        assert_eq!(parse_reg_locale_name(stdout).as_deref(), Some("zh-CN"));
        assert_eq!(
            parse_reg_locale_name("    LocaleName    REG_SZ    \r\n"),
            None
        );
        assert_eq!(parse_reg_locale_name("没有这一行"), None);
    }

    #[test]
    fn detect_layers_system_signals_under_the_environment() {
        // 环境变量优先：这里能断言的只有「不 panic 且返回一个 Signals」，
        // 具体取值取决于跑测试的机器（CI 上没有 tzutil/reg 也要安静退化）。
        let signals = detect_signals();
        // 无论哪种情况，都不该凭空造出「大中华区」以外的时区值。
        if let Some(zone) = &signals.timezone {
            assert!(!zone.trim().is_empty());
        }
    }
}
