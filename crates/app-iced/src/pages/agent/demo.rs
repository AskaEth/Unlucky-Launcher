//! 取证开关（`ULL_AGENT_*`）：三档档位 / 展开宽度 / 会话夹具。
//!
//! ## 与 M104 的区别：这里不再有「样本会话」
//!
//! M104 的 `demo::transcript()` 是**内置会话样本**，右栗出厂就画着四条假对话——那是「壳先搭好、
//! 等 M103 接」的过渡态。M119 接线之后会话的唯一来源是 presenter，于是样本只能以**真链路**的
//! 方式进来：夹具不造消息，而是**走一次真的提交**（`agent_submit` → transport → 事件队列 →
//! `Message::Agent(Events)` → `apply`）。跑什么脚本由 `ULL_AGENT_SCRIPT`（见
//! `transport.rs::SCRIPT_ENV`）决定，夹具只决定「要不要在启动时跑这一轮」。
//!
//! 于是出厂态是**空会话**（空态引导卡）——这才是没有模型的启动器该有的样子。
//!
//! ## 取值表
//!
//! | 变量 | 取值 | 作用 |
//! |---|---|---|
//! | [`RAIL_ENV`] | `collapsed` / `expanded` / `detached` | 启动时直接落到某一档（三态矩阵） |
//! | [`RAIL_ENV`] | [`EXIT_COLLAPSED`] | 展开后**同一拍**收起 ⇒ 折叠轨道真的从 1.0 起跑（M279） |
//! | [`FIXTURE_ENV`] | `turn` / `unread` / `focused` / `empty` | 启动时跑一轮真提交 / 折叠态跑（留未读）/ 键盘焦点环 / 什么都不做 |
//! | [`FIXTURE_ENV`] | `timeout` / `stop` / `retry` | **M388 的三张图**：等空闲时限判超时 / 等几段正文后按停止 / 超时后同一次运行里再发一句 |
//! | [`WIDTH_ENV`] | `280` / `320` / `360` | 展开档宽度（磁铁档位的对照图） |
//!
//! 解析失败一律**忽略**（取证开关不该让进程起不来），并在 stderr 留一行，方便对照日志读图。
//!
//! ## 为什么需要 [`EXIT_COLLAPSED`]（M279）
//!
//! 前三个取值都是**静息落档**：`expanded` 从出厂折叠态展开 ⇒ 真的起一条展开轨道（所以展开腿
//! 拍得出来），而 `collapsed` 在出厂就是折叠态时调 `collapse()` 是**空操作**（`state.rs` 的
//! `collapse()` 只在档位是 `Expanded` 时才改档 + `observe_rail(false)`）⇒ **一条折叠轨道都不起**。
//! 于是配 `ULL_MOTION_AT` 拍「折叠中途」时，拍到的只是「静息折叠 + 页面入场位移」，而折叠腿
//! 恰好是「页面模型比物理区更宽」的那一侧（`content_width` 吃目标档、`rail_width` 吃动画进度），
//! 是最可能出现右侧裁切/重叠的方向。`exit-collapsed` 就是补这条腿的取证开关：
//! 先 `expand()`、把展开这一腿**落成静息**（`snap_rail`），再在同一拍 `collapse()`。
//!
//! 为什么中间那一步 `snap_rail(true)` 不能省：钉住相位时（`ULL_MOTION_AT`）两条轨道共用同一个
//! 起点（`Pump::start_now()` 恒为 0），若不 snap，收起那一腿会从展开的**半路**反向起跑——三张相位图
//! 就不是同一条 1 → 0 的腿，位移表也就没法与展开腿对照了。

use super::metrics::PRESETS;
use super::state::{AgentUi, RailState};

use crate::app::components::motion;
use crate::app::diag::diag;

/// 启动档位。
pub const RAIL_ENV: &str = "ULL_AGENT_RAIL";
/// 会话夹具（`turn` / `unread` / `focused` / `empty`）。
pub const FIXTURE_ENV: &str = "ULL_AGENT_FIXTURE";
/// 展开档宽度（只接受三档预设值）。
pub const WIDTH_ENV: &str = "ULL_AGENT_WIDTH";

/// **真的会回档**的档位夹具名（见模块文档的「为什么需要它」，M279）。
pub const EXIT_COLLAPSED: &str = "exit-collapsed";

/// 夹具对档位做了什么（日志与用例都要读得出「起没起轨道」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RailSeed {
    /// 落到某个**静息**档（`collapsed` / `expanded` / `detached` / 不设变量）。展开是从出厂折叠态
    /// 真展开，所以这一档里只有 `expanded` 会起轨道。
    Settled(RailState),
    /// 展开到位后同一拍收起：折叠轨道从 1.0 起跑。
    ExitCollapsed,
}

impl RailSeed {
    /// 日志里的名字（`from_env` 那一行；与 `RailState::name` 分开，因为落点档位是同一个折叠档）。
    const fn name(self) -> &'static str {
        match self {
            RailSeed::Settled(state) => state.name(),
            RailSeed::ExitCollapsed => EXIT_COLLAPSED,
        }
    }
}

/// 把 [`RAIL_ENV`] 的名字落到 `ui` 上（纯函数：用例直接喂名字，不摆环境变量）。
///
/// 档位一律走**真迁移**（`expand` / `detach` / `collapse`）而不是直接写字段：这样未读的语义
/// （展开即读）在取证图里也成立，不会出现「展开着还挂着 2 条未读」这种自相矛盾的画面。
/// 唯一要单独看徽标的情形用 `ULL_AGENT_FIXTURE=unread` 显式造（折叠态跑一轮）。
fn seed_rail(ui: &mut AgentUi, name: Option<&str>) -> RailSeed {
    let name = name.map(str::trim);

    if name == Some(EXIT_COLLAPSED) {
        ui.expand();
        // 展开这一腿落成**静息**（不是「等它跑完」）：见模块文档里 snap 的理由。
        motion::snap_rail(true);
        ui.collapse();
        return RailSeed::ExitCollapsed;
    }

    match name {
        None => {
            ui.expand();
            RailSeed::Settled(RailState::Expanded)
        }
        Some(raw) => match RailState::from_name(raw) {
            Some(RailState::Collapsed) => {
                ui.collapse();
                RailSeed::Settled(RailState::Collapsed)
            }
            Some(RailState::Detached) => {
                ui.detach();
                RailSeed::Settled(RailState::Detached)
            }
            Some(RailState::Expanded) => {
                ui.expand();
                RailSeed::Settled(RailState::Expanded)
            }
            None => {
                eprintln!(
                    "[agent] {RAIL_ENV}={raw:?} 不是三档之一（{}）也不是 {EXIT_COLLAPSED}，忽略",
                    RailState::ALL.map(RailState::name).join("/")
                );
                // 忽略 = 一动不动（出厂就是折叠档），与 M257 之前的语义逐字一致。
                RailSeed::Settled(ui.rail)
            }
        },
    }
}

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
    /// **超时**（M388 图1）：跑一轮真提交，然后**阻塞等**内核的空闲时限把它判成超时。
    ///
    /// 等的是产品路径本身（`brain::guard` 的判据 + `drain_agent` 的结论落地），不是伪造一个状态。
    Timeout,
    /// **停止**（M388 图2）：跑一轮真提交，等头几段正文上屏之后按一次「停止」。
    ///
    /// 「按」= 派一条 `agent::Message::Stop`，**两处调用点是同一对**：按钮的
    /// `Button::new(catalog.text(Key::AgentStop), Message::Stop)`（`conversation/composer.rs`）
    /// 与夹具的 `self.on_agent(agent::Message::Stop)`，落点都是 `App::on_agent` 的
    /// `Message::Stop` 那一支（`app/update/agent.rs`）。本机没有前台窗口，合成的鼠标事件递不进
    /// iced（`app/update/motion_sync.rs` 写了原因），所以这里替代的是**输入通道**，不是逻辑。
    /// 复审 r1 P1-1 的教训：上一版夹具直接调了 `stop_agent`，把「消息怎么递进来」那一层跳过去了
    /// ——而那正是这张图要证的东西。
    Stop,
    /// **取消后重试**（M388 图3）：第一句超时，紧接着**同一次运行里**再发一句。
    ///
    /// 桩的第二次请求正常回（`stub_provider.py hang-first`），所以这张图同时证两件事：
    /// 超时后能再发、且回的是正常回答（没重启、没手动清状态）。
    Retry,
}

/// 夹具里的那句用户消息（**演示数据**，不是界面文案：它跟实例名一样属于样本内容，
/// 不进 i18n 词表——接上真模型之后它就不存在了）。
pub const DEMO_QUESTION: &str = "Which instance should I start for a 1.20.4 test?";

/// 图1/图3 起跑后最多等多久让「超时」这一档落地（真后端 + 沉默桩 ⇒ 由内核的空闲时限决定，
/// 取证时用 `ULL_AGENT_TIMEOUT_SECS=5` 把它压到 5 秒）。
pub const TIMEOUT_WAIT_MS: u64 = 20_000;

/// 图2：提交之后等多久再按「停止」（要留出几段正文上屏，否则图上看不出「停在半句话上」）。
pub const STOP_AFTER_MS: u64 = 900;

/// 图3：超时之后再发一句，最多等多久让正常回答上屏。
pub const RETRY_WAIT_MS: u64 = 15_000;

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
    let seed = seed_rail(&mut ui, rail.as_deref());

    let fixture = match fixture.as_deref().map(str::trim) {
        Some("turn") => Fixture::Turn,
        Some("unread") => Fixture::Unread,
        Some("focused") => Fixture::Focused,
        Some("armed") => Fixture::Armed,
        Some("timeout") => Fixture::Timeout,
        Some("stop") => Fixture::Stop,
        Some("retry") => Fixture::Retry,
        Some("empty") | None => Fixture::None,
        Some(other) => {
            eprintln!("[agent] {FIXTURE_ENV}={other:?} 不是夹具之一，按 empty 处理");
            Fixture::None
        }
    };

    // `seed=` 是**唯一**能分辨「静息落档」与「真的起了一条折叠轨道」的地方（两者的 `rail` 都是
    // `collapsed`）：`exit-collapsed` 出现在这一行 = 折叠腿的相位图确实拍的是运动。
    diag!(
        "[agent] rail={} width={} fixture={fixture:?} seed={}",
        ui.rail.name(),
        ui.expanded(),
        seed.name(),
    );

    Some((ui, fixture))
}

#[cfg(test)]
mod tests {
    use unlucky_design::motion::MotionScale;

    use super::super::metrics::{COLLAPSED_WIDTH, DEFAULT_EXPANDED_WIDTH};
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

    // ---------------------------------------------------------- M279：折叠腿的夹具

    /// 取证窗口宽（与 D30 的产品默认一致，位移表的坐标系就是它）。
    const WINDOW: f32 = 1180.0;
    /// 帧泵的单帧步进（`motion::FRAME_MS` 的测试口径）。
    const FRAME: f64 = 16.0;

    /// 一条**真的会跑起来**的折叠轨道：进度从 1.0 起、严格递减、末端严格 0。
    ///
    /// 抓得住什么（每一条都有对应变异）：
    /// ①`exit-collapsed` 改回只调 `collapse()`（出厂就是折叠档 ⇒ 空操作）⇒ 「有轨道吗」当场红，
    ///   这正是 M257 r2 复审抓到的旧行为（折叠腿一张相位图都拍不出来）；
    /// ②删掉中间的 `snap_rail(true)` ⇒ 起跑那一帧不是严格 1.0 红（收起会从展开的半路反向）；
    /// ③起点改成硬跳/终点不归零 ⇒ 「第一帧 320」与「末端严格 0」两条红；
    /// ④时长或曲线改了 ⇒ 帧数区间与严格单调两条红。
    ///
    /// 抓不住什么：**像素**。这条只证时间轴真的在跑；「折叠中途页面右侧有没有被右栗压住/裁切」
    /// 只能出图看（M279 的 `DevDocs/artifacts/railexit/`）。
    #[test]
    fn exit_collapsed_really_starts_the_collapse_leg_from_full_open() {
        motion::reset();
        motion::configure_for_test(MotionScale::STANDARD, None);

        let mut agent = AgentUi::new();
        assert_eq!(agent.rail, RailState::Collapsed, "前提：出厂就是折叠档");

        assert_eq!(
            seed_rail(&mut agent, Some(EXIT_COLLAPSED)),
            RailSeed::ExitCollapsed
        );
        assert_eq!(
            agent.rail,
            RailState::Collapsed,
            "落点仍是折叠档（夹具改的是**过程**，不是落点）"
        );

        assert!(
            motion::needs_frames(),
            "没有轨道 ⇒ 三张相位图会全是同一张（M257 r2 复审判定的旧行为）"
        );
        assert_eq!(
            agent.collapse_progress(),
            1.0,
            "起跑那一帧必须在展开档（严格 1.0，不是 0.99x）"
        );
        assert_eq!(agent.rail_width(WINDOW), DEFAULT_EXPANDED_WIDTH);

        let mut progress = vec![agent.collapse_progress()];
        let mut frames = 0;
        while motion::needs_frames() {
            motion::advance_by(FRAME);
            progress.push(agent.collapse_progress());
            frames += 1;
            assert!(frames < 60, "折叠轨道没停，疑似终止条件失效");
        }

        assert_eq!(
            progress.last().copied(),
            Some(0.0),
            "末端进度必须**严格** 0（= 48 图标条）"
        );
        assert_eq!(agent.rail_width(WINDOW), COLLAPSED_WIDTH);
        assert!(
            (10..=20).contains(&frames),
            "200ms / 16ms 应落在 10~20 帧，实测 {frames} 帧"
        );
        for pair in progress.windows(2) {
            assert!(pair[1] < pair[0], "进度必须严格递减：{pair:?}");
        }
        assert!(
            progress.iter().filter(|p| **p > 0.02 && **p < 0.98).count() >= 8,
            "必须有足够多的中间相位（否则就是硬切）：{progress:?}"
        );
        motion::reset();
    }

    /// 原有的 `collapsed` / `expanded` 两个值**语义一字不改**（M257 的取证依赖它们）。
    ///
    /// `collapsed` 仍然是「静息折叠、一条轨道都不起」——这一条正是 M279 要补的那个缺口，
    /// 所以它必须继续成立：改它就等于悄悄换掉 M257 那批图的含义。
    #[test]
    fn the_old_rail_values_keep_their_exact_meaning() {
        motion::reset();
        motion::configure_for_test(MotionScale::STANDARD, None);

        let mut agent = AgentUi::new();
        assert_eq!(
            seed_rail(&mut agent, Some("collapsed")),
            RailSeed::Settled(RailState::Collapsed)
        );
        assert_eq!(agent.collapse_progress(), 0.0, "静息折叠的进度严格 0");
        assert!(
            !motion::needs_frames(),
            "`collapsed` 不许起轨道（出厂就是折叠档 ⇒ `collapse()` 是空操作）"
        );

        motion::reset();
        motion::configure_for_test(MotionScale::STANDARD, None);
        let mut agent = AgentUi::new();
        assert_eq!(
            seed_rail(&mut agent, Some("expanded")),
            RailSeed::Settled(RailState::Expanded)
        );
        assert_eq!(
            agent.collapse_progress(),
            0.0,
            "展开腿的第一帧仍在折叠档（轨道从 48 起跑）"
        );
        assert!(motion::needs_frames(), "`expanded` 起的是**展开**轨道");

        // 不认识的名字：一动不动（出厂折叠档），与 M257 之前的语义一致。
        motion::reset();
        motion::configure_for_test(MotionScale::STANDARD, None);
        let mut agent = AgentUi::new();
        assert_eq!(
            seed_rail(&mut agent, Some("collapsed-plus")),
            RailSeed::Settled(RailState::Collapsed)
        );
        assert!(!motion::needs_frames(), "认不出的取值一律不动");
        motion::reset();
    }
}
