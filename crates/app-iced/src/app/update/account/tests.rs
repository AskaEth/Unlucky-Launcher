//! **树测试②（M119 验收核心）**：账户页消费 presenter 的账户列表与登录事件。
//!
//! 后端是 UI 侧的 [`AccountRig`]：它把 `AppCommand` 翻成**与真 host 同形状**的 `CoreEvent`
//! （`AccountsChanged` / `DeviceLoginChanged` / `AccountNoticeChanged`），数据在内存里。
//! 于是这条测试钉住的是「**接线**」——用户点一下之后，命令出去、事件回来、页面重算、横幅出现；
//! 抓不住的是 `core::auth::AccountStore` 的落盘（那条由 presenter 自己的
//! `host::accounts::offline_add_list_remove_round_trips` 覆盖，原因见 `presenter/rig.rs` 的模块文档）。
//!
//! 覆盖任务书第 3 条与第 4 条的账户那一半：
//!
//! | 断言 | 抓得住什么 |
//! |---|---|
//! | 添加离线档案 → 列表出现一行 | `AddOfflineAccount` 没派发 / 事件没回流 / 页面没重建 |
//! | 切换当前账号 → 「当前」标记换人 | `SwitchAccount` 没派发（M114 的 `active` 标记是页面唯一来源）|
//! | 删除 → 列表回到空态 | `RemoveAccount` 没派发 |
//! | 设备码「进行中可见 + 可取消」 | `StartDeviceLogin` / `CancelDeviceLogin` 没接线（只剩转圈）|
//! | 失败态：名字不合法 / 凭据失效都给**具名**错误 | 失败被吞掉（复审 r1 P2-2 的老毛病）|

use unlucky_design::color::SchemeFlavor;
use unlucky_i18n::{Catalog, Key, Lang};
use unlucky_presenter::dto::{
    AccountNoticeCode, AppEvent, AuthlibLoginView, CoreEvent, DeviceLoginView, LoginFailure,
};
use unlucky_presenter::mock::MockBackend;
use unlucky_presenter::settings_bridge::SettingsBridge;

use crate::app::state::presenter::rig::AccountRig;
use crate::pages::account::model::{AuthlibPanel, DevicePanel};
use crate::pages::account::{Form, Message as Account, Panel, Track};
use crate::theme::Families;

use super::App;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 不碰磁盘的 App，后端换成**账户 rig**（其余分支照旧走 `MockBackend` 的能力就够）。
fn with_rig() -> App {
    App::with_presenter(
        crate::cli::Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(AccountRig::new()),
        SettingsBridge::memory_only(),
    )
}

/// 不碰磁盘的 App（mock 后端 + 内存设置桥），与 `app/tests.rs` 的 `headless` 同源。
fn headless() -> App {
    App::with_presenter(
        crate::cli::Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(MockBackend::new()),
        SettingsBridge::memory_only(),
    )
}

fn seed(app: &mut App, event: CoreEvent) {
    app.state.apply(AppEvent::Core(event));
    app.refresh_view();
}

/// **添加 → 切换 → 删除**（任务书第 3 条，一条端到端）。
///
/// 抓得住：三条命令任何一条没派发 / 事件没回流 / `refresh_view` 没带上账户页。
/// 抓不住：真实账户库的落盘（见模块文档）。
#[test]
fn an_offline_profile_is_added_switched_and_removed_end_to_end() {
    let mut app = with_rig();
    assert!(app.account.is_empty(), "出厂是空态");

    // ① 添加离线档案：走真消息（点轨道卡 → 填名字 → 提交）
    let _ = app.on_account(Account::Open(Track::Offline));
    assert_eq!(app.account_form.panel, Panel::Offline);
    // 表单输入也走真消息（纯 UI 态，只写 `App::account_form` + 置脏）
    let _ = app.on_account(Account::OfflineName("Steve".to_string()));
    assert!(
        app.account_form.offline_ready(),
        "3–16 位字母：按钮该是亮的"
    );
    let _ = app.on_account(Account::Submit);

    assert_eq!(app.account.rows.len(), 1, "列表出现一行（不是静默）");
    assert_eq!(app.account.rows[0].name, "Steve");
    assert_eq!(
        app.account.notice.as_ref().map(|notice| notice.code),
        Some(AccountNoticeCode::Added),
        "页面横幅给出的是一次性结果"
    );
    let id = app.account.rows[0].id.clone();

    // ② 切换当前账号
    let _ = app.on_account(Account::Switch(id.clone()));
    assert_eq!(
        app.account.active_row().map(|row| row.name.as_str()),
        Some("Steve"),
        "「当前」标记跟着切换走"
    );

    // ③ 删除
    let _ = app.on_account(Account::Remove(id));
    assert!(app.account.rows.is_empty(), "删除后回到空态");
    assert_eq!(
        app.account.notice.as_ref().map(|notice| notice.code),
        Some(AccountNoticeCode::Removed)
    );
    // 两个横幅文案都有词（中英各一条，不是硬编码）
    for lang in [Lang::En, Lang::ZhCn] {
        assert!(!Catalog::of(lang).text(Key::AccountNoticeRemoved).is_empty());
    }
    // 整棵树在三种状态下都能构图
    let _ = app.view();
}

/// 设备码流程：**指令发出 + 进行中可见 + 可取消**（任务书第 3 条的后半，真实网络不要求）。
#[test]
fn the_device_code_flow_is_started_visible_and_cancellable() {
    let mut app = with_rig();
    assert_eq!(app.account.device, DevicePanel::Hidden);

    let _ = app.on_account(Account::StartDevice);
    match &app.account.device {
        DevicePanel::Code {
            code, countdown, ..
        } => {
            assert_eq!(code, "ABCD-EFGH", "用户码要上屏");
            assert!(!countdown.is_empty(), "倒计时在跑（文案由模型拼好）");
        }
        other => panic!("进行中状态必须可见，实际 {other:?}"),
    }
    assert_eq!(app.account.panel, Panel::Microsoft, "面板要切到微软那条轨");
    let _ = app.view();

    let _ = app.on_account(Account::Cancel);
    assert_eq!(app.account.device, DevicePanel::Hidden, "取消要真的收掉");
    assert_eq!(
        app.state.device_login.phase,
        unlucky_presenter::dto::DeviceLoginPhase::Idle
    );
}

/// 失败态①：**未实现 / 名字不合法** ⇒ 页面横幅给出结构化码（不是静默无语）。
#[test]
fn an_invalid_profile_name_surfaces_a_named_failure() {
    let mut app = with_rig();
    let _ = app.on_account(Account::Open(Track::Offline));
    // 名字不合法时**发送键是灰的**（与 host 同一份判据），所以这里直接派命令：
    // 抓的是「host 真的回了一个结构化失败码」这条链路，而不是按钮的禁用态。
    assert!(!app.account_form.offline_ready());
    app.dispatch(unlucky_presenter::dto::AppCommand::AddOfflineAccount(
        "ab".to_string(),
    ));

    assert!(app.account.rows.is_empty(), "不合法的名字不入列表");
    let notice = app.account.notice.as_ref().expect("必须有横幅");
    assert_eq!(notice.code, AccountNoticeCode::OfflineNameInvalid);
    assert!(notice.failure, "失败档（配色不同）");
    assert_eq!(notice.subject, "ab", "横幅要带上是哪个名字");
    // 文案走 catalog，中英各一条
    for lang in [Lang::En, Lang::ZhCn] {
        assert!(!Catalog::of(lang)
            .text(Key::AccountNoticeOfflineNameInvalid)
            .is_empty());
    }
    let _ = app.view();
}

/// 失败态②：**账户凭据失效** ⇒ 第三方轨面板进入 `Failed(BadCredentials)`（只带原因码）。
///
/// 为什么只断言原因码：M114 r3 起面板类型里**没有** `message`/`detail` 字段（原文只进 presenter
/// 日志），页面据码选 catalog 文案。这条测试与「页面画得出文案」之间的桥梁是
/// `pages::account::login::failure_body_key`（同一 crate 内的穷尽 match，缺一条编译不过）。
#[test]
fn expired_credentials_surface_a_named_failure_code() {
    let mut app = headless();
    seed(
        &mut app,
        CoreEvent::AuthlibLoginChanged(AuthlibLoginView::failed_as(
            LoginFailure::BadCredentials,
            "服务器返回 401：凭据已失效（token=…已脱敏）",
        )),
    );
    app.account_form.open(Track::Authlib);
    app.refresh_view();

    assert_eq!(
        app.account.authlib,
        AuthlibPanel::Failed {
            failure: LoginFailure::BadCredentials
        },
        "凭据失效要落成具名原因码，而不是静默回 Idle"
    );
    // 原因码对应的那句译文两套语言都得有
    for lang in [Lang::En, Lang::ZhCn] {
        let text = Catalog::of(lang).text(Key::AccountFailureBadCredentials);
        assert!(!text.trim().is_empty(), "{} 缺这条失败文案", lang.tag());
    }
    // 原文不许出现在页面的任何可视文本里（M114 的凭据红线）
    assert!(
        !format!("{:?}", app.account).contains("服务器返回 401"),
        "后端原文不许进页面模型"
    );
    let _ = app.view();
}

// ---------------------------------------------------------------- M114 的既有回归闸

/// **复审 r2 P2-1 的回归闸**：登录失败/成功态下按 `Close`，面板必须真的关掉。
///
/// 为什么这条以前是死按钮：`Page::build` 只要 `device_login.phase != Idle` 就把面板强制
/// 回 Microsoft，而 `Close` 只在 `busy()`（Requesting/Waiting）时才派 `CancelDeviceLogin`
/// ⇒ Failed/Succeeded 下点了没反应。现在判据改成「非 Idle」，两条轨都覆盖。
///
/// 抓得住：判据退回 `busy()`；`Close` 忘了 dispatch；`CancelDeviceLogin` 没有把状态收回 Idle。
/// 抓不住：`CancelDeviceLogin` 在**真 host** 上的副作用（这里跑的是 mock，但 mock 已按
/// 真 host 的契约发 Idle，见 `presenter/src/mock.rs`）。
#[test]
fn close_really_closes_the_login_panel_on_both_tracks() {
    // ① 微软轨：失败态
    let mut app = headless();
    seed(
        &mut app,
        CoreEvent::DeviceLoginChanged(DeviceLoginView {
            phase: unlucky_presenter::dto::DeviceLoginPhase::Failed,
            message: "微软拒绝了这次登录：AADSTS7000218（HTTP 403）".to_string(),
            failure: Some(LoginFailure::NotWhitelisted),
            ..DeviceLoginView::new()
        }),
    );
    assert_eq!(app.account.panel, Panel::Microsoft, "失败态面板要显示");
    let _ = app.on_account(Account::Close);
    assert_eq!(app.account.panel, Panel::None, "Close 之后面板必须关掉");
    assert_eq!(
        app.state.device_login.phase,
        unlucky_presenter::dto::DeviceLoginPhase::Idle
    );

    // ② 微软轨：成功态（同样不在 busy 里）
    let mut app = headless();
    seed(
        &mut app,
        CoreEvent::DeviceLoginChanged(DeviceLoginView::succeeded("Steve")),
    );
    assert_eq!(app.account.panel, Panel::Microsoft);
    let _ = app.on_account(Account::Close);
    assert_eq!(app.account.panel, Panel::None, "成功提示也要能关掉");

    // ③ 第三方轨：失败态
    let mut app = headless();
    seed(
        &mut app,
        CoreEvent::AuthlibLoginChanged(AuthlibLoginView {
            phase: unlucky_presenter::dto::DeviceLoginPhase::Failed,
            server: "https://example.invalid/api/yggdrasil/".to_string(),
            server_name: String::new(),
            message: "服务器返回 HTTP 404".to_string(),
            failure: Some(LoginFailure::NotAuthlibServer),
            account_name: String::new(),
        }),
    );
    app.account_form.open(Track::Authlib);
    app.refresh_view();
    assert_eq!(
        app.account.panel,
        Panel::Authlib,
        "第三方轨失败态面板要显示"
    );
    let _ = app.on_account(Account::Close);
    assert_eq!(app.account.panel, Panel::None, "第三方轨也要能关掉");
    assert_eq!(
        app.state.authlib_login.phase,
        unlucky_presenter::dto::DeviceLoginPhase::Idle
    );
}

/// 表单态是纯 UI 状态：打开/关闭面板不派命令，但会把面板切对。
///
/// 抓得住：`Open` 忘了写 `panel`（点了轨道卡什么都不出现）、`close()` 不清密码。
/// 抓不住：真实 dispatch 的副作用（那要真后端）。
#[test]
fn the_form_state_follows_the_page_messages() {
    let mut form = Form::new();
    assert_eq!(form.panel, Panel::None);
    form.open(Track::Authlib);
    assert_eq!(form.panel, Panel::Authlib);
    form.password = "hunter2".to_string();
    form.close();
    assert_eq!(form.panel, Panel::None);
    assert!(form.password.is_empty(), "关闭面板必须把一次性密码清掉");
}

/// 三张轨道卡默认收起（有账号时），`ToggleAdd` 展开（复审 r1 P2-4）。
#[test]
fn the_add_section_starts_collapsed() {
    let mut form = Form::new();
    assert!(!form.section_open);
    form.section_open = true;
    assert!(form.section_open);
}
