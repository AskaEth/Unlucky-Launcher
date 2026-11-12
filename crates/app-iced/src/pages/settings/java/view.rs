//! 「Java」分组的一张卡片（M175）。
//!
//! ## 版面（自上而下）
//!
//! | 行 | 内容 | 数值 / 证据 |
//! |---|---|---|
//! | 状态 | 未检测到 / N 个 JDK / 正在扫描 3/7 | 扫描按钮与取消按钮同步禁用 |
//! | 运行时列表 | 每个 JDK 一行：路径 + 版本 + 发行方 + 来源 | 「默认」标记 / 「设为默认」chip |
//! | 版本门槛 | 当前实例需要 Java ≥ N，哪一行不满足 | 来自 app 层算好的 [`Context`] |
//! | 手动添加 | 输入框 + 「添加」 | 校验不过时下面直接说原因 |
//! | 默认内存 | 数字框 + 范围提示 + 「现在生效多少来自哪一层」 | 落盘值（不是草稿） |
//! | JVM 参数 | 文本框 + 拼命令行提示 | 落盘的 `java.extra_args` |
//! | 动作 | 保存 | 灰掉时下面是原因 |
//!
//! 文案一律走 [`unlucky_i18n::Catalog`]：本组要 `Lang`，所以走 `settings/mod.rs` 的按下标分发
//! （与 General / Update / Assistant 同一条路），`Group.body` 保持 `None`。

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key, Lang};

use crate::app::components::buttons::{self, Button};
use crate::app::components::selection::{chip, Chip};
use crate::pages::frame::{card, divider, text_role_owned, BASE};
use crate::pages::Message;
use crate::theme::Ui;

use super::fields::{actions_row, add_row, hint_line, jvm_field, memory_field, notice_line};
use super::state::{Progress, RuntimeRow, State};
use super::Message as JavaMessage;
use crate::pages::settings::{GroupMessage, States};

/// 一条本组消息 → 页面消息。
pub(super) fn msg(message: JavaMessage) -> Message {
    Message::Settings(GroupMessage::Java(message))
}

/// 一张卡片（`pages::settings::form` 按下标把 `Lang` 递进来）。
pub(crate) fn body<'a>(ui: &'a Ui, states: States<'a>, lang: Lang) -> Element<'a, Message> {
    body_of(ui, &states.groups.java, lang)
}

fn body_of<'a>(ui: &'a Ui, state: &'a State, lang: Lang) -> Element<'a, Message> {
    let catalog = lang.catalog();
    let mut stacked = column![].spacing(ui.space(Space::X2)).width(Length::Fill);

    stacked = stacked.push(status_row(ui, catalog, state));
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(runtime_list(ui, catalog, state));
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(add_row(ui, catalog, state));
    // M197：真下载一个 JDK（版本选择列表 / 下载中 / 失败三态都在这一块里）。
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(super::download_view::section(ui, catalog, state));
    // M197：三个设置键（安装目录 / 发行版顺序 / 缺 JDK 时自动准备）。
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(super::prefs_view::section(ui, catalog, state));
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(memory_field(ui, catalog, state));
    stacked = stacked.push(jvm_field(ui, catalog, state));
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(actions_row(ui, catalog, state));
    stacked = stacked.push(hint_line(ui, catalog, state));

    if let Some(notice) = &state.notice {
        stacked = stacked.push(notice_line(ui, catalog, notice));
    }

    card(ui, stacked)
}

/// 状态行：未检测到 / N 个 JDK / 正在扫描 / 正在取消 + 两个按钮。
///
/// 「取消」的语义（r1 复审 P2-5）：按下之后到线程真退出之间，界面显示的是**正在取消…**，
/// 「立即扫描」保持禁用——否则用户能在旧线程还活着的时候起第二个。
fn status_row<'a>(ui: &'a Ui, catalog: Catalog, state: &State) -> Element<'a, Message> {
    let (text, scanning) = match (state.scanning, state.scanned) {
        (Some(_), _) if state.cancelled => {
            (catalog.text(Key::SettingsJavaCancelling).to_string(), true)
        }
        // 候选表还在收集（`total == 0`）：不显示 `0/0`，给一句「正在扫描…」。
        (Some(Progress { total: 0, .. }), _) => (
            catalog.text(Key::SettingsJavaScanningStart).to_string(),
            true,
        ),
        (Some(Progress { done, total }), _) => (
            catalog.render(
                Key::SettingsJavaScanning,
                &[
                    ("done", done.to_string().as_str()),
                    ("total", total.to_string().as_str()),
                ],
            ),
            true,
        ),
        (None, true) if state.runtimes.is_empty() => {
            (catalog.text(Key::SettingsJavaEmptyTitle).to_string(), false)
        }
        (None, true) => (
            catalog.render(
                Key::SettingsJavaScanDone,
                &[("count", state.runtimes.len().to_string().as_str())],
            ),
            false,
        ),
        (None, false) => (
            catalog.text(Key::SettingsJavaNeverScanned).to_string(),
            false,
        ),
    };

    let scan_label = catalog.text(Key::SettingsJavaScan);
    // M237：这一整张卡片是 `frame::card` 画的（`Role::SurfaceContainerLow`），所以卡片里的
    // 控件把底传成 `frame::BASE`——禁用态的 10% / 12% 叠在卡底上，不是画布上。
    let scan = if scanning || !state.storage {
        buttons::filled(ui, Button::disabled(scan_label).base(BASE))
    } else {
        buttons::filled(
            ui,
            Button::new(scan_label, msg(JavaMessage::Scan)).base(BASE),
        )
    };
    // 已经按过取消（线程还没退出）时，「取消」也灰掉：再按一次没有任何意义。
    let cancel = buttons::tonal(
        ui,
        if scanning && !state.cancelled {
            Button::new(
                catalog.text(Key::SettingsJavaCancel),
                msg(JavaMessage::CancelScan),
            )
        } else {
            Button::disabled(catalog.text(Key::SettingsJavaCancel))
        }
        .base(BASE),
    );

    // 「扫描已取消」那句只在线程**真退出之后**出现；退出之前状态行本身写着「正在取消…」。
    let cancelled = (state.cancelled && state.scanning.is_none()).then(|| {
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::SettingsJavaCancelled).to_string(),
            ui.color(Role::OnSurfaceVariant),
        )
    });

    let mut block = column![row![
        container(text_role_owned(
            ui,
            TypeRole::BodyMedium,
            true,
            text,
            ui.color(if state.scanning.is_some() {
                Role::Primary
            } else {
                Role::OnSurface
            }),
        ))
        .width(Length::Fill),
        scan,
        cancel,
    ]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill)
    .align_y(Alignment::Center),]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    if let Some(line) = cancelled {
        block = block.push(line);
    }
    block.into()
}

/// 运行时列表：每行一个 JDK，带「默认」标记与「设为默认」chip。
fn runtime_list<'a>(ui: &'a Ui, catalog: Catalog, state: &'a State) -> Element<'a, Message> {
    let mut list = column![].spacing(ui.space(Space::X2)).width(Length::Fill);

    if state.runtimes.is_empty() {
        let body = if state.scanned {
            catalog.text(Key::SettingsJavaEmptyBody)
        } else {
            catalog.text(Key::SettingsJavaNeverScannedBody)
        };
        return text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            body.to_string(),
            ui.color(Role::OnSurfaceVariant),
        );
    }

    for runtime in &state.runtimes {
        list = list.push(runtime_row(ui, catalog, state, runtime));
    }
    list.into()
}

fn runtime_row<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    state: &'a State,
    runtime: &'a RuntimeRow,
) -> Element<'a, Message> {
    let is_default = super::state::same_path(&runtime.binary, &state.selected);
    // M197：**带方向**的三态。M175 的布尔判定对「1.12.2 + Java 21」会说「低于要求：需要 Java 8
    // 及以上」——21 当然 ≥ 8，那句话自相矛盾；现在两个方向各说各的。
    let (verdict, verdict_color) = match state.fit(runtime.major) {
        Some(super::state::Fit::Ok) => (
            catalog.text(Key::SettingsJavaFits).to_string(),
            ui.color(Role::Primary),
        ),
        Some(super::state::Fit::TooOld { required, .. }) => (
            catalog.render(
                Key::SettingsJavaTooOld,
                &[("minimum", required.to_string().as_str())],
            ),
            ui.color(Role::Error),
        ),
        Some(super::state::Fit::TooNew { maximum, .. }) => (
            catalog.render(
                Key::SettingsJavaTooNew,
                &[("maximum", maximum.to_string().as_str())],
            ),
            ui.color(Role::Error),
        ),
        None => (String::new(), ui.color(Role::OnSurfaceVariant)),
    };

    let action = chip(ui, action_chip(catalog, state, runtime).base(BASE));

    let mut block = column![row![
        container(
            column![
                text_role_owned(
                    ui,
                    TypeRole::BodyMedium,
                    is_default,
                    runtime.display_home(),
                    ui.color(Role::OnSurface),
                ),
                text_role_owned(
                    ui,
                    TypeRole::BodySmall,
                    false,
                    format!(
                        "{} · {}",
                        runtime.summary(),
                        catalog.text(runtime.source_key())
                    ),
                    ui.color(Role::OnSurfaceVariant),
                ),
            ]
            .spacing(ui.space(Space::X1))
            .width(Length::Fill),
        )
        .width(Length::Fill),
        action,
    ]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill)
    .align_y(Alignment::Center)]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    if !verdict.is_empty() {
        block = block.push(text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            verdict,
            verdict_color,
        ));
    }
    block.into()
}

/// 一行右侧的动作位（M206 把它抽成纯函数，好让断言直接对着它量对比度）：
///
/// - **当前默认**那一行 → [`Chip::current`]：与下载组选中 chip **同一套颜色**
///   （`SecondaryContainer` + `OnSecondaryContainer`），但没有消息、不吃五态；
/// - 其余 → 可点的 [`Chip::filter`]`(…, false, …)`「设为默认」。
///
/// 为什么不用 `Chip::badge` 表示当前默认：`badge` 走的是**禁用**配色（`OnSurface`@38% 叠 surface，
/// M204 实测 **1.84:1 浅 / 2.28:1 深**）。把「暂不可用」的语汇贴在「哪个 JDK 是默认」这个
/// **答案本身**上，就是 M206 要修的那件事。档位判据表见 `components::selection::chip` 的模块文档。
pub(super) fn action_chip<'a>(
    catalog: Catalog,
    state: &State,
    runtime: &'a RuntimeRow,
) -> Chip<'a, Message> {
    if super::state::same_path(&runtime.binary, &state.selected) {
        Chip::current(catalog.text(Key::SettingsJavaDefault))
    } else {
        Chip::filter(
            catalog.text(Key::SettingsJavaSetDefault),
            false,
            msg(JavaMessage::SelectRuntime(runtime.binary.clone())),
        )
    }
}
