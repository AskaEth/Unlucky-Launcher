//! 启动期诊断行的**双写**：stderr + `<.ULL>/logs/launcher.log`（M188）。
//!
//! ## 为什么要有这一层
//!
//! 双击启动的 GUI 进程**没有 stderr 去向**（Windows 上根本没有控制台）。此前 `[font]` /
//! `[probe]` / `[theme]` / `[appearance]` / `[gpu]` 这些诊断只走 `eprintln!`，于是「启动器视角
//! 到底发生了什么」在事故之后一个字都不剩——用户投诉的「只有一个安装日志」有一半是它。
//!
//! 现在这些行同时进 `launch::launcher_log`：
//!
//! ```text
//! diag!("[font] 求值耗时 {:.0} ms", ms)   →  stderr: [font] 求值耗时 12 ms
//!                                          →  launcher.log: 2026-… [font] 求值耗时 12 ms
//! ```
//!
//! scope 就是行首那个 `[tag]`，所以日志行与老用法**长得一样**，`grep '\[gpu\]'` 仍然好使。
//!
//! ## 哪些行进日志、哪些只进 stderr
//!
//! 判据（M188 复审后收紧成这一句）：**逐帧度量不接；显式取证开关与一次性事故行可接。**
//!
//! | 进日志 | 只进 stderr |
//! |---|---|
//! | 启动编排（探测结论 / 字体重建 / 主题来源 / 后端最终值 / 降级重开） | **验收与出图矩阵的逐帧度量**：`[window]` / `[perf]` / `[caps]` / `[theme] source=`（`renderer::acceptance/**`，一次启动上百行） |
//! | **一次性事故行**：`[gpu] <backend> 初始化失败（具名）` / `[gpu] 失败详情（具名）`（`renderer::report_failure`）、`[gpu] iced_wgpu: …Selected: AdapterInfo…`（`log_bridge`）、`[probe] request=… -> ICED_BACKEND=…`（`probe::apply_with_preflight`）、`[gpu-preflight]`（`ULL_PREFLIGHT=1`） | 组件几何探针的断言输出（`components/**` 的 `[feedback-probe]` / gallery 页面里的 `[gallery]`） |
//! | 用户动作的结果（打开目录、写设置失败、更新忽略/应用、语言没落盘） | 每条日志行的正文里再套一层括号（不是标签，见 [`split_scope`]） |
//! | 显式开关下的一次性行：`[tick]`（`ULL_TICK_TRACE=1`，≈1 行/秒 ≈ 4.8 小时才滚一次）、`[shot]` 的通道提示 | — |
//!
//! 为什么逐帧行不许接：`launcher.log` 有 1 MiB 上限（滚成 `launcher.1.log`，只留一份），
//! 逐帧写会在几分钟内把「事故现场」冲掉。反过来，`report_failure` 那两行**就是**事故本体——
//! 双击启动没有 stderr，把它们留在 stderr 等于在最该有日志的路径上什么都没有（M188 复审 P2-1）。

/// 行首没有 `[tag]` 时的默认 scope。
pub const APP_SCOPE: &str = "app";

/// 记一行诊断：stderr 照旧，另外进启动器日志（best-effort，写不进去也不影响启动）。
pub(crate) fn emit(line: &str) {
    eprintln!("{line}");
    let (scope, message) = split_scope(line);
    unlucky_core::launch::launcher_log::record(scope, message);
}

/// `[font] 求值耗时 12 ms` → (`font`, `求值耗时 12 ms`)。
///
/// scope 只认「短、无空白、非空」的方括号前缀：`[a b]` 这种是正文而不是标签，别把整行拆坏。
pub(crate) fn split_scope(line: &str) -> (&str, &str) {
    let Some(rest) = line.strip_prefix('[') else {
        return (APP_SCOPE, line);
    };
    let Some((scope, tail)) = rest.split_once(']') else {
        return (APP_SCOPE, line);
    };
    if scope.is_empty() || scope.len() > 24 || scope.contains(char::is_whitespace) {
        return (APP_SCOPE, line);
    }
    (scope, tail.trim_start())
}

/// 双写一行诊断（`diag!("[scope] 正文")`，参数同 `eprintln!`）。
macro_rules! diag {
    ($($arg:tt)*) => {{
        crate::app::diag::emit(&format!($($arg)*));
    }};
}

pub(crate) use diag;

/// 串行化「会改 `launcher_log` 全局目录」的用例。
///
/// `core::launch::launcher_log` 是**进程级单例**（目录 + 缓冲），而 app-iced 的测试二进制里
/// 有两条用例会 `set_dir` 到自己的临时目录（本文件的滚动闸、`app/dialog/tests.rs` 的启动事实闸）。
/// 它们必须串行，否则互相把对方的落点改走——那是两条闸互相打架，不是产品问题。
#[cfg(test)]
pub(crate) static LOG_STATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::split_scope;

    #[test]
    fn the_leading_bracket_becomes_the_scope() {
        assert_eq!(
            split_scope("[font] 求值耗时 12 ms"),
            ("font", "求值耗时 12 ms")
        );
        assert_eq!(split_scope("[gpu]"), ("gpu", ""));
        // 正文里的方括号不是标签
        assert_eq!(
            split_scope("[page] 占位动作 [x] pressed"),
            ("page", "占位动作 [x] pressed")
        );
    }

    #[test]
    fn lines_without_a_tag_fall_back_to_the_default_scope() {
        assert_eq!(split_scope("没有标签的一行"), ("app", "没有标签的一行"));
        assert_eq!(
            split_scope("[a b] 空白不是标签"),
            ("app", "[a b] 空白不是标签")
        );
    }

    /// 守门②：**启动器日志的滚动真的会发生**，写超上限之后文件仍然有界。
    ///
    /// ## 为什么要「重试」
    ///
    /// `launcher_log` 是**进程级单例**（目录 + 缓冲），而同进程里还有一条用例
    /// （`app/dialog/tests.rs` 的 `boot_facts_are_recorded_into_the_launcher_log`）会
    /// `set_dir` 到它自己的临时目录。两边撞上时我这几行的落点就换了人，所以每一轮都要
    /// **从头跑一遍**（登记 → 造超限 → 追加 → 看有没有滚），并且用「我这份超限文件有没有被
    /// rename」判断这一轮是不是真的跑在我目录上。重试不是放宽断言：每轮该发生的都得真发生。
    ///
    /// 变异证明：删掉 `core::launch::launcher_log::append` 里那段「超限就 rename 成 launcher.1.log」，
    /// 每一轮都不会出现 `launcher.1.log`，本条在重试用尽后红。
    #[test]
    fn the_launcher_log_rolls_over_and_stays_bounded() {
        use unlucky_core::launch::launcher_log;

        let _guard = super::LOG_STATE_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());

        let dir = std::env::temp_dir().join(format!("ull-launcher-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建夹具目录");
        let path = dir.join(launcher_log::FILE_NAME);
        let rolled = dir.join("launcher.1.log");
        let oversize = usize::try_from(launcher_log::MAX_BYTES).expect("上限能塞进 usize") + 16;

        for _ in 0..50 {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(&rolled);
            launcher_log::set_dir(&dir);
            if launcher_log::path() != Some(path.clone()) {
                continue; // 这一轮被别人的 set_dir 抢先了
            }
            launcher_log::record("boot", "第一行");
            std::fs::write(&path, vec![b'x'; oversize]).expect("造一份超限日志");
            if launcher_log::path() != Some(path.clone()) {
                continue;
            }
            launcher_log::record("boot", "滚过之后这一行必须在新文件里");
            if launcher_log::path() != Some(path.clone()) {
                continue; // 追加落到了别人的目录，这一轮不算
            }

            // 到这儿为止，追加**确实**落在本目录：超限就必须滚。
            assert!(rolled.is_file(), "超限之后旧档必须滚成 launcher.1.log");
            let size = std::fs::metadata(&path).expect("新文件").len();
            assert!(
                size < launcher_log::MAX_BYTES,
                "滚完之后新文件必须远小于上限：{size}"
            );
            let text = std::fs::read_to_string(&path).expect("读新文件");
            assert!(text.contains("滚过之后这一行"), "{text}");

            // 只留一份历史：再滚一次仍然是 `launcher.1.log`，不会长出 launcher.2.log。
            std::fs::write(&path, vec![b'y'; oversize]).expect("再造一份超限日志");
            launcher_log::record("boot", "第二次滚动");
            assert!(rolled.is_file());
            assert!(!dir.join("launcher.2.log").exists(), "历史只留一份");
            assert!(std::fs::metadata(&path).expect("新文件").len() < launcher_log::MAX_BYTES);

            std::fs::remove_dir_all(&dir).ok();
            return;
        }
        panic!("50 轮都撞上别的用例在改全局日志目录，这一条没能真正跑起来");
    }
}
