//! 设置页的**状态漏斗**（M101 r2）：与 [`super::message`] 的消息漏斗同构——一个组的状态归它自己的
//! 文件，[`Groups`] 只是一个按组排开的架子。
//!
//! ## 为什么要这一层（r1 复审的 P1）
//!
//! 消息漏斗解决了「加一支消息要改哪些文件」，但一组要画**真值**就得有数据进渲染器：r1 之前
//! `settings::view` 只收 `Option<&appearance::State>`，于是 java / download / language+appearance
//! 三条线要显示「当前运行时 / 当前并发数 / 当前语言」时，**都得改 `settings/mod.rs`**；带运行期状态
//! 的还要改 `pages/mod.rs` / `app/view.rs` / `app/state.rs`。四条线一铺开就是第二处四路撞车。
//!
//! 现在的形状：
//!
//! ```text
//! App.settings: pages::settings::Groups        ← 唯一的所有者（app/state.rs 一行）
//!   ├─ java: java::State                        → pages/settings/java.rs
//!   ├─ download: download::State                → pages/settings/download.rs
//!   ├─ language_appearance: language_appearance::State → pages/settings/language_appearance.rs
//!   └─ ai_about: ai_about::State                → pages/settings/ai_about.rs
//!
//! settings::view(ui, metrics, States { appearance, groups })   ← 只读视图，按值传
//!   └─ 每组的 GROUP.body(ui, states) 自己从 states 里取自己要的那份
//! ```
//!
//! **加/改某一组的字段** = 只改 `pages/settings/<组>.rs`（状态类型在它自己文件里）+ 用它自己的
//! handler 更新（`app/update/settings/<线>.rs`）。本文件、`pages/mod.rs`、`app/state.rs`、
//! `app/view.rs` **都不用动**——本 mission 的这半边判据就是这么验的（见报告「变异证据」）。
//!
//! **加一整条新线**（多一个组）才需要：本文件加一个字段、`message.rs` 加一个变体、
//! `app/update/settings.rs` 加一条臂。那是「加一条线」，不是「加一个字段」。
//!
//! ## 外观那条真控件路径（M93）没有被挤掉
//!
//! `appearance::State` 仍是 `App` 的字段（不复用、不搬走），[`States::appearance`] 原样借用它；
//! 「没有外观状态（harness）⇒ 画演示行」这条判据现在由 `appearance` 组自己的 `GROUP.body`
//! 表达（返回 `None` ⇒ [`super::frame::demo_card`]），与 r1 之前逐像素等价。

use std::sync::LazyLock;

use unlucky_presenter::dto::SettingGroupView;

use crate::pages::appearance;

use super::{ai_about, download, java, language_appearance};

/// 各组自己拥有的运行期状态：**一个组一个字段**，字段类型定义在那条线自己的文件里。
///
/// 字段的**内部**结构归各组（加字段只改它自己的文件）；架子（有哪几个组）归这里。
#[derive(Debug, Default)]
pub struct Groups {
    /// Java 分组的状态（`pages/settings/java.rs`）。
    pub java: java::State,
    /// Downloads 分组的状态（`pages/settings/download.rs`）。
    pub download: download::State,
    /// 语言 + 外观两组的状态（`pages/settings/language_appearance.rs`）。
    pub language_appearance: language_appearance::State,
    /// AI / About 分组的状态（`pages/settings/ai_about.rs`）。
    pub ai_about: ai_about::State,
}

/// 画一帧时**按值**递给各组渲染器的只读视图：组状态 + 外观运行期状态 + presenter 的设置快照
/// （三个引用，`Copy`）。
///
/// 为什么按值而不是按引用：`Settings::view` 返回的 `Element<'a>` 会借走它，按引用传会逼着
/// harness 那条路径（[`super::demo_groups`] 撑着的 `page()` / `page_with_appearance()`）为一份
/// **临时构造**的 `States` 造 `'static`，或者把默认状态塞进 `Model`（两份真相）。按值传没有
/// 这层借用纠缠，调用方写一行就够。
#[derive(Debug, Clone, Copy)]
pub struct States<'a> {
    /// 外观运行期状态（M93）：`None` = harness / 演示路径（设置页画演示行）。
    pub appearance: Option<&'a appearance::State>,
    /// 各组自己的状态（见 [`Groups`]）。
    pub groups: &'a Groups,
    /// **presenter 的设置组快照**（M201）：`CoreEvent::SettingsChanged` 的回流结果，
    /// `pages::Model` 那份（`Model::settings`）。空切片 = 这一帧没有快照（harness / 首帧），
    /// 组自己的 `body` 据此退回演示行。
    ///
    /// 为什么走这里而不是给 `settings::view` 加形参：`Group.body` 的槽位签名是
    /// `fn(&Ui, States, Lang) -> Option<Element>`，它**只看得到 `States`**——下载组要画真控件
    /// （`GROUP.body = Some(..)`，见 `settings/download.rs`）就必须从这里拿到快照。加形参那条路
    /// 与 `GROUP.body` 互斥，而且要把 `settings/mod.rs`（399 行）再撑出 10 行。
    pub settings: &'a [SettingGroupView],
}

/// 演示 / harness 路径的默认组状态（`pages::page` 与 `pages::page_with_appearance` 用它）。
///
/// `'static` 是有意的：那两条入口不持有任何组状态，而返回的 `Element<'a>` 会借走这份 `Groups`；
/// 让它活得比调用方长，借用就不会把返回值钉在函数栈上。
///
/// **隐性代价（r2 复审 P3）**：`static` 要求 `Groups: Sync + Default`，于是某个组的 `State` 里一旦
/// 放了 `Rc` 这类非 `Sync` 字段，编译会红在**本文件**（共享）而不是那一组自己的文件。真需要
/// 非 `Sync` 状态时的改法：去掉这个 `static`，改成每个调用点自己造一份 `Groups`——`pages/mod.rs` 的
/// **三个**调用点都要改（`page` / `page_with_appearance` / `app_with_appearance`，行 156 / 179 / 234），
/// 一次性代价，见 [`super`] 文档「仍然共享」第 6 条。
pub fn demo_groups() -> &'static Groups {
    static DEMO: LazyLock<Groups> = LazyLock::new(Groups::default);
    &DEMO
}
