//! `dto::events` 的 IPC 契约用例（M223 从 `events.rs` 尾部**纯搬移**，一行未改）。
//!
//! 为什么搬：M223 要给 `AppCommand` / `CoreEvent` 各加一条变体（「导入已有版本」的指令与事件），
//! 而 `events.rs` 当时正好 399 行——加两行变体 + 两行 import 就顶破 400 行硬线（AGENTS §3.1）。
//! 拆分口径与仓库其它拆分一致：**纯搬移、一行不改**，`mod tests;` 只是把位置挪到
//! `dto/events/tests.rs`（`events.rs` 是非 `mod.rs` 文件，子模块目录就是 `events/`）。
//!
//! 搬移后 `events.rs` 只剩契约本体（枚举 + 构造器 + `Backend` trait），本文件的断言一条没动，
//! 所以「搬移零行为变化」可以用逐行 diff 对。

use super::*;
use crate::dto::{AccountKind, DeviceLoginPhase, InstallStage};

#[test]
fn events_are_serializable_ipc_contracts() {
    let event = AppEvent::Core(CoreEvent::InstallProgress(Some(InstallProgressView {
        stage: InstallStage::Downloading,
        files_done: 3,
        files_total: 9,
        ..InstallProgressView::new()
    })));
    let text = serde_json::to_string(&event).expect("序列化");
    let back: AppEvent = serde_json::from_str(&text).expect("反序列化");
    assert_eq!(back, event);

    let account = AppEvent::Core(CoreEvent::AccountsChanged(vec![AccountView {
        id: "account:1".to_string(),
        name: "Steve".to_string(),
        kind: AccountKind::Offline,
        profile_id: "0f8f".to_string(),
        login_name: String::new(),
        created: "2026-10-31 20:12".to_string(),
        active: true,
        server_url: String::new(),
        skin: None,
    }]));
    let text = serde_json::to_string(&account).expect("序列化");
    let back: AppEvent = serde_json::from_str(&text).expect("反序列化");
    assert_eq!(back, account);
    // 凭据永不跨界：序列化文本里没有 token 类字段
    assert!(!text.to_lowercase().contains("token"));
}

#[test]
fn commands_round_trip() {
    let command = AppCommand::CreateInstance(CreateRequest {
        version: "1.20.4".to_string(),
        loader: crate::dto::LoaderKind::Fabric,
        loader_version: "0.16.5".to_string(),
        name: "生存".to_string(),
    });
    let text = serde_json::to_string(&command).expect("序列化");
    let back: AppCommand = serde_json::from_str(&text).expect("反序列化");
    assert_eq!(back, command);

    let login = AppCommand::StartDeviceLogin;
    assert_eq!(
        serde_json::from_str::<AppCommand>(&serde_json::to_string(&login).expect("序列化"))
            .expect("反序列化"),
        login
    );
    assert!(DeviceLoginPhase::Waiting.busy());

    // 第三方轨（M114）：请求进、状态出，两边都不带 token 字段
    let authlib = AppCommand::AddAuthlibAccount(AuthlibRequest::new(
        "https://littleskin.cn/api/yggdrasil/",
        "me@example.com",
        "hunter2",
    ));
    let text = serde_json::to_string(&authlib).expect("序列化");
    assert_eq!(
        serde_json::from_str::<AppCommand>(&text).expect("反序列化"),
        authlib
    );

    let view = AppEvent::Core(CoreEvent::AuthlibLoginChanged(AuthlibLoginView::reachable(
        "https://littleskin.cn/api/yggdrasil/",
        "LittleSkin",
    )));
    let text = serde_json::to_string(&view).expect("序列化");
    let back: AppEvent = serde_json::from_str(&text).expect("反序列化");
    assert_eq!(back, view);
    assert!(
        !text.to_lowercase().contains("token"),
        "回程视图里不该有 token"
    );
}
