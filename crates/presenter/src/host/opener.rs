//! 用系统默认浏览器打开授权 URL（授权码流唯一需要宿主做的事）。
//!
//! core 只负责拼授权 URL 与等本地回调（`core::auth::microsoft::authcode`）；
//! 「谁来打开浏览器」是宿主的决定：GUI 走这里，CLI/无浏览器场景仍用设备码流。
//!
//! 平台差异收在 [`launcher`] 一处，不往业务代码里散写 `cfg` 补丁（`AGENTS.md` §7）。

use std::process::Command;

/// 拉起系统默认浏览器打开 `url`（只接受 http/https）。
///
/// 不等待浏览器退出：用户可能在浏览器里停留几分钟，宿主只关心「有没有拉起来」。
///
/// # Errors
///
/// URL 不是 http/https，或进程拉不起来时返回用户可读文案。
pub fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(format!("不是可打开的网址：{url}"));
    }
    let (program, args) = launcher(url);
    Command::new(program)
        .args(args)
        .spawn()
        .map(|_child| ())
        .map_err(|error| format!("拉起浏览器失败（{program}）：{error}"))
}

#[cfg(target_os = "windows")]
fn launcher(url: &str) -> (&'static str, Vec<String>) {
    // `rundll32` 不用 shell，URL 里的 `&` 不会被当成命令分隔符（`cmd /C start` 会）。
    (
        "rundll32",
        vec!["url.dll,FileProtocolHandler".to_string(), url.to_string()],
    )
}

#[cfg(target_os = "macos")]
fn launcher(url: &str) -> (&'static str, Vec<String>) {
    ("open", vec![url.to_string()])
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn launcher(url: &str) -> (&'static str, Vec<String>) {
    ("xdg-open", vec![url.to_string()])
}

#[cfg(test)]
mod tests {
    use super::{launcher, open_url};

    #[test]
    fn rejects_non_http_urls() {
        assert!(open_url("javascript:alert(1)").is_err());
        assert!(open_url("").is_err());
        // 合法 URL 只验命令形态，不在单测里真去拉浏览器。
        let (program, args) = launcher("http://localhost:1234/auth?code=a&state=b");
        assert!(!program.is_empty());
        assert!(args
            .iter()
            .any(|arg| arg.contains("http://localhost:1234/auth")));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_uses_the_file_protocol_handler() {
        let (program, args) = launcher("https://example.invalid/");
        assert_eq!(program, "rundll32");
        assert_eq!(args[0], "url.dll,FileProtocolHandler");
        assert_eq!(args[1], "https://example.invalid/");
    }
}
