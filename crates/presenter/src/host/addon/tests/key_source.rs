//! 「有没有 Key」在宿主侧只有**一处**判据（M318）：`sources::api_key` → core 的 `resolve_key`。
//!
//! 本卷钉的是「三处说法不会分叉」：检索路径那个 `bool`（[`cf_available`]，也就是
//! `ContentResults::curseforge`）、页面那条来源能不能点（[`curseforge_ready`]）、启动日志那一行
//! （[`curseforge_state_line`]）。三者都必须与 `api_key` 同一个结论。
//!
//! **三档都跑**：只有设置 / 只有环境变量 / 都没有（含「只在设置里留了空白」）。
//! 环境变量是**进程级**的，所以每一段都拿串行锁 + `KeyEnv`（`Drop` 恢复原值，panic 展开也不污染
//! 别的用例）；设置侧走真实的 `SettingStore::set/save`，不手搓 JSON。
//!
//! 抓得住：
//! - [`curseforge_ready`] 或 [`curseforge_state_line`] 改回「只看环境变量」⇒ 第 1 段红（设置里有、
//!   环境变量被清空）；
//! - `api_key` 自己再写一套优先级/滤空（绕过 `resolve_key`）⇒ 第 3 段红（空白值变成 `Some`）。

use super::super::sources::{api_key, cf_available, curseforge_page};
use super::super::{curseforge_ready, curseforge_state_line, KEY_SET, KEY_UNSET};
use super::retired_curseforge::{
    query, write_setting, KeyEnv, Recorded, Root, KEY_ENV_LOCK, SEARCH_FIXTURE,
};

/// 请求头里那一把 Key（大小写不敏感：core 拼的是 `x-api-key`）。
fn sent_key(seen: &[(String, Vec<(String, String)>)]) -> Option<String> {
    seen.first()
        .and_then(|(_, headers)| {
            headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("x-api-key"))
        })
        .map(|(_, value)| value.clone())
}

/// 只在**设置**里填了 Key（环境变量清空）：三处说法都必须说「可用」。
#[test]
fn a_settings_only_key_is_available_to_all_three() {
    let _lock = KEY_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _env = KeyEnv::set(None);
    let root = Root::new("all-settings");
    // 首尾空白也照样认：`trim` 那一档在 `ApiKey::new`（判据的唯一去处），宿主不另写。
    write_setting(&root, "  settings-only-key  ");
    let env = root.env();

    assert!(api_key(&env).is_some(), "设置里有 Key ⇒ 检索路径要拿得到");
    assert!(cf_available(&env), "检索结果里那个 bool 也要说可用");
    assert!(curseforge_ready(&env), "页面那条来源也要说可用");
    assert_eq!(
        curseforge_state_line(&env),
        KEY_SET,
        "启动日志那一行不许还说「没配」"
    );
    for line in [KEY_SET, KEY_UNSET] {
        assert!(
            !line.contains("settings-only-key"),
            "日志措辞不许带明文：{line}"
        );
    }
}

/// 只在**环境变量**里有 Key（设置没写）：同样三处说法一致。
#[test]
fn an_env_only_key_is_available_to_all_three() {
    let _lock = KEY_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _env = KeyEnv::set(Some("env-only-key"));
    let root = Root::new("all-env");
    let env = root.env();

    assert!(api_key(&env).is_some(), "环境变量是兜底那一档，必须认");
    assert!(cf_available(&env));
    assert!(curseforge_ready(&env));
    assert_eq!(curseforge_state_line(&env), KEY_SET);
}

/// 两档都没有 / 只有空白：三处说法都必须是「没配」（空串不是 Key，M316 栽过这条）。
#[test]
fn a_blank_or_absent_key_is_missing_everywhere() {
    let _lock = KEY_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _env = KeyEnv::set(None);
    let root = Root::new("all-none");
    let env = root.env();

    assert!(api_key(&env).is_none(), "出厂没有 Key");
    assert!(!cf_available(&env));
    assert!(!curseforge_ready(&env));
    assert_eq!(curseforge_state_line(&env), KEY_UNSET);

    write_setting(&root, "   ");
    let env = root.env();
    assert!(api_key(&env).is_none(), "设置里只有空白 ⇒ 仍是没有");
    assert!(!cf_available(&env));
    assert!(!curseforge_ready(&env));
    assert_eq!(curseforge_state_line(&env), KEY_UNSET);

    {
        // 环境变量那一档的空白同理（`ULL_CF_API_KEY=""` 与「没设」是同一件事）。
        let _blank = KeyEnv::set(Some("   "));
        assert!(api_key(&env).is_none());
        assert!(!cf_available(&env));
        assert!(!curseforge_ready(&env));
        assert_eq!(curseforge_state_line(&env), KEY_UNSET);
    }
}

/// 两档都有值时**设置赢**（优先级只在 core 的 `resolve_key` 里表达，宿主不许另判一次）。
///
/// 抓得住：宿主侧自己 `or_else` 反了 / core 的优先级被改反 ⇒ 红（core 里那条同名用例两侧同源）。
#[test]
fn the_setting_beats_the_env_var_on_this_side_too() {
    let _lock = KEY_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _env = KeyEnv::set(Some("env-key"));
    let root = Root::new("both");
    write_setting(&root, "settings-key");
    let env = root.env();

    // 值不可读（`ApiKey` 没有公开取值路），能观测的是**可用性**：两档都有值 ⇒ 可用。
    // 「谁赢」在 core 的 `resolve_key` 用例里读回了明文，这里只钉宿主不把兜底档当成唯一来源。
    assert!(api_key(&env).is_some());
    assert!(cf_available(&env));
    assert!(curseforge_ready(&env));
    assert_eq!(curseforge_state_line(&env), KEY_SET);

    // 设置里置空 ⇒ 退回兜底档（而不是「设置出现过这个键就永远可用」）。
    write_setting(&root, "");
    let env = root.env();
    assert!(api_key(&env).is_some(), "设置置空后兜底档要接上");
    assert!(curseforge_ready(&env));
}

/// 两档都有值时，**设置那一把真的进了请求头**（离线假传输层，一个 socket 都不开）。
///
/// 这条是「优先级改反」在**宿主侧**的判据：`ApiKey` 没有公开取值路，可用性也看不出谁赢，
/// 所以让 `api_key` 的结果走一次 `curseforge_page` 的请求头——值只能在那里被观测。
/// 把 `resolve_key` 的两支对调（环境变量优先）⇒ 本用例红（CLI 侧那条同名用例同时红）。
#[test]
fn the_setting_key_is_the_one_that_reaches_the_wire() {
    let _lock = KEY_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _env = KeyEnv::set(Some("env-wire-key"));
    let root = Root::new("wire");
    write_setting(&root, "settings-wire-key");
    let env = root.env();

    let transport = Recorded::with_body(SEARCH_FIXTURE);
    let key = api_key(&env);
    let _ = curseforge_page(&transport, key.as_ref(), &query());

    assert_eq!(
        sent_key(&transport.requests()).as_deref(),
        Some("settings-wire-key"),
        "两档都有值时请求头里必须是设置那一把"
    );
}
