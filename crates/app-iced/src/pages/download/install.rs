//! 安装面板（页面底部）：选中版本 → **安装选项** → 「安装」/ 进度 + 「取消」/ 失败 → 「打开日志」。
//!
//! M105 之前这里是「选中版本 → 直接『安装』」：点下去就开工，加载器维度整个不存在
//! （用户实跑的原话：「没是否有选择有前置加载器(版本选择)选项」）。选项区在 `options/`，
//! 本文件只管**面板形态的决策**（纯函数 [`panel_state`]）与「安装中 / 失败」两种形态。
//!
//! ## 这一块为什么钉在底部而不是跟着列表滚
//!
//! 列表装的是几十行版本；底部按钮如果跟着滚，用户在列表中间永远看不到「安装」在哪。
//! 所以列表独立滚动（`rows::list`），面板固定占尾部一块高度。
//!
//! **M125 的例外**：chips 档（窗口 < 736，竖栏退回顶部 chip 行）里这块面板太贵——它按内容高
//! 钉死会把版本列表压成 18px 的缝。那一档改成「列表与面板各吃一半、面板自己可滚」
//! （`rows::TailShare::Shared`）——但只在**面板真的占高度**时（`Options` / `Install`）；
//! 只画一行提示的 `Hint` 态仍钉死，否则 8 个版本的列表会被砍到 4 行（r1 复审 P2-1）。
//!
//! ## 已知风险：chips 档最多会同时出现**三层**滚动（r1 复审 P3-2，未验）
//!
//! ```text
//! 版本行列表 scrollable          ← 第 1 层（任何档位都在）
//! └ 面板 scrollable（chips 档才有）  ← 第 2 层
//!   └ 加载器版本 menu 的 scrollable   ← 第 3 层（`options/view.rs` 的 `VERSION_ROWS`）
//! ```
//!
//! **鼠标落在第 3 层上滚轮会不会冒泡到第 2 层，本仓没有实测通道**（静态图拍不到滚动嵌套）。
//! iced 的 `scrollable` 在指针悬停于内层时由内层消费滚动，所以「滚到底之后继续滚会带动外层」
//! 属于**未验证行为**；两个方向的坏结果分别是「面板卡住滚不动」与「一滚就跳到外层」。
//!
//! 缓解方案（本轮**没做**，理由：不是一处小改）——把「实例名」那一格从面板的滚动区里移出去，
//! 做成面板的固定页脚：那要把 `options::section` 拆成「可滚的上半块 + 钉死的下半块」，
//! 动 `options/view.rs` / `install.rs` / `rows.rs` 三处，而且改完仍然**无法**验证滚轮冒泡
//! （没有通道），只是把风险从「某一格被挡」换成「面板永不挡那一格」。所以先登记为已知风险。
//!
//! ## 只有意图，没有乐观回显
//!
//! | 控件 | 消息 | app 层落点 |
//! |---|---|---|
//! | 「安装」 | [`Message::InstallVersion`] | `AppCommand::CreateInstance(草稿)`（带加载器与实例名） |
//! | 「取消安装」 | [`Message::CancelInstall`] | `AppCommand::CancelInstall` |
//! | 「打开日志」 | [`Message::Navigate`]（`Route::Logs`） | `AppState::navigate`（不派指令） |
//!
//! 按钮按下**不改任何本地状态**：进度条读的是 `model.install`，它只由 presenter 的
//! `InstallProgress` 事件灌进来（`apply` → 视图模型重建）。于是「点了没变」= 后端还没回事件。
//!
//! ## 三种面板形态刻意不同构（M94 r1 / P2-3 + P2-4）
//!
//! | 状态 | 形态 | 为什么 |
//! |---|---|---|
//! | 选好了 | 版本 id + 实心「安装」 | 本页唯一的实心按钮（rubric K2） |
//! | 安装中 | 版本 id + 阶段 + **进度条**（仅当进度已知）+ 计数 + 文本「取消安装」 | 有进度才有条 |
//! | 失败 | **整块 `ErrorContainer` 面板**：`Error` 图标 + `Error` 色标题 + 停在哪 + 实心「打开日志」 | **不画进度条**——进度条正是「看起来还在跑」的来源；颜色/图标/措辞/按钮四样一起变，才一眼可辨 |
//!
//! 进度未知（刚派发、后端还没给文件计数）时**不画条**：页面没有帧订阅，`Progress::indeterminate`
//! 只会得到一个静止的假进度（复审 P2-3 的「17% 卡住」），不如只写「等第一个文件…」。

use iced::widget::{column, container, row};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_presenter::router::Route;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::app::components::feedback::{linear, Progress};
use crate::theme::Ui;

use super::super::frame;
use super::super::Message;
use super::model::{Download, InstallView};
use super::options::OptionsView;

/// 面板要画哪一种形态（**决策与画法分开**：决策是纯函数，单测直接钉它）。
///
/// 分开的理由就是本轮复审那件事：「装完之后面板还占着、用户再也装不了第二个版本」——
/// 这是**决策**错了，画法本身没错；把决策抽出来，测试才能在不开图形环境的情况下断言它。
#[derive(Debug, Clone, PartialEq)]
pub(super) enum PanelState<'a> {
    /// 没有安装、也没选中：一句「选一个版本再安装」
    Hint,
    /// 选中了一个版本：给出**安装选项区**（M105；加载器 / 加载器版本 / 实例名 + 「安装」）
    Options(&'a OptionsView),
    /// 有安装（进行中 / 失败）：进度或错误面板
    Install(&'a InstallView),
}

/// 面板此刻该画哪一种（纯函数，无 iced）。
///
/// 顺序是**安装优先**：有安装时面板讲安装，讲不了列表；安装结束后 `model.install` 由
/// [`Download::from_snapshot`](super::model::Download::from_snapshot) 清掉，于是这里自然回到
/// [`PanelState::Options`] / [`PanelState::Hint`]，用户能接着选下一个版本。
pub(super) fn panel_state(model: &Download) -> PanelState<'_> {
    match &model.install {
        Some(install) => PanelState::Install(install),
        None => match &model.options {
            Some(options) => PanelState::Options(options),
            None => PanelState::Hint,
        },
    }
}

/// 底部的安装面板。
pub(super) fn panel<'a>(ui: &'a Ui, catalog: Catalog, model: &'a Download) -> Element<'a, Message> {
    let content = match panel_state(model) {
        PanelState::Install(install) => installing(ui, catalog, install),
        // 选中版本之后**先给选项，再点「安装」**（M105：以前是选中就直接开工，等于没有加载器维度）
        PanelState::Options(options) => super::options::section(ui, catalog, options),
        PanelState::Hint => hint(ui, catalog),
    };

    container(content)
        .width(Length::Fill)
        .padding(Padding::from([ui.space(Space::X3), 0.0]))
        .into()
}

/// 没选中任何版本：给一句「选一个版本再安装」（不静默禁用，也不画一个点不动的按钮）。
fn hint<'a>(ui: &'a Ui, catalog: Catalog) -> Element<'a, Message> {
    frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        catalog.text(Key::DownloadSelectHint),
        ui.color(Role::OnSurfaceVariant),
    )
}

/// 安装中 / 已收尾（失败另走 [`failed`]）。
fn installing<'a>(ui: &'a Ui, catalog: Catalog, install: &'a InstallView) -> Element<'a, Message> {
    if install.failed() {
        return failed(ui, catalog, install);
    }

    let head = row![
        container(frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            install.version.as_str(),
            ui.color(Role::OnSurface),
        ))
        .width(Length::Fill),
        frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            install.stage_label.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center);

    let mut body = column![head];
    // 进度已知才画条：未知时画 indeterminate 会得到一个静止的假进度（见模块文档）。
    if install.known {
        body = body.push(linear(ui, Progress::determinate(install.ratio)));
    }
    body = body.push(frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        install.detail.as_str(),
        ui.color(Role::OnSurfaceVariant),
    ));
    if let Some(rate) = &install.rate {
        body = body.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            rate.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ));
    }

    let action = if install.running() {
        buttons::text(
            ui,
            Button::new(catalog.text(Key::DownloadCancel), Message::CancelInstall),
        )
    } else {
        frame::gap(0.0)
    };

    column![
        body.spacing(ui.space(Space::X2)).width(Length::Fill),
        action
    ]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill)
    .into()
}

/// 失败态：一整块 `ErrorContainer` 面板（**没有进度条**）。
///
/// 四样一起变，保证与「正在下载」一眼可辨：容器色（`ErrorContainer`）/ 图标（`Error`）/
/// 措辞（`Install failed` 用 `Error` 色标题）/ 主按钮（实心「打开日志」，不是文本按钮）。
fn failed<'a>(ui: &'a Ui, catalog: Catalog, install: &'a InstallView) -> Element<'a, Message> {
    let head = row![
        frame::glyph(IconKind::Error, ui.color(Role::Error), frame::ICON_PX),
        frame::text_role(
            ui,
            TypeRole::TitleSmall,
            true,
            catalog.text(Key::DownloadStageFailed),
            ui.color(Role::Error),
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center);

    let stopped = frame::text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        format!("{} · {}", install.version, install.detail),
        ui.color(Role::OnErrorContainer),
    );

    let actions = row![
        buttons::filled(
            ui,
            Button::new(
                catalog.text(Key::DownloadOpenLogs),
                Message::Navigate(Route::Logs),
            )
            .leading(IconKind::Logs),
        ),
        // 失败把面板钉住了：不给出口的话用户既重试不了、也装不了别的版本（只能重启）。
        // `CancelInstall` 在 host 侧就是「清 install + 发 InstallProgress(None)」，对没在跑的任务是安全 no-op。
        buttons::text(
            ui,
            Button::new(catalog.text(Key::DownloadDismiss), Message::CancelInstall),
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center);

    container(
        column![
            head,
            stopped,
            frame::text_role(
                ui,
                TypeRole::BodySmall,
                false,
                catalog.text(Key::DownloadFailedHint),
                ui.color(Role::OnErrorContainer),
            ),
            actions,
        ]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding(ui.space(Space::X4))
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ui.color(Role::ErrorContainer))),
        border: Border {
            radius: ui.radius_dp(ShapeScale::Large),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

#[cfg(test)]
mod tests;
