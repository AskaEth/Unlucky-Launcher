//! 账户页视图模型的判据（M114 的核心断言都在这里）。
//!
//! | 用例 | 抓得住什么 |
//! |---|---|
//! | `a_jwt_injected_into_events_never_reaches_the_page` | **凭据红线**：DTO → 状态 → 视图模型这一路漏了脱敏 |
//! | `a_failure_with_a_provider_code_renders_english_and_the_code` | **语言泄漏（r1 p1）**：模型 + i18n 查表这一路又出现中文（视图层由类型禁画原文，见 `tests_text` 的边界说明） |
//! | `the_device_code_panel_shows_a_countdown_and_expires` | 设备码「只转圈」/ 倒计时不是人话格式 |
//! | `a_missing_client_id_tells_the_user_what_to_configure` | **M132**：缺 client id 又被画成通用失败（用户得翻日志才知道要设变量） |
//! | `a_network_failure_does_not_look_like_a_missing_config` | **M132 反向锚**：网络失败被误标成「未配置」，提示变噪音 |

use unlucky_i18n::{Key, Lang};
use unlucky_presenter::dto::{
    AccountKind, AccountNotice, AccountNoticeCode, AccountView, AppEvent, AuthlibLoginView,
    CoreEvent, DeviceLoginPhase, DeviceLoginView, LoginFailure, SkinImage,
};
use unlucky_presenter::state::AppState;

use super::avatar::Cache;
use super::model::{DevicePanel, Form, Page, Panel, Track};
use super::tests_text::{has_cjk, on_screen};

fn account(name: &str, kind: AccountKind, active: bool) -> AccountView {
    AccountView {
        id: format!("account:{name}"),
        name: name.to_string(),
        kind,
        profile_id: "0f8f1d3c4b5a69788796a5b4c3d2e1f0".to_string(),
        login_name: String::new(),
        created: "2026-10-31 20:12".to_string(),
        active,
        server_url: String::new(),
        skin: None,
    }
}

fn build(state: &AppState, form: &Form) -> Page {
    // 断言口径是英文界面（app 钉死 `Lang::En`）：英文目录下「视图里有中文」=「后端原文上屏了」。
    // 宽度取验收窗口 1180（r2 起 `Page` 记中栏可用宽）。
    Page::build(state, form, &mut Cache::default(), Lang::En, 1180.0)
}

const JWT: &str =
    "eyJhbGciOiJSUzI1NiIsIng1dCI6IkFCQyJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJlLXZhbHVl";

/// **凭据红线（审核项）**：把 JWT 形状串注入事件 → 状态与视图模型里都不许出现。
///
/// 抓得住：presenter 状态层的脱敏被删（`AppState::apply_device_login` / `apply_authlib_login`）。
/// 视图层不在此列：面板态里已无 `message`/`detail` 字段，想画原文是编译错误。
#[test]
fn a_jwt_injected_into_events_never_reaches_the_page() {
    let mut state = AppState::new();
    state.accounts = vec![account("Steve", AccountKind::Microsoft, true)];
    // **绕过构造器**直接摆一个带 JWT 的 DTO：模拟「将来某个新构造器忘了脱敏」/「后端把原文塞进来」。
    state.apply(AppEvent::Core(CoreEvent::DeviceLoginChanged(
        DeviceLoginView {
            phase: DeviceLoginPhase::Failed,
            message: format!("微软返回错误：accessToken={JWT} refresh_token={JWT}"),
            failure: Some(LoginFailure::Denied),
            ..DeviceLoginView::new()
        },
    )));
    state.apply(AppEvent::Core(CoreEvent::AuthlibLoginChanged(
        AuthlibLoginView {
            phase: DeviceLoginPhase::Failed,
            server: "https://littleskin.cn/api/yggdrasil/".to_string(),
            server_name: String::new(),
            message: format!("服务器回包异常 {JWT}"),
            failure: Some(LoginFailure::Denied),
            account_name: String::new(),
        },
    )));

    // ① 状态层已经脱敏（这一条抓得住「摘掉 state 层 redact」的变异）
    assert!(
        !state.device_login.message.contains("eyJ"),
        "{}",
        state.device_login.message
    );
    assert!(
        !state.authlib_login.message.contains("eyJ"),
        "{}",
        state.authlib_login.message
    );

    // ② 上屏文案更不可能有它（页面根本不画原文）
    let text = on_screen(&build(&state, &Form::new()));
    assert!(!text.contains("eyJ"), "视图里出现了 JWT：{text}");
    assert!(!text.contains("⟪redacted⟫"), "脱敏占位符也不该上屏：{text}");
}

/// **语言泄漏（复审 r1 p1）**：带 AADSTS 码的失败事件 → 英文取样里不许有中文，且要带 code。
///
/// 抓得住：`failure_body_key` 的码→key 映射被摘/接错；DTO 或状态层把中文混进模型字段。
/// 抓不住（复审 r2 P2-2 实测过）：视图层把原文画上屏——这条断言是**平行实现**。
/// 那道回归改由**类型**拦：`DevicePanel`/`AuthlibPanel` 已无 `message`/`detail` 字段。
/// 也抓不住：catalog 里某条英文译文本身写得差（那只能人工审图）。
#[test]
fn a_failure_with_a_provider_code_renders_english_and_the_code() {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::DeviceLoginChanged(
        DeviceLoginView {
            phase: DeviceLoginPhase::Failed,
            message:
                "微软拒绝了这次登录：AADSTS7000218（HTTP 403）—— 应用白名单还没批准，或网络被拦截"
                    .to_string(),
            failure: Some(LoginFailure::NotWhitelisted),
            ..DeviceLoginView::new()
        },
    )));
    let page = build(&state, &Form::new());
    let text = on_screen(&page);
    assert!(!has_cjk(&text), "英文界面里出现了中文：{text}");
    assert!(
        text.contains("whitelist_pending"),
        "面板要给稳定 code：{text}"
    );
    assert!(
        text.to_lowercase().contains("approved"),
        "要给出可读的英文原因：{text}"
    );

    // 第三方轨的失败同样只有 i18n + code
    state.apply(AppEvent::Core(CoreEvent::AuthlibLoginChanged(
        AuthlibLoginView {
            phase: DeviceLoginPhase::Failed,
            server: "https://example.invalid/".to_string(),
            server_name: String::new(),
            message: "服务器不可达：dns error".to_string(),
            failure: Some(LoginFailure::Unreachable),
            account_name: String::new(),
        },
    )));
    let text = on_screen(&build(&state, &Form::new()));
    assert!(!has_cjk(&text), "{text}");
    assert!(text.contains("unreachable"), "{text}");
}

/// **M132 的主锚**：缺 client id 时**UI 模型**给出的结论必须是「这一项没配 + 怎么补」。
///
/// 抓得住：这一类失败又退回通用的「登录失败 / 完整原因在启动器日志里」（原 bug），
///         或者文案只说「缺 id」却不告诉用户去设哪个变量。
/// 抓不住：排版与配色（那靠截图）；也不证明视图真的照模型画（见 `tests_text` 的边界说明）。
#[test]
fn a_missing_client_id_tells_the_user_what_to_configure() {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::DeviceLoginChanged(
        // 后端原文照旧进这里（只该进日志）：界面上一句都不许出现。
        DeviceLoginView::failed_as(
            LoginFailure::MsaNotConfigured,
            "微软登录未配置：缺少 Azure 公共客户端 id",
        ),
    )));
    let text = on_screen(&build(&state, &Form::new()));
    assert!(
        text.contains("not configured"),
        "标题要说清「未配置」：{text}"
    );
    assert!(
        text.contains("ULL_MSA_CLIENT_ID"),
        "要给出具体要设的变量名：{text}"
    );
    assert!(text.contains("<id>"), "示例只许用占位，不许写值：{text}");
    assert!(text.contains("msa_not_configured"), "{text}");
    assert!(!has_cjk(&text), "英文界面里出现了中文：{text}");
    // 不许退回「去翻日志」那句：那正是用户报的那个 bug。
    assert!(
        !text.contains("full reason is in the launcher log"),
        "{text}"
    );
}

/// **反向锚（M132）**：普通网络失败不许也变成「未配置」——否则提示变噪音，用户会学会无视它。
#[test]
fn a_network_failure_does_not_look_like_a_missing_config() {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::DeviceLoginChanged(
        DeviceLoginView::failed_as(LoginFailure::Unreachable, "连不上服务器：dns error"),
    )));
    let text = on_screen(&build(&state, &Form::new()));
    assert!(text.contains("unreachable"), "{text}");
    assert!(text.to_lowercase().contains("could not reach"), "{text}");
    assert!(!text.to_lowercase().contains("not configured"), "{text}");
    assert!(!text.contains("ULL_MSA_CLIENT_ID"), "{text}");
    assert!(text.contains("Sign-in failed"), "标题仍是通用失败：{text}");
}

/// 空态：**三条轨道都有入口**，不是一张白页；且三张卡的标题各不相同。
#[test]
fn an_empty_account_list_offers_all_three_tracks() {
    let state = AppState::new();
    let page = build(&state, &Form::new());
    assert!(page.is_empty());
    assert_eq!(page.panel, Panel::None);
    assert!(page.active_row().is_none());
    let mut keys: Vec<&str> = Track::ALL
        .iter()
        .map(|track| track.title_key().name())
        .collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 3);
}

/// 设备码：**倒计时是人话格式（mm:ss）**，过期后是「已过期」而不是继续转圈。
#[test]
fn the_device_code_panel_shows_a_countdown_and_expires() {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::DeviceLoginChanged(
        DeviceLoginView::waiting("ABCD-EFGH", "https://www.microsoft.com/link", 900, ""),
    )));
    let page = build(&state, &Form::new());
    match &page.device {
        DevicePanel::Code {
            code,
            remaining,
            expired,
            ..
        } => {
            assert_eq!(code, "ABCD-EFGH");
            assert_eq!(*remaining, Some(900));
            assert!(!expired);
        }
        other => panic!("等待态应该是设备码面板，实际 {other:?}"),
    }
    let text = on_screen(&page);
    assert!(text.contains("15:00"), "倒计时要画成 mm:ss：{text}");
    assert!(!text.contains("900s"), "不再是 `900s left`：{text}");

    // 起点往前挪 20 分钟 ⇒ 过期
    state.device_login_started =
        Some(std::time::Instant::now() - std::time::Duration::from_secs(1_200));
    let page = build(&state, &Form::new());
    match &page.device {
        DevicePanel::Code {
            remaining, expired, ..
        } => {
            assert_eq!(*remaining, Some(0));
            assert!(*expired, "过期必须被标出来（面板要画「已过期，重试」）");
        }
        other => panic!("过期态仍是设备码面板，实际 {other:?}"),
    }

    // 授权码流也走同一条倒计时（没有用户码时是 Browser 面板）
    state.apply(AppEvent::Core(CoreEvent::DeviceLoginChanged(
        DeviceLoginView::browser_waiting("https://login.microsoftonline.com/x", 300, None),
    )));
    let page = build(&state, &Form::new());
    assert!(matches!(page.device, DevicePanel::Browser { .. }));
}

/// 没有皮肤 = 首字母块（不是白块、不是假皮肤）；坏图也必须回落；无皮肤时**明说**。
#[test]
fn a_third_party_account_falls_back_to_the_monogram() {
    let mut state = AppState::new();
    state.accounts = vec![
        account("Alex", AccountKind::Authlib, true),
        account("Steve", AccountKind::Microsoft, false),
    ];
    let page = build(&state, &Form::new());
    assert!(
        page.rows.iter().all(|row| row.head.is_none()),
        "没有皮肤就不许有图"
    );
    assert!(page.rows.iter().all(|row| !row.monogram.is_empty()));
    assert!(page.active_skin().is_none(), "皮肤预览区没有图就不画图");
    let text = on_screen(&page);
    assert!(!has_cjk(&text));

    // 塞一张坏图：解码失败 → 仍然回落到首字母块（不 panic、不白块）
    state.accounts[0].skin = Some(SkinImage {
        png: b"not a png".to_vec(),
        source: "test".to_string(),
    });
    let page = build(&state, &Form::new());
    assert!(page.rows[0].head.is_none());
}

/// 离线档案的说明**必须**带「无法连接启用正版验证的服务器」这条限定（中英都要）。
#[test]
fn the_offline_track_keeps_the_agreed_wording() {
    for lang in Lang::ALL {
        let catalog = lang.catalog();
        for key in [Key::AccountTrackOfflineBody, Key::AccountOfflineWarning] {
            let text = catalog.text(key);
            assert!(
                text.contains("cannot join servers that require a genuine Minecraft account")
                    || text.contains("无法连接启用正版验证的服务器"),
                "{} 的 {key:?} 丢了「仅本地/隔离网络测试用」口径：{text}",
                lang.tag()
            );
            assert!(
                !text.contains("migration") && !text.contains("迁移"),
                "不许改写成「兼容旧启动器迁移」：{text}"
            );
        }
    }
}

/// **离线轨的失败必须可见**（复审 r1 P2-2）：横幅进模型、文案走 i18n、原文不上屏。
#[test]
fn a_failed_offline_add_is_visible() {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::AccountNoticeChanged(Some(
        AccountNotice::new(
            AccountNoticeCode::OfflineNameInvalid,
            "ab",
            "离线名字必须是 3–16 位字母/数字/下划线，收到 \"ab\"",
        ),
    ))));
    let page = build(&state, &Form::new());
    let notice = page.notice.as_ref().expect("失败要有横幅");
    assert_eq!(notice.code, AccountNoticeCode::OfflineNameInvalid);
    assert!(notice.failure, "失败横幅要用 error 配色");
    let text = on_screen(&page);
    assert!(text.contains("3–16"), "要说清规则：{text}");
    assert!(!has_cjk(&text), "英文界面不许出现中文：{text}");
}

/// 表单状态：三条轨道各开各的，密码关面板即清，三卡默认收起。
#[test]
fn the_form_tracks_are_independent() {
    let mut form = Form::new();
    assert!(!form.section_open, "有账号时三卡默认收起（P2-4）");
    form.open(Track::Offline);
    assert_eq!(form.panel, Panel::Offline);
    assert!(!form.offline_ready(), "空名字不能提交");
    form.offline_name = "ab".to_string();
    assert!(
        !form.offline_ready(),
        "太短也不许提交（与 host 同一条规则）"
    );
    assert!(
        !unlucky_presenter::dto::valid_offline_name("ab"),
        "UI 判据 = host 那份规则"
    );
    form.offline_name = "Alex".to_string();
    assert!(form.offline_ready());

    form.open(Track::Authlib);
    assert!(!form.authlib_ready(), "地址/登录名/密码缺一不可");
    form.server = "https://littleskin.cn/api/yggdrasil/".to_string();
    form.login = "alex".to_string();
    form.password = "hunter2".to_string();
    assert!(form.authlib_ready());
    form.clear_secret();
    assert!(form.password.is_empty());
    assert!(!form.authlib_ready());
}

/// 登录阶段的可判据性：Requesting / Waiting / Failed 各自映射到不同面板。
#[test]
fn login_phases_map_to_distinct_panels() {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::DeviceLoginChanged(
        DeviceLoginView {
            phase: DeviceLoginPhase::Requesting,
            message: "正在向微软申请设备码…".to_string(),
            ..DeviceLoginView::new()
        },
    )));
    let page = build(&state, &Form::new());
    assert!(matches!(page.device, DevicePanel::Requesting));
    // 进行中也只有 i18n（不再复读 presenter 的中文状态串）
    assert!(!has_cjk(&on_screen(&page)));

    state.apply(AppEvent::Core(CoreEvent::DeviceLoginChanged(
        DeviceLoginView::failed("HTTP 403"),
    )));
    assert!(matches!(
        build(&state, &Form::new()).device,
        DevicePanel::Failed { .. }
    ));

    state.apply(AppEvent::Core(CoreEvent::DeviceLoginChanged(
        DeviceLoginView::new(),
    )));
    assert!(matches!(
        build(&state, &Form::new()).device,
        DevicePanel::Hidden
    ));
}
