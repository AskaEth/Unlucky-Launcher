//! 设置页「Downloads」分组：并发 / 单源尝试次数 / 地区 / 镜像 / 代理 / GitHub 加速前缀（M201）。
//!
//! **这一组归 download 线**：正文渲染器在本文件（`GROUP.body = Some(body)`），`settings/mod.rs`
//! 的注册表不用动。
//!
//! 注意与下载页（`pages/download*`）的区别：那是「装哪个版本」，这里是「怎么下」的默认值。
//!
//! ## 数据从哪来、写到哪去（M201 接的就是这条通路）
//!
//! ```text
//! presenter::host::settings::groups(&env)        ← 真值（core schema 的键 + 生效值）
//!   └─ CoreEvent::SettingsChanged(Vec<SettingGroupView>)
//!        └─ AppState::settings_groups            ← presenter 的唯一写入口（apply_core）
//!             └─ pages::Model.settings           ← refresh_view：这一帧要画的设置值
//!                  └─ settings::States.settings  ← 本文件按 `id` 找到本组，逐行按 `control` 画
//!                       └─ 控件（choice / text / action）
//!                            └─ Message → app/update/settings/download.rs
//!                                 └─ AppCommand::SetSetting → host 写盘 + 重新发 SettingsChanged
//! ```
//!
//! **闭环**：控件不回显自己点的结果（不做乐观回显）。点了以后界面变，只能是因为 host 把
//! `SettingsChanged` 发回来了——那也正是「设置页里改了就生效」这句话的证据链。
//!
//! ## 三件事分开画（这一组是「设置页里改不了」的修复落点）
//!
//! 1. **当前值**（choice 的选中 chip / text 的输入框）——来自快照，不来自任何字面量；
//! 2. **这一行在讲什么**（标题 + 说明）——说明里带着 presenter 算出来的「引擎生效值」，
//!    文件原值与生效值不一致时那句「原值 X 已按出厂 N 生效」也在里面（P3-5）；
//! 3. **怎么改**（点 chip / 输入 + 应用 / 点动作按钮）——写回走 `AppCommand::SetSetting`。
//!
//! **没有快照**（harness / `pages_demo` / 首帧）时 `body` 返回 `None` ⇒ 退回去画 `GROUP.rows`
//! 那几行语义行（`frame::demo_card`）。那几行**说的不是值**（「出厂 8 · 可调 1~64」），
//! 所以不会再把演示值当成真值（M193 用户投诉的形状）。
//!
//! ## 文本行为什么要点「应用」
//!
//! 每敲一个键就写一次盘会把半截值落进 `launcher.json`（`off` 敲到 `of` 时 schema 会让它通过），
//! 而且 `store.set` 失败会弹一串 Toast。所以输入只改本地草稿（[`State::edit`]），点「应用」
//! 才走 [`Message::Commit`] → `AppCommand::SetSetting`。
//!
//! **为什么不是回车提交**：本仓的 `text_field`（`app/components/inputs/field.rs`）只暴露 `on_input`，
//! 没有 `on_submit` 的口子；给它加一个口子要动组件库（组件库有自己的验收线），所以这一版只有
//! 「应用」这一颗按钮。回车提交登记为后续（见报告「没验/没做」）。

use std::collections::BTreeMap;

use iced::widget::{column, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Key, Lang};
use unlucky_presenter::dto::{SettingControl, SettingGroupView, SettingRowView};
// `find_row` 只被 `#[cfg(test)]` 的探针用（产品路径按 `GROUP_ID` 取组、逐行画）。
#[cfg(test)]
use unlucky_presenter::dto::find_row;

use crate::app::components::buttons::{self, Button};
use crate::app::components::inputs::{text_field, Field, FieldVariant};
use crate::app::components::selection::{chip, switch, Chip, Switch};
use crate::pages::frame::text_role_owned;
use crate::pages::Message as PageMessage;
use crate::theme::Ui;

use super::frame::{card, divider};
use super::{Group, States};

/// Downloads 分组。
///
/// `rows` 只在**没有快照**时画（harness / 首帧）：它们是语义行（出厂值 / 区间 / 三态），
/// 不是当前值——M193 用户的投诉正是「把演示值当成了真值」。
pub(super) const GROUP: Group = Group {
    name: "Downloads",
    icon: IconKind::Download,
    rows: &[
        ("并发数", "出厂 8 · 可调 1~64"),
        ("单源尝试次数", "出厂 3 · 含首次 · 0/1 = 只试一次"),
        ("GitHub 加速前缀", "留空 = 跟随内置默认"),
        ("关掉它", "填 off = 关闭"),
        ("换掉它", "填 URL 前缀 = 覆盖"),
    ],
    body: Some(body),
};

/// 这一组在 presenter 快照里的 id（`setting::groups` 建组时给的那个）。
const GROUP_ID: &str = "download";

/// Downloads 分组的运行期状态（M101 r2：**只归 download 线**）。
///
/// 只装「用户正在编辑、还没提交」的文本草稿。**当前值不在这里**——它来自 presenter 快照
/// （`States::settings`），这里存一份就成了第二份真相。
#[derive(Debug, Default)]
pub struct State {
    /// schema 键 → 正在输入但还没提交的文本。
    drafts: BTreeMap<String, String>,
}

impl State {
    /// 这一行此刻要显示的文本：有草稿用草稿，否则用快照里那个值（= 文件里的值）。
    pub(crate) fn text_of<'a>(&'a self, row: &'a SettingRowView) -> &'a str {
        self.drafts
            .get(&row.key)
            .map_or(row.value_text.as_str(), String::as_str)
    }

    /// 记一条草稿（`text_field` 的 `on_input` 落点；**不派发**任何指令）。
    pub(crate) fn edit(&mut self, key: &str, value: String) {
        self.drafts.insert(key.to_string(), value);
    }

    /// 这一行要提交的值（草稿优先）。`None` = 没草稿，调用方回落到快照里的值。
    pub(crate) fn pending(&self, key: &str) -> Option<String> {
        self.drafts.get(key).cloned()
    }

    /// 丢掉一行的草稿（提交之后用：快照重新成为唯一真值）。
    pub(crate) fn clear(&mut self, key: &str) {
        self.drafts.remove(key);
    }
}

/// Downloads 分组的消息（M101：**只归 download 线**，加变体只改本文件）。
///
/// 落点是 `app/update/settings/download.rs`；`pages/mod.rs` 与 `page_messages.rs` 都不用动（它们只认
/// [`GroupMessage`](super::GroupMessage)）。
#[derive(Debug, Clone)]
pub enum Message {
    /// `choice` 行：选了一个选项 —— 直接提交（选择本身就是意图，没有「半截」状态）。
    Set {
        /// schema 键（`download.concurrency`）
        key: String,
        /// 选项原文（`8` / `cn` / `off` 这类，写回时按 schema 的类型转换）
        value: String,
    },
    /// `text` 行：正在输入 —— **只改本地草稿**，不派发。
    Edit {
        /// schema 键
        key: String,
        /// 输入框里的整段文本
        value: String,
    },
    /// `text` 行：提交（「应用」按钮 / 回车）。
    Commit {
        /// schema 键
        key: String,
    },
    /// `action` 行：动作按钮（例如「重新检测地区」）。
    Action {
        /// 快照里那一行的键（动作 id 由 host 在 `settings::set` 里认）
        key: String,
    },
}

/// 把本组消息包进设置页那条**唯一**投递口（`pages::Message::Settings(GroupMessage::Download)`）。
///
/// 为什么必须有它：本模块里 `Message` 是本组自己的枚举，而 `Element` 的消息类型是
/// `crate::pages::Message`（别名 [`PageMessage`]）。不包这一层，控件会带上本组的消息类型，
/// 于是 `settings/mod.rs` 那个 `and_then` 与页面树就对不上（编译期就红，不会静默丢消息）。
fn msg(message: Message) -> PageMessage {
    PageMessage::Settings(super::GroupMessage::Download(message))
}

// --------------------------------------------------------------- 正文

/// 整组正文：快照里按 [`GROUP_ID`] 找到本组，一行一个控件。
///
/// 返回 `None` = 这一帧没有本组的快照（harness / 首帧）⇒ `settings/mod.rs` 退回 `GROUP.rows`。
///
/// **行间距 = `Space::X3`（M201 r1 视觉复审 P2-1）**：分隔线的对比度只有 1.55:1（`OutlineVariant`
/// 本色，正常），「这是新的一行」这件事全靠**间距**承担。r1 复审实测这一组原本是 0 间距：说明末行
/// 墨迹 → 分隔线只有 2–3px、分隔线 → 下一行标题 4–5px，100% 缩放下「地区判定」「镜像偏好」被读成
/// 上一段说明的续行。
///
/// 为什么是 X3（12）而不是兄弟组的 X2（8）：本组每一行都是「标题 + 控件 + 多行说明」三段，比 Java
/// 组的行**密得多**，沿用 8 实测只到 8–9px（文本盒里 12–13px 的墨迹落在 16px 行盒中，盒间距 8 会被
/// 吃掉 1px）；X3 把两侧都推到 12px 以上，落在「同族页 9–18px」的中段（实测见报告的表）。
///
/// 为什么不给 [`block`] 加 padding：那会把卡片的**第一行顶距**也撑开（卡内边距 20 + 12），与其它四组
/// 的第一行不再对齐；`spacing` 只作用在子节点之间，首行顶距与末行底距仍是卡片的 20。
fn body<'a>(ui: &'a Ui, states: States<'a>, lang: Lang) -> Option<Element<'a, PageMessage>> {
    let group = state_group(&states)?;
    let drafts = &states.groups.download;
    let catalog = lang.catalog();
    let mut stacked = column![].spacing(ui.space(Space::X3)).width(Length::Fill);

    for (index, item) in group.rows.iter().enumerate() {
        if index > 0 {
            stacked = stacked.push(divider(ui));
        }
        stacked = stacked.push(match item.control {
            SettingControl::Toggle => toggle_row(ui, item),
            SettingControl::Choice => choice_row(ui, item),
            SettingControl::Text => {
                text_row(ui, item, drafts, catalog.text(Key::SettingsDownloadsApply))
            }
            SettingControl::ReadOnly => read_only_row(ui, item),
            SettingControl::Action => action_row(ui, item),
        });
    }

    Some(card(ui, stacked))
}

/// 快照里的本组（`States::settings` 是 `Model.settings` 那一份）。
fn state_group<'a>(states: &States<'a>) -> Option<&'a SettingGroupView> {
    states.settings.iter().find(|group| group.id == GROUP_ID)
}

/// 一行的标题（`BodyMedium`）。
fn heading<'a>(ui: &'a Ui, item: &'a SettingRowView) -> Element<'a, PageMessage> {
    text_role_owned(
        ui,
        TypeRole::BodyMedium,
        false,
        item.label.clone(),
        ui.color(Role::OnSurface),
    )
}

/// 一行的说明（`BodySmall`；P3-5 那句「原值 X 已按出厂 N 生效」就在这段文字里）。
fn hint<'a>(ui: &'a Ui, item: &'a SettingRowView) -> Element<'a, PageMessage> {
    text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        item.description.clone(),
        ui.color(Role::OnSurfaceVariant),
    )
}

/// 标题 + 控件 + 说明（三行一组的常规排法）。
fn block<'a>(
    ui: &'a Ui,
    item: &'a SettingRowView,
    control: Element<'a, PageMessage>,
) -> Element<'a, PageMessage> {
    column![heading(ui, item), control, hint(ui, item)]
        .spacing(ui.space(Space::X1))
        .width(Length::Fill)
        .into()
}

/// 开 / 关两态（本组暂时没有 Toggle 行，但 `SettingControl` 是穷尽枚举，画法在这里定下来）。
fn toggle_row<'a>(ui: &'a Ui, item: &'a SettingRowView) -> Element<'a, PageMessage> {
    let key = item.key.clone();
    let toggle = switch(
        ui,
        Switch::new(
            &item.label,
            item.flag,
            msg(Message::Set {
                key,
                // schema 的 Bool 键认 `true` / `false`（`settings::parse_value` 也认 on/off/1/0）。
                value: (!item.flag).to_string(),
            }),
        ),
    );
    column![toggle, hint(ui, item)]
        .spacing(ui.space(Space::X1))
        .width(Length::Fill)
        .into()
}

/// 分段单选：一个选项一颗 filter chip，选中的那颗是当前值。
fn choice_row<'a>(ui: &'a Ui, item: &'a SettingRowView) -> Element<'a, PageMessage> {
    let mut options = row![]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center);
    for (index, option) in item.options.iter().enumerate() {
        let selected = i32::try_from(index)
            .map(|index| index == item.selected)
            .unwrap_or(false);
        let key = item.key.clone();
        let value = option.clone();
        options = options.push(chip(
            ui,
            Chip::filter(option, selected, msg(Message::Set { key, value })),
        ));
    }
    block(ui, item, options.into())
}

/// 单行文本 + 「应用」按钮（`on_input` 只改草稿，点按钮 / 回车才写盘）。
fn text_row<'a>(
    ui: &'a Ui,
    item: &'a SettingRowView,
    drafts: &'a State,
    apply_label: &'a str,
) -> Element<'a, PageMessage> {
    let key = item.key.clone();
    let field = Field::new(drafts.text_of(item)).variant(FieldVariant::Outlined);
    let input = text_field(ui, field, move |value| {
        msg(Message::Edit {
            key: key.clone(),
            value,
        })
    });
    let apply = buttons::text(
        ui,
        Button::new(
            apply_label,
            msg(Message::Commit {
                key: item.key.clone(),
            }),
        ),
    );
    let controls = row![input, apply]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center)
        .width(Length::Fill);
    block(ui, item, controls.into())
}

/// 只读一行（本组没有；`SettingControl` 是穷尽枚举）。
fn read_only_row<'a>(ui: &'a Ui, item: &'a SettingRowView) -> Element<'a, PageMessage> {
    block(
        ui,
        item,
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            item.value_text.clone(),
            ui.color(Role::OnSurfaceVariant),
        ),
    )
}

/// 动作按钮（「重新检测地区」）。
fn action_row<'a>(ui: &'a Ui, item: &'a SettingRowView) -> Element<'a, PageMessage> {
    let button = buttons::tonal(
        ui,
        Button::new(
            &item.label,
            msg(Message::Action {
                key: item.key.clone(),
            }),
        ),
    );
    column![button, hint(ui, item)]
        .spacing(ui.space(Space::X1))
        .width(Length::Fill)
        .into()
}

// --------------------------------------------------------------- 测试探针

/// 这一帧本组要画的**控件种类**（顺序 = 快照里的行序）。
///
/// 存在的理由：`Element` 没有内省接口，「页面画了哪几种控件」只能从行模型上问。测试用它区分
/// 「画了真控件」与「退回了演示行」——这是 `body.is_some()` 之外唯一能看的东西。
///
/// `#[cfg(test)]`：产品路径不需要它（`body` 自己就在画），它只为断言存在。
#[cfg(test)]
pub(crate) fn controls_of(settings: &[SettingGroupView]) -> Vec<SettingControl> {
    settings
        .iter()
        .find(|group| group.id == GROUP_ID)
        .map(|group| group.rows.iter().map(|item| item.control).collect())
        .unwrap_or_default()
}

/// 快照里本组某一行的**当前值**（choice 取选中的选项、text 取 `value_text`）。
///
/// 与 [`controls_of`] 同一条理由：页面把控件画出来之后，「它认为当前值是几」只能从这个入口问，
/// 不能从 `Element` 里读。
#[cfg(test)]
pub(crate) fn current_value(settings: &[SettingGroupView], key: &str) -> Option<String> {
    let row = find_row(settings, key)?;
    Some(match row.control {
        SettingControl::Choice => row
            .options
            .get(usize::try_from(row.selected).ok()?)
            .cloned()
            .unwrap_or_default(),
        // 其余三种控件的「当前值」就是 `value_text`（开关那一种用它表示 true/false）。
        SettingControl::Toggle
        | SettingControl::Text
        | SettingControl::ReadOnly
        | SettingControl::Action => row.value_text.clone(),
    })
}
