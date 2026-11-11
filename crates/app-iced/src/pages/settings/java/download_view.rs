//! 「下载一个 JDK」的视图（M197）：**版本选择列表 / 下载中 / 失败**三态。
//!
//! 三态各自能说清什么，是这一屏的全部要求：
//!
//! | 态 | 必须出现的 |
//! |---|---|
//! | 选择列表 | 8 / 11 / 17 / 21 / 25 每一档 + 它由哪几家发行版承接 + 与当前实例要求的关系 |
//! | 下载中 | 大版本 + 阶段 + 百分比（分母未知就不给）+ 当前源 + 最近一次「哪个源失败、多久后重试」 |
//! | 失败 | 大版本 + 最后试到的源 + 「完整原因在日志里」+ 下一步（查网络 / 改顺序） |
//!
//! 「与当前实例要求的关系」是任务书第三条：**不许静默失败**。老版本 MC 要 8、新版本要 21/25，
//! 用户在列表里能直接看到「装这一档也起不来」（[`Fit`] 的两向措辞），不必等启动那一刻才发现。
//!
//! 列表里的「已安装」判定来自**真扫盘的结论**（`state.runtimes`），不是另存一份安装记录。

mod result;

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

use unlucky_core::download::DownloadErrorKind;
use unlucky_core::game::java::catalog::{self, JdkChoice};
use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};

use crate::app::components::selection::{chip, Chip};
use crate::pages::frame::{divider, text_role_owned, BASE};
use crate::pages::Message;
use crate::theme::Ui;

use super::downloads::{Event, Job, Phase};
use super::state::{Fit, State};
use super::view::msg;
use super::Message as JavaMessage;

/// 一整块（标题 + 口径 + 目录 + 结果）。
pub(super) fn section<'a>(ui: &'a Ui, catalog: Catalog, state: &'a State) -> Element<'a, Message> {
    let report = state.downloads.report();
    let mut block = column![].spacing(ui.space(Space::X2)).width(Length::Fill);

    block = block.push(text_role_owned(
        ui,
        TypeRole::BodyMedium,
        true,
        catalog.text(Key::SettingsJavaDownloadTitle).to_string(),
        ui.color(Role::OnSurface),
    ));
    block = block.push(text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        catalog.text(Key::SettingsJavaDownloadHint).to_string(),
        ui.color(Role::OnSurfaceVariant),
    ));
    if let Some(refusal) = state.download_notice {
        block = block.push(text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.render(
                Key::SettingsJavaDownloadRefused,
                &[("reason", catalog.text(refusal.label()))],
            ),
            ui.color(Role::Error),
        ));
    }
    if let Some(required) = &state.context.required {
        block = block.push(text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.render(
                Key::SettingsJavaDownloadNeeded,
                &[("requirement", required.describe().as_str())],
            ),
            ui.color(Role::OnSurface),
        ));
    }

    let recommended = super::downloads::recommend(
        state.context.required.as_ref(),
        &state.runtimes,
        state.prefs.saved_auto_install,
    );
    let busy = report.job.is_some();
    for choice in catalog::choices() {
        block = block.push(choice_row(
            ui,
            catalog,
            state,
            *choice,
            recommended == Some(choice.major),
            busy,
        ));
    }

    if let Some(job) = &report.job {
        block = block.push(divider(ui));
        block = block.push(running(ui, catalog, job));
    }
    if let Some(success) = &report.success {
        block = block.push(divider(ui));
        block = block.push(result::done(ui, catalog, success));
    }
    if let Some(failure) = &report.failure {
        block = block.push(divider(ui));
        block = block.push(result::failed(ui, catalog, failure, &report.tries));
    }
    block.into()
}

/// 目录里的一行：大版本 + 徽标 + 承接的发行版 + 与当前实例的关系 + 安装按钮。
fn choice_row<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    state: &'a State,
    choice: JdkChoice,
    recommended: bool,
    busy: bool,
) -> Element<'a, Message> {
    let title = format!("Java {}", choice.major);
    let mut badges = row![].spacing(ui.space(Space::X1));
    // M237：徽标是**禁用配色**（`Chip::badge` = 12% 容器 / 38% 内容），而这个板块整个住在
    // `frame::card` 里 ⇒ 底必须是卡底，否则徽标会把禁用色叠在画布色上。
    if choice.lts {
        badges = badges.push(chip(
            ui,
            Chip::badge(catalog.text(Key::SettingsJavaDownloadLts)).base(BASE),
        ));
    }
    if choice.baseline {
        badges = badges.push(chip(
            ui,
            Chip::badge(catalog.text(Key::SettingsJavaDownloadBaseline)).base(BASE),
        ));
    }
    if recommended {
        badges = badges.push(chip(
            ui,
            Chip::badge(catalog.text(Key::SettingsJavaDownloadRecommended)).base(BASE),
        ));
    }

    let vendors = catalog::vendors_for(
        choice.major,
        &state.priority_vendors(),
        state.effective.domestic,
    );
    let vendors_line = if vendors.is_empty() {
        catalog.text(Key::SettingsJavaDownloadNoVendor).to_string()
    } else {
        catalog.render(
            Key::SettingsJavaDownloadVendors,
            &[(
                "vendors",
                vendors
                    .iter()
                    .map(|vendor| vendor.display())
                    .collect::<Vec<_>>()
                    .join(" → ")
                    .as_str(),
            )],
        )
    };

    let installed = state
        .runtimes
        .iter()
        .find(|runtime| runtime.major == Some(choice.major));

    let mut block = column![row![
        container(
            column![
                text_role_owned(
                    ui,
                    TypeRole::BodyMedium,
                    recommended,
                    title,
                    ui.color(Role::OnSurface),
                ),
                badges,
            ]
            .spacing(ui.space(Space::X1))
            .width(Length::Fill),
        )
        .width(Length::Fill),
        install_button(ui, catalog, choice.major, installed.is_some(), busy),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .align_y(Alignment::Center)]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    block = block.push(text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        vendors_line,
        ui.color(Role::OnSurfaceVariant),
    ));
    if let Some(fit) = state.fit(Some(choice.major)) {
        let (text, color) = fit_line(catalog, ui, fit);
        block = block.push(text_role_owned(ui, TypeRole::BodySmall, false, text, color));
    }
    if let Some(runtime) = installed {
        block = block.push(text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.render(
                Key::SettingsJavaDownloadInstalled,
                &[("path", runtime.display_home().as_str())],
            ),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    block.into()
}

/// 「装哪一档与当前实例合不合」那一行（两向措辞，来自 [`Fit`]）。
fn fit_line(catalog: Catalog, ui: &Ui, fit: Fit) -> (String, iced::Color) {
    match fit {
        Fit::Ok => (
            catalog.text(Key::SettingsJavaDownloadFits).to_string(),
            ui.color(Role::Primary),
        ),
        Fit::TooOld { required, .. } => (
            catalog.render(
                Key::SettingsJavaDownloadTooLow,
                &[("minimum", required.to_string().as_str())],
            ),
            ui.color(Role::Error),
        ),
        Fit::TooNew { maximum, .. } => (
            catalog.render(
                Key::SettingsJavaDownloadTooHigh,
                &[("maximum", maximum.to_string().as_str())],
            ),
            ui.color(Role::Error),
        ),
    }
}

/// 安装按钮：已在跑的作业期间一律灰掉（同时只允许一个），已装过的写成「重装」。
fn install_button<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    major: u32,
    installed: bool,
    busy: bool,
) -> Element<'a, Message> {
    let label = catalog.text(if installed {
        Key::SettingsJavaDownloadReinstall
    } else {
        Key::SettingsJavaDownloadAction
    });
    if busy {
        // 忙时那枚「下载」是**禁用配色**的徽标（不是可点 chip），同样叠在卡底上（M237）。
        return chip(ui, Chip::badge(label).base(BASE));
    }
    chip(
        ui,
        Chip::filter(label, false, msg(JavaMessage::Download(major))).base(BASE),
    )
}

/// 下载中：阶段 + 百分比 + 当前源 + 最近一次退避 / 换源。
fn running<'a>(ui: &'a Ui, catalog: Catalog, job: &Job) -> Element<'a, Message> {
    let header = catalog.render(
        Key::SettingsJavaDownloadBusy,
        &[("major", job.major.to_string().as_str())],
    );
    let phase = match job.phase {
        Phase::Preparing => catalog
            .text(Key::SettingsJavaDownloadPhasePreparing)
            .to_string(),
        Phase::Downloading => match job.percent() {
            Some(percent) => catalog.render(
                Key::SettingsJavaDownloadPhaseDownloading,
                &[
                    ("percent", percent.to_string().as_str()),
                    ("done", result::mega(job.downloaded).as_str()),
                    ("total", result::mega(job.total.unwrap_or(0)).as_str()),
                ],
            ),
            // 分母未知：只说已经下了多少，不给假的百分比。
            None => result::mega(job.downloaded),
        },
        Phase::Verifying => catalog
            .text(Key::SettingsJavaDownloadPhaseVerifying)
            .to_string(),
        Phase::Extracting => catalog
            .text(Key::SettingsJavaDownloadPhaseExtracting)
            .to_string(),
    };
    let mut block = column![
        text_role_owned(
            ui,
            TypeRole::BodyMedium,
            true,
            header,
            ui.color(Role::Primary),
        ),
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            phase,
            ui.color(Role::OnSurface),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);
    if let Some(source) = &job.source {
        block = block.push(text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.render(
                Key::SettingsJavaDownloadSource,
                &[("source", source.as_str())],
            ),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    if let Some(event) = &job.last {
        let text = event_line(catalog, event);
        block = block.push(text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            text,
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    block.into()
}

/// 「哪个源失败 / 多久后重试」或「换源」那一条。
fn event_line(catalog: Catalog, event: &Event) -> String {
    match event {
        Event::Retry {
            source,
            kind,
            attempt,
            retry_in_ms,
        } => catalog.render(
            Key::SettingsJavaDownloadRetry,
            &[
                ("source", source.as_str()),
                ("reason", source_reason(catalog, *kind).as_str()),
                ("attempt", attempt.to_string().as_str()),
                (
                    "seconds",
                    format!(
                        "{:.1}",
                        f64::from(u32::try_from(*retry_in_ms).unwrap_or(u32::MAX)) / 1000.0
                    )
                    .as_str(),
                ),
            ],
        ),
        Event::Switched { from, to } => catalog.render(
            Key::SettingsJavaDownloadSwitch,
            &[("from", from.as_str()), ("to", to.as_str())],
        ),
    }
}

/// `DownloadErrorKind` → 页面文案（core 的原句只进日志，见 `downloads.rs`）。
pub(super) fn source_reason(catalog: Catalog, kind: DownloadErrorKind) -> String {
    let key = match kind {
        DownloadErrorKind::NotFound => Key::SettingsJavaSourceReasonNotFound,
        DownloadErrorKind::ServerError => Key::SettingsJavaSourceReasonServer,
        DownloadErrorKind::Timeout => Key::SettingsJavaSourceReasonTimeout,
        DownloadErrorKind::Network => Key::SettingsJavaSourceReasonNetwork,
        DownloadErrorKind::Integrity => Key::SettingsJavaSourceReasonIntegrity,
        DownloadErrorKind::Io => Key::SettingsJavaSourceReasonIo,
        DownloadErrorKind::InvalidRequest
        | DownloadErrorKind::RangeUnsupported
        | DownloadErrorKind::AllSourcesFailed => Key::SettingsJavaSourceReasonOther,
    };
    catalog.text(key).to_string()
}
