//! 窗口**定位**（M372）：把窗口居中放进显示器可用区。纯函数、无 IO。
//!
//! ## 为什么单独一份（M386 从 `fit.rs` 纯搬移）
//!
//! `fit.rs` 当时 397 行、余量 3；I7 要加第五档日志码 + 一条负起点判据，加不进去。这里搬的是
//! **一整段**（矩形类型 + 四档 `reason` 常量 + 居中算式 + 日志行），`fit.rs` 用 `pub use` 保持
//! `acceptance::fit::<名字>` 这条路不变 —— 所以 `app/boot/window_fit` 一个字都不用跟着改。
//!
//! ## 算式（可复核）
//!
//! ```text
//! 居中位 = 可用区起点 + (可用区尺寸 − 窗口尺寸) ÷ 2      ← 逐维；越界推回边界
//! ```
//!
//! 为什么居中而不是「越界才推回」：级联位由系统计数器驱动，每次启动都不同，「夹进可用区」会让
//! 位置随上一次启动漂移、取证无法逐字节比较；居中只依赖 (窗口尺寸, 可用区)。反例与取舍见
//! `DevDocs/artifacts/winpos/REPORT.md`。
//!
//! **起点可以是负数**（左置副屏：Windows 的 `rcWork` 在 `x < 0` 一侧，本机实测 `-1920,0`）：
//! 判据只要求「待在**这块**矩形里」，不要求「待在主屏里」——把窗口从负起点拉回 0 就是把用户
//! 拖到另一块屏上（I7-a 的用例钉这一条）。
//!
//! ## `reason=` 的取值集合（M386 之后：**五个定位结论 + 两个 skipped**）
//!
//! `[window] place` 这一行的 `reason=` 一共七个取值，分两组；**同一行只出现一个**，别把两组合并读：
//!
//! ### ① 定位结论（五档，本文件的判据 + `window_fit` 的接线）
//!
//! | 值 | 谁给 | 含义 |
//! |---|---|---|
//! | `centered` | 本文件的 [`place_in_work_area`] 算出坐标 | 真去定位了（`move_to` 与这行**同源**） |
//! | `no-window` | `app/boot/window_fit::task` 的 `window::latest() == None` | **窗口还没建出来**（正常第一拍） |
//! | `unknown-area` | 本文件（有物理矩形却没算出坐标） | 矩形 / 折算不合法（`usable` 判否） |
//! | `probe-failed` | `app/boot/window_fit::place_reason` | 本平台**有**探测（Windows）却这次没拿到矩形 |
//! | `no-work-area` | 同上 | 本平台**没有**可用区矩形（macOS / Linux） |
//!
//! `no-window` 是 M386 新开的一档：这一格以前被归成 `unknown-area`，读日志的人分不清「窗口还没
//! 建出来」与「矩形算不出来」——前者不是 bug（第一拍就是这样），后者是布局 / 句柄 bug。
//!
//! ### ② 采集被跳过（两档，**根本不进定位判据**）
//!
//! `skipped-capture`（取证启动 `--screenshot`）/ `skipped-source`（`ULL_WINDOW_SIZE` 或
//! `termux-screen` 这类显式尺寸档）由 `fit.rs::fit_skip_reason` 给，那一支连显示器都不问就返回，
//! 所以它**同时**写到 `[window] fit` 与 `[window] place` 两行上，两行的 `reason=` 这时是同一个值。
//! 见到 `skipped-*` 就不必再找定位结论了——这一跑压根没打算定位。

/// 可用区在**逻辑**坐标下的矩形：`((x, y), (宽, 高))`。定位（M372）要起点，拿在手上的是物理矩形的
/// 调用方（`platform::win32::work_area`）负责按同一个折算因子把起点搬过来；[`fit_to_work_area`] 的
/// 入参仍是 `(宽, 高)`，一个字没改。
///
/// [`fit_to_work_area`]: super::fit::fit_to_work_area
pub type WorkRect = ((f32, f32), (f32, f32));

/// [`place_in_work_area`] 的结论取值（进日志 `[window] place … reason=`）：`centered` = 真去定位了，
/// 其余四档都是「没定位」及其原因，必须能分开读（取值集合见模块文档那张表）。
pub const PLACE_CENTERED: &str = "centered";
/// 本平台**没有**可用区矩形（macOS / Linux）：保持平台默认位，不编坐标。
pub const PLACE_NO_WORK_AREA: &str = "no-work-area";
/// 本平台**有**探测（Windows）但这次没拿到矩形（布局 / 句柄 bug）。
pub const PLACE_PROBE_FAILED: &str = "probe-failed";
/// 有物理矩形，但可用区尺寸或显示器逻辑尺寸不合法（见 `fit.rs` 的 `usable`）。
pub const PLACE_UNKNOWN_AREA: &str = "unknown-area";
/// **窗口还没建出来**（`window::latest()` 给 `None`）：与 [`PLACE_UNKNOWN_AREA`] 必须分开读——
/// 前者是正常的第一拍，后者是矩形 / 折算算不出来（M386 / I7-b）。
pub const PLACE_NO_WINDOW: &str = "no-window";

/// 把窗口**居中**放进 `work`（全部**逻辑 px**）。纯函数：无 IO、无环境、可单测。
///
/// 逐维算 `起点 + (可用区 − 窗口) ÷ 2`，再夹回 `[起点, 起点 + 可用区 − 窗口]`；窗口比可用区还大
/// （`fit` 之后理论上不会发生）⇒ 钉在起点，**不许算出屏外的负坐标**。`work` 或 `size` 不可用
/// （非有限 / ≤0）⇒ `None`：非 Windows 上根本没有矩形，调用方据此保持平台默认位（**不编坐标**）。
///
/// 起点是负数（左置副屏）**不是**不可用：判据是「有限」，不是「非负」。
#[must_use]
pub fn place_in_work_area(size: (f32, f32), work: WorkRect) -> Option<(f32, f32)> {
    let ((left, top), (width, height)) = work;
    let ok = [size.0, size.1, width, height]
        .into_iter()
        .all(super::fit::usable)
        && left.is_finite()
        && top.is_finite();
    ok.then(|| (centered(left, width, size.0), centered(top, height, size.1)))
}

/// 一维的居中 + 边界夹取（`start` = 可用区起点，`span` = 可用区尺寸，`len` = 窗口尺寸）。
///
/// 取整到整逻辑 px：winit 把逻辑坐标乘 scale 换成物理 px，非整数逻辑坐标在 125% / 150% 上会算出
/// 半个物理像素（观感是 1 px 抖动）。
fn centered(start: f32, span: f32, len: f32) -> f32 {
    let max = start + span - len;
    if max <= start {
        return start.round();
    }
    (start + (span - len) / 2.0).clamp(start, max).round()
}

/// 见 [`report_place`]：一行写清「定位到哪 + 依据哪块矩形 + 为什么」。`work` / `pos` 为 `None` 时写
/// `<none>`，与 `fit_line` 同一套读法：**「没定位」与「定位到 (0,0)」必须能分开读**。`work_rect`
/// 写成 `x,y+宽x高`（起点与尺寸一起）——副屏的起点是 1920 这种非零值（左置副屏还会是负数），
/// 丢了起点的日志读不出居中在哪。
#[must_use]
pub fn place_line(
    size: (f32, f32),
    work: Option<WorkRect>,
    pos: Option<(f32, f32)>,
    reason: &str,
) -> String {
    let rect = work.map_or_else(
        || "<none>".to_owned(),
        |((x, y), (width, height))| format!("{x},{y}+{width}x{height}"),
    );
    let at = pos.map_or_else(|| "<none>".to_owned(), |(x, y)| format!("{x},{y}"));
    format!(
        "[window] place size={} work_rect={rect} pos={at} reason={reason}",
        super::fit::dim(size)
    )
}

/// `[window] place`：窗口定位怎么定的（**与真窗口同源**：调用方拿同一个坐标去 `move_to`）。
pub fn report_place(
    size: (f32, f32),
    work: Option<WorkRect>,
    pos: Option<(f32, f32)>,
    reason: &str,
) {
    eprintln!("{}", place_line(size, work, pos, reason));
}
