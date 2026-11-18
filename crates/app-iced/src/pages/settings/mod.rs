//! 设置页：**supporting-pane** 的用例（左侧分组导航 240 + 右侧表单列 ≤424）。
//!
//! ## 为什么是 supporting-pane 而不是 list-detail
//!
//! 分组导航是**辅助**（它不承载数据，只切换右侧表单的内容），右侧表单才是阅读对象；
//! 官方把这种「主列 + 窄侧列」叫 supporting pane。list-detail 的前提是左列本身是一份可浏览的数据集
//! （实例列表就是），设置页不满足。
//!
//! ## 侧列与 rail 的关系（D33）
//!
//! 宽度 240 直接取 `LayoutMetrics::nav_drawer_width`（D33：同一容器 rail 88 ⇄ drawer 240 的展开态），
//! 但**不复制 rail 的实现**：这里没有二级标签、没有 `Message::Select`。M299 起分组项也有 hover /
//! pressed 药丸（用户点名的「整个方形区域加深」），但它走设置页自己的 `SelectGroup`，颜色走
//! `interaction::selected_fill` 那个唯一入口（见 `frame::group_item`）。将来真正的
//! `NavigationDrawer` 由 M78 提供，本页换掉 [`frame::group_nav`] 即可。
//!
//! ## 退化（窗口 <720 或 compact 档）
//!
//! 侧列消失，分组导航退化成「按组堆叠」：同一列里每组一张卡 + 一段 12pt 组名。整页仍居中、正文列仍 ≤424。
//!
//! ## 文件地图（M99 `SETTINGSFRAME`：**纯搬移**，行为与视觉零变化）
//!
//! 本文件只做三件事：**分组注册表**（[`GROUPS`]）、**内容分发**（[`form`] 按分组挑渲染器）、
//! 以及 M101 加的两条漏斗的**转发**（消息 → [`message`]，状态 → [`state`]）。
//! 一组的正文、文案、演示行、状态、消息都在它自己的文件里：
//!
//! | 分组 | 文件（正文 / 状态 / 消息） | 谁会动它 |
//! |---|---|---|
//! | General | [`general`] | language+appearance 线（语言下拉 + 主题 / 启动页） |
//! | Appearance | [`appearance`] | language+appearance 线（真控件已在 `pages/appearance/section/`） |
//! | Java | [`java`] | java 线 |
//! | Downloads | [`download`] | download 线 |
//! | Updates | [`update`] | update 线（M159，按下标分发） |
//! | Assistant | [`ai_about`]（正文在 [`ai`]） | ai 线（M174，按下标分发：本组要 `Lang`） |
//! | 页头 / 侧列 / 滚动区 / 演示卡片 | [`frame`] | 谁都不该改（改了就是改版面） |
//!
//! ## 测试也按 section 分（M100）
//!
//! 断言跟着它守的东西走，于是四条 section 线**各改各的测试**：
//!
//! | 测试文件 | 守什么 | 谁会动它 |
//! |---|---|---|
//! | `tests/general_section.rs` | General 自己的行 / 图标 / body 现状 | language+appearance 线 |
//! | `tests/appearance_section.rs` | Appearance 自己的行 / 图标 / body 现状 | language+appearance 线 |
//! | `tests/java_section.rs` | Java 自己的行 / 图标 / body 现状 | java 线 |
//! | `tests/download_section.rs` | Downloads 自己的行 / 图标 / body 现状 | download 线 |
//! | `tests/mod.rs` | **跨 section**：版面契约、行宽上限、窄窗退化、注册表顺序 / 分组下标 / 每条线的漏斗安装情况 | 谁都别随手改（每条都写了为什么不能下放） |
//!
//! 判据：**给某一组加一条新断言 = 只改它自己那个测试文件**。
//!
//! **加一个分组要动哪里**：① 在它自己的文件里写一个 `pub(super) const GROUP`（名字 / 图标 / 演示行 /
//! 可选的真控件渲染器 `body`）；② 本文件的 [`GROUPS`] 加一行、数组长度 +1；③ [`state::Groups`] 加一个
//! 字段、[`message::GroupMessage`] 加一个变体、`app/update/settings.rs` 加一条臂。**不需要**动 `frame.rs`、
//! 别的分组文件或 `pages/mod.rs`。
//!
//! **M159 的第五组（更新）是另一条形状**：它不持有组状态、也不走 `GroupMessage`，而是读
//! `settings::view` 的 `update` 形参（presenter 快照），由 [`UPDATE_GROUP`] 按下标分发——与 M106 的
//! 语言行同一取舍（那两样都不是「某一组的内部状态」）。所以加它只动了 ① `pages/settings/update.rs`、
//! ② 本文件的 [`GROUPS`] / [`GROUP_LABELS`] / `render` 的形参、③ `pages/mod.rs` 与 `pages/model.rs`
//! （模型多一格 `update`），**没有**碰 `state::Groups` 与 `message::GroupMessage`。
//!
//! ## 消息：一个投递口，四条线各写各的（M101 `SETTINGSMSG`）
//!
//! 四条填页线都要往 `pages::Message` 里加变体，而那是**一个**枚举加上
//! `app/update/page_messages.rs` 里一条不许 `_ =>` 的 match —— 四路并行必然四路冲突。
//! M101 把这件事收成一个投递口：
//!
//! ```text
//! pages::Message::Settings(GroupMessage)   ← 全 crate 只有这一个设置页变体
//!   ├─ Java(java::Message)                             → settings/java.rs
//!   ├─ Download(download::Message)                     → settings/download.rs
//!   ├─ LanguageAppearance(language_appearance::Message) → settings/language_appearance.rs
//!   └─ AiAbout(ai_about::Message)                      → settings/ai_about.rs
//! ```
//!
//! 加一支消息 = 改自己那个消息口文件 + 自己那条落点（`app/update/settings/<线>.rs`），2 个文件、
//! 0 个共享文件。四个消息口枚举声明成 `pub` 是为了过 `private_interfaces`：那条 lint 按**类型自身
//! 的声明可见性**判，所以模块用最小的 `pub(crate)`（`pages/mod.rs` 的 `pub(crate) mod settings;`）
//! 就够，不必把整个设置页模块树暴露成 crate 公开 API。
//!
//! ## 状态：与消息同构的状态漏斗（M101 r2，r1 复审 P1）
//!
//! ```text
//! App.settings: state::Groups  ← app/state.rs 一行（M101 加一次，四条线都不碰）
//!   ├─ java / download / language_appearance / ai_about：各自的 State 在各自文件里
//! States { appearance, groups } ← 只读视图，按值传给 view；每组的 GROUP.body 自己取自己要的那份
//! ```
//!
//! **加/改某一组的字段**只改 `pages/settings/<组>.rs`；**异步结果**在自己那条落点里构造
//! `Message::Page(pages::Message::Settings(GroupMessage::<线>(…)))` 回来——`Page` 这条既有通道就是
//! 异步结果的入口，`app::Message` / `app/update.rs` 都不用动。
//!
//! ### 仍然共享的点（四条线都可能碰到，只能靠 tower 串行合并，或者由专人一次性做掉）
//!
//! 1. `crates/i18n/src/catalog/golden.rs` —— 每加一条文案 key 都要按声明顺序往金表里插一行
//!    （`split_keeps_every_translation` 逐条比 key + 英/中译文）。**注意**：`catalog.rs` 的 `Key`
//!    枚举由 `catalog!` 宏从各域文件累积生成，加 key **不用**改 `catalog.rs`，也不用改
//!    `catalog/settings/mod.rs`（五个域已全部接进装配链）。
//! 2. 本文件的 [`GROUPS`] —— 加一整组时加一行（M174 的助手组就是这么来的：`GROUPS` 与
//!    [`GROUP_LABELS`] 各加一行、长度 +1，`tests/mod.rs` 的「左列顺序」随之更新）。
//! 3. `tests/mod.rs` —— 「左列顺序」那条字面量断言会连带红，需要人确认（M174 就是把它加了一个
//!    `"Assistant"`）。加分组自己的测试文件时还要在 `tests/mod.rs` 顶部挂一行 `mod <名>;`；
//!    M174 的用例走的是组内路径（`pages/settings/ai/tests.rs`），所以没有新增挂载行。
//! 4. **读页外真值**（r2 复审 P1 补的）—— [`States`] 只有 `appearance` + `groups` 两格，组正文读不到
//!    `pages::Model`（「当前语言」的值在 `Model.lang`，`pages/model.rs`），也读不到 presenter 的设置
//!    快照。两条路的**真实成本**（`States` 没有 `Default`，加字段必然 E0063，已实测）：
//!    - 把 `model` **塞进 [`States`]** ⇒ struct literal 有 **4 处**（`pages/mod.rs:154/177/232` 与
//!      `app/view.rs:70`），四处都要补字段 ⇒ **3 个共享文件**：`settings/state.rs`、`pages/mod.rs`、
//!      **`app/view.rs`**（最后这个正是 M104 的落点，本 mission 反复设锚点要避开的就是它）；
//!    - 给 `settings::view` **加一个 `model: &Model` 形参**（`pages/mod.rs` 的 `render` 本来就拿着
//!      `model`，直接往下传）⇒ **2 个共享文件**（`settings/mod.rs` + `pages/mod.rs`），`app/view.rs`
//!      不用动 —— **推荐这个形状**。语言也已有真值源（`ui/lang.rs` 的 `App::lang` → `view::model(…)`
//!      → `Model.lang`）：某组读「当前语言」走它或 `Groups` 里本线字段，别再另造一层状态。
//! 5. **首帧取数 / 异步发起**（同上，r2 复审 P1 补）—— [`Body`] 是
//!    `fn(&Ui, States, Lang) -> Option<Element>`，返回不了 `Task`；而且 `settings::view` → `pages::render`
//!    → `App::view` **一路都返回 `Element`**（iced 0.14 的 `view` 也只返回 `Element`，`view(&self)`
//!    又改不了状态），**view 期间产出的 `Task` 没有出口**。所以只有一条路：在 `app/update.rs` 的
//!    `Message::Boot` 分支补一次首帧派发（**1 个共享文件**——`on_boot` 本来就在派
//!    `AppCommand::RefreshAll`）。想让「组自己」发起首帧探测，得先在 app 侧另造一条旁路
//!    （`settings/mod.rs` + `pages/mod.rs` + `app/view.rs` 三处都要动），**不建议**。不改的代价：
//!    首屏显示演示值、等用户点一次才变真值——四条线都能接受就不必动。
//! 6. **隐性代价：`Groups` 必须是 `Sync + Default`**（r2 复审 P3 指出）——
//!    [`demo_groups`] 用 `static LazyLock<Groups>` 撑着 harness 路径，所以某个组的 `State` 里一旦
//!    放了 `Rc` 这类非 `Sync` 字段，编译会在**本模块**（`settings/state.rs`）红。选择 `LazyLock`
//!    是为了让 `page()` / `page_with_appearance()` 的返回 `Element<'a>` 借得到一个 `'static` 的默认
//!    状态；如果哪天必须放非 `Sync` 状态，改法是把默认状态改成「每个调用点自己造一个 `Groups`」
//!    ——`pages/mod.rs` 的**三个**调用点都要改（`page` / `page_with_appearance` /
//!    `app_with_appearance`，行 156 / 179 / 234），同样是**一次性**代价。

// 四条填页线的文件要能被**各自那条落点**（`app/update/settings/<线>.rs`）与状态架子
// （`state::Groups`）命名，所以是 `pub(crate)`：最小面，不是 crate 公开 API。
// 分组正文文件 `appearance` / `frame` / `general` 只在设置页内部用，保持私有。
pub(crate) mod ai;
pub(crate) mod ai_about;
mod appearance;
pub(crate) mod download;
mod frame;
mod general;
pub(crate) mod java;
pub(crate) mod language_appearance;
mod logs_row;
mod message;
mod registry;
mod state;
pub(crate) mod update;

#[cfg(test)]
mod tests;

pub use message::GroupMessage;
pub use state::{demo_groups, Groups, States};

use iced::widget::{button, column};
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Lang, LangChoice};

use crate::app::components::buttons::Button;
use crate::app::components::interaction::Interaction;
use crate::theme::Ui;

use super::layout::{FrameMetrics, PageLayout};
use super::update::Update;
use super::Message;

pub(super) use registry::Group;
use registry::{AI_GROUP, GENERAL_GROUP, GROUPS, GROUP_LABELS, JAVA_GROUP, SELECTED, UPDATE_GROUP};

/// 这一帧某个控件**实际要画**的交互态：`forced`（`--force-state`）优先，否则退回观测到的 `status`。
///
/// **全仓唯一的 resolve 点（M285）**：语言档 / 主题档 / 日志 `action_button` / 外观 `choice` 一律调它
/// ——各自再写一遍 `unwrap_or_else` 就是把同一行语义复制四份，下次只改三处就等着漂。语义与
/// `pages/instances.rs` 的 `forced_state` 相同（`Some` 覆盖、`None` 不动），只是裸 `iced::button` 没有
/// `.state()`，覆盖只能发生在样式闭包**里面**（换掉它拿到的那一态）。
pub(super) fn resolve_state(forced: Option<Interaction>, status: button::Status) -> Interaction {
    forced.unwrap_or_else(|| Interaction::from_status(status))
}

/// 把一个 `components::buttons::Button` 钉到 `forced` 那一态（`pages/instances.rs` 的 `forced_state`
/// 在设置页的这一份）：`Some` ⇒ `.state(..)`，`None` ⇒ 原样返回。
///
/// ⚠️ **优先级是 `state` 赢**（`Button::forced()` = `self.state.or(没有消息 ⇒ Disabled)`，`state` 在前）
/// ⇒ `forced_button(Button::disabled(..), Some(Hovered))` 会把禁用按钮**画成 hover**。今天 7 个站点都
/// 写在**启用分支里**所以安全，**别**套到 `if..else` 外面。第二份不是抽象：三次法则到 2。
pub(super) fn forced_button<'a, M>(
    button: Button<'a, M>,
    forced: Option<Interaction>,
) -> Button<'a, M> {
    match forced {
        Some(state) => button.state(state),
        None => button,
    }
}

/// 设置页 General 分组要的两件（M106）：用户选的档位 + 这一次生效的语言。
/// 为什么不塞进 `pages::Model`：那是页面与 presenter 之间的**展示契约**（由快照派生），而档位是
/// **用户意图**（`App` 的字段）；塞进去会让 `pages::demo_model` 也得伪造一个档位。
/// 为什么不是单个 `Lang`：语言行要画**选中态**——「我明明选了跟随系统」得在界面上看得出来。
/// 为什么不塞进 [`States`]：那条路要改 `settings/state.rs` 并补齐 4 处 struct literal（文档「仍然
/// 共享」第 4 条已算过账），而 `view` 多一个形参只动 2 个文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// 用户选的那一档（跟随系统 / 手选）。
    pub choice: LangChoice,
    /// 这一次真正生效的语言（目录选的就是它）。
    pub effective: Lang,
}

impl Language {
    /// 夹具 / 演示路径的语言（`pages::app` 与它的两个兄弟入口用）：档位记**跟随系统**
    /// （`ULL_DL_LANG=zh` 演的是「系统语言是中文」，不是「用户手选了中文」）。`effective` 是形参
    /// 而不是写死 `Lang::En`：M353 之前那条入口恒英文，`ULL_DL_LANG=zh` 出不了中文设置页（P1-2）。
    pub fn fixture(effective: Lang) -> Self {
        Self {
            choice: LangChoice::System,
            effective,
        }
    }
}

/// 整页。
///
/// `states` 是「画一帧时那份只读状态」：[`States::appearance`] 是 M93 接进来的外观运行期状态
/// （`None` = harness / 演示路径，设置页仍画占位行），[`States::groups`] 是各组自己的状态。
///
/// `forced` 是 `pages::Model::forced`（`--force-state`，M285）：一路递给各组控件，由 [`resolve_state`]
/// 决定「这一帧画哪一态」。`None` = 照旧按观测态画（产品路径恒为 `None`，取证启动除外）⇒ 默认逐字节不变。
pub fn view<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    states: States<'a>,
    language: Language,
    update: &'a Update,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    // 选中分组：有真实外观状态时看状态（点左列可切），否则退回演示值 0。
    let selected = states
        .appearance
        .map_or(SELECTED, |state| state.selected_group);

    let body = match metrics.layout {
        PageLayout::SupportingPane => frame::panes(
            metrics,
            frame::group_nav(ui, selected, language.effective, forced),
            frame::left_aligned(
                metrics.column,
                form(ui, Some(selected), states, language, update, forced),
            ),
        ),
        _ => frame::single_column(metrics, form(ui, None, states, language, update, forced)),
    };

    frame::page(ui, metrics, frame::header(ui, language.effective), body)
}

// --------------------------------------------------------------- 内容分发

/// 表单列（正文列宽 ≤424）。`only = Some(i)` 只渲染一组（supporting-pane），`None` 渲染全部（窄窗按组堆叠）。
///
/// **分发点（两条路，M106 起就并存）**：①`GROUP.body`——`Some(f)` 自己去 [`States`] 取数，并从形参拿
/// **生效语言**（`None` 退回演示行）；②**按下标分发**——General（语言档位）/ Update（presenter 快照）/
/// Assistant（`Lang` + 测试结论）/ Java（`Lang` + 扫描槽）在 [`section_columns`] 里各有 `else if`，它们
/// 的 `GROUP.body` 仍是 `None`（理由见各自的 `<组>_GROUP` 常量）；分组名只作 `GROUPS` 元素的 id。
///
/// **尾部不许再挂东西（M196）**：循环之后原先无条件叠了一张「没有逐实例覆盖」的空态卡（来历见
/// `frame.rs`），于是**每一组**下面都长出同一张卡。它**放错了地方**：真控件在**实例详情页**的「启动
/// 参数」区（M157 `pages/instances/launch_args.rs` → `AppCommand::set_launch_args`），设置页拿不到那份
/// 快照（[`view`] 没有 `Model`）；无人产出的只有 `Action::AddOverride`。删掉，守门见 `frame.rs` 末尾。
fn form<'a>(
    ui: &'a Ui,
    only: Option<usize>,
    states: States<'a>,
    language: Language,
    update: &'a Update,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let mut stack = column![].spacing(ui.space(Space::X6)).width(Length::Fill);

    for section in section_columns(ui, only, states, language, update, forced) {
        stack = stack.push(section);
    }

    frame::scroll(ui, stack)
}

/// 正文里**每组一件**的列（顺序 = [`GROUPS`]，只含被选中的那些；分发点见 [`form`] 的文档）。抽出来是
/// 为了能被**行为断言**量到：`frame.rs` 的 `the_form_holds_exactly_one_column_per_group` 数它的列数。
fn section_columns<'a>(
    ui: &'a Ui,
    only: Option<usize>,
    states: States<'a>,
    language: Language,
    update: &'a Update,
    forced: Option<Interaction>,
) -> Vec<Element<'a, Message>> {
    let catalog = language.effective.catalog();
    let mut columns = Vec::new();

    for (index, group) in GROUPS.iter().enumerate() {
        if only.is_some_and(|selected| selected != index) {
            continue;
        }

        // 「外观」分组：有真实外观状态时画真控件（M93），否则退回演示行（harness）。
        // 这两条的差别由 `GROUP.body` 的返回值表达，而不是「下标 == APPEARANCE_GROUP」的硬比较。
        //
        // M106：General 是**唯一一处按下标分发**的例外（理由见 [`GENERAL_GROUP`]）——语言行的
        // 档位与生效语言不在 [`States`] 里，只能从 `pages::Language` 进来；它与 `appearance` 在不在场
        // 无关（harness 那条路径也画得出真控件）。
        let card = if index == GENERAL_GROUP {
            general::body(ui, states, language, forced)
        } else if index == UPDATE_GROUP {
            // M159：更新组读 presenter 快照（`view` 的 `update` 形参），与 General 同一条按下标分发。
            update::body(ui, update, language.effective, forced)
        } else if index == AI_GROUP {
            // M174：助手组要 i18n 文案与「测试连接」结论，与 General / Update 同一条按下标分发。
            ai::view::body(ui, states, language.effective, forced)
        } else if index == JAVA_GROUP {
            // M175：Java 组要 i18n 文案 + 扫描快照，同样按下标分发（`GROUP.body` 保持 `None`）。
            java::view::body(ui, states, language.effective, forced)
        } else {
            group
                .body
                .and_then(|body| body(ui, states, language.effective, forced))
                .unwrap_or_else(|| frame::demo_card(ui, catalog, group.rows))
        };

        columns.push(
            column![
                frame::text_role(
                    ui,
                    TypeRole::LabelMedium,
                    false,
                    // 卡片标题与左列同一个词源（`GROUP_LABELS`）：`Group.name` 只是 id。
                    GROUP_LABELS
                        .get(index)
                        .map_or(group.name, |key| catalog.text(*key)),
                    ui.color(Role::OnSurfaceVariant),
                ),
                card,
            ]
            .spacing(ui.space(Space::X2))
            .width(Length::Fill)
            .into(),
        );
    }

    columns
}

#[cfg(test)]
mod resolve_tests {
    use super::*;

    /// 两条语义各钉一次（M285）：`forced_button` 的 `Some` 钉态 / `None` 原样（**禁用态也会被 `state`
    /// 盖掉**——`Button::forced()` 里 `state` 在 disabled 默认之前，钉住实际行为，别让站点踩）、
    /// `resolve_state` 的 `Some` 覆盖观测 status / `None` 退回观测值。`Button::state` 没有 getter，
    /// 所以断 derived Debug 的那一格。抓不住：按钮真画成那色（出图负责）、站点传错 forced（只有像素）。
    #[test]
    fn forced_button_forces_only_when_asked() {
        let plain = Button::new("x", ());
        let forced = format!(
            "{:?}",
            forced_button(plain.clone(), Some(Interaction::Hovered))
        );
        assert!(
            forced.contains("state: Some(Hovered)"),
            "forced 必须钉到那一态：{forced}"
        );
        let untouched = format!("{:?}", forced_button(plain, None));
        assert!(
            untouched.contains("state: None"),
            "没给 forced 时必须原样返回：{untouched}"
        );
        // 禁用按钮同样会被钉态（`state` 覆盖 disabled 默认，类型层不挡）：
        let disabled: Button<'_, ()> =
            forced_button(Button::disabled("x"), Some(Interaction::Hovered));
        assert!(
            format!("{disabled:?}").contains("state: Some(Hovered)"),
            "state 在 disabled 默认之前 ⇒ 禁用按钮也会被画成 hover/pressed"
        );
    }

    #[test]
    fn forced_state_wins_over_the_observed_status() {
        assert_eq!(
            resolve_state(Some(Interaction::Hovered), button::Status::Pressed),
            Interaction::Hovered,
            "forced 必须覆盖观测到的 status，否则 --force-state 在这四个控件上等于没接线"
        );
        assert_eq!(
            resolve_state(None, button::Status::Pressed),
            Interaction::Pressed,
            "没给 forced 时必须退回观测值（既有行为不变）"
        );
    }
}
