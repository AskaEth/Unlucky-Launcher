//! 「日志文件夹」在 app 侧的唯一解析口（M188）：路径从哪来、点了按钮之后开哪个目录。
//!
//! ## 为什么不在页面里现拼路径
//!
//! 日志根 = `<启动器数据目录>/logs`，而「启动器数据目录」是三模式解析（便携 / 标准 / 自定义）
//! 的结果。那条解析链只有一份实现（`core::platform::data_dir` + `presenter::settings_bridge`），
//! 页面若自己拼 `exe_dir/.ULL/logs`，标准模式与自定义模式的用户就会看到一个**不存在的目录**。
//!
//! 本模块只做两件事：
//!
//! 1. 从设置桥给定的 `config/launcher.json` 反推 `<home>`，再用 `LauncherLayout::logs()` 取分区
//!    （分区名只从 core 的 `layout` 来，不手拼字符串）；
//! 2. 打开前先确认目录真的在（不在就建），失败时给出的文本**带完整路径**——用户至少能照着去翻。
//!
//! ## 谁来登记
//!
//! `App::new` 拿到设置桥之后调 [`register`]：那是本进程唯一知道「实际生效的数据根」的地方
//! （`--data-dir` 也在这里生效）。没登记时（`pages_demo` 出图通道、单测）退回
//! `platform::data::layout()`——同一份解析链的缓存版本。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use unlucky_core::platform::layout::LauncherLayout;
use unlucky_presenter::settings_bridge::SettingsBridge;

static LOGS_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// `<home>/config/launcher.json` → `<home>/logs`（**只有**这一条形状，别处不许手拼）。
pub(crate) fn logs_dir_of(config: &Path) -> Option<PathBuf> {
    let home = config.parent()?.parent()?;
    Some(LauncherLayout::new(home).logs())
}

/// 登记本进程的日志目录（`App::new` 在第一帧之前调一次；重复调用以**第一次**为准）。
pub(crate) fn register(bridge: &SettingsBridge) {
    let _ = LOGS_DIR.set(bridge.path().and_then(logs_dir_of));
}

/// 本进程的日志目录。
///
/// - 已登记且解析出来 ⇒ 那条；
/// - 已登记但解析不出来（内存态桥）⇒ `None`：页面照实说「数据根没解析出来」，不猜一个目录；
/// - 还没登记（`pages_demo` / 单测）⇒ 退回 `platform::data::layout()`。
pub(crate) fn logs_dir() -> Option<PathBuf> {
    match LOGS_DIR.get() {
        Some(known) => known.clone(),
        None => crate::platform::data::layout().map(|layout| layout.logs()),
    }
}

/// 确保日志目录存在（不在就建），返回给用户看的路径。
///
/// 失败文本里**带完整路径**：界面把这段话原样显示出来，用户能复制去自己找。
pub(crate) fn prepare(dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("{}（这个目录建不出来：{error}）", dir.display()))?;
    if !dir.is_dir() {
        return Err(format!("{}（这个路径不是一个目录）", dir.display()));
    }
    Ok(dir.to_path_buf())
}

/// 打开日志文件夹：解析 → 确保存在 → 交给平台打开器。
///
/// # Errors
///
/// 返回的文本要说清**是哪一步失败**，不是机械拼路径（M386 r1 复审 p2-2 订正：这句话原来写的是
/// 「永远带路径」，换到 core 打开器之后对 spawn 失败那一支就不成立了）：
///
/// | 哪一步 | 文本形状 | 带路径？ |
/// |---|---|---|
/// | 数据根没解析出来 | `日志目录：数据根没解析出来（…用 --data-dir 指定一个）` | 没有（**没有**路径可言） |
/// | 目录建不出来 / 不是目录 | `{路径}（这个目录建不出来：…）` | **带** |
/// | 平台打开器拉不起来 | `拉起失败（explorer）：{OS 原文}` | 不带（带程序名 + OS 原文；路径由调用方的日志行给） |
///
/// 为什么不给 spawn 失败补路径：那句话的主语是「**哪个程序**没拉起来」，再拼一个路径会变成
/// `拉起失败（explorer）：<OS 原文>（<路径>）`——两个冒号两个括号，读的人得在两截里找「原因到底是哪个」。
/// 路径没有丢：`[logs] 打开日志文件夹失败：…` 那一行由调用方 `app/update/page_messages.rs` 打，同一条
/// 消息也进剪贴板，用户能照着日志自己找（见 `page_messages.rs` 里那段注释）。
///
/// # M386（I2）：打开器换成 core 那一个
///
/// 这里以前调 `crate::app::dialog::export::open_dir` ——那是**崩溃弹窗**那一支的打开器，
/// 三处平台分支各自裸 `Command::new("explorer" | "open" | "xdg-open")`（M217 特意留裸，
/// 并在 `crates/core/tests/child_quiet_guard.rs` 的 `ALLOWED_ROLES` 里登记成「目录打开器」角色）。
/// 日志页这条走 [`unlucky_core::platform::opener::open_dir`]：它内部是
/// [`unlucky_core::platform::child::quiet`]（Windows 带 `CREATE_NO_WINDOW`），是**唯一**一份
/// 「平台打开器」实现，也符合「子进程一律安静」那条纪律。
///
/// 为什么 `quiet` 不会把文件管理器窗口也抑制掉：`CREATE_NO_WINDOW` 管的是**子进程要不要一块
/// 控制台**，而 `explorer.exe` / `open` / `xdg-open` 本来就是 GUI 侧程序、根本不分配控制台。
/// 本机实测过（不是只读文档）：`opens_the_real_folder_through_the_quiet_opener` 真开一次窗，
/// 进程外用 `Shell.Application.Windows()` 列到了同一个 `file:///…/.ULL/logs`。
pub(crate) fn open() -> Result<PathBuf, String> {
    let dir = logs_dir().ok_or_else(|| {
        "日志目录：数据根没解析出来（启动器拿不到自己所在的目录；用 --data-dir 指定一个）"
            .to_string()
    })?;
    let dir = prepare(&dir)?;
    unlucky_core::platform::opener::open_dir(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 落点形状：`<home>/config/launcher.json` → `<home>/logs`。
    ///
    /// 变异证明：把 `logs_dir_of` 里的 `LauncherLayout::logs` 换成 `cache`，这条立刻红。
    #[test]
    fn the_log_folder_is_the_logs_partition_next_to_config() {
        let config = PathBuf::from("/tmp/ull-home/.ULL/config/launcher.json");
        assert_eq!(
            logs_dir_of(&config),
            Some(PathBuf::from("/tmp/ull-home/.ULL/logs"))
        );
        // 少了 config 那一层就没有 `<home>` 可言：宁可不猜。
        assert_eq!(logs_dir_of(Path::new("launcher.json")), None);
    }

    /// **目录真的存在**（守门③）：`prepare` 在不在时把它建出来，并且返回同一个路径。
    ///
    /// 变异证明：删掉 `create_dir_all` 那一行，`is_dir` 立刻为假。
    #[test]
    fn prepare_creates_the_folder_and_reports_the_same_path() {
        let root = std::env::temp_dir().join(format!("ull-logs-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join(".ULL/logs");
        assert!(!dir.is_dir(), "夹具目录不该已经存在");
        let prepared = prepare(&dir).expect("应当建得出来");
        assert_eq!(prepared, dir);
        assert!(dir.is_dir(), "打开之前必须真的存在");
        std::fs::remove_dir_all(&root).ok();
    }

    /// 失败文本带路径（「不许只在成功时可用」）。
    #[test]
    fn a_failure_names_the_path_it_could_not_use() {
        let file = std::env::temp_dir().join(format!("ull-logs-notadir-{}", std::process::id()));
        std::fs::write(&file, b"x").expect("写夹具");
        let error = prepare(&file).expect_err("一个文件不是目录");
        assert!(error.contains(&file.display().to_string()), "{error}");
        std::fs::remove_file(&file).ok();
    }

    /// **I2 的静态判据**：日志页那条「打开文件夹」走的是 core 唯一的平台 opener（内部 `quiet`），
    /// 不是崩溃弹窗那份裸 `Command::new("explorer")`。
    ///
    /// 抓得住：把 [`open`] 里那一行改回 `crate::app::dialog::export::open_dir`（= 回到 M386 之前）。
    /// 抓不住：文件管理器**真的**弹出来了没有（`spawn` 没有回值，那要进程外看，见下一条）。
    #[test]
    fn the_logs_folder_opens_through_the_quiet_opener() {
        // 只扫**非测试**部分：这段断言自己就写着 `Command::new` / `dialog::export::open_dir`
        // 那两个词，扫进来会自己把自己判红。
        let source = include_str!("logs_folder.rs");
        let subject = source.split("#[cfg(test)]").next().unwrap_or(source);
        let code: Vec<&str> = subject
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect();
        assert!(
            code.iter()
                .any(|line| line.contains("platform::opener::open_dir(&dir)")),
            "打开器必须是 core 那一个（它内部走 child::quiet）"
        );
        for line in &code {
            assert!(
                !line.contains("Command::new"),
                "不许在这里直接构造平台 opener（子进程一律安静）：{line}"
            );
            assert!(
                !line.contains("dialog::export::open_dir"),
                "崩溃弹窗那份打开器不许再用在日志页：{line}"
            );
        }
    }

    /// **I2 的运行时取证**（默认 `#[ignore]`：它会真的开一个文件管理器窗口）。
    ///
    /// 手动跑：
    /// `cargo test -p unlucky-app-iced --lib opens_the_real_folder -- --ignored --nocapture`
    ///
    /// 做的是产品路径的两步：①[`logs_dir`]（只读，不建目录）②[`prepare`] +
    /// [`unlucky_core::platform::opener::open_dir`]（建目录并真的把打开意图交出去）。
    ///
    /// 断言只覆盖「拉得起来」；「窗口真的出现了」只能从进程外看（报告里贴的是这条用例打出的
    /// 路径 + `Shell.Application.Windows()` 里出现同一个 `file:///…`，M386 实测过）。
    ///
    /// ⚠️ 夹具路径必须**逐组件**拼（`.join(".ULL").join("logs")`）。M386 实测：
    /// 给 `explorer.exe` 一个混分隔符的字符串（`…\.ULL/logs`）它**不报错**，而是回退去开
    /// 「文档」——判据会看到窗口数 +1 却没看到那个路径，白跑一趟。
    #[test]
    #[ignore = "真会开一个文件管理器窗口（I2 运行时取证，手动 --ignored 跑）"]
    fn opens_the_real_folder_through_the_quiet_opener() {
        eprintln!("[m386] logs_dir()={:?}", logs_dir());
        let root = std::env::temp_dir().join(format!("ull-m386-logs-{}", std::process::id()));
        let dir = prepare(&root.join(".ULL").join("logs")).expect("日志目录建得出来");
        unlucky_core::platform::opener::open_dir(&dir).expect("平台打开器应当拉得起来");
        eprintln!("[m386] opened={}", dir.display());
        // 不删目录：文件管理器可能正停在里面
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}
