//! 取证开关（`ULL_AGENT_*`）：三档档位 / 展开宽度 / 会话夹具。
//!
//! ## 与 M104 的区别：这里不再有「样本会话」
//!
//! M104 的 `demo::transcript()` 是**内置会话样本**，右栗出厂就画着四条假对话——那是「壳先搭好、
//! 等 M103 接」的过渡态。M119 接线之后会话的唯一来源是 presenter，于是样本只能以**真链路**的
//! 方式进来：夹具不造消息，而是**走一次真的提交**（`agent_submit` → transport → 事件队列 →
//! `Message::Agent(Events)` → `apply`）。跑什么脚本由 `ULL_AGENT_SCRIPT`（见
//! [`crate::app::state::presenter`]）决定，夹具只决定「要不要在启动时跑这一轮」。
//!
//! 于是出厂态是**空会话**（空态引导卡）——这才是没有模型的启动器该有的样子。
//!
//! ## 取值表
//!
//! | 变量 | 取值 | 作用 |
//! |---|---|---|
//! | [`RAIL_ENV`] | `collapsed` / `expanded` / `detached` | 启动时直接落到某一档（三态矩阵） |
//! | [`FIXTURE_ENV`] | `turn` / `unread` / `focused` / `empty` | 启动时跑一轮真提交 / 折叠态跑（留未读）/ 键盘焦点环 / 什么都不做 |
//! | [`WIDTH_ENV`] | `280` / `320` / `360` | 展开档宽度（磁铁档位的对照图） |
//!
//! 解析失败一律**忽略**（取证开关不该让进程起不来），并在 stderr 留一行，方便对照日志读图。

use super::metrics::PRESETS;
use super::state::{AgentUi, RailState};

/// 启动档位。
pub const RAIL_ENV: &str = "ULL_AGENT_RAIL";
/// 会话夹具（`turn` / `unread` / `focused` / `empty`）。
pub const FIXTURE_ENV: &str = "ULL_AGENT_FIXTURE";
/// 展开档宽度（只接受三档预设值）。
pub const WIDTH_ENV: &str = "ULL_AGENT_WIDTH";

/// 夹具：启动时对**真 presenter** 做什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fixture {
    /// 什么都不做（出厂态：空会话）。`ULL_AGENT_FIXTURE=empty` 显式写出来也一样。
    None,
    /// 跑一轮真提交（脚本由 `ULL_AGENT_SCRIPT` 决定：问答 / 断流 / 写操作 / 两种失败）。
    Turn,
    /// 折叠态跑一轮真提交：于是未读会真的被 presenter 累加（不是硬编码的数字）。
    Unread,
    /// 键盘巡航一次的样子（焦点环 = 10% 状态层）。
    Focused,
    /// 跑一轮真提交，然后**按一次 `Tab`**（M119 r2 出图用）。
    ///
    /// 复现的是键盘用户走到确认条的路径：待确认出现 ⇒ 焦点已落在「允许」但**键盘闸没开**
    /// （图 `05`，看不到环）；按一次 `Tab` 才算表态（图 `09`，环亮出来且焦点不动）。
    Armed,
}

/// 夹具里的那句用户消息（**演示数据**，不是界面文案：它跟实例名一样属于样本内容，
/// 不进 i18n 词表——接上真模型之后它就不存在了）。
pub const DEMO_QUESTION: &str = "Which instance should I start for a 1.20.4 test?";

/// 按环境变量构造一份 UI 状态 + 夹具（没设任何变量 = `None`，调用方用出厂态）。
pub fn from_env() -> Option<(AgentUi, Fixture)> {
    let rail = std::env::var(RAIL_ENV).ok();
    let fixture = std::env::var(FIXTURE_ENV).ok();
    let width = std::env::var(WIDTH_ENV).ok();

    if rail.is_none() && fixture.is_none() && width.is_none() {
        return None;
    }

    let mut ui = AgentUi::new();

    if let Some(raw) = width.as_deref().map(str::trim) {
        match raw.parse::<f32>() {
            Ok(value) if PRESETS.contains(&value) => ui.set_expanded(value),
            _ => eprintln!("[agent] {WIDTH_ENV}={raw:?} 不是预设档位 {PRESETS:?} 之一，忽略"),
        }
    }

    // 档位走**真迁移**（`expand` / `detach` / `collapse`）而不是直接写字段：这样未读的语义
    // （展开即读）在取证图里也成立，不会出现「展开着还挂着 2 条未读」这种自相矛盾的画面。
    // 唯一要单独看徽标的情形用 `ULL_AGENT_FIXTURE=unread` 显式造（折叠态跑一轮）。
    match rail.as_deref().map(str::trim) {
        None => ui.expand(),
        Some(name) => match RailState::from_name(name) {
            Some(RailState::Collapsed) => ui.collapse(),
            Some(RailState::Detached) => ui.detach(),
            Some(RailState::Expanded) => ui.expand(),
            None => eprintln!(
                "[agent] {RAIL_ENV}={name:?} 不是三档之一（{}），忽略",
                RailState::ALL.map(RailState::name).join("/")
            ),
        },
    }

    let fixture = match fixture.as_deref().map(str::trim) {
        Some("turn") => Fixture::Turn,
        Some("unread") => Fixture::Unread,
        Some("focused") => Fixture::Focused,
        Some("armed") => Fixture::Armed,
        Some("empty") | None => Fixture::None,
        Some(other) => {
            eprintln!("[agent] {FIXTURE_ENV}={other:?} 不是夹具之一，按 empty 处理");
            Fixture::None
        }
    };

    eprintln!(
        "[agent] rail={} width={} fixture={fixture:?}",
        ui.rail.name(),
        ui.expanded(),
    );

    Some((ui, fixture))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_question_is_plain_ascii_so_a_clean_ci_has_the_glyphs() {
        assert!(
            DEMO_QUESTION.is_ascii(),
            "夹具正文不带非 ascii（干净 CI 无中文字体）"
        );
    }

    #[test]
    fn every_rail_state_name_round_trips_through_the_env_parser() {
        for state in RailState::ALL {
            assert_eq!(RailState::from_name(state.name()), Some(state));
        }
        assert_eq!(RailState::from_name("nope"), None);
    }
}
