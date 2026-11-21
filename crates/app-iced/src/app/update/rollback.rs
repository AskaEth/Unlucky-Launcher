//! 「新版起不来就退回上一版」在壳层的接线（M309）：启动对账 + 首帧确认 + 用户可见文案。
//!
//! 判据与搬运全在 `unlucky_core::update::rollback`（标记/阈值/guard/一次性凭证的来龙去脉见
//! 那边的模块文档）。本模块只做三件壳层才做得对的事：
//!
//! | 件事 | 为什么只能是壳层 |
//! |---|---|
//! | **落点** | `<home>/config/launcher.json` → `<home>` → `LauncherLayout` 的 `cache/` 分区。本进程实际生效的数据根只有设置桥知道（`--data-dir` 也在那儿生效），与 `logs_folder::register` 同一条形状 |
//! | **时机** | 启动对账要在 `build_backend` **之前**（渲染器/后端探测是最常见的启动崩溃点；排在后面则 `trial` 永远落不了地，超窗口判定不可达），首帧确认要等界面真的画出来 |
//! | **文案** | presenter 不依赖 `unlucky-i18n`（同 `InstallCancelled` 那条纪律），所以「刚才那次升级没能启动」只能在这里按 `Key` 现求 |
//!
//! # 「首次成功」的判据
//!
//! **首帧真的画出来了**：`frames > 0`（至少一条 `window::frames()` 帧事件走完）**且** `ticks > 0`
//! （至少一拍 `advance` 走完）。比「事件循环起来了」严：一个能进循环、一画界面就崩的坏版本不会
//! 被判成成功，它留下的标记过期后仍会被退回。
//!
//! **已知边界（r1 复审核过 `iced_winit` 的 `RedrawRequested` 顺序）**：`frames > 0` 只证到「重绘
//! 已开始 + `view()` 成功返回过一次」，**证不到「这一帧画完并提交了」**——「界面构建得出来、绘制/
//! 提交那一拍才崩」的构建仍会被判成成功。要再严一档得在 iced 的绘制回调里拿「帧已提交」的证据，
//! 那在壳层之外（`app/view.rs` 与 iced 运行时之间），本 mission 不动。
//!
//! 判据比用户手速早得多——用户看到窗口、点关闭，至少已经是几十帧之后的事。所以「升级后启动、
//! 看一眼、一分钟内正常关掉」不会被下一次启动误判成「起不来」：标记在第一帧就走了。正常重启
//! （[`super::update_action`] 的「立即重启」）另有一处显式确认，道理相同。
//!
//! # 未登记 = 整条机制不启动
//!
//! [`arm`] 只在生产构造（`App::new`）里调一次；单测与出图通道走 `App::with_presenter`，
//! 那里从未登记 ⇒ [`reconcile_before_boot`] / [`confirm_after_first_frame`] 全是空操作，
//! 一个盘文件都不碰（与 `logs_folder` 的 `OnceLock<Option<PathBuf>>` 同一条形状）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use unlucky_core::platform::layout::LauncherLayout;
use unlucky_core::update::rollback::{
    confirm, reconcile, Action, Notice, NoticeKind, RollbackCause,
};
use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::toast::ToastKind;

use crate::app::diag::diag;

use super::super::state::App;

/// 本进程的更新域落点（`<home>/cache`）。`None` = 没登记 / 拿不到数据根 ⇒ 整条机制不启动。
static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();

/// 已经确认过一次（只做一次：确认是「清标记 + 删旧备份」，没必要每拍都去 stat 一遍盘）。
static CONFIRMED: AtomicBool = AtomicBool::new(false);

/// 登记本进程的更新域落点（`App::new` 在第一帧之前调一次；重复调用以**第一次**为准）。
///
/// `config` 是设置桥的落点 `<home>/config/launcher.json`；`home` 就是 `<.ULL>`（或用户在
/// `--data-dir` 下钉的那个），与 backend / 日志目录用的是同一份解析结果——本模块不自己再解析
/// 一次三模式数据根（那会有两份真相）。
pub(in crate::app) fn arm(config: Option<&Path>) {
    let _ = CACHE.set(config.and_then(cache_of));
}

/// `<home>/config/launcher.json` → `<home>` → `cache` 分区（只有这一条形状）。
fn cache_of(config: &Path) -> Option<PathBuf> {
    let home = config.parent()?.parent()?;
    Some(LauncherLayout::new(home).cache())
}

fn armed() -> Option<&'static Path> {
    CACHE.get()?.as_deref()
}

/// 启动对账：该回滚就回滚，并**返回**要告诉用户的结论（没有标记时是 `None`）。
///
/// 刻意不在这一拍里碰 `App`：它必须在 [`build_backend`](unlucky_presenter::backend::build_backend)
/// **之前**跑（`App` 还不存在）。理由不是「早点更好」，而是**判据可达性**：`trial.started_ms`
/// 只在 `reconcile` 落地，一个在「建后端 / 装配 App」里就 panic 的坏版本，如果对账排在后面，
/// 每次启动都到不了它 ⇒ `trial` 恒 `None` ⇒ 超窗口判定永远不成立 ⇒ 无限 crash loop 且一次
/// 自动回滚都不会发生。拿到结论之后再建后端，建好后用 [`announce`] 说出去。
pub(in crate::app) fn reconcile_before_boot() -> Option<Notice> {
    let cache = armed()?.to_path_buf();
    let outcome = reconcile(&cache, version(), now_ms());
    if outcome.action != Action::Nothing {
        diag!("[update] 启动对账：{:?}", outcome.action);
    }
    outcome.notice
}

/// 把 [`reconcile_before_boot`] 的结论说给用户（`App` 建好之后叫；`None` = 一句废话都不说）。
pub(in crate::app) fn announce(app: &mut App, notice: Option<Notice>) {
    if let Some(notice) = notice {
        say(app, &notice);
    }
}

/// 对账 + 立刻说的合成版（**单测直达**：落点与时刻都由调用方给，不经过那条 `OnceLock`）。
#[cfg(test)]
pub(in crate::app) fn reconcile_with(app: &mut App, cache: &Path, at_ms: u64) {
    let outcome = reconcile(cache, version(), at_ms);
    announce(app, outcome.notice);
}

/// 一拍走完：首帧真的画出来了 ⇒ 这一版「进得了主界面」，清标记 + 删旧备份。
pub(in crate::app) fn confirm_after_first_frame(app: &App) {
    let Some(cache) = armed() else {
        return;
    };
    if !first_frame_proven(app.frames, app.ticks) || CONFIRMED.swap(true, Ordering::Relaxed) {
        return;
    }
    note(confirm(cache, version()));
}

/// 正常退出 / 重启前显式确认一次（程序自己决定关掉 = 用户确实看到过这个界面）。
pub(in crate::app) fn confirm_on_graceful_exit() {
    let Some(cache) = armed() else {
        return;
    };
    if CONFIRMED.swap(true, Ordering::Relaxed) {
        return;
    }
    note(confirm(cache, version()));
}

fn note(confirmed: unlucky_core::update::rollback::Confirmed) {
    if confirmed.marker_cleared {
        diag!(
            "[update] 这一版进得了主界面：清掉升级尝试标记（旧备份已删={}）",
            confirmed.backup_removed
        );
    }
}

/// 正常退出也算这一版起得来（M309）。
///
/// 「窗口 ×（`Message::Close`）→ `iced::exit()`」那条落点在 `app/update.rs` 与 `app/boot.rs`，
/// 不在本 mission 的 scope 里；但判据只有一条——**程序自己决定退出**（不崩、不被杀）。挂在
/// `Drop` 上正好覆盖所有这类离场：`iced::run` 的闭包返回后状态被丢弃，这一拍就跑。
///
/// 三条边界：
///
/// * **崩溃不算**：`std::thread::panicking()` 为真时跳过；硬杀 / 断电根本不会有 `Drop`。
///   这两种离场留下的标记原样保留——那正是本机制要抓的「起不来」；
/// * **只对得上正在跑的这一版**：`confirm` 内部比对版本号，所以「装上更新、没重启就正常关掉」
///   不会误清新版本的标记（新版本还没被跑过一次）；
/// * 与首帧确认共用 `CONFIRMED`：正常路径上首帧那一拍早就做完了，这里是空操作。
impl Drop for App {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            confirm_on_graceful_exit();
        }
    }
}

/// 「首帧真的画出来了」的判据（纯函数，测试直接打这里）。
///
/// `frames > 0`：至少一条帧事件走完 ⇒ `view()` 构建 + 绘制都成功过；`ticks > 0`：至少一拍
/// `advance` 走完（判据落在「第一拍之后」，避开「刚进循环就崩」那一档）。
pub(in crate::app) const fn first_frame_proven(frames: u32, ticks: u64) -> bool {
    frames > 0 && ticks > 0
}

/// 一条结论 → （提示级别，人读文案）。文案全从 i18n 目录现求，一个中文串都不硬编码。
pub(in crate::app) fn line(notice: &Notice, catalog: Catalog) -> (ToastKind, String) {
    let reason = match notice.cause {
        RollbackCause::Crash => catalog.text(Key::UpdateRollbackReasonCrash),
        RollbackCause::Timeout => catalog.text(Key::UpdateRollbackReasonTimeout),
    };
    match notice.kind {
        NoticeKind::RolledBack => (
            ToastKind::Warn,
            catalog.render(
                Key::UpdateRollbackToast,
                &[
                    ("version", notice.to.as_str()),
                    ("from", notice.from.as_str()),
                    ("reason", reason),
                ],
            ),
        ),
        NoticeKind::GaveUp => (
            ToastKind::Warn,
            catalog.render(
                Key::UpdateRollbackGaveUp,
                &[("version", notice.to.as_str())],
            ),
        ),
        // 搬不动时把 core 的原文一起端出来：那是唯一能说明「为什么没搬动」的东西。
        NoticeKind::RollbackFailed => (
            ToastKind::Error,
            catalog.render(
                Key::UpdateRollbackFailed,
                &[("reason", notice.detail.as_str())],
            ),
        ),
    }
}

fn say(app: &mut App, notice: &Notice) {
    let (kind, text) = line(notice, app.lang().catalog());
    app.state.toasts.push(kind, text);
}

/// 本进程版本。整个 workspace 同版本（根 `Cargo.toml` 的 `[workspace.package]`），与 presenter
/// 的 `current_version()` 同一个常量来源。
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 当前毫秒（since UNIX_EPOCH；标记与窗口判定用）。
fn now_ms() -> u64 {
    unlucky_core::launch::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    //! 壳层这条接线的判据：文案跟着语言走、对账把结论送到 toast、首帧判据不早不晚。
    //!
    //! 起不来的**真进程**不在单测范围（见报告「没验的部分」）。

    use super::*;
    use crate::theme::Families;
    use unlucky_design::color::SchemeFlavor;
    use unlucky_i18n::Lang;
    use unlucky_presenter::mock::MockBackend;
    use unlucky_presenter::settings_bridge::SettingsBridge;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    fn app() -> App {
        App::with_presenter(
            crate::cli::Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(MockBackend::new()),
            SettingsBridge::memory_only(),
        )
    }

    fn notice(kind: NoticeKind, cause: RollbackCause) -> Notice {
        Notice {
            kind,
            from: "0.2.2".to_string(),
            to: "0.2.3".to_string(),
            cause,
            detail: String::new(),
        }
    }

    /// 回滚文案要把三件事说全：没启动的是哪个版本、退回的是哪个、原因是什么（中英各一遍）。
    #[test]
    fn the_rollback_toast_names_both_versions_and_the_reason() {
        for lang in Lang::ALL {
            let catalog = lang.catalog();
            let (kind, text) = line(
                &notice(NoticeKind::RolledBack, RollbackCause::Crash),
                catalog,
            );
            assert_eq!(kind, ToastKind::Warn);
            for part in [
                "0.2.3",
                "0.2.2",
                catalog.text(Key::UpdateRollbackReasonCrash),
            ] {
                assert!(text.contains(part), "{} 缺 {part}：{text}", lang.tag());
            }
            let (_, timed_out) = line(
                &notice(NoticeKind::RolledBack, RollbackCause::Timeout),
                catalog,
            );
            assert!(
                timed_out.contains(catalog.text(Key::UpdateRollbackReasonTimeout)),
                "超时那条要换原因：{timed_out}"
            );
            assert_ne!(text, timed_out, "两种原因不许说成同一句话");
        }
    }

    /// 首帧判据：事件循环刚起来（`frames == 0`）**不算**成功；一帧画出来才算。
    #[test]
    fn the_success_evidence_needs_a_rendered_frame() {
        assert!(!first_frame_proven(0, 0), "什么都没画过，不许判成功");
        assert!(!first_frame_proven(0, 5), "只有拍、没有帧 ⇒ 界面还没画出来");
        assert!(
            !first_frame_proven(1, 0),
            "帧到了但一拍都没走完（判据落在第一拍之后）"
        );
        assert!(first_frame_proven(1, 1), "一帧 + 一拍 ⇒ 这一版进得了主界面");
    }

    /// **判据可达性守卫**：对账必须排在 `build_backend` **之前**（源码级）。
    ///
    /// 抓得住：把两行调换（r1 复审 P1-1 的那个缺陷）——`trial.started_ms` 只在 `reconcile` 里落地，
    /// 排在后面的对账在「建后端就 panic」的版本上永远跑不到 ⇒ `trial` 恒 `None` ⇒ 超窗口判定
    /// 不可达 ⇒ 无限 crash loop。这类缺陷运行期看不见（不崩就没区别），只能守源码顺序。
    /// 抓不住：`reconcile_before_boot` 的实现被掏空（那是 core 侧用例的事）。
    #[test]
    fn the_reconcile_runs_before_the_backend_is_built() {
        let source = include_str!("../state/construct.rs");
        let reconcile = source
            .find("rollback::reconcile_before_boot")
            .expect("construct.rs 里必须调对账");
        let backend = source
            .find("build_backend(")
            .expect("construct.rs 里必建后端");
        assert!(
            reconcile < backend,
            "对账必须早于 build_backend（否则在构建后端里崩的版本永远到不了对账 ⇒ 永不回滚）"
        );
    }

    /// `GaveUp` 与 `RollbackFailed` 两条文案分支各自说得清（r1 复审 P2-1 指出的空洞）。
    #[test]
    fn the_other_two_outcomes_say_what_happened() {
        for lang in Lang::ALL {
            let catalog = lang.catalog();
            let (kind, gave_up) = line(&notice(NoticeKind::GaveUp, RollbackCause::Crash), catalog);
            assert_eq!(kind, ToastKind::Warn);
            let tail = catalog
                .text(Key::UpdateRollbackGaveUp)
                .split("{version}")
                .nth(1)
                .unwrap_or_default();
            assert!(
                gave_up.contains("0.2.3") && gave_up.contains(tail),
                "{} 要说清是哪一版不再自动回退，且用 GaveUp 那条 key：{gave_up}",
                lang.tag()
            );

            let mut failed = notice(NoticeKind::RollbackFailed, RollbackCause::Timeout);
            failed.detail = "注入：第 1 次改名被拒".to_string();
            let (kind, text) = line(&failed, catalog);
            assert_eq!(kind, ToastKind::Error, "搬不动是错误档，不是警告档");
            assert!(
                text.contains("注入：第 1 次改名被拒"),
                "{} 必须把 core 的原因原样端出来：{text}",
                lang.tag()
            );
        }
    }

    /// 对账把结论送进 toast 中了（不是一个只写日志的空壳）：正常启动途中一句不说，
    /// 超窗口回滚时说一句、且把两个版本都点出来。
    #[test]
    fn a_rolled_back_start_says_it_out_loud() {
        let tmp = std::env::temp_dir().join(format!("ull-m309-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let cache = tmp.join("cache");
        let site = tmp.join("install");
        std::fs::create_dir_all(&site).expect("建临时目录");
        let dest = site.join("launcher.exe");
        let backup = site.join(".launcher.exe.ullold-1-0");
        std::fs::write(&dest, b"new").expect("写新版");
        std::fs::write(&backup, b"old").expect("写旧版");
        let receipt = unlucky_core::update::install::InstallReceipt {
            dest: dest.clone(),
            backup: Some(backup),
            bytes: 3,
        };
        unlucky_core::update::rollback::arm(receipt, &cache, "0.2.2", version(), 1_000)
            .expect("落标记");

        let mut app = app();
        // 第一次启动：只登记，不回滚，也不弹任何东西。
        reconcile_with(&mut app, &cache, 2_000);
        assert!(app.state.toasts.is_empty(), "正常启动途中不该有提示");
        assert!(!app.state.update.applying, "更新域状态不该被这条机制动到");

        // 过了窗口再启动：回滚 + 一条提示。
        let expired = 2_000 + unlucky_core::update::rollback::TRIAL_DEADLINE_MS;
        reconcile_with(&mut app, &cache, expired);
        assert_eq!(
            std::fs::read(&dest).expect("读目标"),
            b"old",
            "旧版必须回到目标路径上"
        );
        let texts: Vec<&str> = app
            .state
            .toasts
            .items()
            .iter()
            .map(|toast| toast.text.as_str())
            .collect();
        assert_eq!(texts.len(), 1, "回滚要说一句，且只说一句：{texts:?}");
        assert!(
            texts[0].contains(version()) && texts[0].contains("0.2.2"),
            "文案要把两个版本都说出来：{}",
            texts[0]
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
