//! **布局轨道**（M461）：切视图 / 换磁贴尺寸时，整张列表的每一行（磁贴视图里是每个格子）
//! **重走一遍进场腿**。
//!
//! ## 为什么这不是「第二套动效系统」
//!
//! 这条轨道没有自己的时钟、曲线、错位表或采样器。它复用的就是
//! [`super::rows`] 那一份账：
//!
//! | 这件事 | 谁说话（一处也不新写） |
//! |---|---|
//! | 采样 `from → to`、端点**逐位**等于两端 | [`super::rows::RowTrack::sample`] |
//! | 腿长与曲线（`Short4` 200 ms / `EmphasizedDecelerate`） | [`super::transition::row_spec`] 的 `enter` |
//! | 错位（`Short1` 50 ms，`min(index, max_stagger)` 封顶） | [`super::rows::Stagger`] |
//! | 到期当场删（空闲不烧帧的唯一出口） | [`super::Rows::expire`] |
//! | 「要不要订阅帧」 | `Pump::is_animating` 里已有的 `rows.any_animating()` |
//!
//! 新增的只有**触发时机**：`observe_rows` 的触发是「真值清单变了」，这里是「布局变了」。
//! 而布局变化（6 列 ⇄ 4 列 ⇄ 3 列 ⇄ 一列）在 id 清单上**看不出来**——行还是那几行、顺序也没变，
//! 只有摆放换了。所以它必须由 update 侧显式报一声（`observe_rows` 那条差分永远看不见它）。
//!
//! ## 起点：走到一半的接着走，静息的从 0 起
//!
//! `from` 取**当前采样**（[`super::rows::Rows::visibility_of`]）而不是永远 0：连点两次尺寸
//! chip 时上一腿还没走完，从当前值接着走才不会闪一下。而在静息的行走（可见度 `1.0`）必须
//! 回到 `0`——那正是「切视图时逐格淡入」这个动作本身。若静息的也走 `1 → 1`，那是一段
//! **什么也看不见却照样占满 200 ms 的假动效**：`samples()` 会涨、`needs_frames()` 为真，
//! 而像素一帧都不变（M461 的帧数判据就是拿这条当反例写的）。
//!
//! ## `MotionScale::NONE`
//!
//! **不插轨道**：`row_frame` 恒 [`super::RowFrame::REST`]、`is_animating()` 恒假 ⇒ 一帧都不订阅，
//! 界面当场就是最终状态（与 `observe_rows` 的同一条退化口径，不是「跑一遍 0 时长动画」）。
//!
//! ## 为什么实现在这里而不是 `rows.rs`
//!
//! `rows.rs` 收工在 400 行硬线上（M461 接手时 371 行）。`impl Rows` 写在本模块里是 Rust 允许的
//! （同一个 crate 内），代价只是那几项要提到 `pub(super)`；好处是「布局」这条触发时机的论证
//! 与代码放在一起，且 `rows.rs` 一行不涨。仓内先例：`pump/lists.rs` 拆出去守同一条硬线。

use super::super::transition::row_spec;
use super::lists::List;
use super::rows::{Rows, Stagger};
use super::Pump;

impl Rows {
    /// 在册行**全部**重走一遍进场腿。返回 `true` = 真的起了轨道。
    ///
    /// `off` = `MotionScale::NONE`（不插轨道、零帧）；`start_ms` / `step_ms` / `leg_ms` 都
    /// **已经按档位缩放好**（缩放口只有 `MotionScale`，见 `pump/mod.rs::leg_ms`）。
    ///
    /// 只见 `order`（这一帧该画的次序，退场行还钉在里面）：布局换了的时候连正在退场的行
    /// 也一起重放——它们本来就还在画，突然不动比一起重放更显眼。
    pub(super) fn replay(
        &mut self,
        now_ms: f64,
        start_ms: f64,
        step_ms: f64,
        leg_ms: f64,
        off: bool,
    ) -> bool {
        if off || !self.seeded {
            return false;
        }
        let leg = row_spec().enter;
        let stagger = Stagger { start_ms, step_ms };
        let starts: Vec<(String, f32)> = self
            .order
            .iter()
            .map(|id| (id.clone(), self.reenter_from(id, now_ms)))
            .collect();
        for (index, (id, from)) in starts.iter().enumerate() {
            self.retarget(id, *from, 1.0, leg, leg_ms, stagger, index);
        }
        self.is_animating()
    }

    /// 重走进场时的起点：静息（`1.0`）的从 `0` 起，走到一半的接着当前采样走（不跳变）。
    ///
    /// 两个分支都必要，见模块文档「起点」。**不许**改成恒 `0`（连点两次尺寸会闪一下）
    /// 或恒 `track.from`（静息行的 `from` 是 `None` ⇒ 判断不出「它已经在静息」）。
    fn reenter_from(&self, id: &str, now_ms: f64) -> f32 {
        let at = self.visibility_of(id, now_ms);
        if at >= 1.0 {
            0.0
        } else {
            at
        }
    }
}

impl Pump {
    /// 布局换了（切视图 / 换磁贴尺寸）：在册行全部重走一遍进场腿。
    ///
    /// 返回 `true` = 真的起跑了（调用方要重置帧基准，见 [`super::MAX_STEP_MS`]）。
    ///
    /// **幂等性由调用方负责**：只在「档位真的变了」那一拍调——`observe_rows` 那条有差分表兜着，
    /// 这条没有（泵不知道上一次的列数是多少，而记一份就等于第二处「当前视图」的真相）。
    /// 两个调用点（`app/update/instances/view_mode.rs`）本来就各有一道「值没变就返回」的守卫。
    pub fn replay_rows(&mut self, list: List) -> bool {
        let spec = row_spec();
        let start_ms = self.start_now();
        let step_ms = self.leg_ms(spec.stagger);
        let leg_ms = self.leg_ms(spec.enter.duration);
        let now = self.elapsed_ms();
        let off = self.scale.is_off();
        self.rows
            .of_mut(list)
            .replay(now, start_ms, step_ms, leg_ms, off)
    }
}
