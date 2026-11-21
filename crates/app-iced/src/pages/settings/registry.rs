//! 设置页的**分组注册表**（M285 `GROUPSPLIT`：从 `settings/mod.rs` **纯搬移**）。
//!
//! 本文件只装「有哪些分组、怎么画一组」这一份数据：正文渲染器 [`Body`]、分组形状 [`Group`]、
//! 顺序表 [`GROUPS`]、左列显示名 [`GROUP_LABELS`]，以及按下标分发用的那几个下标常量。
//!
//! ## 为什么切在这里
//!
//! `settings/mod.rs` 撞了 400 行硬线（`components/gate.rs`），而 M285 要给 [`Body`] 加一个
//! `forced: Option<Interaction>` 形参（`--force-state` 直通，见 `settings::resolve_state`）。
//! 切分按**语义**走，不按行数：`mod.rs` 留「整页 → 表单列 → 分组正文」这条流水线，本文件留
//! 「分组是什么」。搬移是逐字搬的——除了声明前面的可见性标注（见下），语句一个字没动；注释里',
//! 说的「本文件」仍指 `settings/mod.rs`（那些句子说的是**原来那个文件**的行数压力），别读成本文件。
//!
//! ## 可见性
//!
//! `registry` 是 `settings` 的**私有子模块**，所以这里写宽一点也出不去：[`Group`] / [`Body`] 是
//! `pub(crate)`，因为 `settings/mod.rs` 要 `pub(super) use registry::Group`（E0365：再导出不能比原
//! 声明更可见）；其余常量是 `pub(super)`，由 `mod.rs` 私有 `use` 进来，子模块仍按既有写法
//! `super::GROUPS` / `super::GROUP_LABELS` 取用（私有 `use` 对后代模块可见）。

use iced::Element;

use unlucky_design::icons::IconKind;
use unlucky_i18n::{Key, Lang};

use crate::app::components::interaction::Interaction;
use crate::theme::Ui;

use super::{ai_about, appearance, download, general, java, update, Message, States};

/// 一个分组正文的**真控件渲染器**：入参是「画一帧时那份只读状态」（组状态 + 外观状态）
/// 与**这一次生效的语言**，返回 `None` = 这一组现在没东西可画（例如外观分组在 harness 路径下
/// 没有外观状态），由 [`super::form`] 退回演示行。
///
/// 各组自己的渲染器**定义在自己文件里**，签名是这一条：于是「这一组要读哪些状态」由它自己决定，
/// 本文件不需要认识任何一组的字段（r1 复审 P1 要的正是这一点）。
///
/// `lang` 是 M179 加的：槽位此前拿不到语言，唯一挂了真控件的外观组只能写死英文；接进**唯一的那个
/// 槽位**比再给 [`super::form`] 加一条按下标分发的 `else if` 更省行数（本文件贴着 400 行硬顶）。
///
/// `forced` 是 M285 加的（`pages::Model::forced`，即 `--force-state`）：`None` = 按控件自己的观测态画
/// （产品路径的常态），`Some(..)` 只服务取证。站点拿它去调 [`super::resolve_state`]——那是全仓唯一的
/// resolve 点，别在这里或各组里另写一遍。
pub(crate) type Body =
    for<'a> fn(&'a Ui, States<'a>, Lang, Option<Interaction>) -> Option<Element<'a, Message>>;

/// 一个设置分组：名字、图标、演示行、（可选）把这一组正文画成真控件的渲染器。
///
/// 除 `name`/`icon`/`rows`/`body` 外没有别的字段：**分组自己的状态归分组自己**（见 [`super::state`]），
/// 这里只装「怎么画」。
pub(crate) struct Group {
    /// 左列显示名（也是表单列的分组标题）。
    pub(super) name: &'static str,
    /// 左列图标。
    pub(super) icon: IconKind,
    /// 演示行（`body` 为 `None` 时画这些；harness 出图也靠它们）。
    ///
    /// **第一列是 key、第二列是数据**（M314 r1）：标签是界面文案，必须跟着语言走
    /// （`catalog/settings/demo_label.rs`）；值（`0.2.2` / URL / `deepseek-chat` / `100%`）是
    /// locale 中立的**数据**，建 key 只会逼出假译文（`catalog/tests.rs` 要求每条 key 中英必须不同）。
    pub(super) rows: &'static [(Key, &'static str)],
    /// 真控件渲染器；`None` = 这一组还没接真控件，画 `rows`。
    pub(super) body: Option<Body>,
}

/// 分组注册表：**顺序 = 左列自上而下**，也是 `crate::pages::appearance::APPEARANCE_GROUP` 的下标来源。
///
/// 每一项都指向它自己文件里的 `GROUP`（M99 之前是四份字面量挤在本文件里）。
pub(super) const GROUPS: [Group; 6] = [
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
pub(super) const SELECTED: usize = 0;

/// General 分组在 [`GROUPS`] 里的下标（0）。它与 [`SELECTED`] 同值**是巧合，不是依赖**。
///
/// 为什么需要它：语言行要的是「当前档位 + 生效语言」（[`super::Language`]），而 `Group.body` 只带
/// [`States`] 与**生效语言**（`Lang`，M179）——档位 `LangChoice` 装不进去（`States` 也装不下：
/// 加字段要动 `settings/state.rs` 与 4 处 struct literal，见本文件文档「仍然共享」第 4 条）。
/// 所以这里做一次**按下标的分发**：代价是一行 `if`，换来 `general::GROUP.body` 仍是 `None`
/// （`tests/mod.rs` 的「只有外观组带真控件 body」照样成立）。
pub(super) const GENERAL_GROUP: usize = 0;

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
/// 所以由 `view` 的 `update` 形参一路递到下标的 [`super::form`] 分发点，`GROUP.body` 保持 `None`。
pub(super) const UPDATE_GROUP: usize = update::UPDATE_GROUP;

/// 「助手（AI）」分组在 [`GROUPS`] 里的下标（真源在 `settings/ai_about.rs`，这里只转发）。
///
/// 与 [`GENERAL_GROUP`] / [`UPDATE_GROUP`] 同一条做法：由 `view` 的 `language` 形参递到 [`super::form`]
/// 按本下标分发，`GROUP.body` 保持 `None`（本组**尚未**迁到 [`Body`] 槽位：槽位现在带 `Lang` 了，
/// 迁它要把 `ai::view::body` 的返回值改成 `Option<Element>`，是一次独立重构——M179 不做）。
pub(super) const AI_GROUP: usize = ai_about::AI_GROUP;

/// 「Java」分组在 [`GROUPS`] 里的下标（真源在 `settings/java.rs`，这里只转发）。
///
/// M175 起这一组也按下标分发：它要 `Lang`（中英文案）并且要读后台扫描的快照
/// （扫描线程不碰 `App`，结论由心跳灌进 `Groups.java`，页面只从 [`States`] 里取）。
pub(super) const JAVA_GROUP: usize = java::JAVA_GROUP;
