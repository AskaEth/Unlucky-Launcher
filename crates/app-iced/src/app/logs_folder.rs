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
/// 返回的文本永远带路径（`...logs` 那一串），可以直接贴给用户/贴进 issue。
pub(crate) fn open() -> Result<PathBuf, String> {
    let dir = logs_dir().ok_or_else(|| {
        "日志目录：数据根没解析出来（启动器拿不到自己所在的目录；用 --data-dir 指定一个）"
            .to_string()
    })?;
    let dir = prepare(&dir)?;
    crate::app::dialog::export::open_dir(&dir)?;
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
}
