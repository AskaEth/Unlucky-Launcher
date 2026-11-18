//! 用**系统默认程序**打开一个 URL 或一个目录（M235）。
//!
//! # 为什么收敛到 `core::platform`
//!
//! `DevDocs/AGENTS.md` §6：平台差异判断收敛到 `core::platform`，不在各模块里散写
//! `cfg(target_os)` 补丁。本文件里的两种"交出去"：
//!
//! | 目标 | 程序 | 谁在用 |
//! |---|---|---|
//! | 目录 | Windows `explorer` / macOS `open` / 其它 `xdg-open` | 实例详情页「打开文件夹」（`game::saves`，M235） |
//! | URL（`http`/`https`） | Windows `rundll32 url.dll,FileProtocolHandler` / macOS `open` / 其它 `xdg-open` | 微软授权码流（**当前仍走** `presenter::host::opener`，见下） |
//!
//! `open_url` 放在这里是为了给 URL 通道一个归宿：它现在还在 `presenter::host::opener`，因为
//! 把那份实现搬过来会同时改到 `crates/core/tests/child_quiet_guard.rs` 的角色登记（M217 把
//! 那个文件钉在"裸构造 1 处"，不在 M235 的改动范围），所以本 mission **只搬目录这一支**，
//! 迁移 URL 那支留给后续（见 M235 的 handoff）。
//!
//! `app-iced/src/app/dialog/export.rs` 是第三处目录打开器（崩溃对话框的「打开日志文件夹」，
//! M217 特意留裸）；它与这里的 `open_dir` 是同一件事，迁移同样留给后续。
//!
//! # 子进程纪律（M213 / M217）
//!
//! 所有启动都经 [`child::quiet`]：Windows 上带 `CREATE_NO_WINDOW`，否则 GUI 父进程 spawn
//! 控制台程序会给子进程新建控制台（桌面上是"黑框闪一下"）。本文件是本 crate 里**唯一**出现
//! `explorer` / `xdg-open` / `open` 这三个字面量的地方，[`tests::the_launcher_literals_live_here_only`]
//! 扫源码钉住这一条（防"有人又在某个业务模块里塞一个平台分支"）。
//!
//! # 不等待、不判退出码
//!
//! 打开浏览器/文件管理器是**把意图交出去**：用户可能在那边停留很久，`explorer.exe` 即便成功
//! 也常返回非 0 退出码。所以这里只 `spawn` 不 `wait`，`Ok(())` 的含义是「拉起来了」。

use std::path::Path;

use super::child::quiet;

/// 拉起系统默认浏览器打开 `url`（只接受 `http` / `https`）。
///
/// # Errors
///
/// URL 不是 `http`/`https`，或进程拉不起来时返回用户可读文案。
pub fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(format!("不是可打开的网址：{url}"));
    }
    let (program, args) = url_launcher(url);
    launch(program, args)
}

/// 用系统文件管理器打开一个**已存在的目录**。
///
/// 先确认它真是一个目录：拿一个不存在的路径去问 `explorer` 会得到一个"我的电脑"窗口或一句
/// OS 报错，两种都不是用户想要的答案——所以在这里就说清楚。
///
/// # Errors
///
/// 路径不存在 / 不是目录 / 进程拉不起来时返回用户可读文案（含 OS 原文）。
pub fn open_dir(dir: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(dir)
        .map_err(|error| format!("找不到目录（{}）：{error}", dir.display()))?;
    if !meta.is_dir() {
        return Err(format!("不是目录（{}）", dir.display()));
    }
    let (program, args) = dir_launcher(dir);
    launch(program, args)
}

/// 真去拉进程（两个入口共用）。
fn launch(program: &'static str, args: Vec<std::ffi::OsString>) -> Result<(), String> {
    quiet(program)
        .args(args)
        .spawn()
        .map(|_child| ())
        .map_err(|error| format!("拉起失败（{program}）：{error}"))
}

/// 打开 URL 的程序与参数。
#[cfg(target_os = "windows")]
fn url_launcher(url: &str) -> (&'static str, Vec<std::ffi::OsString>) {
    // `rundll32` 不用 shell，URL 里的 `&` 不会被当成命令分隔符（`cmd /C start` 会）。
    (
        "rundll32",
        vec!["url.dll,FileProtocolHandler".into(), url.into()],
    )
}

#[cfg(target_os = "macos")]
fn url_launcher(url: &str) -> (&'static str, Vec<std::ffi::OsString>) {
    ("open", vec![url.into()])
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn url_launcher(url: &str) -> (&'static str, Vec<std::ffi::OsString>) {
    ("xdg-open", vec![url.into()])
}

/// 打开目录的程序与参数。
#[cfg(target_os = "windows")]
fn dir_launcher(dir: &Path) -> (&'static str, Vec<std::ffi::OsString>) {
    // `explorer` 吃的是**一个**路径参数；别拼引号（它不是 shell，引号会变成名字的一部分）。
    ("explorer", vec![dir.as_os_str().to_os_string()])
}

#[cfg(target_os = "macos")]
fn dir_launcher(dir: &Path) -> (&'static str, Vec<std::ffi::OsString>) {
    ("open", vec![dir.as_os_str().to_os_string()])
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn dir_launcher(dir: &Path) -> (&'static str, Vec<std::ffi::OsString>) {
    ("xdg-open", vec![dir.as_os_str().to_os_string()])
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{dir_launcher, open_dir, open_url, url_launcher};

    /// URL 通道只收 http/https（不在单测里真去拉浏览器，只验命令形态）。
    #[test]
    fn rejects_non_http_urls() {
        assert!(open_url("javascript:alert(1)").is_err());
        assert!(open_url("").is_err());
        let (program, args) = url_launcher("http://localhost:1234/auth?code=a&state=b");
        assert!(!program.is_empty());
        assert!(args
            .iter()
            .any(|arg| arg.to_string_lossy().contains("http://localhost:1234/auth")));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_opens_urls_through_the_file_protocol_handler() {
        let (program, args) = url_launcher("https://example.invalid/");
        assert_eq!(program, "rundll32");
        assert_eq!(args[0], "url.dll,FileProtocolHandler");
        assert_eq!(args[1], "https://example.invalid/");
    }

    /// 目录通道：不存在的路径直接拒（**不 spawn**，否则会弹出"我的电脑"或一句 OS 报错）。
    #[test]
    fn missing_dirs_are_rejected_before_spawning() {
        let gone = PathBuf::from("ull-no-such-dir-m235/definitely-missing");
        let error = open_dir(&gone).expect_err("不存在的目录居然打开了？");
        assert!(error.contains("找不到目录"), "错误文案：{error}");

        // 存在但不是目录（用一个文件）也要拒。
        let file = std::env::temp_dir().join(format!("ull-opener-m235-{}", std::process::id()));
        std::fs::write(&file, b"x").expect("写临时文件");
        let error = open_dir(&file).expect_err("文件居然被当目录打开了？");
        let _ = std::fs::remove_file(&file);
        assert!(error.contains("不是目录"), "错误文案：{error}");
    }

    /// 目录通道的命令形态（各平台一支，只验"程序名 + 一个路径参数"，不 spawn）。
    #[test]
    fn dir_launcher_passes_the_path_as_a_single_argument() {
        let (program, args) = dir_launcher(&PathBuf::from("D:/ULL/.ULL/data/instances/x/saves"));
        assert!(!program.is_empty());
        assert_eq!(args.len(), 1, "路径必须是**一个**参数：{args:?}");
        assert_eq!(
            args[0].to_string_lossy().replace('\\', "/"),
            "D:/ULL/.ULL/data/instances/x/saves"
        );
        if cfg!(target_os = "windows") {
            assert_eq!(program, "explorer");
        } else if cfg!(target_os = "macos") {
            assert_eq!(program, "open");
        } else {
            assert_eq!(program, "xdg-open");
        }
    }

    /// **防退化判据**（`AGENTS.md` §6）：`explorer` / `xdg-open` 这两个字面量只许出现在本文件。
    ///
    /// 抓得住：有人在 `game::saves` 或别的业务模块里再写一份平台分支（tower 在 M235 明确要求
    /// 把这条写成可执行判据，而不是只写在提交信息里）。
    /// 抓不住：经由变量间接拼出的程序名、以及本 crate 之外的 crate（presenter/app 侧另有
    /// `tests/child_quiet_guard.rs` 管"是否经 `quiet` 起进程"，不管"在哪儿起"）。
    #[test]
    fn the_launcher_literals_live_here_only() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut hits = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                    continue;
                }
                if path == root.join("platform").join("opener.rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for (index, line) in text.lines().enumerate() {
                    // 注释里提到这两个名字（文档、handoff）不算退化。
                    let code = line.split("//").next().unwrap_or("");
                    if code.contains("\"explorer\"") || code.contains("\"xdg-open\"") {
                        hits.push(format!("{}:{}", path.display(), index + 1));
                    }
                }
            }
        }
        assert!(
            hits.is_empty(),
            "平台 opener 必须只在 platform/opener.rs 里：{}",
            hits.join(", ")
        );
    }
}
