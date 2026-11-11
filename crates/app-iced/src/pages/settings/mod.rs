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
//! 但**不复制 rail 的实现**：这里没有 hover 药丸、没有二级标签、没有 `Message::Select`。
//! 将来真正的 `NavigationDrawer` 由 M78 提供，本页换掉 [`frame::group_nav`] 即可。
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
mod state;
pub(crate) mod update;

#[cfg(test)]
mod tests;

pub use message::GroupMessage;
pub use state::{demo_groups, Groups, States};

use iced::widget::column;
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Lang, LangChoice};

use crate::theme::Ui;

use super::layout::{FrameMetrics, PageLayout};
use super::update::Update;
use super::Message;

/// 设置页 General 分组要的两件（M106）：用户选的档位 + 这一次生效的语言。
///
/// 为什么不塞进 `pages::Model`：那是页面与 presenter 之间的**展示契约**（由快照派生），而档位是
/// **用户意图**（`App` 的字段）；塞进去会让 `pages::demo_model` 也得伪造一个档位。
/// 为什么不是单个 `Lang`：语言行要画**选中态**——「我明明选了跟随系统」得在界面上看得出来。
/// 为什么不塞进 [`States`]：那条路要改 `settings/state.rs` 并补齐 4 处 struct literal
/// （`settings/mod.rs` 文档「仍然共享」第 4 条已算过账），而 `view` 多一个形参只动 2 个文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// 用户选的那一档（跟随系统 / 手选）。
    pub choice: LangChoice,
    /// 这一次真正生效的语言（目录选的就是它）。
    pub effective: Lang,
}

impl Default for Language {
    /// harness / 演示路径的默认值：跟随系统 + 英文基线（与 M84 起的截图基线一致）。
    fn default() -> Self {
        Self {
            choice: LangChoice::System,
            effective: Lang::En,
        }
    }
}

/// 一个分组正文的**真控件渲染器**：入参是「画一帧时那份只读状态」（组状态 + 外观状态）
/// 与**这一次生效的语言**，返回 `None` = 这一组现在没东西可画（例如外观分组在 harness 路径下
/// 没有外观状态），由 [`form`] 退回演示行。
///
/// 各组自己的渲染器**定义在自己文件里**，签名是这一条：于是「这一组要读哪些状态」由它自己决定，
/// 本文件不需要认识任何一组的字段（r1 复审 P1 要的正是这一点）。
///
/// `lang` 是 M179 加的：槽位此前拿不到语言，唯一挂了真控件的外观组只能写死英文；接进**唯一的那个
/// 槽位**比再给 [`form`] 加一条按下标分发的 `else if` 更省行数（本文件贴着 400 行硬顶）。
type Body = for<'a> fn(&'a Ui, States<'a>, Lang) -> Option<Element<'a, Message>>;

/// 一个设置分组：名字、图标、演示行、（可选）把这一组正文画成真控件的渲染器。
///
/// 除 `name`/`icon`/`rows`/`body` 外没有别的字段：**分组自己的状态归分组自己**（见 [`state`]），
/// 这里只装「怎么画」。
struct Group {
    /// 左列显示名（也是表单列的分组标题）。
    name: &'static str,
    /// 左列图标。
    icon: IconKind,
    /// 演示行（`body` 为 `None` 时画这些；harness 出图也靠它们）。
    rows: &'static [(&'static str, &'static str)],
    /// 真控件渲染器；`None` = 这一组还没接真控件，画 `rows`。
    body: Option<Body>,
}

/// 分组注册表：**顺序 = 左列自上而下**，也是 `crate::pages::appearance::APPEARANCE_GROUP` 的下标来源。
///
/// 每一项都指向它自己文件里的 `GROUP`（M99 之前是四份字面量挤在本文件里）。
const GROUPS: [Group; 6] = [
    general::GROUP,
    appearance::GROUP,
    java::GROUP,
    download::GROUP,
    // M159：自更新只**追加**在末尾（插在中间会让 `APPEARANCE_GROUP` / `GENERAL_GROUP` 两个下标
    // 全部失效，`tests/mod.rs` 的「左列顺序」那条也会红）。
    update::GROUP,
    // M174：助手（AI）配置。同样只追加（下标 = `ai_about::AI_GROUP`），正文走按下标分发。
    ai_about::GROUP,
];

/// 演示选中的分组（没有真实外观状态时——`pages_demo` harness——用它）。
const SELECTED: usize = 0;

/// General 分组在 [`GROUPS`] 里的下标（0）。它与 [`SELECTED`] 同值**是巧合，不是依赖**。
///
/// 为什么需要它：语言行要的是「当前档位 + 生效语言」（[`Language`]），而 `Group.body` 只带
/// [`States`] 与**生效语言**（`Lang`，M179）——档位 `LangChoice` 装不进去（`States` 也装不下：
/// 加字段要动 `settings/state.rs` 与 4 处 struct literal，见本文件文档「仍然共享」第 4 条）。
/// 所以这里做一次**按下标的分发**：代价是一行 `if`，换来 `general::GROUP.body` 仍是 `None`
/// （`tests/mod.rs` 的「只有外观组带真控件 body」照样成立）。
const GENERAL_GROUP: usize = 0;

/// 左列分组的**显示名** key（下标与 [`GROUPS`] 一一对应，M106）。
///
/// 为什么不放进 [`Group`]：那会让另外三组的 `GROUP` 字面量（各自文件、各自填页线）编译不过——
/// 与 [`GENERAL_GROUP`] 同一条理由。`Group.name` 因此退化为**id**（测试按它认组；显示走这张表）。
pub(super) const GROUP_LABELS: [unlucky_i18n::Key; 6] = [
    unlucky_i18n::Key::SettingsGroupGeneral,
    unlucky_i18n::Key::SettingsGroupAppearance,
    unlucky_i18n::Key::SettingsGroupJava,
    unlucky_i18n::Key::SettingsGroupDownloads,
    // M159
    unlucky_i18n::Key::SettingsGroupUpdate,
    // M174
    unlucky_i18n::Key::SettingsGroupAssistant,
];

/// 「更新」分组在 [`GROUPS`] 里的下标（真源在 `settings/update.rs`，这里只转发）。
///
/// 与 [`GENERAL_GROUP`] 同一种做法：这一组要画的是 **presenter 快照**（不在 [`States`] 里），
/// 所以由 `view` 的 `update` 形参一路递到下标的 [`form`] 分发点，`GROUP.body` 保持 `None`。
const UPDATE_GROUP: usize = update::UPDATE_GROUP;

/// 「助手（AI）」分组在 [`GROUPS`] 里的下标（真源在 `settings/ai_about.rs`，这里只转发）。
///
/// 与 [`GENERAL_GROUP`] / [`UPDATE_GROUP`] 同一条做法：由 `view` 的 `language` 形参递到 [`form`]
/// 按本下标分发，`GROUP.body` 保持 `None`（本组**尚未**迁到 [`Body`] 槽位：槽位现在带 `Lang` 了，
/// 迁它要把 `ai::view::body` 的返回值改成 `Option<Element>`，是一次独立重构——M179 不做）。
const AI_GROUP: usize = ai_about::AI_GROUP;

/// 「Java」分组在 [`GROUPS`] 里的下标（真源在 `settings/java.rs`，这里只转发）。
///
/// M175 起这一组也按下标分发：它要 `Lang`（中英文案）并且要读后台扫描的快照
/// （扫描线程不碰 `App`，结论由心跳灌进 `Groups.java`，页面只从 [`States`] 里取）。
const JAVA_GROUP: usize = java::JAVA_GROUP;

/// 整页。
///
/// `states` 是「画一帧时那份只读状态」：[`States::appearance`] 是 M93 接进来的外观运行期状态
/// （`None` = harness / 演示路径，设置页仍画占位行），[`States::groups`] 是各组自己的状态。
pub fn view<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    states: States<'a>,
    language: Language,
    update: &'a Update,
) -> Element<'a, Message> {
    // 选中分组：有真实外观状态时看状态（点左列可切），否则退回演示值 0。
    let selected = states
        .appearance
        .map_or(SELECTED, |state| state.selected_group);

    let body = match metrics.layout {
        PageLayout::SupportingPane => frame::panes(
            metrics,
            frame::group_nav(ui, selected, language.effective),
            frame::left_aligned(
                metrics.column,
                form(ui, Some(selected), states, language, update),
            ),
        ),
        _ => frame::single_column(metrics, form(ui, None, states, language, update)),
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
) -> Element<'a, Message> {
    let mut stack = column![].spacing(ui.space(Space::X6)).width(Length::Fill);

    for section in section_columns(ui, only, states, language, update) {
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
            general::body(ui, states, language)
        } else if index == UPDATE_GROUP {
            // M159：更新组读 presenter 快照（`view` 的 `update` 形参），与 General 同一条按下标分发。
            update::body(ui, update, language.effective)
        } else if index == AI_GROUP {
            // M174：助手组要 i18n 文案与「测试连接」结论，与 General / Update 同一条按下标分发。
            ai::view::body(ui, states, language.effective)
        } else if index == JAVA_GROUP {
            // M175：Java 组要 i18n 文案 + 扫描快照，同样按下标分发（`GROUP.body` 保持 `None`）。
            java::view::body(ui, states, language.effective)
        } else {
            group
                .body
                .and_then(|body| body(ui, states, language.effective))
                .unwrap_or_else(|| frame::demo_card(ui, group.rows))
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
