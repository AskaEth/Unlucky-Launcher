//! 「助手」分组状态机的用例（M174）。
//!
//! | 用例 | 守什么 | 变异（改成什么会红） |
//! |---|---|---|
//! | `provider_vocabulary_is_the_schema_vocabulary` | 四个取值 == `core::setting::keys::AI_PROVIDERS` | 词表里写 `open_ai_compatible` |
//! | `secret_debug_is_masked` | 密钥不进 `{:?}` | 把 `Secret` 改成 `String` |
//! | `validate_refuses_bad_endpoint_and_empty_model` | 空/非法值不保存 | 删掉 `starts_with("http")` 检查 |
//! | `missing_names_exactly_one_field` | 「未配置」说清缺哪一项 | 所有缺项都返回 `BaseUrl` |
//! | `can_test_is_false_until_configured` | 未配置时按钮禁用 | `can_test` 恒 `true` |
//! | `dirty_draft_is_not_overwritten` | 外部改动不冲掉用户输入 | `hydrate` 先无条件赋值 |
//! | `saving_a_new_target_drops_the_old_pass` | 端点/模型变了，旧「已通过」作废 | `keep_probe_if_targeted` 删掉 |
//! | `not_configured_outcome_carries_no_target` | 「没发请求」的结论里没有目标/详情 | 把 `ProbeOutcome::NotConfigured` 当失败类 |

use unlucky_core::setting::keys::AI_PROVIDERS;
use unlucky_i18n::{Catalog, Lang};

use super::outcome::{MissingField, ProbeKind, ProbeResult};
use super::state::{Probe, ProviderId, Refusal, Secret, Snapshot, State, Status};

fn configured() -> State {
    State {
        enabled: true,
        provider: ProviderId::OpenAiCompatible,
        base_url: "https://api.deepseek.com/v1".to_string(),
        model: "deepseek-chat".to_string(),
        key: Secret::new("sk-test"),
        key_chars: 0,
        dirty: false,
        storage: true,
        ..State::default()
    }
}

#[test]
fn provider_vocabulary_is_the_schema_vocabulary() {
    let ours: Vec<&str> = ProviderId::ALL.iter().map(|id| id.setting()).collect();
    assert_eq!(
        ours, AI_PROVIDERS,
        "四个取值必须与 schema 的允许值表逐条相等"
    );
    for id in ProviderId::ALL {
        assert_eq!(ProviderId::from_setting(id.setting()), id);
    }
    // 未知值回落 OpenAI 兼容（与 schema 默认值同一档），不是 panic 也不是禁用。
    assert_eq!(
        ProviderId::from_setting("open_ai_compatible"),
        ProviderId::OpenAiCompatible
    );
}

#[test]
fn secret_debug_is_masked() {
    let secret = Secret::new("sk-live-LEAK-MARKER");
    let printed = format!("{secret:?}");
    assert!(!printed.contains("sk-live"), "Debug 漏了密钥：{printed}");
    assert!(printed.contains("***"), "应当显示掩码：{printed}");
    // 空草稿的 Debug 也要能分辨（否则「留空 = 保留旧密钥」这条规则在日志里看不出）。
    assert!(format!("{:?}", Secret::default()).contains("空"));
    assert!(Secret::new("   ").is_empty());
}

/// **P2-2**：`Message::EditKey` 的 `Debug` 不许把密钥打进日志。
///
/// 现场（r1 复审）：载荷是裸 `String`，而 harness / 断言失败输出会把页面消息 `{:?}` 打出来 ⇒
/// 明文 key 进 stderr 与 CI 日志。载荷改成 [`Secret`] 之后由它自己的 `Debug` 兜住。
///
/// 抓得住：谁把 `EditKey(String)` 改回去（`Secret` 的 Debug 掩码见 [`secret_debug_is_masked`]）。
#[test]
fn the_key_message_never_prints_the_key() {
    use crate::pages::settings::ai_about::Message;

    const SECRET: &str = "sk-live-MSG-LEAK-7c2f";
    let message = Message::EditKey(Secret::new(SECRET));
    let printed = format!("{message:?}");
    assert!(
        !printed.contains(SECRET),
        "页面消息的 Debug 泄漏了密钥（它会进日志/CI 输出）：{printed}"
    );
    assert!(printed.contains("***"), "应当看得见掩码标记：{printed}");

    // 整条投递口（`pages::Message`）也一样：它才是真的被一层层 `{:?}` 的那个类型。
    let page =
        crate::pages::Message::Settings(crate::pages::settings::GroupMessage::AiAbout(message));
    let printed = format!("{page:?}");
    assert!(
        !printed.contains(SECRET),
        "页面消息漏斗泄漏了密钥：{printed}"
    );
}

#[test]
fn validate_refuses_bad_endpoint_and_empty_model() {
    let mut state = configured();
    assert_eq!(state.validate(), Ok(()));

    state.base_url = "api.deepseek.com/v1".to_string();
    assert_eq!(
        state.validate(),
        Err(Refusal::BaseUrl),
        "没有协议头不算端点"
    );

    state.base_url = String::new();
    assert_eq!(
        state.validate(),
        Err(Refusal::BaseUrl),
        "OpenAI 兼容形态没有默认端点，空值必须拒绝（不保存）"
    );

    state.base_url = "https://api.deepseek.com/v1".to_string();
    state.model = "  ".to_string();
    assert_eq!(state.validate(), Err(Refusal::Model));

    // Ollama 有默认端点：空 base_url 是合法的（生效值回落默认）。
    state.provider = ProviderId::Ollama;
    state.base_url = String::new();
    state.model = "llama3".to_string();
    assert_eq!(state.validate(), Ok(()));
    assert_eq!(state.resolved_base_url(), "http://127.0.0.1:11434");

    // 禁用档：什么都不填也能保存（那正是「关掉助手」）。
    state.provider = ProviderId::Disabled;
    state.model = String::new();
    assert_eq!(state.validate(), Ok(()));
}

#[test]
fn missing_names_exactly_one_field() {
    let mut state = configured();
    assert_eq!(state.missing(), None);

    state.enabled = false;
    assert_eq!(state.missing(), Some(MissingField::Disabled));

    let mut state = configured();
    state.base_url = String::new();
    assert_eq!(state.missing(), Some(MissingField::BaseUrl));

    let mut state = configured();
    state.model = String::new();
    assert_eq!(state.missing(), Some(MissingField::Model));

    // Anthropic 必须带密钥：草稿空 + 已存 0 字符 = 缺密钥。
    let mut state = configured();
    state.provider = ProviderId::Anthropic;
    state.base_url = String::new();
    state.model = "claude-sonnet-4".to_string();
    state.key = Secret::default();
    state.key_chars = 0;
    assert_eq!(state.missing(), Some(MissingField::ApiKey));
    // 已存过密钥（只有长度，没有值）就算配置齐了。
    state.key_chars = 40;
    assert_eq!(state.missing(), None);
}

#[test]
fn can_test_is_false_until_configured() {
    let mut state = configured();
    assert!(state.can_test());

    state.model = String::new();
    assert!(!state.can_test(), "缺模型名时「测试连接」必须禁用");

    let mut state = configured();
    state.storage = false;
    assert!(!state.can_test(), "没有设置文件时也禁用");

    let mut state = configured();
    state.probe = Probe::Running;
    assert!(!state.can_test(), "正在测的时候不许再点一次");
}

#[test]
fn dirty_draft_is_not_overwritten() {
    let mut state = configured();
    state.base_url = "https://typed.example/v1".to_string();
    state.dirty = true;
    let changed = state.hydrate(Snapshot {
        enabled: true,
        provider: ProviderId::Ollama,
        base_url: "http://127.0.0.1:11434".to_string(),
        model: "llama3".to_string(),
        key_chars: 12,
    });
    assert!(!changed);
    assert_eq!(
        state.base_url, "https://typed.example/v1",
        "脏草稿不许被冲掉"
    );
    assert_eq!(state.provider, ProviderId::OpenAiCompatible);
    // 密钥长度不属于草稿：外部改了密钥，长度照样要更新（界面上的「已存 N 字符」不能骗人）。
    assert_eq!(state.key_chars, 12);

    // 不脏的时候照常灌入。
    let mut state = State::default();
    let changed = state.hydrate(Snapshot {
        enabled: true,
        provider: ProviderId::Ollama,
        base_url: "http://127.0.0.1:11434".to_string(),
        model: "llama3".to_string(),
        key_chars: 0,
    });
    assert!(changed);
    assert_eq!(state.provider, ProviderId::Ollama);
    assert!(state.storage, "灌过真值之后设置文件就是可用的");
}

#[test]
fn saving_a_new_target_drops_the_old_pass() {
    let mut state = configured();
    state.probe = Probe::Done(ProbeResult {
        kind: ProbeKind::Passed,
        missing: None,
        base_url: state.resolved_base_url(),
        model: state.model.clone(),
        elapsed_ms: 120,
        detail: String::new(),
    });
    assert_eq!(state.status(), Status::Verified);

    // 同一个目标重复灌入：结论仍然有效。
    let snapshot = Snapshot {
        enabled: true,
        provider: state.provider,
        base_url: state.base_url.clone(),
        model: state.model.clone(),
        key_chars: 0,
    };
    state.hydrate(snapshot);
    assert_eq!(
        state.status(),
        Status::Verified,
        "目标没变，别无故作废旧结论"
    );

    // 换了端点：旧结论不再关于现在这套配置。
    state.dirty = false;
    state.hydrate(Snapshot {
        enabled: true,
        provider: ProviderId::OpenAiCompatible,
        base_url: "https://other.example/v1".to_string(),
        model: "deepseek-chat".to_string(),
        key_chars: 0,
    });
    assert_eq!(state.probe, Probe::Idle, "目标变了，旧「已通过」必须作废");
    assert_eq!(state.status(), Status::Configured);
}

/// **P2-1**：失败结论必须带着「对哪个端点 / 哪个模型」，只有「没发请求」才允许没有 target。
///
/// 抓得住：`ProbeResult::from_outcome` 又把失败类的 target 丢掉（那正是 r1 复审的缺陷——
/// 界面上显示成「目标：  · 模型 …」，而截图因为走的是手填种子看不见它）。
#[test]
fn failures_carry_the_target_that_was_dialed() {
    use unlucky_cli::commands::agent::probe::{Missing, ProbeOutcome, Target};

    let target = || Target {
        base_url: "https://api.deepseek.com/v1".to_string(),
        model: "deepseek-chat".to_string(),
    };

    let not_configured = ProbeResult::from_outcome(
        ProbeOutcome::NotConfigured {
            missing: Missing::Model,
        },
        0,
    );
    assert_eq!(not_configured.kind, ProbeKind::NotConfigured);
    assert_eq!(not_configured.missing, Some(MissingField::Model));
    assert!(not_configured.base_url.is_empty(), "没发请求就没有目标地址");
    assert!(not_configured.model.is_empty());
    assert!(!not_configured.sent_a_request());

    for (outcome, expected) in [
        (
            ProbeOutcome::Auth {
                target: target(),
                detail: "401".to_string(),
            },
            ProbeKind::Auth,
        ),
        (
            ProbeOutcome::ModelNotFound {
                target: target(),
                detail: "404".to_string(),
            },
            ProbeKind::ModelNotFound,
        ),
        (
            ProbeOutcome::Unreachable {
                target: target(),
                detail: "refused".to_string(),
            },
            ProbeKind::Unreachable,
        ),
        (
            ProbeOutcome::Timeout {
                target: target(),
                detail: "8 s".to_string(),
            },
            ProbeKind::Timeout,
        ),
        (
            ProbeOutcome::Failed {
                target: target(),
                detail: "?".to_string(),
            },
            ProbeKind::Failed,
        ),
        (ProbeOutcome::Passed { target: target() }, ProbeKind::Passed),
    ] {
        let result = ProbeResult::from_outcome(outcome, 5);
        assert_eq!(result.kind, expected);
        assert_eq!(
            result.base_url, "https://api.deepseek.com/v1",
            "{expected:?}"
        );
        assert_eq!(result.model, "deepseek-chat", "{expected:?}");
        if expected != ProbeKind::Passed {
            assert!(result.sent_a_request());
        }
    }
}

#[test]
fn screenshot_seed_covers_the_state_matrix() {
    // 未配置 / 已配置 / 已通过 / 失败四档都要能出图（D30 的矩阵）。
    let unconfigured = State::screenshot_seed("unconfigured").expect("种子");
    assert_eq!(unconfigured.status(), Status::Unconfigured);
    assert!(unconfigured.storage, "截图里也要如实显示「有设置文件」");

    let configured = State::screenshot_seed("configured").expect("种子");
    assert_eq!(configured.status(), Status::Configured);
    assert_eq!(configured.model, "deepseek-chat");

    let verified = State::screenshot_seed("verified").expect("种子");
    assert_eq!(verified.status(), Status::Verified);

    let failed = State::screenshot_seed("failed-auth").expect("种子");
    assert_eq!(failed.status(), Status::Failed);
    // P2-1：种子的**形状**要走产品路径（`ProbeResult::from_outcome`），所以失败态也带 target。
    // 上一版种子手填字段、把 target 填满，于是「产品上失败态目标是空串」这个缺陷截图看不见。
    if let Probe::Done(result) = &failed.probe {
        assert_eq!(result.base_url, "https://api.deepseek.com/v1");
        assert_eq!(result.model, "deepseek-chat");
        assert!(!result.detail.is_empty(), "失败态应当带一句脱敏详情");
    } else {
        panic!("失败种子没带上探测结论");
    }
    assert!(
        State::screenshot_seed("nonsense").is_none(),
        "未知种子不伪造状态"
    );
}

#[test]
fn every_key_used_here_exists_in_the_catalog() {
    // 中英各一遍，逐条取词：目录里没这条 key 就编译不过（`Key` 是枚举），
    // 但**译文为空**这件事只有这里抓得住。
    let mut keys: Vec<unlucky_i18n::Key> = ProviderId::ALL.iter().map(|id| id.label()).collect();
    keys.extend([
        MissingField::Disabled.label(),
        MissingField::Provider.label(),
        MissingField::BaseUrl.label(),
        MissingField::Model.label(),
        MissingField::ApiKey.label(),
        Refusal::BaseUrl.label(),
        Refusal::Model.label(),
        Status::Unconfigured.label(),
        Status::Configured.label(),
        Status::Verified.label(),
        Status::Failed.label(),
        ProbeKind::Passed.label(),
        ProbeKind::Auth.label(),
        ProbeKind::ModelNotFound.label(),
        ProbeKind::Unreachable.label(),
        ProbeKind::Timeout.label(),
        ProbeKind::Failed.label(),
        ProbeKind::NotConfigured.label(),
    ]);
    for lang in [Lang::ZhCn, Lang::En] {
        let catalog = Catalog::of(lang);
        for key in &keys {
            assert!(
                !catalog.text(*key).trim().is_empty(),
                "{key:?} 在 {lang:?} 下没有译文"
            );
        }
    }
}
