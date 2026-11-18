//! Toast 的**动效时间轴**（M298）：一条提示的进场 / 退场进度从哪来。
//!
//! ## 为什么不新造一条轨道
//!
//! `components::motion` 里的帧泵只认三类轨道（页面 / 模态 / 右栗折叠），而 toasts 的寿命不归
//! 它管：`ToastCenter` 已经按 33 ms 一拍递减 `remaining_ms`（`app/update/clock.rs`），到期自己
//! 消失。再往帧泵里塞一条「toast 轨道」等于把**同一件事的时钟记两遍**，两边一旦不同步就是
//! 「提示还在、动画已经结束」。
//!
//! ## 只有**一个**时钟：toast 自己的 `remaining_ms`
//!
//! 第一版（M298 r0）用 `Instant::now()` 算进场年龄、用 `remaining_ms` 算退场——两个时钟。
//! 复审 P2-1 的探针把它撞出来了：`Instant` 走**墙上时间**、`remaining_ms` 走**节拍时间**，
//! 于是「测试里连走 120 拍」和「真机上过了 4 秒」不是同一件事；更要紧的是它掩盖了真 bug
//! （退场腿没有触发点，见下）。
//!
//! 现在两条腿都从同一个量推：第一次看见这条提示时把 `remaining_ms` 记成它的**寿命**
//! （`life_ms` = TTL），此后 `已活 = life_ms - remaining_ms`。进场腿吃 `已活`，退场腿吃
//! `remaining_ms` —— 与 `expire` 抹掉它的那一刻天然对齐，没有第二份时间。
//!
//! 时长档与缓动档**复用模态那一对腿**（[`motion::modal_spec`]）：toast 与对话框同属 M3 的
//! 「用户刚做了一件事的回应」短档，没有第二条曲线可言。
//!
//! ## 为什么采样发生在 `update` 侧，且退场还要**另外一个**置脏判据
//!
//! 与帧泵同一条纪律（`components::motion` 模块文档的五条契约）：`view` 只采样、不起跑。这里的
//! [`App::sync_toasts`](super::App) 在 [`App::refresh_view`](super::App::refresh_view) 里跑，
//! 结果是「这一帧每条该缩放多少」——`view` 拿到的只是一份只读快照。
//!
//! 但 `refresh_view` 只被 `view_dirty` 开门，而进场腿自己会续帧、退场腿**不会**：进场结束时
//! `view_dirty` 回到 `false`，等 3.6 秒后 `remaining_ms` 掉进退场窗口时没有任何代码把它重新
//! 置脏 ⇒ 150 ms 的收缩从不发生，`expire` 一抹胶囊就整块消失（复审 P2-1 实测：120 拍之后
//! `progress` 仍是 1.0）。所以 [`ToastTracks::needs_frame`] 是给 `update/clock.rs` 的**无副作用**
//! 判据：只要还有条目在退场窗口里，就把 `view_dirty` 点亮，让既有的
//! `if self.view_dirty { refresh_view() }` 去采样。**`refresh_view` 没有变成每帧无条件调用**，
//! 静止态（无提示 / 提示都在位）帧订阅判据与改造前一样（M192「空闲不烧帧」）。
//!
//! ## 关掉动效 = 一帧都不动
//!
//! `MotionScale::NONE`（截图通道 / CPU 档 / 系统「减少动效」）时进度**恒为 1.0**，
//! [`ToastTracks::observe`] 与 [`ToastTracks::needs_frame`] 都返回 `false` ⇒ 不会为了动画续帧。
//! 出图因此可复现（D30）。

use unlucky_presenter::toast::{Toast, ToastId};

use crate::app::components::motion;

/// 一条 toast 的动效记录。
#[derive(Debug, Clone, Copy)]
struct Entry {
    /// toast id（`ToastCenter` 重新编号，进程内唯一）。
    id: ToastId,
    /// 这条提示**总共活多久**：第一次被看见时 `remaining_ms` 的值（= `ToastCenter` 的 TTL）。
    life_ms: u64,
    /// 这一拍算出来的进度（`0` = 刚起手 / 正在退场尾声，`1` = 完全在位）。
    progress: f32,
}

/// toast 的动效时间轴（挂在 [`App`](super::App) 上，`Default` = 空）。
#[derive(Debug, Default)]
pub(crate) struct ToastTracks {
    entries: Vec<Entry>,
}

impl ToastTracks {
    /// 这一拍动没动过（`false` = 全都静止在位，不必续帧）。
    ///
    /// **update 侧每拍调一次**：登记新出现的（记下寿命）、忘掉已经消失的、重算每条进度。
    pub(crate) fn observe(&mut self, toasts: &[Toast]) -> bool {
        self.entries
            .retain(|entry| toasts.iter().any(|toast| toast.id == entry.id));
        let spec = motion::modal_spec();
        let scale = motion::scale();
        let mut moving = false;
        for toast in toasts {
            let life_ms = match self.entries.iter().find(|entry| entry.id == toast.id) {
                Some(entry) => entry.life_ms,
                None => {
                    self.entries.push(Entry {
                        id: toast.id,
                        life_ms: toast.remaining_ms,
                        progress: 1.0,
                    });
                    toast.remaining_ms
                }
            };
            // 关着动效：进度就是终值，且**不**算「在动」（不续帧、不出图差异）。
            let progress = if scale.is_off() {
                1.0
            } else {
                let (progress, in_flight) = sample(toast, life_ms, &spec, scale);
                moving |= in_flight;
                progress
            };
            if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == toast.id) {
                entry.progress = progress;
            }
        }
        moving
    }

    /// 这一条当前该缩放成什么样（`0..=1`；没登记过 = 在位，宁可不动也不许卡在中间）。
    pub(crate) fn progress(&self, id: ToastId) -> f32 {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .map_or(1.0, |entry| entry.progress)
    }

    /// 还有没有条目落在**退场窗口**里（`update/clock.rs` 唯一的置脏判据，**无副作用**）。
    ///
    /// 判据与 [`sample`] 里退场腿那一项同一个算式：`remaining_ms < exit` 腿的时长。
    /// 它只看**已登记**的条目——`observe` 还没跑过的那条不算（那时 `progress` 也是终值）。
    pub(crate) fn needs_frame(&self, toasts: &[Toast]) -> bool {
        let scale = motion::scale();
        if scale.is_off() {
            return false;
        }
        let exit_ms = scale.ms(motion::modal_spec().exit.duration).max(1.0);
        self.entries.iter().any(|entry| {
            toasts
                .iter()
                .any(|toast| toast.id == entry.id && (toast.remaining_ms as f32) < exit_ms)
        })
    }
}

/// 一条 toast 的进 / 出采样（纯函数，测试直接打这里）。
///
/// 进场：已活时长走完 `enter` 腿；退场：`remaining_ms` 掉进 `exit` 腿的时长里之后走到 0。
/// 两条腿同时跑（进场还没完就到期的极端情况取**小的那个**，于是它一开始就在退场，不会闪一下
/// 全尺寸再消失）。
fn sample(
    toast: &Toast,
    life_ms: u64,
    spec: &motion::ModalSpec,
    scale: unlucky_design::motion::MotionScale,
) -> (f32, bool) {
    let enter_ms = scale.ms(spec.enter.duration).max(1.0);
    let exit_ms = scale.ms(spec.exit.duration).max(1.0);
    let lived_ms = life_ms.saturating_sub(toast.remaining_ms) as f32;
    let enter_t = (lived_ms / enter_ms).clamp(0.0, 1.0);
    let exit_t = (1.0 - toast.remaining_ms as f32 / exit_ms).clamp(0.0, 1.0);
    let progress = spec
        .enter
        .easing
        .sample(enter_t)
        .min(1.0 - spec.exit.easing.sample(exit_t))
        .clamp(0.0, 1.0);
    (progress, enter_t < 1.0 || exit_t > 0.0)
}

#[cfg(test)]
mod tests {
    //! **进/出两条腿的数值取证**：起点、终点、单调、以及「不动就一帧都不续」。
    //!
    //! 抓得住：进场方向做反（`progress` 从 1 掉到 0 即红）、退场没接上（`remaining_ms` 掉了
    //! 进度却还是 1）、关掉动效仍然报「在动」（`NONE` 下 `observe` / `needs_frame` 必须为假）、
    //! **退场腿在产品路径上没有触发点**（两条 `ticking_app` 用例走真节拍）。
    //! 抓不住：**观感**（截图见 `DevDocs/artifacts/m298/`）。

    use super::*;
    use std::time::Duration;
    use unlucky_design::color::SchemeFlavor;
    use unlucky_design::motion::MotionScale;
    use unlucky_presenter::mock::MockBackend;
    use unlucky_presenter::settings_bridge::SettingsBridge;
    use unlucky_presenter::toast::ToastKind;

    use crate::app::state::App;
    use crate::cli::Options;
    use crate::theme::Families;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    /// 一拍的时长（= `App::TICK_MS`，`update/clock.rs` 的节拍口径）。
    const TICK: Duration = Duration::from_millis(33);

    /// 注入式 `App`（不碰磁盘、mock 后端），并把动效钉在 `STANDARD`。
    ///
    /// 这两条用例走的是**真节拍**（`update/clock.rs` 的 `advance`），不是在测试里直接喂
    /// `observe`——复审 P2-1 漏掉的正是这一层：`observe` 自己算得再对，没人抽它就是没播。
    fn ticking_app() -> App {
        motion::reset();
        motion::configure_for_test(MotionScale::STANDARD, None);
        App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(MockBackend::new()),
            SettingsBridge::memory_only(),
        )
    }

    fn toast(id: ToastId, remaining_ms: u64) -> Toast {
        Toast {
            id,
            kind: ToastKind::Info,
            text: "已取消安装".to_string(),
            remaining_ms,
        }
    }

    fn tracks(scale: MotionScale) -> ToastTracks {
        motion::reset();
        motion::configure_for_test(scale, None);
        ToastTracks::default()
    }

    #[test]
    fn a_fresh_toast_enters_from_zero_and_stays_at_one() {
        let mut tracks = tracks(MotionScale::STANDARD);
        // 第一次被看见 = 寿命登记当拍，进度在起点。
        assert!(tracks.observe(&[toast(1, 4_000)]), "进场这一段必须在动");
        assert!(
            tracks.progress(1) < 0.05,
            "刚出现必须是进场起点，实测 {}",
            tracks.progress(1)
        );
        // 已活 400 ms（进场腿 200 ms）⇒ 停在 1.0，且不再报「在动」。
        assert!(!tracks.observe(&[toast(1, 3_600)]), "进场走完就不该再续帧");
        assert_eq!(tracks.progress(1), 1.0, "在位就必须是满进度");
    }

    #[test]
    fn the_last_moments_play_the_exit_leg_down_to_zero() {
        let mut tracks = tracks(MotionScale::STANDARD);
        tracks.observe(&[toast(7, 4_000)]);
        tracks.observe(&[toast(7, 3_600)]);
        assert_eq!(tracks.progress(7), 1.0);

        // 寿命只剩 60 ms（小于退场腿 150 ms）⇒ 进度必须已经掉下来且仍在动。
        assert!(tracks.observe(&[toast(7, 60)]), "退场这一段必须在动");
        let mid = tracks.progress(7);
        assert!(
            mid < 1.0 && mid > 0.0,
            "退场途中的进度应在 (0,1)，实测 {mid}"
        );

        // 到期当拍（`remaining_ms` 归零）：进度必须是 0 —— 它就在这一刻被 `expire` 抹掉，
        // 中间不许留一帧「还满着」。
        tracks.observe(&[toast(7, 0)]);
        assert!(tracks.progress(7) < 0.05, "到期当拍必须归零");
    }

    #[test]
    fn motion_off_is_bit_exact_still_and_never_asks_for_frames() {
        let mut tracks = tracks(MotionScale::NONE);
        assert!(
            !tracks.observe(&[toast(1, 4_000)]),
            "关掉动效时一帧都不该续（截图 / CPU 档的可复现前提）"
        );
        assert_eq!(tracks.progress(1), 1.0, "NONE 下进度恒为终值");
        assert!(
            !tracks.needs_frame(&[toast(1, 40)]),
            "NONE 下连退场窗口都不该点灯"
        );
    }

    /// **复审 P2-1 的红灯**：退场腿在产品路径上必须真的被抽到。
    ///
    /// 抓得住：`observe` 只在 `refresh_view` 里跑、而 `refresh_view` 只被 `view_dirty` 开门 ⇒
    /// 进场腿自己续帧（所以进场能跑），进场结束后没人再置脏，**退场那 150 ms 从不发生**，
    /// `expire` 把条目一抹胶囊就整块消失（复审探针实测：120 拍之后 `progress` 仍是 1.0）。
    /// 抓不住：观感（静态图看不出一段没播的动画），也抓不住「置脏了但没画出来」。
    #[test]
    fn the_tick_clock_actually_carries_the_exit_leg() {
        let mut app = ticking_app();
        let id = app.state.toasts.push(ToastKind::Info, "已取消安装");
        // 进场腿：这几拍由 `observe` 自己续帧（所以它本来就能跑）。
        for _ in 0..10 {
            app.advance(TICK);
        }
        assert_eq!(app.toast_tracks.progress(id), 1.0, "进场走完应当停在满进度");
        // 之后**没有任何事件**，只让节拍继续走：4 s TTL - 10 拍 ≈ 还剩 3.6 s。
        for _ in 0..110 {
            app.advance(TICK);
        }
        let remaining = app.state.toasts.items().first().map(|it| it.remaining_ms);
        let progress = app.toast_tracks.progress(id);
        assert!(
            remaining.is_some_and(|ms| ms < 150),
            "这一拍应当已经落在 150 ms 的退场腿里（remaining={remaining:?}ms）"
        );
        assert!(
            progress < 1.0,
            "退场窗口内必须真的开始收（remaining={remaining:?}ms, progress={progress}）"
        );
    }

    /// 到期之后**不许**继续置脏：否则退场窗口的修复会变成「永不停止的每帧重建」。
    #[test]
    fn an_expired_toast_stops_asking_for_frames() {
        let mut app = ticking_app();
        app.state.toasts.push(ToastKind::Info, "已取消安装");
        for _ in 0..130 {
            app.advance(TICK);
        }
        assert!(app.state.toasts.is_empty(), "TTL 用完必须被 expire 抹掉");
        for _ in 0..5 {
            app.advance(TICK);
        }
        assert!(!app.view_dirty, "没有提示时不许每帧置脏（M192 空闲不烧帧）");
        assert!(!app.needs_frames(), "静止态不该订阅帧");
    }

    #[test]
    fn a_vanished_toast_is_forgotten() {
        let mut tracks = tracks(MotionScale::STANDARD);
        tracks.observe(&[toast(1, 4_000)]);
        assert!(tracks.observe(&[toast(2, 4_000)]), "新的那条在进场");
        assert_eq!(tracks.progress(1), 1.0, "已消失的按在位处理（不留半截）");
        assert!(
            !tracks.entries.iter().any(|entry| entry.id == 1),
            "消失的条目必须被清掉（否则长时间常驻会线性增长）"
        );
    }
}
