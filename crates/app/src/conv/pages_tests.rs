//! conv/pages.rs 的单测（拆文件守 400 行上限）。

use super::*;
use slint::Model;
use unlucky_presenter::dto::{
    AccountView, DeviceLoginView, InstallProgressView, JavaPageView, JavaVendorView, NoticeView,
    SettingGroupView, SettingRowView, VersionSummary,
};
use unlucky_presenter::wizard::WizardState;

#[test]
fn accounts_and_device_login_render() {
    let mut state = AppState::new();
    state.accounts = vec![AccountView {
        id: "account:1".to_string(),
        name: "Steve".to_string(),
        kind: DtoAccountKind::Offline,
        profile_id: "0f8f".to_string(),
        login_name: String::new(),
        created: "2026-10-31 20:12".to_string(),
        active: true,
    }];
    state.device_login =
        DeviceLoginView::waiting("ABCD-EFGH", "https://www.microsoft.com/link", 900, "");

    let rows = accounts(&state);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].active);
    assert_eq!(rows[0].kind_label, "离线");
    assert_eq!(active_account_name(&state), "Steve");

    let device = device(&state);
    assert!(device.visible && device.busy);
    assert_eq!(device.user_code, "ABCD-EFGH");
    assert_eq!(device.phase_label, "等待确认");
}

#[test]
fn browser_login_panel_renders_the_authorization_link() {
    let mut state = AppState::new();
    state.device_login = DeviceLoginView::browser_waiting(
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?client_id=x",
        300,
        None,
    );
    let device = device(&state);
    assert!(device.visible && device.busy);
    assert_eq!(device.user_code, "", "授权码流没有用户码");
    assert!(device.verification_uri.contains("/authorize?client_id=x"));
    assert_eq!(device.qr_link, device.verification_uri);
    assert_eq!(device.expires, 300);
}

#[test]
fn install_progress_renders_labels_and_terminal_flags() {
    let mut state = AppState::new();
    state.install = Some(InstallProgressView {
        version: "1.20.4".to_string(),
        instance: "生存".to_string(),
        loader: "Fabric 0.16.5".to_string(),
        stage: DtoInstallStage::Downloading,
        files_done: 3,
        files_total: 6,
        bytes_done: 1024,
        bytes_total: 4096,
        speed_bps: 2048,
        eta_secs: 2,
        message: "下载中".to_string(),
    });
    let ui = install(&state);
    assert!(ui.visible && !ui.terminal && !ui.failed);
    assert!((ui.ratio - 0.5).abs() < f32::EPSILON);
    assert_eq!(ui.files_label, "3/6 文件");
    assert_eq!(ui.speed, "2.0 KB/s");
    assert_eq!(ui.eta, "剩 2s");

    state.install = None;
    assert!(!install(&state).visible);
}

#[test]
fn java_chain_and_vendors_map_outcomes() {
    let mut state = AppState::new();
    state.java = JavaPageView {
        vendors: vec![JavaVendorView {
            id: "temurin".to_string(),
            display: "Eclipse Temurin".to_string(),
            rank: 1,
            majors: "17/21".to_string(),
            sources: vec!["BMCLAPI".to_string(), "GitHub".to_string()],
            installed: true,
        }],
        chain: vec![unlucky_presenter::dto::JavaChainStep {
            vendor: "temurin".to_string(),
            outcome: JavaChainOutcome::Hit,
            detail: "21".to_string(),
        }],
        ..JavaPageView::new()
    };
    let vendors = java_vendors(&state);
    assert_eq!(vendors.len(), 1);
    assert_eq!(vendors[0].sources, "BMCLAPI · GitHub");
    let chain = java_chain(&state);
    assert!(matches!(chain[0].outcome, JavaStepOutcome::Hit));
    assert_eq!(chain[0].outcome_label, "命中");
}

#[test]
fn settings_groups_map_controls() {
    let mut state = AppState::new();
    state.settings_groups = vec![SettingGroupView::new("download", "下载")
        .with(SettingRowView::toggle("agent.mcp.enabled", "MCP", "", true))
        .with(SettingRowView::choice(
            "download.region",
            "地区",
            "",
            &["auto", "cn"],
            1,
        ))];
    let groups = setting_groups(&state);
    assert_eq!(groups.len(), 1);
    let rows = groups[0].rows.clone();
    assert_eq!(rows.row_count(), 2);
    let first = rows.row_data(0).expect("第一行");
    let second = rows.row_data(1).expect("第二行");
    assert!(first.flag);
    assert!(matches!(second.control, UiControl::Choice));
    assert_eq!(second.selected, 1);
}

#[test]
fn notices_and_wizard_render() {
    let mut state = AppState::new();
    state.notices = vec![NoticeView::new(
        "isolation:x",
        NoticeLevel::Warn,
        "隔离档位已切换",
        "两侧都有内容",
    )];
    let notices = notices(&state);
    assert!(matches!(notices[0].kind, NoticeKind::Warn));

    state.wizard = WizardState::open();
    state.wizard.select_version("1.20.4");
    let wizard = wizard(&state);
    assert!(wizard.open);
    assert_eq!(wizard.step_label, "选择版本");
    assert!(wizard.can_next);
    assert_eq!(wizard.loader, 0);
}

#[test]
fn wizard_versions_skip_ancient_ones() {
    let mut state = AppState::new();
    state.versions = vec![
        VersionSummary {
            id: "1.20.4".to_string(),
            kind: DtoVersionKind::Release,
            released: "2023-12-07".to_string(),
            installed: false,
        },
        VersionSummary {
            id: "rd-132211".to_string(),
            kind: DtoVersionKind::Ancient(unlucky_presenter::dto::AncientKind::Classic),
            released: "2009-05-13".to_string(),
            installed: false,
        },
    ];
    let rows = wizard_versions(&state);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "1.20.4");
}
