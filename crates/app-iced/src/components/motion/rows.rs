//! 列表行增删的**公开采样口**（M305；M313 起按列表分账）：页面只拿得到「这一帧这个 id
//! 画成什么样」。
//!
//! ## 谁起跑 / 谁采样
//!
//! | 侧 | 函数 | 干什么 |
//! |---|---|---|
//! | update | [`observe`] | 报一份**真值清单**（模型此刻的行 id，按显示顺序）+ 哪张列表 |
//! | update | [`relayout`] | 报「布局换了」（切视图 / 换磁贴尺寸）：清单没变但摆放变了，重走进场腿（M461） |
//! | view | [`drawn`] | 这一帧该画哪些 id（退场行还钉在原来的位置） |
//! | view | [`frame`] | 这一帧某个 id 的可见度 + 纵向偏移 |
//!
//! **起跑必须在 update 侧**：iced 先算订阅表、后建视图（`app/subscription.rs` 的模块文档有源码
//! 行号），在 `view` 里插轨道当拍订阅不上——那正是 M192 r1 复审 P1-1 的原始缺陷。
//!
//! ## 为什么每个函数都要一个 [`List`]
//!
//! 进程里只有**一条**时间轴，但有两张列表在用行增删动效（下载页的版本列表、实例页的实例列表）。
//! 清单按列表分账（`pump/lists.rs` 的模块文档写了「不分账会怎样」），而进 / 出 / 错位 / 到期
//! 那一套算术只有一份。所以「哪张列表」是**入参**，不是第二套实现。
//!
//! ## 退场行为什么要「留一份数据」
//!
//! [`drawn`] 会吐出**已经不在模型里**的 id（退场腿还在跑）。它们的行数据得由调用方留着
//! （`pages/download/model.rs` 的 `leaving` 池、`pages::Instances::leaving`），否则那一路只能
//! 画一个空壳。这笔账是「删除项淡出后才真正移除」的代价：退场行必须还能被画出来。
//!
//! ## `MotionScale::NONE`
//!
//! [`observe`] 不插轨道、`order` 直接等于真值 ⇒ [`drawn`] 就是模型本身、[`frame`] 恒为
//! [`RowFrame::REST`]（可见度**严格** 1、偏移**严格** 0）⇒ `needs_frames()` 不会因为列表变化
//! 而为真（一帧都不订阅）。

use std::hash::{Hash as _, Hasher as _};

pub use super::pump::List;
use super::pump::RowFrame;
use super::PUMP;

/// **update 侧**：报一份真值清单（模型此刻的行 id，按显示顺序）。
///
/// 幂等：同一份清单再报一次是空操作。返回 `true` = 真的起跑了轨道（帧基准由本模块重置）。
pub fn observe(list: List, ids: &[&str]) -> bool {
    let started = PUMP.with(|pump| pump.borrow_mut().observe_rows(list, ids));
    if started {
        super::reset_frame_clock();
    }
    started
}

/// **view 侧**：这一帧该画哪些 id（退场行还钉在原来的位置）。只采样、无副作用。
pub fn drawn(list: List) -> Vec<String> {
    PUMP.with(|pump| {
        pump.borrow()
            .rows_drawn(list)
            .into_iter()
            .map(str::to_owned)
            .collect()
    })
}

/// **view 侧**：这一帧某个 id 的可见度 + 纵向偏移。只采样、无副作用。
pub fn frame(list: List, id: &str) -> RowFrame {
    PUMP.with(|pump| pump.borrow().row_frame(list, id))
}

/// 行 id → 键控列的 key（`keyed::Column` 要求 `Key: Copy + PartialEq`）。
///
/// 只要求**进程内稳定**（同一 id 每帧算出同一个数）：`DefaultHasher` 不保证跨进程/跨版本一致，
/// 而这里要的正是「同一份视图前后两帧能不能认出是同一行」。
///
/// 为什么是**一处**：下载页与实例页都要把 id 折成 key，各写一份就等于两份「同一行」的判据；
/// 哪天其中一份改成别的哈希，那张表的节点复用就静默失效（表照样画得出来，只是每帧整表重建）。
pub fn key(id: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

/// 行轨道在不在跑（日志与断言用；`needs_frames` 走的是泵的 `is_animating`）。
#[allow(dead_code)]
pub fn running(list: List) -> bool {
    PUMP.with(|pump| pump.borrow().rows_animating(list))
}

/// **update 侧**：布局换了（切视图 / 换磁贴尺寸）⇒ 在册行全部重走一遍进场腿（M461）。
///
/// 与 [`observe`] 的唯一区别是**触发时机**：那条的触发是「真值清单变了」，这条是「摆放变了」。
/// 清单不变（行还是那几行、顺序也没变）时差分判据永远看不见布局变化，所以必须显式报一声。
/// 轨道本身**一条都不新写**：腿长 / 曲线 / 错位 / 到期删除全是 [`super::pump`] 里行轨道那一份
/// （论证在 `pump/layout.rs` 的模块文档「为什么这不是第二套动效系统」）。
///
/// **只在档位真的变了那一拍调**：本函数没有差分表（泵不知道上一次的列数，记一份就等于
/// 第二处「当前视图」的真相）。返回 `true` = 真的起跑了（帧基准已由本模块重置）。
///
/// `MotionScale::NONE` 时返回 `false` 且**不插轨道**：零帧，界面当场就是终态。
pub fn relayout(list: List) -> bool {
    let started = PUMP.with(|pump| pump.borrow_mut().replay_rows(list));
    if started {
        super::reset_frame_clock();
    }
    started
}
