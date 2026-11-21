//! 设置页「General」分组：语言选择器 + 主题 / 启动页 / 关闭按钮（M106 接真控件）。
//!
//! **这一组归 language+appearance 线**（M101 的分线口径）。
//!
//! ## 这一组为什么特殊
//!
//! 它**不走** `Group::body`：那个渲染器拿的是 [`States`] 与**生效语言** `Lang`
//! （M179 起；此前连 `Lang` 都没有），而语言行要的是「当前语言**档位** `LangChoice` + 生效语言」
//! 这一对——档位不在 `States` 里。所以它由 `pages::settings::view` 的 `language` 形参一路递进来、
//! `form` 按下标转交 [`body`]；`GROUP.body` 仍是 `None`，`tests/mod.rs` 那条「只有外观组带真控件」
//! 照样成立。
//!
//! ## 语言切换**没有**走 M101 的 `GroupMessage` 投递口（偏离，已登记）
//!
//! `settings/mod.rs` 的文档说本线要加消息就加到 `settings/language_appearance.rs` 的 `Message` 上。
//! M106 走的是 `pages::Message::Language(choice)` 这条独立增量，理由：语言档位不是「某一组的状态」，
//! 它要**落盘**（`ui.language`）+ **重建整棵视图模型**（目录是 `pages::Model` 的一部分），而落点
//! `app/update/settings/language_appearance.rs` 与消息口 `language_appearance.rs` 都不在本 mission 的
//! scope 内。代价：`page_messages.rs` 多一支（那是 M101 想收掉的共享锚点）。**收口方向**：等本线
//! 再动设置页时，把 `Message::Language` 折进 `GroupMessage::LanguageAppearance(SetChoice)`，并把
//! `pages/mod.rs` 的这支删掉——finding 已登记。
//!
//! ## 语言行的三个事实
//!
//! 1. **选中态显示的是档位**（跟随系统 / 简体中文 / 英文），不是「猜出来的语言」——用户点过什么
//!    就得画回什么，否则「我明明选了跟随系统」这件事在界面上无从确认；
//! 2. **旁边回显生效语言**（`界面当前使用：简体中文`）：中文系统 + 跟随系统时，这一行是「界面
//!    真的用了中文」的唯一可见证据（用户报的 bug 正是「识别到了但没用上」）；
//! 3. **文案全部走 catalog**（[`Key`]），三档选项名跟着界面语言走（见 `catalog/general.rs`
//!    的头注释：为什么不写语言自称）。
//!
//! ## 主题行已接真控件（M229），另两行仍只读
//!
//! 主题三档（Ink / Silver / 跟随系统）在 M229 接成真控件：点一下换色板 + 落盘
//! `appearance.theme_mode`（实现在 `general/theme.rs`，消息是 `pages::Message::Theme`）。
//! 启动页 / 关闭按钮还没有后端（`pages::Action` 里是占位动作），仍是**只读行**；
//! 但标签与取值已经进 catalog，中文界面下不会再冒出英文。
//!
//! 分组元数据只有一份（这里的 [`GROUP`]）：`settings/mod.rs` 的 `GROUPS` 直接引用它。

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::catalog::Key;

use crate::app::components::interaction::Interaction;
use crate::fonts::install::{CjkInstall, FetchClass};
use crate::pages::{frame, Language, Message};
use crate::theme::Ui;

use super::frame::{card, divider};
use super::{Group, States};

mod language;
mod theme;

/// General 分组（左列名固定英文：左列是**导航**，与导航轨同源，不进 catalog）。
pub(super) const GROUP: Group = Group {
    name: "General",
    icon: IconKind::Settings,
    // 只给 harness（`pages_demo` 走 `appearance: None` 时）用的演示行：产品路径走 [`body`]。
    // 语言与主题那两行**不在**这里——它们的值跟着语言/档位变，写死在静态表里就是用户看到的假值
    // （语言 M106 移出、主题 M229 移出）。
    rows: &[
        (Key::SettingsDemoStartupPage, "Instances"),
        (Key::SettingsDemoCloseButton, "Exit to tray"),
    ],
    body: None,
};

/// 这一组的正文（**产品路径**）。
///
/// `forced` 是 `--force-state`（M285）：本组只有语言行 / 主题行 / 日志行三处有可交互控件，三处都往下递。
pub(super) fn body<'a>(
    ui: &'a Ui,
    states: States<'a>,
    language: Language,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let catalog = language.effective.catalog();
    let mut stacked = column![].spacing(0.0).width(Length::Fill);
    stacked = stacked.push(language::view(ui, language, forced));
    // M168（D28 后半）：中文字体的下载状态行。紧贴语言行——用户在这里手选中文，反馈就该在这行下面，
    // 而不是等他切到别的页面才发现「刚才那几秒在干嘛」。
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(font_row(ui, language));

    // M229：主题行接真控件（Ink / Silver / 跟随系统）。档位住在设置组的自有状态里
    // （`language_appearance::State::theme`），与语言行同一个排版：标题 + 三枚互斥 chip + 一句说明。
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(theme::view(
        ui,
        &catalog,
        states.groups.language_appearance.theme,
        forced,
    ));

    // M188：日志文件夹（路径 + 打开 + 复制路径）。放在「启动页/关闭按钮」之前——用户投诉
    // 「找不到崩溃日志」要的答案，不该埋在只读行后面。
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(super::logs_row::folder_row(ui, &catalog, forced));

    // 启动页 / 关闭按钮：还没有后端，只读；文案已经进 catalog。
    for (label, value) in [
        (Key::SettingsStartupPage, Key::SettingsStartupInstances),
        (Key::SettingsCloseButton, Key::SettingsCloseToTray),
    ] {
        stacked = stacked.push(divider(ui));
        stacked = stacked.push(setting_row(ui, catalog.text(label), catalog.text(value)));
    }

    card(ui, stacked)
}

/// 中文字体状态行的**文案**（纯函数：状态 × 目录 → 一句话 + 可选的一句解释）。
///
/// 为什么单独拎出来：截图只能证明「画出来了」，而这里能被断言证明「四种状态的词都在、占位符都被
/// 替换、两套目录都没有 `⟪missing⟫`」（见 `tests/general_section.rs`）。
///
/// 失败态**不写具体错误串**：那是给我们看的（`[font] cjk-download failed` 那行日志），用户只需要
/// 知道「网络 / 内容 / 磁盘」里哪一类，以及「界面照常可用」。
pub(super) fn font_status(
    catalog: &unlucky_i18n::catalog::Catalog,
    status: &CjkInstall,
) -> (String, Option<String>) {
    match status {
        CjkInstall::Unknown => (catalog.text(Key::FontCjkStatusUnknown).to_owned(), None),
        CjkInstall::Unprobeable => (catalog.text(Key::FontCjkStatusUnprobeable).to_owned(), None),
        CjkInstall::NotNeeded => (catalog.text(Key::FontCjkStatusNotNeeded).to_owned(), None),
        CjkInstall::System { family } => (
            catalog.render(Key::FontCjkStatusSystem, &[("family", family)]),
            None,
        ),
        CjkInstall::Downloading { bytes } => (
            catalog.render(
                Key::FontCjkStatusDownloading,
                &[("size", &format_size(*bytes))],
            ),
            Some(catalog.text(Key::FontCjkStatusHint).to_owned()),
        ),
        CjkInstall::Ready { family, .. } => (
            catalog.render(Key::FontCjkReady, &[("family", family)]),
            Some(catalog.text(Key::FontCjkStatusHint).to_owned()),
        ),
        CjkInstall::Failed { class, .. } => {
            let reason = catalog.text(match class {
                FetchClass::Network => Key::FontCjkReasonNetwork,
                FetchClass::Integrity => Key::FontCjkReasonIntegrity,
                FetchClass::Disk => Key::FontCjkReasonDisk,
            });
            (
                catalog.render(Key::FontCjkStatusFailed, &[("reason", reason)]),
                None,
            )
        }
    }
}

/// 字节数 → 人话体积（17 772 300 B → `16.9 MB`，按 1 MiB = 1048576 B）。只用于「≈」级别的量。
fn format_size(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    let mib = bytes as f64 / MIB;
    if mib >= 1.0 {
        format!("{mib:.1} MB")
    } else {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    }
}

/// 中文字体状态行：标签一行、状态一行（失败红、就绪主色）、需要时再补一句解释。
///
/// 为什么**竖排**而不是像主题那几行「标签左、取值右」：失败那句话是整句（「获取失败（网络不通）。
/// 界面照常可用…」），塞进右列会把它压成一行挤掉标签（实测截图就是一整行红字、标签消失）。
/// 语言行也是竖排（标题 + 选项 + 说明），这一行贴着它，观感一致。
fn font_row<'a>(ui: &'a Ui, language: Language) -> Element<'a, Message> {
    let catalog = language.effective.catalog();
    let status = crate::fonts::install::status();
    let (value, hint) = font_status(&catalog, &status);
    let color = match status {
        CjkInstall::Failed { .. } => ui.color(Role::Error),
        CjkInstall::Ready { .. } | CjkInstall::System { .. } => ui.color(Role::Primary),
        _ => ui.color(Role::OnSurfaceVariant),
    };

    let mut stacked = column![
        frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            catalog.text(Key::FontCjkStatusLabel),
            ui.color(Role::OnSurface),
        ),
        container(frame::text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            value,
            color
        ))
        .width(Length::Fill),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .padding(Padding::from([ui.space(Space::X2), 0.0]));

    if let Some(hint) = hint {
        stacked = stacked.push(container(frame::text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            hint,
            ui.color(Role::OnSurfaceVariant),
        )));
    }

    stacked.into()
}

/// 「当前值回显」那一格的颜色（语言行「界面当前使用：…」与主题行「跟随系统」）。
///
/// ## 为什么是 [`Role::Primary`]，不是什么随便一个 accent（M353 P1-1）
///
/// M351 的取证量到这两格在**深色档只有 1.80:1**，浅色档 10.87:1——同一枚令牌两档差 6 倍，
/// 深色下等于看不见。原因是它用的是 [`Role::OnPrimaryFixedVariant`]：固定角色在明暗两档
/// **同值 tone 30**（`color/scheme.rs` 的固定角色行有测试钉死），它是给 `PrimaryFixedDim`
/// （tone 80）当配对的（`palette/wcag.rs` 的 `TEXT_PAIRS`），压在 canvas 色（tone 96 / 10）上时
/// 深色档必然掉下来。
///
/// `Primary` 是 M3 里**给 surface 上的高强调文字/图标**的那一枚（本项目已在这条语义上用过它：
/// `ListItem` 选中态的 trailing 图标就是 `Primary`）。它两档都过 AA——`Ui` 实际用的出厂色板
/// （Ink / Silver）实测：
///
/// | 档 | 卡片底 `SurfaceContainerLow` 上的对比度 | 改前（固定角色） |
/// |---|---|---|
/// | 深色 | **17.11:1** | 1.80:1 |
/// | 浅色 | **5.77:1** | 10.87:1 |
///
/// 浅色档看起来「变淡了」，但 5.77 仍在 AA 的 4.5 之上——这是同一个角色名在两档各取自己那一半
/// tone 轴的必然结果（推导链的 tone 是明 40 / 暗 80；出厂色板把这两枚调得更极端，方向一致）。
/// 想换回 `OnSurfaceVariant` 的人请注意：它会与下面那行说明（`SettingsLanguageHint`）同色，
/// 「当前值」与「解释」的层级就没了。像素与脚本见 `DevDocs/artifacts/darkfix/`。
pub(super) const VALUE_ECHO: Role = Role::Primary;

/// 一个只读的「标签 + 取值」行（与 `frame::demo_card` 的一行同高同排版）。
fn setting_row<'a>(ui: &'a Ui, label: &'a str, value: &'a str) -> Element<'a, Message> {
    let height = ui.metrics().control_height + Space::X2.dp();

    row![
        container(frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            label,
            ui.color(Role::OnSurface),
        ))
        .width(Length::Fill)
        .align_y(Alignment::Center),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            value,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .height(height)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .into()
}
