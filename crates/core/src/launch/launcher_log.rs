//! 启动器**自己的**运行日志（M187）：`<.ULL>/logs/launcher.log`。
//!
//! ## 为什么要有这个文件
//!
//! 用户实测（2026-11-08）：「游戏崩溃后只有一个安装日志，没有崩溃日志」。核查结论是：
//! `logs/` 分区里此前只有 `install.log`（安装流水）与 auth journal——**启动器自己没有运行期
//! 日志**。双击启动的 GUI 进程没有 stderr 去向，所有 `eprintln!` 诊断当场蒸发，于是
//! 「启动器视角发生了什么」在事故之后无法复原。
//!
//! 本模块只做一件事：把一行行 `[scope] message` 追加到一个文件里，**落盘即脱敏**。
//!
//! ## 三条约束（都是有意的）
//!
//! 1. **best-effort**：写不进去就静默放过。日志失败不该让启动器崩、也不该弹提示。
//! 2. **不阻塞**：不建后台线程、不做异步队列。调用点都是「一次启动 / 一次导出」这种低频事件，
//!    每次一行 `write_all` 的代价可以忽略；引入通道反而多一处要收尾的生命周期。
//! 3. **有上限**：超过 [`MAX_BYTES`] 就滚成 `launcher.1.log`（只保留一份历史）。日志文件
//!    无限长会让「导出诊断包」变成搬砖。
//!
//! ## 目录从哪来
//!
//! 目录**不是**本模块自己解析的：`.ULL` 的落点由 `platform::data_dir` 的三模式决定（导出的
//! exe 目录 / 标准目录 / 自定义根），解析它需要读配置——那是宿主（presenter / app）已经做过
//! 的事。所以这里是「**先到者登记**」：知道真相的调用方用 [`set_dir`] 登记，
//! 登记之前 [`record`] 的行进内存缓冲（最多 [`MAX_BUFFERED`] 行），登记时一次刷出去。
//! 这样启动早期（字体 / GPU / 路由）那几行也不会丢。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use super::launch_session::SessionHandle;
use super::redact::Redactor;

/// 日志文件名。
pub const FILE_NAME: &str = "launcher.log";
/// 单个文件上限（超过就滚成 `launcher.1.log`，只留一份历史）。
pub const MAX_BYTES: u64 = 1024 * 1024;
/// 登记目录之前最多缓冲多少行。
pub const MAX_BUFFERED: usize = 200;

#[derive(Debug, Default)]
struct State {
    dir: Option<PathBuf>,
    buffer: Vec<String>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default()))
}

/// 登记日志目录（`<.ULL>/logs`）。重复登记时以**最后一次**为准（数据根可能被 `redirect.json` 改）。
pub fn set_dir(logs_dir: impl Into<PathBuf>) {
    let dir = logs_dir.into();
    let mut guard = match state().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if guard.dir.as_ref() == Some(&dir) {
        return;
    }
    guard.dir = Some(dir);
    let buffered = std::mem::take(&mut guard.buffer);
    let Some(directory) = guard.dir.clone() else {
        return;
    };
    drop(guard);
    for line in buffered {
        append(&directory, &line);
    }
}

/// 当前日志文件路径（`None` = 还没登记目录）。
pub fn path() -> Option<PathBuf> {
    let guard = state().lock().ok()?;
    guard.dir.as_ref().map(|dir| dir.join(FILE_NAME))
}

/// 记一行（`[scope] message`）。scope 是稳定短的英文标签（`boot` / `launch` / `dialog`）。
///
/// 时间戳走 `launch::time::now()`（与事件流同一份时钟口径），所以日志行与 JSONL 能对上。
pub fn record(scope: &str, message: &str) {
    let line = format!(
        "{} [{scope}] {message}",
        super::time::format_rfc3339_millis(super::time::now())
    );
    let dir = {
        let mut guard = match state().lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        match guard.dir.clone() {
            Some(dir) => dir,
            None => {
                if guard.buffer.len() < MAX_BUFFERED {
                    guard.buffer.push(line);
                }
                return;
            }
        }
    };
    append(&dir, &line);
}

/// 一次启动的「启动前」一行：`instance=… game_dir=… java=… memory=…MB`（M211/D5）。
///
/// GUI（`presenter::host::launch`）与 CLI（`ull launch`）写的是**同一份** `launcher.log`，
/// 所以字段名/顺序只在这里定义一次——两边各写一句 `format!` 就等于迟早漂成两种形状。
fn start_line(
    instance_id: &str,
    game_dir: &Path,
    java: Option<(&Path, u32)>,
    memory_mb: Option<u32>,
) -> String {
    let java = java.map_or_else(
        || "未找到".to_string(),
        |(binary, major)| java_label(binary, major),
    );
    format!(
        "instance={instance_id} game_dir={} java={java} memory={}MB",
        game_dir.display(),
        memory_mb.unwrap_or(0)
    )
}

/// `java=` 字段的形状：`<二进制> (major <主版本>)`。
pub fn java_label(binary: &Path, major: u32) -> String {
    format!("{} (major {major})", binary.display())
}

/// 一次启动的「收尾」一行：`instance=… session=… exit=… type=… uptime=…ms outcome=… artifacts=…`。
///
/// `outcome` 与 GUI 的 `LogOutcome` 同词（`Ok` / `Crashed` / `SilentFail`），见
/// [`SessionHandle::outcome_label`]。
#[allow(clippy::too_many_arguments)]
fn end_line(
    instance_id: &str,
    session_id: &str,
    exit_code: i32,
    exit_type: &str,
    uptime_ms: u64,
    outcome: &str,
    artifacts: usize,
) -> String {
    format!(
        "instance={instance_id} session={session_id} exit={exit_code} type={exit_type} \
         uptime={uptime_ms}ms outcome={outcome} artifacts={artifacts}"
    )
}

/// 记「启动前」一行（`instance` / `game_dir` / `java` / `memory`）。
///
/// 装配失败也要留下这一行：事故之后「到底打算用什么起的」是第一个要看的。
pub fn record_start(
    instance_id: &str,
    game_dir: &Path,
    java: Option<(&Path, u32)>,
    memory_mb: Option<u32>,
) {
    record(
        "launch",
        &start_line(instance_id, game_dir, java, memory_mb),
    );
}

/// 记「收尾」一行（`session` / `exit` / `type` / `uptime` / `outcome` / `artifacts`）。
pub fn record_end(instance_id: &str, handle: &SessionHandle) {
    record(
        "launch",
        &end_line(
            instance_id,
            &handle.session_id,
            handle.exit.code,
            handle.exit.exit_type.as_str(),
            handle.uptime_ms,
            handle.outcome_label(),
            handle.artifacts.len(),
        ),
    );
}

/// 脱敏器：主目录换成 `<home>`（同一份规则见 [`super::redact`]）。
fn redactor() -> Redactor {
    let mut redactor = Redactor::new();
    for key in ["USERPROFILE", "HOME"] {
        if let Ok(home) = std::env::var(key) {
            if !home.trim().is_empty() {
                redactor = redactor.with_home_str(home);
            }
        }
    }
    redactor
}

fn append(dir: &Path, line: &str) {
    let path = dir.join(FILE_NAME);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // 滚动：只在超过上限时做一次 rename，平时的路径只有「打开 → 追加 → 关」。
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, dir.join("launcher.1.log"));
    }
    let scrubbed = redactor().scrub(line).into_owned();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = file.write_all(scrubbed.as_bytes());
        let _ = file.write_all(b"\n");
    }
}

#[cfg(test)]
fn state_reset() {
    if let Ok(mut guard) = state().lock() {
        guard.dir = None;
        guard.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::MAX_BUFFERED;
    use std::path::PathBuf;

    /// 测试之间共享全局状态，必须串行（`cargo test` 默认多线程）。
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn temp_dir(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("ull-m187-log-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建夹具目录");
        root
    }

    #[test]
    fn buffered_lines_are_flushed_once_the_dir_is_known() {
        let _guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
        super::state_reset();
        let dir = temp_dir("flush");

        super::record("boot", "before the dir was known");
        assert!(super::path().is_none(), "没登记目录时不该报出路径");
        super::set_dir(&dir);
        super::record("launch", "after");

        let text = std::fs::read_to_string(dir.join(super::FILE_NAME)).expect("读日志");
        assert!(text.contains("before the dir was known"), "{text}");
        assert!(text.contains("[launch] after"), "{text}");
    }

    #[test]
    fn home_paths_are_scrubbed_on_write() {
        let _guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
        super::state_reset();
        let dir = temp_dir("scrub");
        super::set_dir(&dir);
        let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"));
        let Ok(home) = home else {
            return;
        };
        super::record("launch", &format!("argv --nativesDir {home}/natives"));
        let text = std::fs::read_to_string(dir.join(super::FILE_NAME)).expect("读日志");
        assert!(text.contains("<home>"), "{text}");
        assert!(!text.contains(home.as_str()), "家目录原文还在：{text}");
    }

    #[test]
    fn buffer_is_bounded_before_the_dir_is_known() {
        let _guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
        super::state_reset();
        for index in 0..(MAX_BUFFERED + 50) {
            super::record("boot", &format!("line {index}"));
        }
        let dir = temp_dir("bounded");
        super::set_dir(&dir);
        let text = std::fs::read_to_string(dir.join(super::FILE_NAME)).expect("读日志");
        let lines = text.lines().count();
        assert!(lines <= MAX_BUFFERED, "缓冲没有上限：{lines} 行");
        assert!(lines > 0);
    }

    /// M211/D5：GUI 与 `ull launch` 写**同一份** `launcher.log`，所以形状只能有一套。
    ///
    /// 变异证明：把 `start_line`/`end_line` 里的键名或顺序改掉（例如 `memory` → `mem`）⇒
    /// 本条的字符串相等断言红——这正是「两条路各自 `format!`」时会发生的事。
    #[test]
    fn launch_lines_have_one_shape_for_both_entries() {
        use std::path::Path;
        assert_eq!(
            super::start_line(
                "inst",
                Path::new("C:/mc"),
                Some((Path::new("C:/jdk/bin/java.exe"), 17)),
                Some(4096),
            ),
            "instance=inst game_dir=C:/mc java=C:/jdk/bin/java.exe (major 17) memory=4096MB"
        );
        assert_eq!(
            super::start_line("inst", Path::new("C:/mc"), None, None),
            "instance=inst game_dir=C:/mc java=未找到 memory=0MB"
        );
        assert_eq!(
            super::end_line("inst", "sid", 1, "application_error", 1234, "Crashed", 2),
            "instance=inst session=sid exit=1 type=application_error \
             uptime=1234ms outcome=Crashed artifacts=2"
        );
    }
}
