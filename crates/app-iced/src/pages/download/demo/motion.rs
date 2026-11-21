//! **列表增删动效的中间态**取样点（M305）：`ULL_DL_DEMO=list-enter|list-exit`。
//! **进度插值 / 收束脉冲**取样点（M386）：`ULL_DL_DEMO=progress-mid|progress-pulse`。
//!
//! 从 `demo.rs` 拆出来（那边正好 397 行、零余量）：本文件只管「制造一次真实的变化」。
//!
//! ## 两件事都走产品路径
//!
//! 1. 模型由 [`Download::from_snapshot`] 建（分类过滤、退场池都在里面）；
//! 2. 差分由泵真做（`observe_rows` 比对两次观察 / `observe_progress` 比对两次读数），
//!    **不是**硬塞一个 progress 值。
//!
//! ## 唯一的「演示特权」是顺序与定档
//!
//! 产品里两次观察隔着一条事件；这里在同一个 view 里连做两次（`Once` 只做一次，否则每帧重摆会把
//! 轨道重起）。相位由 `ULL_MOTION_AT` 钉死 ⇒ 同一张图每次都一样（D30）。
//!
//! 为什么定档（`motion::configure`）在这一层：截图走的是 `pages/harness.rs` 那条独立 app，
//! 它**不建 `App`**，也就不会经过 `app/update.rs` 的定档；而 harness 不在本 mission 的 scope 里。
//! 给了 `ULL_MOTION_AT` 才开动效，没给就保持静息——与产品截图通道「不钉相位就是静帧」同一条纪律。
//!
//! ## M386：进度那两格为什么必须走两次 `from_snapshot`
//!
//! 进度条唯一的绘制点是 `pages/download/install.rs` 的 `motion::progress::frame(key, ratio)`，而它
//! 只在泵里**已有一条对得上的轨道**时才回插值（否则**故意**回目标值，见 `pump/progress.rs` 判据 1）。
//! 轨道只由 `observe_progress` 起、唯一调用点是 `Download::from_snapshot` ⇒ 「停在两个值之间」
//! 这件事只能靠**两次真读数**制造，不能靠塞一个中间值（那是造假图）。

use unlucky_design::motion::MotionScale;
use unlucky_i18n::Catalog;
use unlucky_presenter::dto::{AppEvent, CoreEvent, InstallProgressView, InstallStage};
use unlucky_presenter::state::AppState;

use crate::app::components::motion;
use crate::pages::download::model::{Category, Download};
use crate::pages::download::options::InstallDraft;

use super::demo_versions;

/// 出图通道的定档：`ULL_MOTION_AT` 给了相位才开动效，否则保持静息（D30）。
///
/// harness 不建 `App`，这一档只能由演示模型自己钉（见模块文档）。两个取样点共用同一段，
/// 免得「两个夹具两套旋钮语义」。
fn pin_phase_from_env() {
    let pin = std::env::var("ULL_MOTION_AT")
        .ok()
        .and_then(|raw| raw.trim().parse::<f64>().ok());
    if pin.is_some() {
        motion::configure(MotionScale::STANDARD, pin);
    }
}

/// 演一次「分类从 A 换到 B」的列表增删，返回**换完之后**的模型（退场池已填好）。
pub(super) fn list_motion(catalog: Catalog, entering: bool) -> Download {
    static ONCE: std::sync::Once = std::sync::Once::new();

    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(
        demo_versions(),
    )));
    let draft = InstallDraft::default();
    ONCE.call_once(|| {
        // 出图通道的定档：harness 不建 `App`，所以这一档只能由演示模型自己钉。
        pin_phase_from_env();
        // 用「远古版 ⇄ 全部」而不是「正式版 ⇄ 全部」：远古版只有两行、落在列表**最前面**，
        // 于是进去的 6 行错位档是 0/1/2/3/3/3（`list-enter` 图里能一眼看出「错位」）；
        // 反过来退的那 6 行同理。
        let (before, after) = if entering {
            (Category::Ancient, Category::All)
        } else {
            (Category::All, Category::Ancient)
        };
        // 两次真转换 = 两次真观察（第二次的差分就是这一次增删）。
        let _ = Download::from_snapshot(&state, before, &draft, catalog);
        let _ = Download::from_snapshot(&state, after, &draft, catalog);
    });

    let category = if entering {
        Category::All
    } else {
        Category::Ancient
    };
    Download::from_snapshot(&state, category, &draft, catalog)
}

/// 演一次「进度从 `from` 跳到 `to`」，返回**跳完之后**的模型（泵里留下一条 `from → to` 的轨道）。
///
/// 两次 `from_snapshot` = 两次真读数：第一次认条（`shown = from`、不起轨道），第二次起一腿 200ms
/// 的追值轨道（`short4`）。之后每一帧的 `frame(key, to)` 都采**同一个相位**——因为时钟被
/// `ULL_MOTION_AT` 钉住（`start_now()` 返 0、`elapsed_ms()` 返 pin）。
///
/// ## 相位 → 画出来的值（`probe.py` 量的就是它）
///
/// `value = from + (to − from) · Standard(pin / 200)`，`Standard` = cubic-bezier(0.2, 0, 0, 1)。
/// 所以 `progress-mid`（0.20 → 0.60）配 `ULL_MOTION_AT=0` 画 0.200、`=100` 画 ≈0.551、
/// `=200` 画 0.600：**pin=100 那一帧停在两个值之间**，且「凭什么说是 100ms」写在运行日志里
/// （`[motion] progress … phase=100ms value=…`，见 `components/motion/progress.rs`）。
///
/// `progress-pulse`（0.60 → 1.00）配 `ULL_MOTION_AT=275`：轨道在 200ms 到达，脉冲（`short3`
/// 150ms）的顶点就在 275ms ⇒ `settle=1.0`，条走到满格且强调色被推到 `InversePrimary`。
pub(super) fn progress_motion(catalog: Catalog, from: f32, to: f32) -> Download {
    static ONCE: std::sync::Once = std::sync::Once::new();

    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(
        demo_versions(),
    )));
    let draft = InstallDraft::default();
    ONCE.call_once(|| {
        pin_phase_from_env();
        state.install = Some(install_progress(from));
        let _ = Download::from_snapshot(&state, Category::All, &draft, catalog);
        state.install = Some(install_progress(to));
        let _ = Download::from_snapshot(&state, Category::All, &draft, catalog);
    });

    // 返回的模型要停在 `to`：视图报的 `ratio` 与泵记的目标一致，`frame()` 才会采轨道
    // （不一致时它故意回目标值，见 `pump/progress.rs` 判据 1）。
    state.install = Some(install_progress(to));
    Download::from_snapshot(&state, Category::All, &draft, catalog)
}

/// 一次「正在下载」的进度快照（**计数**给整，`ratio` 精确等于 `files_done / files_total`）。
///
/// 512 那一份在 `demo.rs::install` 里；这里用 500 是因为本取样点的 `ratio` 要能进日志与像素判据
/// 对得上（`100/500`、`300/500`、`500/500` 都是有限位小数）。
fn install_progress(ratio: f32) -> InstallProgressView {
    let total: u32 = 500;
    let done = (f64::from(ratio) * f64::from(total)).round() as u64;
    InstallProgressView {
        version: "1.21.9".to_string(),
        instance: "1.21.9".to_string(),
        loader: "Vanilla".to_string(),
        stage: InstallStage::Downloading,
        files_done: done,
        files_total: u64::from(total),
        bytes_done: 0,
        bytes_total: 0,
        speed_bps: 1_258_291,
        eta_secs: 75,
        message: String::new(),
        failure_kind: None,
        retry: Vec::new(),
    }
}
