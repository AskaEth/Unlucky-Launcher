//! [`super`] 的用例：真读写五键 + 密钥只留长度 + 非法值不落盘 + 外观写回不盖 AI 键。
//!
//! | 用例 | 守什么 | 变异 |
//! |---|---|---|
//! | `five_keys_round_trip` | 四项 + 密钥都真的落进 `launcher.json` | 少写一个键 / 写错键名 |
//! | `snapshot_only_reports_key_length` | 快照里没有密钥值，只有长度 | 把 `api_key` 值放进 `Snapshot` |
//! | `empty_draft_keeps_the_stored_key` | 密钥输入框留空 = 保留旧密钥 | 无条件写空串覆盖 |
//! | `schema_rejects_and_nothing_is_written` | 非法值不保存 | 忽略 `set` 的 `Err` 直接 `save` |
//! | `external_edit_is_read_back` | 现读现写（别人的改动不被旧快照盖掉） | 缓存在内存里不回读 |
//! | `appearance_write_back_keeps_the_ai_keys` | **用户场景**：存完 AI 再改主题，值还在 | 两处任一方不重读 |
//! | `read_only_config_is_reported_as_a_failure` | 只读配置上保存**不许**报成功 | `save()` 恒返回 Ok |

use std::path::PathBuf;

use super::{load, save};
use crate::pages::settings::ai::state::{ProviderId, Secret, State};

fn temp_config(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ull-ai-store-{label}-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(dir.join("config")).expect("临时目录");
    let path = dir.join("config/launcher.json");
    let _ = std::fs::remove_file(&path);
    path
}

fn draft() -> State {
    State {
        enabled: true,
        provider: ProviderId::Anthropic,
        base_url: "https://api.anthropic.com".to_string(),
        model: "claude-sonnet-4".to_string(),
        key: Secret::new("sk-ant-secret"),
        dirty: true,
        storage: true,
        ..State::default()
    }
}

#[test]
fn five_keys_round_trip() {
    let path = temp_config("roundtrip");
    let saved = save(&path, &draft()).expect("应当能保存");
    assert_eq!(saved.provider, ProviderId::Anthropic);
    assert_eq!(saved.base_url, "https://api.anthropic.com");
    assert_eq!(saved.model, "claude-sonnet-4");
    assert_eq!(saved.key_chars, "sk-ant-secret".chars().count());

    // 文件里真的是这五个键（不是我们内存里的对象）。
    let text = std::fs::read_to_string(&path).expect("读文件");
    assert!(
        text.contains("\"base_url\": \"https://api.anthropic.com\""),
        "{text}"
    );
    assert!(text.contains("\"model\": \"claude-sonnet-4\""), "{text}");
    assert!(text.contains("\"provider\": \"anthropic\""), "{text}");
    assert!(text.contains("\"api_key\": \"sk-ant-secret\""), "{text}");

    // 新开一份读回来（= 重启语义）。
    let reloaded = load(&path);
    assert_eq!(reloaded, saved);
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

#[test]
fn snapshot_only_reports_key_length() {
    let path = temp_config("length");
    let saved = save(&path, &draft()).expect("保存");
    // `Snapshot` 里唯一与密钥有关的字段是长度：类型上就没有 `String` 可以装明文。
    let printed = format!("{saved:?}");
    assert!(
        !printed.contains("sk-ant-secret"),
        "快照泄漏密钥：{printed}"
    );
    assert_eq!(saved.key_chars, "sk-ant-secret".len());
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

#[test]
fn empty_draft_keeps_the_stored_key() {
    let path = temp_config("keep-key");
    save(&path, &draft()).expect("先存一把");
    let mut next = draft();
    next.key = Secret::default();
    next.model = "claude-opus-4".to_string();
    let saved = save(&path, &next).expect("再存一次（密钥留空）");
    assert_eq!(saved.model, "claude-opus-4");
    assert_eq!(
        saved.key_chars,
        "sk-ant-secret".chars().count(),
        "密钥输入框留空时必须保留旧密钥"
    );
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

#[test]
fn schema_rejects_and_nothing_is_written() {
    let path = temp_config("reject");
    save(&path, &draft()).expect("先存一份合法的");
    let before = std::fs::read_to_string(&path).expect("读文件");

    // `ai.model` 的上限是 128 字符：越界的值必须被 schema 拒绝，**且文件不动**。
    let mut bad = draft();
    bad.model = "m".repeat(400);
    bad.base_url = "https://example.invalid/v1".to_string();
    let error = save(&path, &bad).expect_err("越界模型名必须被拒");
    assert!(!error.is_empty());
    let after = std::fs::read_to_string(&path).expect("读文件");
    assert_eq!(before, after, "被拒绝的保存不该改动文件：{after}");
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

#[test]
fn external_edit_is_read_back() {
    let path = temp_config("external");
    save(&path, &draft()).expect("先存一份");
    // 外部写者（`ull config set ai.model …`）改一个键：我们不缓存，下一次读就是新值。
    std::fs::write(
        &path,
        r#"{"ai":{"enabled":true,"provider":"ollama","base_url":"http://127.0.0.1:11434","model":"llama3.1"}}"#,
    )
    .expect("写文件");
    let reloaded = load(&path);
    assert_eq!(reloaded.provider, ProviderId::Ollama);
    assert_eq!(reloaded.model, "llama3.1");
    assert_eq!(reloaded.key_chars, 0, "外部把 api_key 删了，长度就该是 0");
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

/// **P2-4 的用户场景原样**（r1 复审要求的用例）：设置页保存 AI 配置 → 同一秒内改主题 → 值还在。
///
/// 这一条与 `presenter/src/settings_bridge.rs` 的 `persist_keeps_keys_another_writer_just_saved`
/// 守的是同一个缺陷，但从**产品的两个写者**出发：`ai::store::save`（现读现写）与
/// `SettingsBridge::persist`（长生命 store + 写前 reload）。任一方少了「写前现读」，这条红。
#[test]
fn appearance_write_back_keeps_the_ai_keys() {
    use unlucky_presenter::prefs::{AppearancePrefs, ThemeMode};
    use unlucky_presenter::settings_bridge::SettingsBridge;

    let path = temp_config("appearance-keeps-ai");
    // 外观桥先加载过一份文件（它的 store 就是从这里开始的旧快照）。
    let mut bridge = SettingsBridge::with_path(&path);
    bridge.persist(&AppearancePrefs::factory(), ThemeMode::Light);

    // 用户在「助手」页保存配置（真落盘、含密钥）。
    let mut draft = draft();
    draft.key = Secret::new("sk-must-survive");
    save(&path, &draft).expect("保存 AI 配置");

    // 紧接着用户去「外观」改主题（外观桥写回它那份整文档）。
    bridge.persist(&AppearancePrefs::factory(), ThemeMode::Dark);

    let text = std::fs::read_to_string(&path).expect("读文件");
    assert!(
        text.contains("sk-must-survive"),
        "密钥被外观写回盖掉了：{text}"
    );
    assert!(text.contains("claude-sonnet-4"), "模型名也被盖掉了：{text}");
    assert_eq!(load(&path).key_chars, "sk-must-survive".chars().count());
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

/// **P2-1（r2）第二条**：只读配置上的保存必须报**失败**，而不是「已保存」。
///
/// 现场：`$schema` 认不出（或文件被更新版本的启动器写过）⇒ `FileAccess::ReadOnly`。
/// `SettingStore::save` 在这种文件上会 `Skipped(ReadOnly)`（**不是** Err），旧代码用 `.is_ok()`
/// 判定，于是「什么都没写进磁盘」在界面上是「已保存」，重启回旧值。
///
/// 抓得住：①把 `save()` 的 `SaveOutcome` 分支去掉（恒返回 Ok）；②把只读档当成功。
/// 抓不住：`Skipped` 分支自身的文案（见 [`super::skip_reason`] 的单测）。
#[test]
fn read_only_config_is_reported_as_a_failure() {
    let path = temp_config("read-only");
    // `Invalid` 兼容档（`$schema` 前缀认不出）⇒ `FileAccess::ReadOnly`：值照读，不许改写。
    std::fs::write(
        &path,
        r#"{"$schema":"https://example.invalid/ull/launcher/9.9.9","ai":{"model":"keep-me"}}"#,
    )
    .expect("写配置");
    let before = std::fs::read_to_string(&path).expect("读配置");

    let error = save(&path, &draft()).expect_err("只读配置上的保存必须报失败");
    assert!(
        error.contains("只读") || error.contains("只读文件") || error.contains("9.9.9"),
        "错误文案要说得清为什么没写：{error}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("读配置"),
        before,
        "被只读闸门拦下的保存不许改动文件"
    );
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

/// `skip_reason` 的三档都有人话（`Skipped` 不许被静默当成成功）。
#[test]
fn skip_reasons_are_named() {
    use super::skip_reason;
    use unlucky_core::setting::store::SkipReason;

    for reason in [
        SkipReason::NotDirty,
        SkipReason::ReadOnly,
        SkipReason::Unreadable,
    ] {
        assert!(!skip_reason(reason).is_empty(), "{reason:?} 没有文案");
    }
}
