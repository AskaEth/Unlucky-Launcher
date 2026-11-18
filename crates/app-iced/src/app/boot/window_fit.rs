//! 开窗**之后**把尺寸收进显示器可用区（M362 的接线，M371 换成真工作区）。
//!
//! ## 为什么在开窗之后（不是开窗之前）
//!
//! iced 0.14 拿显示器尺寸只有一条路：[`iced::window::monitor_size`]，**入参是窗口 `Id`**
//! （`iced_runtime-0.14.0/src/window.rs:503-508` → `Action::GetMonitorSize` →
//! `iced_winit-0.14.1/src/lib.rs:1665-1677`）。没有窗口就没有 `Id`，因此**开窗前问不到显示器**。
//! M371 要读 `MONITORINFO::rcWork` 更需要窗口：`MonitorFromWindow` 要句柄，而句柄只能从
//! `window::run(id, …)` 的回调里取（`iced::window::Window` 只实现 `HasWindowHandle`）。
//!
//! 替代路都查过了，都是死路（源码依据见 `DevDocs/artifacts/winshrink/REPORT.md`）：
//! 自己开 winit `EventLoop` 取 `primary_monitor()` 会撞上「事件循环进程级一次性」
//! （`winit-0.30.13/src/event_loop.rs:115-119` → `RecreationAttempt`，而 iced 自己在
//! `iced_winit-0.14.1/src/lib.rs:78-81` 建它）；`Settings::visible=false` 是单程票（运行时没有
//! show/visible 动作，`iced_runtime-0.14.0/src/window.rs` 的 `Action` 里没有 `SetVisible`）。
//!
//! ## 三步链（M371 版，M372 在后面补了定位）
//!
//! ```text
//! window::latest          → Id
//! window::run(id, probe)  → 窗口所在显示器的 (rcWork, rcMonitor)，**物理** px；非 Windows ⇒ None
//! window::monitor_size    → 显示器全尺寸，**逻辑** px（物理→逻辑的折算基准，见下）
//! 纯函数 fit_to_work_area → 尺寸 + 最小尺寸 + 结论
//! 纯函数 place_in_work_area → 位置（居中于可用区；矩形没有就不定位）
//! set_min_size 然后 resize 然后 move_to
//! ```
//!
//! 为什么第三步不能省：`rcWork` 是物理 px，而判据吃逻辑 px，折算基准取 iced 自己的逻辑值
//! （`work = monitor_logical × rcWork / rcMonitor`）。为什么不用 `window::scale_factor`：
//! 它返回 winit 的**原始** scale，而 `resize` 用的是 iced **状态**里的 scale，`--scale-factor`
//! 下两者会分叉（`iced_winit-0.14.1/src/lib.rs:1546-1552` vs `1447-1470`）——
//! 详细的取舍写在 [`crate::platform::win32::work_area`] 的模块文档里。
//!
//! 非 Windows 平台上 `probe` 恒 `None`，可用区来源退回 `monitor`（= M362 今天的行为），
//! **且不做任何定位**（没有可用区矩形，不编坐标）：日志 `[window] place … reason=no-work-area`。
//!
//! ## M372：定位（居中、排在尺寸之后、不新增往返）
//!
//! **为什么也在开窗之后**：位置要用窗口所在显示器的 `rcWork` 起点，与尺寸是同一份探针
//! （`MonitorFromWindow` 要句柄）。M372 **没有新增任何往返**：`move_to` 与 `resize` 一样是
//! fire-and-forget 的 `Action`（`iced_runtime-0.14.0/src/window.rs:75` 只有 `(Id, Point)`），同链。
//!
//! **为什么居中**：级联位由系统计数器驱动，每次启动都不一样（本机观测 104/130/156/182/260），
//! 「越界才推回」等于让位置随上一次启动漂移；居中只依赖 (窗口尺寸, 可用区)。反例与取舍见
//! `DevDocs/artifacts/winpos/REPORT.md`。**这是一次产品行为变更**：大屏 100% 上窗口也从级联位
//! 变成居中（尺寸一个像素没变）。「尺寸先于位置」的理由同 [`super`] 的纪律：两步吃同一份 `Fit`
//! （日志、resize、定位三者同源）；`move_to` 设的是**外框左上角**而 resize 不改左上角，所以顺序
//! 不影响结果。窗口无系统装饰（`decorations: false`）⇒ 外框 == 客户区，`Fit::size` 可直接当外框用。
//!
//! ⚠️ 单位分叉（登记，未修）：`resize` 用 iced **状态** scale，`move_to` 交给 winit 的
//! `LogicalPosition` 由 winit 用**原始** scale 换算（`winit-0.30.13/src/platform_impl/windows/window.rs:187`）
//! ⇒ `--scale-factor` 强制档下位置坐标被按 1.0 解释（真实 DPI 变化时二者相等）。修它要再问一次
//! `window::scale_factor(id)`（多一拍往返），本 mission 不做。
//!
//! ## 代价（登记，不藏）
//!
//! 用户可能看到**一次跳变**：窗口先按请求尺寸出现在级联位，一两次动作往返之后才缩到可用区并
//! 挪到居中位。这条链在 `App::new` 之前就开始跑（`iced_winit-0.14.1/src/lib.rs:108-114`：iced 把
//! 应用的初始 `Task` **挂在窗口 open 之后**），所以多数情况下第一帧就是收缩并居中后的样子，
//! 但慢机器上可能闪一下。M372 不加往返，但多了一次**位移**（级联位 → 居中位）。
//!
//! ## 顺序
//!
//! `set_min_size` **再** `resize`（winit 的 `set_min_inner_size` 会夹 `request_inner_size`，
//! `iced_winit-0.14.1/src/lib.rs:1447-1470`：可用区小于 `MIN_SIZE` 时先 resize 等于白发），
//! 最后 `move_to`。

use iced::{window, Task};

use crate::platform;
use crate::platform::win32::work_area::{self, MonitorRects};
use crate::renderer::acceptance;

use super::super::message::Message;

/// 排进 iced 启动那条 `Task` 链的收缩任务（`capture` = 本次是取证启动）。
pub(super) fn task(
    requested: (f32, f32),
    min: (f32, f32),
    size_source: &'static str,
    capture: bool,
) -> Task<Message> {
    if let Some(why) = acceptance::fit_skip_reason(size_source, capture) {
        acceptance::report_skip(requested, min, size_source, why);
        // 定位与收缩同进同出：取证图的窗口位置不该由宿主显示器决定（D30 逐字节对照的前提）
        acceptance::fit::report_place(requested, None, None, why);
        return Task::none();
    }

    window::latest().then(move |id| match id {
        Some(id) => {
            window::run(id, work_area::probe).then(move |raw| probe_done(id, requested, min, raw))
        }
        // 窗口还没建出来：如实写 `unknown`，不假装「放得下」
        None => {
            let fit = acceptance::fit_to_work_area(requested, min, None);
            acceptance::report_fit(requested, &fit, None, "none", None);
            acceptance::fit::report_place(
                requested,
                None,
                None,
                acceptance::fit::PLACE_UNKNOWN_AREA,
            );
            Task::none()
        }
    })
}

/// 第二步：物理矩形有了（Windows）或没有（其它平台 / 读不到），再问一次显示器的**逻辑**尺寸。
///
/// 为什么无论如何都问：它既是折算基准（物理 → 逻辑），也是非 Windows 上唯一的来源；
/// 失败（`None`）时工作区也随之得 `None`（没有基准就不许编一个可用区出来）。
fn probe_done(
    id: window::Id,
    requested: (f32, f32),
    min: (f32, f32),
    raw: Option<MonitorRects>,
) -> Task<Message> {
    window::monitor_size(id).then(move |monitor| {
        let monitor = monitor.map(|size| (size.width, size.height));
        let work = match (raw, monitor) {
            (Some(rects), Some(grid)) => rects.logical_work_area(grid),
            _ => None,
        };
        // 本平台**有**探测（Windows）却没拿到 ⇒ 记 `monitor:probe-failed`，与「macOS/Linux 本来就没有」分开
        let probe_failed = platform::WORK_AREA_IMPLEMENTED && work.is_none();
        apply(id, requested, min, raw, work, monitor, probe_failed)
    })
}

/// 第三步：有了可用区（逻辑 px）与显示器尺寸，定**尺寸**与**位置**（并真去改窗口）。
///
/// `work` 是 Windows 的 `rcWork`（M371），`monitor` 是显示器全尺寸，`raw` 是同一份 `rcWork` 的
/// **物理起点**（M372 定位要用，尺寸用不到）。`work` 为 `None` 时 [`acceptance::work_area`] 会退回
/// `monitor` —— 那就是 M362 的行为（`probe_failed` 只影响日志来源）。
fn apply(
    id: window::Id,
    requested: (f32, f32),
    min: (f32, f32),
    raw: Option<MonitorRects>,
    work: Option<(f32, f32)>,
    monitor: Option<(f32, f32)>,
    probe_failed: bool,
) -> Task<Message> {
    let (area, area_source) = acceptance::work_area(work, monitor, probe_failed);
    let fit = acceptance::fit_to_work_area(requested, min, area);
    // 日志与真窗口同源：下面 resize / move_to 用的就是这一份 `fit` + `pos`
    acceptance::report_fit(requested, &fit, area, area_source, monitor);

    // M372：位置。矩形 = rcWork 的**物理**起点 × 折算因子 + 已解析的可用区尺寸
    let (rect, pos) = placement(fit.size, raw, area, monitor);
    let place_reason = place_reason(pos, raw);
    acceptance::fit::report_place(fit.size, rect, pos, place_reason);

    // 尺寸先定（min 再 resize），位置后定：`move_to` 设的是外框左上角，resize 不改它，所以
    // 这个顺序不影响结果；反过来（先定位再 resize）会让「按哪个尺寸算的居中位」无从追溯。
    let mut actions: Task<Message> = Task::none();
    if fit.reason.adjusts() {
        actions =
            window::set_min_size::<Message>(id, Some(iced::Size::new(fit.min.0, fit.min.1))).chain(
                window::resize::<Message>(id, iced::Size::new(fit.size.0, fit.size.1)),
            );
    }
    // 位置照发：`fits` 时尺寸不动，但**位置仍要居中**（M372 的产品行为变更就在这里）
    match pos {
        Some((x, y)) => actions.chain(window::move_to::<Message>(id, iced::Point::new(x, y))),
        None => actions,
    }
}

/// 定位决策（纯）：`(可用区矩形, 坐标)`，两者一起给出去是为了让日志与真动作**同源**。两者为 `None`
/// 都意味着「本次不定位」（理由见 [`place_reason`]）；矩形要**物理** `rcWork` + 显示器逻辑尺寸 +
/// 已解析的可用区三样齐全才算得出来。
fn placement(
    size: (f32, f32),
    raw: Option<MonitorRects>,
    area: Option<(f32, f32)>,
    monitor: Option<(f32, f32)>,
) -> (Option<acceptance::fit::WorkRect>, Option<(f32, f32)>) {
    let rect = match (raw, area, monitor) {
        (Some(raw), Some(area), Some(monitor)) => work_rect(&raw, monitor, area),
        _ => None,
    };
    let pos = rect.and_then(|rect| acceptance::fit::place_in_work_area(size, rect));
    (rect, pos)
}

/// `rcWork` → 逻辑坐标下的矩形：起点 = 物理起点 × 这一维的 scale，尺寸 = 已解析的可用区。
///
/// 尺寸取 `area` 而不是 `rcWork × scale`：`ULL_WORK_AREA` 是**只改尺寸**的取证旋钮，若这里再用
/// `rcWork` 算一遍，覆盖生效时同一行会出现两个可用区（`work_area=1316x612` 而
/// `work_rect=0,0+2632x1224`），读日志的人没法判断哪个才是判据吃的那块。起点则必须用**真** scale
/// （`monitor_logical ÷ rcMonitor`）：覆盖只该改尺寸，不该把窗口挪到被缩放过的坐标上。
fn work_rect(
    raw: &MonitorRects,
    monitor: (f32, f32),
    area: (f32, f32),
) -> Option<acceptance::fit::WorkRect> {
    let (monitor_w, monitor_h) = raw.monitor.size();
    let scale = (
        scale_of(monitor.0, monitor_w)?,
        scale_of(monitor.1, monitor_h)?,
    );
    let origin = (
        raw.work.left as f32 * scale.0,
        raw.work.top as f32 * scale.1,
    );
    Some((origin, area))
}

/// 这一维的 scale（`逻辑 ÷ 物理`）：物理值非正 / 结果不有限非正 ⇒ `None`。
fn scale_of(logical: f32, physical: i32) -> Option<f32> {
    let value = logical / physical as f32;
    (physical > 0 && value.is_finite() && value > 0.0).then_some(value)
}

/// 本次没定位的原因（进日志 `reason=`），四档都在 `fit.rs` 里定义。
///
/// 「本平台没有」与「有探测但没拿到」必须分开（同 `work_area_source` 的 `monitor` vs
/// `probe-failed`）：前者是降级，后者是布局 / 句柄 bug。
fn place_reason(pos: Option<(f32, f32)>, raw: Option<MonitorRects>) -> &'static str {
    use acceptance::fit::{
        PLACE_CENTERED, PLACE_NO_WORK_AREA, PLACE_PROBE_FAILED, PLACE_UNKNOWN_AREA,
    };
    if pos.is_some() {
        return PLACE_CENTERED;
    }
    // 有物理矩形却没算出坐标 ⇒ 尺寸 / 折算不合法，不许静默
    if raw.is_some() {
        return PLACE_UNKNOWN_AREA;
    }
    match platform::WORK_AREA_IMPLEMENTED {
        true => PLACE_PROBE_FAILED,
        false => PLACE_NO_WORK_AREA,
    }
}

#[cfg(test)]
mod tests {
    //! 接线层只有一件事能在这里钉：**不该发动作的那些路，一条动作都不发**，以及
    //! **该发动作的那些路真的排了动作**（M372 加了「位置」这一类动作）。
    //! 纯判据在 `renderer/acceptance/fit_tests.rs`，真窗口量在 `DevDocs/artifacts/winpos`。
    //!
    //! **抓不住**：resize / move_to 是否真的落到窗口上（两者都是 fire-and-forget，`Task` 里没有
    //! 可断言的返回值）。这条只能靠真开窗量窗口矩形。

    use super::*;
    use crate::platform::win32::work_area::PhysicalRect;

    /// 一块矩形（`left, top, right, bottom`，Win32 的 RECT 语义：右下**不含**）。
    const fn rect(left: i32, top: i32, right: i32, bottom: i32) -> PhysicalRect {
        PhysicalRect {
            left,
            top,
            right,
            bottom,
        }
    }

    /// 取证启动 / 显式尺寸档：`Task` 的 units 必须是 0（发出去就会改写 D30 的图）。
    #[test]
    fn skipped_paths_issue_no_window_action() {
        for (source, capture) in [
            ("ULL_WINDOW_SIZE", false),
            ("termux-screen", false),
            ("default", true),
        ] {
            let task = task((1520.0, 900.0), (880.0, 560.0), source, capture);
            assert_eq!(task.units(), 0, "{source}（capture={capture}）不许发动作");
        }

        // 自动档 + 非取证：真的排了动作（否则这条测试对「接线断了」毫无感觉）
        let wired = task((1180.0, 760.0), (880.0, 560.0), "default", false);
        assert!(wired.units() > 0, "自动档必须真的去问显示器尺寸");
    }

    /// 判据 ② 在**接线层**的断言：小屏（`--scale-factor 2` 那格）定位之后
    /// 「顶边 ≥ 可用区顶边」且「底边 ≤ 可用区底边」。真机同款数字在 `DevDocs/artifacts/winpos`。
    ///
    /// 抓得住：把 `placement` 的坐标整条抽掉（= 退回不设 position ⇒ 级联位），这条当场红。
    /// 抓不住：`move_to` 是否真落到窗口上（fire-and-forget，没有回值）。
    #[test]
    fn the_small_screen_placement_stays_inside_the_work_area() {
        // iced 状态 scale = 2 ⇒ 显示器逻辑 1316×636、可用区 1316×612（任务栏 24 逻辑 px）
        let (area, grid) = ((1316.0, 612.0), (1316.0, 636.0));
        let fit = acceptance::fit_to_work_area((1180.0, 760.0), (880.0, 560.0), Some(area));
        let (_, pos) = placement(fit.size, Some(primary()), Some(area), Some(grid));
        let (x, y) = pos.expect("小屏必须算得出位置");
        assert_eq!(fit.size, (1180.0, 612.0), "尺寸仍由 M362 的纯函数定");
        assert_eq!((x, y), (68.0, 0.0), "居中位");
        assert!(y >= 0.0, "顶边不许在可用区上面：{y}");
        assert!(y + fit.size.1 <= 612.0, "底边溢出：{y} + {}", fit.size.1);
    }

    /// 本机主屏（2632×1272 @100%，任务栏 48 ⇒ `rcWork` = (0,0)+2632×1224）。
    fn primary() -> MonitorRects {
        MonitorRects {
            work: rect(0, 0, 2632, 1224),
            monitor: rect(0, 0, 2632, 1272),
        }
    }

    /// M372 的**产品行为变更**写成断言：`fits`（尺寸一个像素都不动）时**仍然**要定位；
    /// 而没有可用区矩形（macOS / Linux / probe 失败）时**一条动作都不发**。
    ///
    /// 抓得住：把定位塞进 `if fit.reason.adjusts()` 里面（于是大屏上位置仍是级联位）、
    /// 或没有矩形也照样发 `move_to`（编坐标）。抓不住：真窗口是否真挪过去了（`move_to` 没有回值）。
    #[test]
    fn the_actions_follow_the_placement() {
        let (area, grid) = ((2632.0, 1224.0), (2632.0, 1272.0));
        let placed = apply(
            window::Id::unique(),
            (1180.0, 760.0),
            (880.0, 560.0),
            Some(primary()),
            Some(area),
            Some(grid),
            false,
        );
        assert!(placed.units() > 0, "fits 时尺寸不动，但位置必须居中");

        let nowhere = apply(
            window::Id::unique(),
            (1180.0, 760.0),
            (880.0, 560.0),
            None,
            None,
            Some((1920.0, 1080.0)),
            platform::WORK_AREA_IMPLEMENTED,
        );
        assert_eq!(nowhere.units(), 0, "没有矩形就不许定位（尺寸也不必动）");
    }

    /// M372：`rcWork` 的**物理**起点要按真 scale 搬进逻辑坐标；尺寸取**已解析**的可用区。
    ///
    /// 抓得住：把起点写成 0（丢掉起点 ⇒ 副屏的窗口被搬到主屏）、或把折算因子写成常量 1.0
    /// （125% / 150% 上位置会差一整圈）。
    #[test]
    fn work_rect_converts_the_origin_with_the_monitor_grid() {
        // 本机 1 号格：2632×1272 @100%，任务栏 48 ⇒ rcWork = (0,0)+2632×1224
        assert_eq!(
            work_rect(&primary(), (2632.0, 1272.0), (2632.0, 1224.0)),
            Some(((0.0, 0.0), (2632.0, 1224.0)))
        );
        // 同一块屏 `--scale-factor 2`：逻辑网格 1316×636，任务栏在逻辑 px 下是 24
        assert_eq!(
            work_rect(&primary(), (1316.0, 636.0), (1316.0, 612.0)),
            Some(((0.0, 0.0), (1316.0, 612.0)))
        );

        // 2 号屏（物理起点非零、scale 1）：起点必须跟着搬 ⇒ (1920, 0)，不是 (0, 0)
        let secondary = MonitorRects {
            work: rect(1920, 0, 3840, 1040),
            monitor: rect(1920, 0, 3840, 1080),
        };
        assert_eq!(
            work_rect(&secondary, (1920.0, 1080.0), (1920.0, 1040.0)),
            Some(((1920.0, 0.0), (1920.0, 1040.0)))
        );
    }

    /// 折算基准非法 ⇒ 没有矩形、没有坐标（**不编 (0,0)**）：`rcMonitor` 宽为 0、逻辑尺寸为 0、
    /// 物理值非正，逐个来一遍（`scale_of` 的边界都在这一格里）。
    #[test]
    fn no_scale_means_no_work_rect_and_no_coordinates() {
        let degenerate = MonitorRects {
            work: rect(0, 0, 2632, 1224),
            monitor: rect(0, 0, 0, 1272),
        };
        assert_eq!(
            work_rect(&degenerate, (2632.0, 1272.0), (2632.0, 1224.0)),
            None
        );
        assert_eq!(scale_of(1316.0, 2632), Some(0.5));
        assert_eq!(scale_of(1920.0, 0), None, "零物理尺寸 = 没有折算基准");
        assert_eq!(scale_of(1920.0, -100), None);
        assert_eq!(scale_of(0.0, 1920), None, "逻辑尺寸为 0 也算不出 scale");
    }

    /// 「没定位」的四档理由必须能分开读（本平台没有 / 有探测却失败 / 有矩形但算不出坐标 / 成功）。
    #[test]
    fn place_reason_separates_every_reason_not_to_place() {
        use acceptance::fit::{
            PLACE_CENTERED, PLACE_NO_WORK_AREA, PLACE_PROBE_FAILED, PLACE_UNKNOWN_AREA,
        };
        let rects = primary();
        assert_eq!(place_reason(Some((68.0, 0.0)), Some(rects)), PLACE_CENTERED);
        // 有物理矩形却没算出坐标：尺寸 / 折算不合法（不许静默）
        assert_eq!(place_reason(None, Some(rects)), PLACE_UNKNOWN_AREA);
        // 没有物理矩形：本平台**有**探测 ⇒ probe-failed；本平台没有 ⇒ no-work-area
        let missing = match platform::WORK_AREA_IMPLEMENTED {
            true => PLACE_PROBE_FAILED,
            false => PLACE_NO_WORK_AREA,
        };
        assert_eq!(place_reason(None, None), missing);
    }
}
