//! 状态 → `page_state.slint` 的视图模型（账户 / Java / 安装 / 设置分组 / 通知 / 向导）。
//!
//! 与 `conv/{instances,versions,logs,agent}.rs` 同一套路：只做「领域值 → 展示值」的搬运，
//! 不含业务判断。`ui_bind::sync` 把这些灌进 `PageState` global。

use slint::{ModelRc, SharedString, VecModel};

use crate::dto::{
    AccountKind as DtoAccountKind, DeviceLoginPhase, InstallStage as DtoInstallStage,
    JavaChainOutcome, NoticeLevel, SettingControl, VersionKind as DtoVersionKind,
};
use crate::state::AppState;
use crate::wizard::WizardStep;
use crate::{
    AccountKind, InstallStage, JavaStepOutcome, NoticeKind, SettingControl as UiControl, UiAccount,
    UiDeviceCode, UiInstallProgress, UiJavaRuntime, UiJavaStep, UiJavaVendor, UiNotice,
    UiSettingGroup, UiSettingRow, UiVersion, UiWizard,
};

fn strings(items: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .iter()
            .map(|item| SharedString::from(item.as_str()))
            .collect::<Vec<_>>(),
    ))
}

/// 账户行。
pub fn accounts(state: &AppState) -> Vec<UiAccount> {
    state
        .accounts
        .iter()
        .map(|account| UiAccount {
            id: account.id.as_str().into(),
            name: account.name.as_str().into(),
            kind: match account.kind {
                DtoAccountKind::Offline => AccountKind::Offline,
                DtoAccountKind::Microsoft => AccountKind::Microsoft,
                DtoAccountKind::Authlib => AccountKind::Authlib,
            },
            kind_label: account.kind.label().into(),
            profile_id: account.profile_id.as_str().into(),
            login_name: account.login_name.as_str().into(),
            created: account.created.as_str().into(),
            active: account.active,
        })
        .collect()
}

/// 当前启动账户的展示名（导航底部头像）。
pub fn active_account_name(state: &AppState) -> String {
    state
        .accounts
        .iter()
        .find(|account| account.active)
        .map(|account| account.name.clone())
        .unwrap_or_else(|| "未登录".to_string())
}

/// 微软登录面板。
pub fn device(state: &AppState) -> UiDeviceCode {
    let view = &state.device_login;
    UiDeviceCode {
        visible: view.phase.visible(),
        busy: view.phase.busy(),
        user_code: view.user_code.as_str().into(),
        verification_uri: view.verification_uri.as_str().into(),
        qr_link: view.qr_link.as_str().into(),
        expires: i32::try_from(view.expires_in).unwrap_or(i32::MAX),
        message: view.message.as_str().into(),
        phase_label: match view.phase {
            DeviceLoginPhase::Idle => "空闲",
            DeviceLoginPhase::Requesting => "申请中",
            DeviceLoginPhase::Waiting => "等待确认",
            DeviceLoginPhase::Succeeded => "成功",
            DeviceLoginPhase::Failed => "失败",
        }
        .into(),
    }
}

/// Java 发行版行。
pub fn java_vendors(state: &AppState) -> Vec<UiJavaVendor> {
    state
        .java
        .vendors
        .iter()
        .map(|vendor| UiJavaVendor {
            id: vendor.id.as_str().into(),
            display: vendor.display.as_str().into(),
            rank: i32::try_from(vendor.rank).unwrap_or(0),
            majors: vendor.majors.as_str().into(),
            sources: vendor.sources.join(" · ").into(),
            installed: vendor.installed,
        })
        .collect()
}

/// Java 回退链。
pub fn java_chain(state: &AppState) -> Vec<UiJavaStep> {
    state
        .java
        .chain
        .iter()
        .map(|step| UiJavaStep {
            vendor: step.vendor.as_str().into(),
            outcome: match step.outcome {
                JavaChainOutcome::Pending => JavaStepOutcome::Pending,
                JavaChainOutcome::Skip => JavaStepOutcome::Skip,
                JavaChainOutcome::Failed => JavaStepOutcome::Failed,
                JavaChainOutcome::Hit => JavaStepOutcome::Hit,
            },
            outcome_label: step.outcome.label().into(),
            detail: step.detail.as_str().into(),
        })
        .collect()
}

/// Java 本地运行时。
pub fn java_runtimes(state: &AppState) -> Vec<UiJavaRuntime> {
    state
        .java
        .runtimes
        .iter()
        .map(|runtime| UiJavaRuntime {
            binary: runtime.binary.as_str().into(),
            version: runtime.version.as_str().into(),
            vendor: runtime.vendor.as_str().into(),
            origin: runtime.origin.as_str().into(),
        })
        .collect()
}

/// 安装进度卡片。
pub fn install(state: &AppState) -> UiInstallProgress {
    let Some(progress) = &state.install else {
        return UiInstallProgress {
            visible: false,
            stage: InstallStage::Planning,
            ..UiInstallProgress::default()
        };
    };
    UiInstallProgress {
        visible: true,
        version: progress.version.as_str().into(),
        instance: progress.instance.as_str().into(),
        loader: progress.loader.as_str().into(),
        stage: match progress.stage {
            DtoInstallStage::Planning => InstallStage::Planning,
            DtoInstallStage::Downloading => InstallStage::Downloading,
            DtoInstallStage::Installing => InstallStage::Installing,
            DtoInstallStage::Registering => InstallStage::Registering,
            DtoInstallStage::Done => InstallStage::Done,
            DtoInstallStage::Failed => InstallStage::Failed,
            DtoInstallStage::Cancelled => InstallStage::Cancelled,
        },
        stage_label: progress.stage.label().into(),
        ratio: progress.ratio(),
        known: progress.is_known(),
        files_label: progress.files_label().into(),
        bytes_label: if progress.bytes_total > 0 {
            format!(
                "{}/{}",
                crate::dto::human_bytes(progress.bytes_done),
                crate::dto::human_bytes(progress.bytes_total)
            )
        } else {
            String::new()
        }
        .into(),
        speed: progress.speed_label().into(),
        eta: progress.eta_label().into(),
        message: progress.message.as_str().into(),
        terminal: progress.stage.is_terminal(),
        failed: progress.stage == DtoInstallStage::Failed,
    }
}

/// 设置分组（行 + 控件）。
pub fn setting_groups(state: &AppState) -> Vec<UiSettingGroup> {
    state
        .settings_groups
        .iter()
        .map(|group| UiSettingGroup {
            id: group.id.as_str().into(),
            title: group.title.as_str().into(),
            rows: ModelRc::new(VecModel::from(
                group
                    .rows
                    .iter()
                    .map(|row| UiSettingRow {
                        key: row.key.as_str().into(),
                        label: row.label.as_str().into(),
                        description: row.description.as_str().into(),
                        control: match row.control {
                            SettingControl::Toggle => UiControl::Toggle,
                            SettingControl::Choice => UiControl::Choice,
                            SettingControl::Text => UiControl::Text,
                            SettingControl::ReadOnly => UiControl::Readonly,
                            SettingControl::Action => UiControl::Action,
                        },
                        flag: row.flag,
                        selected: row.selected,
                        options: strings(&row.options),
                        value_text: row.value_text.as_str().into(),
                        action: row.action.as_str().into(),
                    })
                    .collect::<Vec<_>>(),
            )),
        })
        .collect()
}

/// 横幅通知。
pub fn notices(state: &AppState) -> Vec<UiNotice> {
    state
        .notices
        .iter()
        .map(|notice| UiNotice {
            id: notice.id.as_str().into(),
            kind: match notice.level {
                NoticeLevel::Info => NoticeKind::Info,
                NoticeLevel::Warn => NoticeKind::Warn,
                NoticeLevel::Error => NoticeKind::Error,
            },
            title: notice.title.as_str().into(),
            body: notice.body.as_str().into(),
        })
        .collect()
}

/// 向导状态。
pub fn wizard(state: &AppState) -> UiWizard {
    let wizard = &state.wizard;
    let step = wizard.step;
    let can_next = match step {
        WizardStep::Version => !wizard.version.trim().is_empty(),
        WizardStep::Name => crate::wizard::validate_name(&wizard.name).is_ok(),
        WizardStep::Loader | WizardStep::Confirm => true,
        WizardStep::Progress => false,
    };
    UiWizard {
        open: wizard.open,
        step: i32::try_from(step.index()).unwrap_or(0),
        step_label: step.title().into(),
        step_index: i32::try_from(step.index()).unwrap_or(0),
        step_total: 5,
        version: wizard.version.as_str().into(),
        loader: loader_index(wizard.loader),
        loader_label: if wizard.loader == crate::dto::LoaderKind::Vanilla {
            "原版".to_string()
        } else {
            format!("{} {}", wizard.loader.label(), wizard.loader_version.trim())
                .trim()
                .to_string()
        }
        .into(),
        loader_version: wizard.loader_version.as_str().into(),
        name: wizard.name.as_str().into(),
        error: wizard.error.as_str().into(),
        busy: wizard.submitting,
        can_next,
    }
}

/// 加载器在向导里的单选序号（与 `wizard.slint` 的 LoaderChoice 顺序一致）。
pub fn loader_index(loader: crate::dto::LoaderKind) -> i32 {
    match loader {
        crate::dto::LoaderKind::Vanilla => 0,
        crate::dto::LoaderKind::Fabric => 1,
        crate::dto::LoaderKind::Quilt => 2,
        crate::dto::LoaderKind::Forge => 3,
        crate::dto::LoaderKind::NeoForge => 4,
        crate::dto::LoaderKind::OptiFine | crate::dto::LoaderKind::LiteLoader => 0,
    }
}

/// 向导里的版本列表（正式版 + 快照；远古版走下载页的显式入口）。
pub fn wizard_versions(state: &AppState) -> Vec<UiVersion> {
    state
        .versions
        .iter()
        .filter(|version| !version.kind.is_ancient())
        .map(|version| UiVersion {
            id: version.id.as_str().into(),
            kind: match version.kind {
                DtoVersionKind::Release => crate::VersionKind::Release,
                _ => crate::VersionKind::Snapshot,
            },
            released: version.released.as_str().into(),
            installed: version.installed,
            ancient: false,
        })
        .collect()
}

#[cfg(test)]
#[path = "pages_tests.rs"]
mod tests;
