//! **列表行增删**的轨道（M305）：一行一个可见度，进/出各走一条腿，**退场走完才真移除**。
//!
//! ## 为什么需要「保留旧顺序」这张表
//!
//! 「删除项淡出后才真正移除」不是把行从列表里拿掉的时机问题，而是**顺序**问题：
//! 用户删掉第 3 行，如果这一帧就把它从 `column` 里摘掉，第 4 行起全部上移一格 —— 那就是
//! **整表瞬间重排闪跳**（任务里点名禁止）。所以本模块记两份清单：
//!
//! | 字段 | 是什么 | 谁读 |
//! |---|---|---|
//! | `order` | **这一帧该画哪个顺序**（退场行还钉在它原来的位置） | [`Rows::drawn`] → 视图的列 |
//! | `current` | 数据模型**现在**给的行（真值） | 差分判据（进 / 出） |
//!
//! 退场行在 `order` 里活到它的退场腿走完（[`Rows::expire`] 那一刻才从 `order` 删掉），
//! 而那一路它的可见度从 1 走到 0 ⇒ 高度也一路收到 0 ⇒ 下面那些行是**逐帧**滑上来的，
//! 收尾那一帧没有位移台阶。
//!
//! ## 为什么只有一个标量
//!
//! 一行的动效只由 `visibility: 0..1` 驱动：淡入淡出（色板预合成，见 `motion/fade.rs`）、
//! 高度（`motion/reveal.rs` 的裁切框）、滑动偏移（[`RowFrame::offset`]）**都从它算**。
//! 一个标量 ⇒ 进/出两个方向天然读**同一个时间源**（[`Pump::elapsed_ms`]），不会出现
//! 「淡入走 200ms、高度走 150ms」这种两套时钟的错位。
//!
//! ## 关掉动效（`MotionScale::NONE`）
//!
//! [`Pump::observe_rows`] 在 `is_off()` 时**不插轨道、不留幽灵**：`order` 直接等于 `current`，
//! 于是 [`Rows::drawn`] 就是模型本身、[`Rows::frame`] 恒为 [`RowFrame::REST`]。换句话说：
//! 关掉动效 = 界面立刻是**最终状态**，而不是「跑一遍 0 时长动画」（那会在中途停一拍）。

use unlucky_design::motion::Easing;

use super::super::transition::{row_spec, Leg};
use super::lists::List;
use super::{leg_fraction, Pump};

/// 一帧的行状态（视图只看这两个数，**不含任何时长**）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowFrame {
    /// 可见度：`0` = 完全收起 / 融进背景，`1` = 静息。
    pub visibility: f32,
    /// 纵向偏移（px，正 = 向下）。入场从下方 `+slide` 浮上来，退场向 `-slide` 卷走。
    pub offset: f32,
}

impl RowFrame {
    /// 静息：可见度 1、零偏移（`MotionScale::NONE` 与「没在动」恒为它）。
    pub const REST: Self = Self {
        visibility: 1.0,
        offset: 0.0,
    };
}

/// 一条正在跑的行轨道（进 / 出共用一张形状，`from → to` 表达方向）。
#[derive(Debug, Clone, Copy)]
struct RowTrack {
    /// 起跑那一刻的可见度（退场中途又回列表时是中间值 ⇒ **不跳变**）。
    from: f32,
    /// 终点（进 = 1，出 = 0）。
    to: f32,
    /// 起跑时刻（含错位；钉住相位时恒为 0）。
    start_ms: f64,
    /// 这一腿的时长（已按 `MotionScale` 缩放）。
    duration_ms: f64,
    /// 这一腿的曲线。
    easing: Easing,
}

impl RowTrack {
    /// `elapsed` 毫秒之后的可见度（端点**逐位**等于 `from` / `to`）。
    fn sample(&self, elapsed_ms: f64) -> f32 {
        let t = leg_fraction(elapsed_ms, self.duration_ms);
        if t >= 1.0 {
            return self.to;
        }
        if t <= 0.0 {
            return self.from;
        }
        self.from + (self.to - self.from) * self.easing.sample(t)
    }

    /// 这一腿走完了（错位的前置时间也算在内：`start_ms` 已经是「它自己的起跑时刻」）。
    fn done(&self, now_ms: f64) -> bool {
        now_ms - self.start_ms >= self.duration_ms
    }
}

/// 一次观察的**错位口径**（所有行共用一份）：起跑基准 + 一档间隔。
///
/// 错位档取 `min(这一行在新/旧顺序里的下标, max_stagger)`：列表很长时不许「越往后越等」。
#[derive(Debug, Clone, Copy)]
struct Stagger {
    start_ms: f64,
    step_ms: f64,
}

impl Stagger {
    /// 第 `index` 行的起跑时刻。
    fn at(self, index: usize) -> f64 {
        let slot = row_spec()
            .max_stagger
            .min(u8::try_from(index).unwrap_or(u8::MAX));
        self.start_ms + f64::from(slot) * self.step_ms
    }
}

/// 列表行的两条清单 + 在跑的轨道（见模块文档）。
#[derive(Debug, Default)]
pub(crate) struct Rows {
    /// 这一帧该画的顺序（退场行还钉在里面）。
    order: Vec<String>,
    /// 数据模型现在给的行（真值）。
    current: Vec<String>,
    /// 在跑的轨道（进 + 出）。`Vec` 而不是 `HashMap`：一屏几十行，线性查找比哈希分配便宜且稳。
    tracks: Vec<(String, RowTrack)>,
    /// 第一次观察只对表、不动画（否则**页面第一次出现**就是一整屏淡入）。
    seeded: bool,
}

impl Rows {
    /// 出厂态：两张空清单、没有轨道、没对过表。
    pub(super) const fn new() -> Self {
        Self {
            order: Vec::new(),
            current: Vec::new(),
            tracks: Vec::new(),
            seeded: false,
        }
    }

    /// 数据真的变了没有（真值口径：跟 `current` 比）。
    fn differs(&self, ids: &[&str]) -> bool {
        self.current.len() != ids.len()
            || self
                .current
                .iter()
                .zip(ids)
                .any(|(current, id)| current != id)
    }

    /// **这一帧**的可见度（没有轨道 = 1.0）。
    ///
    /// 必须取**当前采样**而不是 `track.from`：进场走到一半就被删的行，`from` 是 0（它从不可见
    /// 开始），拿它当退场起点会得到「0 → 0」——可见度与高度当拍归零、下面整列上跳，正是任务①
    /// 禁的闪跳。退场中途回来的那一侧本来就走采样，两个方向必须同一条口径（M305 r1 P2-1）。
    fn visibility_of(&self, id: &str, now_ms: f64) -> f32 {
        self.tracks
            .iter()
            .find(|(tracked, _)| tracked == id)
            .map_or(1.0, |(_, track)| track.sample(now_ms - track.start_ms))
    }

    /// 换一条轨道（同 id 覆盖：中途反向就接着当前进度走）。
    fn retarget(
        &mut self,
        id: &str,
        from: f32,
        to: f32,
        leg: Leg,
        leg_ms: f64,
        stagger: Stagger,
        index: usize,
    ) {
        let track = RowTrack {
            from,
            to,
            start_ms: stagger.at(index),
            duration_ms: leg_ms,
            easing: leg.easing,
        };
        match self.tracks.iter_mut().find(|(tracked, _)| tracked == id) {
            Some(entry) => entry.1 = track,
            None => self.tracks.push((id.to_owned(), track)),
        }
    }

    /// 这一帧该画的顺序：旧顺序为骨，新行按它在**新顺序里的前驱**插进去。
    ///
    /// 三条性质（都有用例）：①没变的行**位置逐帧不变**；②退场行还钉在原位；
    /// ③进入的行落在它该在的地方（不是一律追加到末尾）。
    pub(crate) fn drawn(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self.order.iter().map(String::as_str).collect();
        for (index, id) in self.current.iter().enumerate() {
            if out.contains(&id.as_str()) {
                continue;
            }
            // 找「新顺序里紧跟它、又已经在旧顺序里」的那一项：插在它前面。
            let anchor = self.current[index + 1..]
                .iter()
                .find_map(|successor| out.iter().position(|drawn| drawn == successor));
            match anchor {
                Some(position) => out.insert(position, id.as_str()),
                None => out.push(id.as_str()),
            }
        }
        out
    }

    /// 这一帧某一行画成什么样（**只采样、无副作用**）。
    pub(crate) fn frame(&self, id: &str, now_ms: f64, slide: f32) -> RowFrame {
        let Some((_, track)) = self.tracks.iter().find(|(tracked, _)| tracked == id) else {
            return RowFrame::REST;
        };
        let visibility = track.sample(now_ms - track.start_ms);
        // 进场：从下方浮上来（`+slide → 0`）；退场：向**上**卷走（`0 → -slide`）。
        let remaining = 1.0 - visibility;
        let offset = if track.to > track.from {
            slide * remaining
        } else {
            -slide * remaining
        };
        RowFrame { visibility, offset }
    }

    /// 清掉走完的轨道，并把「退场也走完了」的行从画的顺序里删掉。
    ///
    /// 这一步是**唯一**让退场行消失的地方——删早了就是闪跳，删晚了就是占着位置不动。
    pub(super) fn expire(&mut self, now_ms: f64) {
        self.tracks.retain(|(_, track)| !track.done(now_ms));
        let current = std::mem::take(&mut self.current);
        self.order.retain(|id| {
            current.contains(id) || self.tracks.iter().any(|(tracked, _)| tracked == id)
        });
        self.current = current;
        if self.tracks.is_empty() {
            // 收尾归位：`order` 的「幽灵位」全部消失后两边应当逐项相同。
            self.order.clone_from(&self.current);
        }
    }

    /// 有没有轨道在跑（= 要不要订阅帧）。
    pub(super) fn is_animating(&self) -> bool {
        !self.tracks.is_empty()
    }

    /// 定档 / 复位（`Pump::configure` 与测试的 `reset`）。
    pub(super) fn clear(&mut self) {
        self.order.clear();
        self.current.clear();
        self.tracks.clear();
        self.seeded = false;
    }
}

impl Pump {
    /// 列表的真值换了：差分出进 / 出两类行并起跑轨道。返回 `true` = 真的起跑了。
    ///
    /// `list` 说**哪一张列表**（M313 起有两张：下载页的版本列表、实例页的实例列表）——清单
    /// 按列表分账，否则两张列表会互相把对方的行判成退场（见 `pump/lists.rs`）。
    ///
    /// **必须在 update 侧调**（`pages/download/model.rs::from_snapshot` 与 `app/view.rs::model`）：
    /// iced 先算订阅表、后建视图，在 `view` 里起跑的轨道当拍订阅不上（M192 r1 复审 P1-1 的原始缺陷）。
    ///
    /// 幂等：同一份清单再喂一次是空操作（视图重建 / 截图 harness 每帧重建模型都靠这条）。
    pub fn observe_rows(&mut self, list: List, ids: &[&str]) -> bool {
        // 与档位有关的量先取好：下面把 `self.rows` 借成 `&mut`（同一结构体的另一块），
        // 借出去之后就不能再读 `self.scale` / `self.now_ms` 了。
        let spec = row_spec();
        let scale = self.scale;
        let stagger = Stagger {
            start_ms: self.start_now(),
            step_ms: f64::from(scale.ms(spec.stagger)),
        };
        let enter_ms = f64::from(scale.ms(spec.enter.duration));
        let exit_ms = f64::from(scale.ms(spec.exit.duration));
        let now = self.elapsed_ms();
        let off = scale.is_off();
        let rows = self.rows.of_mut(list);

        if !rows.seeded {
            rows.order = ids.iter().map(|id| (*id).to_owned()).collect();
            rows.current = rows.order.clone();
            rows.seeded = true;
            return false;
        }
        if !rows.differs(ids) {
            return false;
        }
        if off {
            // 关掉动效 = 立刻落到最终状态：不留退场幽灵、也不插 0 时长轨道。
            rows.order = ids.iter().map(|id| (*id).to_owned()).collect();
            rows.current = rows.order.clone();
            rows.tracks.clear();
            return false;
        }

        // ① 离开的：用**旧顺序**里的位置算错位（用户看到的原本次序）。
        let leaving: Vec<(String, usize)> = rows
            .current
            .iter()
            .enumerate()
            .filter(|(_, id)| !ids.contains(&id.as_str()))
            .map(|(index, id)| (id.clone(), index))
            .collect();
        let left = leaving.len();
        for (id, index) in leaving {
            let from = rows.visibility_of(&id, now);
            rows.retarget(&id, from, 0.0, spec.exit, exit_ms, stagger, index);
        }

        // ② 进入的（以及退场中途又回列表的）：用**新顺序**里的位置算错位。
        let arriving: Vec<(String, usize)> = ids
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let known = rows.current.iter().any(|current| current == id);
                let retracting = rows
                    .tracks
                    .iter()
                    .any(|(tracked, track)| tracked == id && track.to < 1.0);
                (!known || retracting).then(|| ((*id).to_owned(), index))
            })
            .collect();
        let arrived = arriving.len();
        for (id, index) in arriving {
            // 退场中途回来（还有轨道）：接着**当前**可见度走，不跳回 0；全新的行从 0 起。
            let from = match rows.tracks.iter().find(|(tracked, _)| tracked == &id) {
                Some((_, track)) => track.sample(now - track.start_ms),
                None if rows.current.iter().any(|current| current == &id) => 1.0,
                None => 0.0,
            };
            rows.retarget(&id, from, 1.0, spec.enter, enter_ms, stagger, index);
        }

        // ③ **纯重排**（同一批 id 换了顺序）：没有进、也没有出，所以一条轨道都不建——
        // 而 `advance` 在没轨道时早退、`expire` 也就不跑，`order` 会永远停在旧序
        // （`Rows::expire` 是它唯一的归位处）。所以这里当场把画的顺序对齐真值
        // （M305 r1 P2-3；今天下载页没有排序切换，属机制级隐患）。
        rows.current = ids.iter().map(|id| (*id).to_owned()).collect();
        if left == 0 && arrived == 0 {
            rows.order.clone_from(&rows.current);
        }
        rows.is_animating()
    }

    /// 这一帧该画哪些行（顺序即画出来的顺序，退场行还钉在原位）。
    pub fn rows_drawn(&self, list: List) -> Vec<&str> {
        self.rows.of(list).drawn()
    }

    /// 定档 / 复位行的两条清单（`Pump::configure` 用；两张列表一起清）。
    pub fn reset_rows(&mut self) {
        self.rows.clear();
    }

    /// 这一帧某一行画成什么样（视图逐行采样）。
    pub fn row_frame(&self, list: List, id: &str) -> RowFrame {
        self.rows
            .of(list)
            .frame(id, self.elapsed_ms(), row_spec().slide.dp())
    }

    /// 行轨道在不在跑（日志与断言用；`needs_frames` 走的是 `is_animating`）。
    #[allow(dead_code)]
    pub fn rows_animating(&self, list: List) -> bool {
        self.rows.of(list).is_animating()
    }

    /// 到期清理（`Pump::expire` 调）：轨道走完就删，退场行这时才真的从画的顺序里移除。
    pub(super) fn expire_rows(&mut self, now_ms: f64) {
        self.rows.expire(now_ms);
    }

    /// 进 / 出两条腿的时长（毫秒，已按档缩放）——测试与日志用，产品路径不读。
    #[cfg(test)]
    pub fn row_leg_ms(&self, entering: bool) -> f64 {
        let spec = row_spec();
        let leg = if entering { spec.enter } else { spec.exit };
        f64::from(self.scale.ms(leg.duration))
    }
}
