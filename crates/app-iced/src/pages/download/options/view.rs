//! 选项区的控件树（M105；窄档形态 M227）。
//!
//! ## 版面（424 正文列里的四段，三栏档 `Shared`）
//!
//! ```text
//! ── 滚动区 ──────────────────────────────────────
//! 1.12.2                                          [安装]
//! [原版][Forge][NeoForge][Fabric][Quilt][LiteLoader]   ← 六枚一行
//! ── 钉死区 ──────────────────────────────────────
//! Vanilla parent only — Forge goes through the CLI   ← 说明行；没有说明时是「加载器版本」标签
//! ┌ 1.12.2-SNAPSHOT   ┐  ← 官方那份列表（menu + 滚动，按条目数占高：1 条 64 / 2 条 112）
//! └───────────────────┘
//! [实例名                ]
//! ```
//!
//! 窄档（chips 档，可用宽 < 600，`rows::TailShare::Narrow`）**钉死区只剩实例名**，说明行与
//! 菜单都进滚动区——那里的高度只够「钉住实例名 + 3 整行版本列表」，算术见
//! `rows/tail.rs::CHIPS_PANEL_BUDGET`：
//!
//! ```text
//! ── 滚动区（首屏 104px = head 40 + 间距 4 + chips 32 + 间距 8 + 说明 20）──
//! 1.12.2                                          [安装]
//! [原版][Forge][NeoForge][Fabric][Quilt][LiteLoader]
//! Vanilla parent only — Forge goes through the CLI
//! ┌ 1.12.2-SNAPSHOT   ┐  ← 露在折线下方，滚得到（滚动条在右）
//! ── 视口底 12px 淡出遮罩（`fade::viewport_fade`）────
//! ── 钉死区 ──────────────────────────────────────
//! [实例名                ]
//! ```
//!
//! - **钉死档**（原版等不带版本列表的面板）走第三条路：标签、说明、菜单、实例名都在同一列里
//!   顺序铺开（`04` 基线图就是这一档，不能改）；
//! - 不兼容时的原因行（Error 色）在滚动区末尾（**窄档**可达；三栏档 + 带版本列表时
//!   `problem` 恒为 `None`，所以那支看不到它）；
//! - 说明只有 LiteLoader 有（`Key::DownloadOptionsLiteLoaderVanillaOnly`）；
//! - 窄档的滚动视口底边会硬裁内容（M227 返工前正切在版本菜单容器上沿，露出 21px 没有字形的
//!   色块），所以那里铺一条 12px 淡出遮罩把内容渐变到画布色——遮罩在视口内（≤660），
//!   压不到钉死区的实例名（≥668）。
//!
//! ## 为什么加载器是**六枚一行**（M222 D30 r1 实测订正）
//!
//! 这里原先写「五家一起排会溢出 ⇒ 折成两行」，那是 M105 的推算。D30 的实测图（1180×760，
//! 正文列 424）是**六枚一行、末枚右缘约 404px**，没有溢出——所以示意与说明都改成了实际形态。
//! chip 的几何仍是组件自己的真源（`ChipGeometry`），页面不许压它；再来第七家时先看图再决定
//! 是否换行。
//!
//! ## 为什么「不兼容」不禁用 chip、而是禁用确认键
//!
//! 禁用的 chip 点不动，用户就**看不到原因**（那正是「选完才失败」的另一种形态）。
//! 现在的做法：五家永远都能点，选中不兼容的那家时下面出现一行具名原因，同时「安装」变成禁用态。
//! 于是「为什么不能装」和「怎么才能装」永远同屏。

use iced::widget::{column, container, row, scrollable, stack};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::app::components::inputs::{menu, Menu, MenuItem};
use crate::app::components::inputs::{text_field, Field, FieldVariant};
use crate::app::components::selection::{chip, Chip};
use crate::theme::Ui;

use super::super::frame;
use super::super::rows::TailShare;
use super::super::Message;
use super::fade;
use super::model::{Loader, LoaderVersionsView, OptionsView};

// 加载器 chip 的顺序与集合**委派**给 `Loader::ALL`（原版 / Forge / NeoForge / Fabric /
// Quilt / LiteLoader）。这里原先是手写的五枚常量——同一份清单的第三处副本
// （`dto::wizard_choices`、`Loader::ALL`、这里），M222 加 LiteLoader 时三处都得改，索性委派。
// D30 r1 实测（1180×760 与 720×760 两种宽度、浅/深两套主题）：六枚 chip 排在一行，
// 末枚（LiteLoader）右缘约 404px，正文列 424 仍有余量；窄窗 720 下同一行也没有溢出。

/// 加载器版本菜单的**可见高度**（像素）：最多两行项 + 上下留白 8×2。
///
/// 项高 48 是 `MenuGeometry` 的真源；`Space::X2 = 8` 是上下留白 ⇒ 两行 = 112、一行 = 64。
/// 为什么按**条目数**取（M222 D30 r1 S1）：LiteLoader 对 1.12.2 只有一条版本，原来固定撑到
/// 两行高（112px），那 48px 纯属空档，却正好是把上面版本列表挤成 2.4 行的元凶之一。
/// 单测 `options/view::tests` 直接钉这两档的数值。
pub(crate) fn version_menu_height(items: usize) -> f32 {
    const ITEM: f32 = 48.0;
    let rows = items.clamp(1, 2) as f32;
    rows * ITEM + Space::X2.dp() * 2.0
}

/// 选项区（面板的下半块）：**可滚的上半块 + 钉死的下半块**。
///
/// `share` = 分高方式（`rows::TailShare`，三个形态见 `install::panel_with` 的文档）：
///
/// - `Pinned`（原版等不带版本列表的面板）走另一条路：标签、说明、菜单、实例名都在同一列里
///   顺序铺开（`04` 基线图就是这一档，不能改）；
/// - `Shared`（三栏档 + 带版本列表）：
///   - **滚动区**：head（版本 id + 安装按钮）+ 六枚加载器 chips；
///   - **钉死区**：说明行（有说明时；否则是「加载器版本」标签）+ 版本菜单 + 实例名输入框。
/// - `Narrow`（chips 档 + 面板占高度，M227）：
///   - **滚动区**：head + 六枚 chips + 说明行 + 错误行 + **版本菜单**；
///   - **钉死区**：只有实例名输入框。
///
/// 为什么菜单在三栏档钉、在窄档滚：裁切位置由**钉死区高度**决定（D30 四审），
/// 760 下菜单曾正好落在切线上（容器 642–706 只有 18px 可见、条目文字一格没画出来）。
/// 三栏档把菜单一起钉住之后不再依赖首屏剩余高度分配；窄档没有那么多高度——
/// 钉住菜单 + 说明 + 实例名要 180~228px，再加首屏 head+chips 76 就把列表挤到 ≤1 整行，
/// 而 M125 的像素守门要求 ≥3 整行。所以窄档只钉实例名，其余进滚动区（预算算术见
/// `rows/tail.rs::CHIPS_PANEL_BUDGET`）。
///
/// D30 八审实测（1180×760，`Shared`）：滚动区可见高 ≈96–104px，内容（head 40 + chips 32 +
/// 间距 4）≈76px ⇒ head（墨迹 462–501）与六枚 chips（506–537，32px 四角闭合）完整在首屏。
/// **M238 复量**（同一判据：视口 = 面板内容顶 → 钉死区上一行 − 8px 间距）：1180×760 与 736×760
/// 修前都只有 **54px**（这解释了「六枚 chip 只露 10px」——不是 736 独有），修后 **98px**；
/// 900 档修前 62px、修后 106px。差数来源见 `rows/tail.rs::panel_budget`。
/// 窄档 720×760 = **120px**，对首屏 104px（head 40 + 4 + chips 32 + 8 + 说明行 20）留 16px 余量
/// ⇒ head、六枚 chips、说明行三者完整在首屏，版本菜单露出容器上沿（滚动条在右）。
pub(crate) fn section<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    options: &'a OptionsView,
    share: TailShare,
) -> Element<'a, Message> {
    let head = row![
        container(frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            options.version.as_str(),
            ui.color(Role::OnSurface),
        ))
        .width(Length::Fill),
        confirm(ui, catalog, options),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center);

    // 钉死档（原版 / 没有加载器版本列表）**保持旧结构**：标签与说明都在列内。
    // 为什么不一并改：`04` 是别处基线图的对照，它的面板矮、列表 6 行——把标签撤出会
    // 让面板矮 26px，列表涨到 302px（6.86 行）反而切出半行。方案 B 只对共享档生效。
    if matches!(share, TailShare::Pinned) {
        let mut plain = column![
            head,
            frame::text_role(
                ui,
                TypeRole::LabelMedium,
                false,
                catalog.text(Key::DownloadOptionsTitle),
                ui.color(Role::OnSurfaceVariant),
            ),
            loader_row(ui, catalog, options),
        ]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill);
        if let Some(note) = &options.note {
            plain = plain.push(frame::text_role(
                ui,
                TypeRole::BodySmall,
                false,
                note.as_str(),
                ui.color(Role::OnSurfaceVariant),
            ));
        }
        if show_versions(options) {
            plain = plain.push(frame::text_role(
                ui,
                TypeRole::LabelMedium,
                false,
                catalog.text(Key::DownloadOptionsLoaderVersion),
                ui.color(Role::OnSurfaceVariant),
            ));
            plain = plain.push(version_list(ui, catalog, options));
        }
        if let Some(problem) = &options.problem {
            plain = plain.push(frame::text_role(
                ui,
                TypeRole::BodySmall,
                false,
                problem.as_str(),
                ui.color(Role::Error),
            ));
        }
        plain = plain.push(text_field(
            ui,
            Field::new(options.instance_name.as_str())
                .variant(FieldVariant::Outlined)
                .label(catalog.text(Key::DownloadOptionsInstanceName))
                .placeholder(catalog.text(Key::DownloadOptionsInstanceNameHint)),
            Message::SetInstanceName,
        ));
        return plain.into();
    }

    // 共享档的滚动区以 head + 六枚 chips 起头（方案 B，D30 六审裁决；错误行见下）。
    //
    // 为什么把「安装选项」标签撤出滚动区：五审 82px → 六审 103px 的视口里，
    // head(40) + 标签(17) + chips(32) + 两处间距(16) = 105px，差 2px 就把 chip 的底边框裁掉；
    // 再塞说明（20px）就差 27~29px，说明整块 0 像素。
    // 并排标签（方案 C）在这里不可行：六枚 chip 实测已占 404px 正文列只有 424px，
    // 加上「安装选项」必然横向溢出、把第六枚裁掉——那是拿一个已完成项换另一个。
    // 所以按塔授权的 B 落地：**说明**搬到钉死区（见下面的 `pinned`）；「安装选项」标签在
    // 共享档**删掉**（不是搬——八审裁决，见下三行），Pinned 档保留。滚动区只装控件。
    // 共享档**没有**「安装选项」标签：这是设计决定（D30 八审裁决）——六枚 chip 自解释，
    // 且正文列只有 424px、六枚 chip 已占 404px，加标签必然横向裁掉最后一枚。
    // 原版等 Pinned 档仍保留该标签（见上面 `Pinned` 那条路）。
    let narrow = matches!(share, TailShare::Narrow);
    let mut body = column![head, loader_row(ui, catalog, options)]
        .spacing(ui.space(Space::X1))
        .width(Length::Fill);

    // 窄档：说明行（或「加载器版本」标签）与版本菜单**都进滚动区**——钉死区只剩实例名。
    // 顺序必须是 head → chips → 说明 → 菜单：预算（`CHIPS_PANEL_BUDGET`）保证前 104px 完整，
    // 菜单露在折线下方（滚动条可见，滚得到）。
    if narrow && show_versions(options) {
        body = body.push(versions_label(ui, catalog, options));
    }

    // 中性说明在三栏档**只在钉死区画一份**（八审：这里原先也画了一份，两处像素级重影）。
    // 窄档没有钉死的那一行（见上），所以它在这里画、也只画一次。
    if let Some(problem) = &options.problem {
        body = body.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            problem.as_str(),
            ui.color(Role::Error),
        ));
    }

    if narrow && show_versions(options) {
        body = body.push(version_list(ui, catalog, options));
    }

    // 钉死区：三栏档是「说明/标签 + 菜单 + 实例名」，窄档只剩实例名。见 `section` 的文档。
    let mut pinned = column![].spacing(ui.space(Space::X2)).width(Length::Fill);
    if !narrow && show_versions(options) {
        pinned = pinned.push(versions_label(ui, catalog, options));
        pinned = pinned.push(version_list(ui, catalog, options));
    }
    pinned = pinned.push(text_field(
        ui,
        Field::new(options.instance_name.as_str())
            .variant(FieldVariant::Outlined)
            .label(catalog.text(Key::DownloadOptionsInstanceName))
            .placeholder(catalog.text(Key::DownloadOptionsInstanceNameHint)),
        Message::SetInstanceName,
    ));

    let scroll_area: Element<'a, Message> = scrollable(body)
        .height(Length::Fill)
        .style(frame::scroll_style(ui))
        .into();

    // 窄档：视口底部铺一条 12px 淡出遮罩（M227 返工 ①，理由见 `fade::viewport_fade`）。
    // 三栏档不铺：那里滚动区内容恒为 head 40 + 间距 4 + chips 32 = 76px，而视口 ≈118px，
    // 永远不会出现「半截内容」；铺上去只是「画布盖画布」，反而动了 M222 已验的 1180 像素。
    let scroll_area: Element<'a, Message> = if narrow {
        stack![
            scroll_area,
            container(fade::viewport_fade(ui, fade::VIEWPORT_FADE_PX))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_y(Alignment::End),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        scroll_area
    };

    column![scroll_area, pinned]
        .spacing(ui.space(Space::X2))
        .height(Length::Fill)
        .width(Length::Fill)
        .into()
}

/// 版本菜单上方那一行：**有中性说明就显示说明**（它讲的是「这一家的叠法」，位置就在版本菜单
/// 正上方，语义对得上）；没有说明时才退回通用标签。
///
/// 高度相同（`BodySmall` 与 `LabelMedium` 都是一行），所以无论放滚动区还是钉死区都不改预算。
/// 注意标签不是「搬」下来的：共享档的「安装选项」标签是**删掉**的（八审裁决，见 `section`）。
fn versions_label<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    options: &'a OptionsView,
) -> Element<'a, Message> {
    match &options.note {
        Some(note) => frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            note.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
        None => frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            catalog.text(Key::DownloadOptionsLoaderVersion),
            ui.color(Role::OnSurfaceVariant),
        ),
    }
}

/// 加载器版本这一段要不要画（见 `section` 的注释）。
fn show_versions(options: &OptionsView) -> bool {
    if options.loader == Loader::Vanilla || options.problem.is_some() {
        return false;
    }
    match &options.versions {
        LoaderVersionsView::Loading => true,
        LoaderVersionsView::Ready(items) => !items.is_empty(),
        LoaderVersionsView::Failed(_) => false,
    }
}

/// 一行加载器 chip（Filter 档：选中 = `SecondaryContainer`）。
fn loader_row<'a>(ui: &'a Ui, catalog: Catalog, options: &'a OptionsView) -> Element<'a, Message> {
    let mut line = row![]
        .spacing(ui.space(Space::X1))
        .align_y(Alignment::Center);
    for loader in Loader::ALL.iter() {
        line = line.push(chip(
            ui,
            Chip::filter(
                loader.label(catalog),
                options.loader == *loader,
                Message::SetLoader(*loader),
            ),
        ));
    }
    line.into()
}

/// 官方那份加载器版本列表（`menu` + 滚动：Fabric 有上百条，不做下拉也不铺满屏）。
fn version_list<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    options: &'a OptionsView,
) -> Element<'a, Message> {
    match &options.versions {
        LoaderVersionsView::Loading => frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::DownloadOptionsLoaderVersionsLoading),
            ui.color(Role::OnSurfaceVariant),
        ),
        // 取不到 / 一条都没有：原因已经在上面那行 `problem` 里说清，这里不再重复。
        LoaderVersionsView::Failed(_) => frame::gap(0.0),
        LoaderVersionsView::Ready(items) if items.is_empty() => frame::gap(0.0),
        LoaderVersionsView::Ready(items) => {
            // **内联列表口径**（M238）：这张菜单被套在下面的 `scrollable` 里，而 CPU 后端画
            // quad 阴影时绕过裁切掩码 ⇒ L2 阴影会漏出视口、把钉死区的实例名压暗 7.2%
            // （浅色画像布 `(249,249,250)` → `(231,231,232)`）。理由与两后端差异见
            // `components::inputs::menu::shadow` 的 `flat` 文档。
            let mut list = Menu::new().width(Length::Fill).flat();
            for item in items {
                let mut entry =
                    MenuItem::new(item.as_str(), Message::SetLoaderVersion(item.clone()));
                if *item == options.loader_version {
                    entry = entry.selected();
                }
                list = list.item(entry);
            }
            scrollable(menu(ui, list))
                .height(Length::Fixed(version_menu_height(items.len())))
                .style(frame::scroll_style(ui))
                .into()
        }
    }
}

/// 「安装」：不兼容 / 列表没到位 / 已装 都按下不去（禁用态保留几何，只清消息）。
fn confirm<'a>(ui: &'a Ui, catalog: Catalog, options: &'a OptionsView) -> Element<'a, Message> {
    if options.installed {
        return buttons::outlined(ui, Button::disabled(catalog.text(Key::DownloadInstalled)));
    }
    if !options.can_confirm {
        return buttons::outlined(ui, Button::disabled(catalog.text(Key::DownloadInstall)));
    }
    buttons::filled(
        ui,
        Button::new(
            catalog.text(Key::DownloadInstall),
            Message::InstallVersion(options.version.clone()),
        )
        .leading(IconKind::Download),
    )
}
