//! 行增删轨道的**按列表分账**（M313）：一套 [`Rows`] 逻辑、每张列表各自一份账。
//!
//! ## 为什么必须分账
//!
//! `Pump` 是进程内唯一的时间轴，而 M305 的 `Rows` 只有**一份**清单（`order` / `current` /
//! `tracks`）。实例页（M313）接上来之后，同一拍里两张列表都会报自己的真值清单：共用一个 `Rows`
//! 就等于「版本列表刚喂进来 12 个 id，实例列表下一拍喂进来 7 个」——两张列表互相把对方的行判成
//! 退场，整表闪跳、永远停不下来。所以清单的账**必须按列表分开**，而进 / 出 / 错位 / 到期那套
//! 算术仍然只有一份（[`super::rows`]）。
//!
//! ## 为什么不写两份 `Rows`
//!
//! 那是 M305 明确禁止的形状（「不许把动效逻辑再写一份」）：两份 `Rows` 字段 + 两套 `Pump` 方法
//! 之后，任何一处修（例如 r1 P2-1 的「退场中途回来接着当前采样走」）都要记得改两遍。
//! 这里只多一个**枚举 + 两个字段的选择器**，逻辑一行没复制。
//!
//! ## `MotionScale::NONE`
//!
//! 分账不影响退化：两张列表各自走 [`super::rows::Rows`] 的同一条 `is_off()` 分支，
//! 关掉动效时两张都**立刻**是最终状态，`any_animating()` 恒假。

use super::rows::Rows;

/// 哪一张列表在用行增删轨道。
///
/// 取值点只有两个（各一处）：下载页 [`List::Versions`]（`pages/download/model.rs`）、
/// 实例页 [`List::Instances`]（`app/view.rs::model`）。加第三张列表 = 在这里加一档 +
/// [`Lists`] 加一份字段，**不要**另起一套轨道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    /// 下载页的版本列表（M305 接线）。
    Versions,
    /// 实例页的实例列表（M313 接线）。
    Instances,
}

/// 两张列表各一份 [`Rows`]（同一套逻辑、各自的时间轴与清单）。
#[derive(Debug)]
pub(crate) struct Lists {
    versions: Rows,
    instances: Rows,
}

impl Lists {
    /// 出厂态：两张空账、没有轨道、都没对过表。
    pub(super) const fn new() -> Self {
        Self {
            versions: Rows::new(),
            instances: Rows::new(),
        }
    }

    /// 某一张列表的账（只读）。
    pub(super) fn of(&self, list: List) -> &Rows {
        match list {
            List::Versions => &self.versions,
            List::Instances => &self.instances,
        }
    }

    /// 某一张列表的账（可写）。
    pub(super) fn of_mut(&mut self, list: List) -> &mut Rows {
        match list {
            List::Versions => &mut self.versions,
            List::Instances => &mut self.instances,
        }
    }

    /// **任意一张**列表还有轨道在跑（= 要不要订阅帧）。
    pub(super) fn any_animating(&self) -> bool {
        self.versions.is_animating() || self.instances.is_animating()
    }

    /// 定档 / 复位（`Pump::configure` 与测试的 `reset`）。
    pub(super) fn clear(&mut self) {
        self.versions.clear();
        self.instances.clear();
    }

    /// 到期清理（两张都清，各自的「到期即删」出口留在 [`super::rows::Rows::expire`]）。
    pub(super) fn expire(&mut self, now_ms: f64) {
        self.versions.expire(now_ms);
        self.instances.expire(now_ms);
    }
}
