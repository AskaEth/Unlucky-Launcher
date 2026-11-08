//! M161：`update.auto_check` / `update.channel` 的 schema 契约 + 旧 `cache/update/prefs.json` 的迁移。
//!
//! 迁移的四条硬约束在这里逐条钉住（幂等 / 不丢 `false` / 新值胜出 / 旧文件退役），
//! 另外还有一条 schema 层面的：`update.channel` 现在只允许 `stable`——清单里没有的渠道
//! 写进文件等于「设了但没生效」，宁可让 `config set` 当面拒绝。

use serde_json::json;
use unlucky_core::setting::document::{RenderMode, SettingDocument};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::keys::update::UPDATE_CHANNELS;
use unlucky_core::setting::spec::{Sensitivity, ValueKind};
use unlucky_core::setting::store::SettingStore;
use unlucky_core::setting::update_prefs::{
    self, Migration, Targets, DEFAULT_AUTO_CHECK, DEFAULT_CHANNEL, KEY_AUTO_CHECK, KEY_CHANNEL,
    RECEIPT_FILE,
};

use super::common::TempDir;

/// 三个路径（`<root>/config/launcher.json` + `<root>/cache/update/prefs.json` + 收据）。
fn targets(_temp: &TempDir) -> (String, String, String) {
    (
        "config/launcher.json".to_string(),
        "cache/update/prefs.json".to_string(),
        format!("config/{RECEIPT_FILE}"),
    )
}

fn run(temp: &TempDir) -> Migration {
    let (launcher, legacy, receipt) = targets(temp);
    update_prefs::migrate(
        Targets {
            launcher: &temp.child(&launcher),
            legacy: &temp.child(&legacy),
            receipt: &temp.child(&receipt),
        },
        1_700_000_000_000,
    )
}

/// 读主配置里的显式值（`None` = 文件里没这个键）。
fn explicit(temp: &TempDir, key: &str) -> Option<serde_json::Value> {
    let store = SettingStore::load(temp.child("config/launcher.json"), launcher_schema());
    store.doc().get(key).cloned()
}

#[test]
fn the_two_keys_are_registered_with_the_expected_shape() {
    let schema = launcher_schema();
    let auto = schema
        .spec(KEY_AUTO_CHECK)
        .expect("update.auto_check 必须登记");
    assert_eq!(auto.default, json!(DEFAULT_AUTO_CHECK), "出厂是开");
    assert_eq!(auto.kind, ValueKind::Bool);
    assert_eq!(
        auto.ai.sensitivity,
        Sensitivity::Medium,
        "改它会让启动器自己下载并替换 exe，AI 改过要留审计痕迹"
    );

    let channel = schema.spec(KEY_CHANNEL).expect("update.channel 必须登记");
    assert_eq!(channel.default, json!(DEFAULT_CHANNEL));
    assert_eq!(channel.kind, ValueKind::Enum(UPDATE_CHANNELS));
    assert!(schema.validate(KEY_CHANNEL, &json!("stable")).is_ok());
    // 清单里还没有的渠道：**当面拒绝**（写进文件等于「设了但没生效」）。
    assert!(schema.validate(KEY_CHANNEL, &json!("beta")).is_err());
    assert!(schema.validate(KEY_CHANNEL, &json!("nightly")).is_err());
    assert!(schema.validate(KEY_AUTO_CHECK, &json!(1)).is_err());
}

#[test]
fn the_keys_render_in_the_nested_shape() {
    let (mut document, problems) = SettingDocument::load(&launcher_schema(), &json!({}));
    assert!(problems.is_empty(), "{problems:?}");
    // 默认值不落盘：空文档渲染出来不该有 update 段。
    assert!(
        document.render(RenderMode::Full).get("update").is_none(),
        "出厂值不该写进文件"
    );

    document.set(KEY_AUTO_CHECK, json!(false)).expect("写");
    document.set(KEY_CHANNEL, json!("stable")).expect("写");
    let rendered = document.render(RenderMode::Full);
    assert_eq!(
        rendered["update"],
        json!({ "auto_check": false, "channel": "stable" }),
        "点分键在文件里是嵌套对象（与既有键同一形状）"
    );
    // 读回来也要是同一个值（IPC/CLI 都读这一份）。
    let (reloaded, problems) = SettingDocument::load(&launcher_schema(), &rendered);
    assert!(problems.is_empty(), "{problems:?}");
    assert!(
        !update_prefs::auto_check(&reloaded),
        "读回来的生效值必须是文件里的 false"
    );
    assert_eq!(update_prefs::channel(&reloaded), "stable");
}

#[test]
fn a_false_switch_is_migrated_and_the_old_file_is_retired() {
    let temp = TempDir::new("m161-migrate-false");
    temp.write("cache/update/prefs.json", r#"{"auto_check": false}"#);

    let outcome = run(&temp);
    let Migration::Migrated {
        auto_check,
        retired,
        receipt,
    } = outcome.clone()
    else {
        panic!("应当迁入，实际 {outcome:?}");
    };
    assert!(!auto_check, "旧文件里的 false 不许被默认值盖掉");

    // 主配置里是显式的 false（不是「缺键取默认」）。
    assert_eq!(explicit(&temp, KEY_AUTO_CHECK), Some(json!(false)));
    // 旧文件不能再被读到。
    assert!(!temp.exists("cache/update/prefs.json"), "旧文件必须退役");
    assert!(temp.exists("cache/update/prefs.json.migrated"), "留着取证");
    assert_eq!(retired, temp.child("cache/update/prefs.json.migrated"));
    // 收据可查：落盘、类型/指纹/端点齐全。
    assert!(receipt.exists(), "收据要落盘：{}", receipt.display());
    let text = temp.read(&format!("config/{RECEIPT_FILE}"));
    assert!(text.contains("\"legacy_config\""), "{text}");
    assert!(text.contains("prefs.json"), "{text}");
    assert!(text.contains("launcher.json"), "{text}");
    assert!(text.contains("\"applied_at_ms\": 1700000000000"), "{text}");
}

#[test]
fn running_twice_is_idempotent() {
    let temp = TempDir::new("m161-migrate-twice");
    temp.write("cache/update/prefs.json", r#"{"auto_check": false}"#);

    assert!(run(&temp).touched(), "第一遍要真的动手");
    let after_first = temp.read("config/launcher.json");
    let receipt_first = temp.read(&format!("config/{RECEIPT_FILE}"));

    // 第二遍：旧文件已经改名 ⇒ 什么都没得做。
    let second = run(&temp);
    assert_eq!(second, Migration::Nothing, "第二遍必须是空操作");
    assert_eq!(temp.read("config/launcher.json"), after_first, "文件不许变");
    assert_eq!(
        temp.read(&format!("config/{RECEIPT_FILE}")),
        receipt_first,
        "收据也不该被重写（源指纹相同 = 跳过）"
    );
}

#[test]
fn a_missing_legacy_file_is_a_silent_noop() {
    let temp = TempDir::new("m161-migrate-absent");
    assert_eq!(run(&temp), Migration::Nothing);
    // 连主配置都不该建（没有值要写，就不该白建一个文件）。
    assert!(!temp.exists("config/launcher.json"), "不该凭空建配置");
    assert!(
        !temp.exists(&format!("config/{RECEIPT_FILE}")),
        "不该写收据"
    );
}

#[test]
fn an_explicit_new_value_wins_over_the_legacy_file() {
    let temp = TempDir::new("m161-migrate-new-wins");
    temp.write(
        "config/launcher.json",
        &json!({
            "$schema": schema_url(),
            "schemaVersion": 1,
            "update": { "auto_check": true }
        })
        .to_string(),
    );
    temp.write("cache/update/prefs.json", r#"{"auto_check": false}"#);

    let outcome = run(&temp);
    assert_eq!(
        outcome.as_str(),
        "new_value_wins",
        "主配置里有显式值时不许覆盖：{outcome:?}"
    );
    assert_eq!(
        explicit(&temp, KEY_AUTO_CHECK),
        Some(json!(true)),
        "新值胜出"
    );
    // 旧文件照样退役（它已经不是真相源），收据也写了。
    assert!(!temp.exists("cache/update/prefs.json"));
    assert!(temp.exists(&format!("config/{RECEIPT_FILE}")));
}

#[test]
fn the_default_true_is_not_written_but_the_file_still_retires() {
    let temp = TempDir::new("m161-migrate-default");
    temp.write("cache/update/prefs.json", r#"{"auto_check": true}"#);

    let outcome = run(&temp);
    assert_eq!(outcome.as_str(), "migrated");
    // 「默认值不落盘」：写进去只是噪音，生效值本来就是 true。
    assert_eq!(explicit(&temp, KEY_AUTO_CHECK), None, "出厂值不落盘");
    assert!(!temp.exists("cache/update/prefs.json"), "旧文件仍要退役");
    let store = SettingStore::load(temp.child("config/launcher.json"), launcher_schema());
    assert!(update_prefs::auto_check(store.doc()));
}

#[test]
fn a_corrupt_legacy_file_touches_nothing() {
    let temp = TempDir::new("m161-migrate-corrupt");
    temp.write("cache/update/prefs.json", "{ not json");
    let outcome = run(&temp);
    assert_eq!(outcome.as_str(), "unreadable", "{outcome:?}");
    // 坏文件保持原样：下一次还能再试（我们既不猜值也不删证据）。
    assert!(temp.exists("cache/update/prefs.json"));
    assert!(!temp.exists(&format!("config/{RECEIPT_FILE}")));
}

#[test]
fn an_empty_legacy_file_still_means_the_default() {
    // `serde(default)`：老文件里没有 auto_check 字段（或整份是 `{}`）⇒ 按出厂值处理，
    // 而不是把整份文件判成坏文件。
    let temp = TempDir::new("m161-migrate-empty");
    temp.write("cache/update/prefs.json", "{}");
    let outcome = run(&temp);
    assert_eq!(outcome.as_str(), "migrated");
    let store = SettingStore::load(temp.child("config/launcher.json"), launcher_schema());
    assert!(update_prefs::auto_check(store.doc()), "缺字段取出厂值 true");
}

/// **源指纹闸门**：旧文件还在原地（改名失败 / 用户从备份捞回来）但内容没变 ⇒ 一次都不再迁。
///
/// 复审 P2-3：把 `update_prefs.rs` 里那道闸门整段删掉，113 条 setting 用例全绿——因为
/// `running_twice_is_idempotent` 的第二遍之所以空操作，是第一遍已经把文件改名成 `.migrated`，
/// **根本没走到闸门**。这条用例是唯一真正撞它的：先按旧文件字节写好收据，再让 `prefs.json`
/// 原地留着调 `migrate`。
///
/// 抓得住：①闸门被删（会走「新值胜出」那一支并**重写收据**，`applied_at_ms` 变）；②指纹比对
/// 写成「有收据就跳过」而不是「指纹相同才跳过」（下面第二条反证）；③闸门放行时忘了退役旧文件。
#[test]
fn the_receipt_gate_skips_a_replayed_source_byte_for_byte() {
    let temp = TempDir::new("m161-gate-replay");
    let legacy = temp.write("cache/update/prefs.json", r#"{"auto_check": false}"#);
    let receipt_path = temp.child(&format!("config/{RECEIPT_FILE}"));

    // 先按真实字节写一张收据（模拟「上一轮迁过、但旧文件又回到了原地」）。
    let bytes = std::fs::read(&legacy).expect("读旧文件");
    let receipt = unlucky_core::platform::receipt::MigrationReceipt::legacy_config(
        legacy.clone(),
        temp.child("config/launcher.json"),
        unlucky_core::platform::receipt::fingerprint_bytes(&bytes),
        1_600_000_000_000,
    );
    unlucky_core::platform::receipt::write(&receipt_path, &receipt).expect("写收据");
    let receipt_before = temp.read(&format!("config/{RECEIPT_FILE}"));

    let outcome = run(&temp);
    assert_eq!(
        outcome.as_str(),
        "nothing",
        "源指纹相同 ⇒ 必须走闸门（Nothing），实际 {outcome:?}"
    );
    // 关键断言：**收据没被重写**（重写 = 走了迁移分支，闸门失效）。
    assert_eq!(
        temp.read(&format!("config/{RECEIPT_FILE}")),
        receipt_before,
        "收据被重写了 ⇒ 闸门没拦住重放"
    );
    // 主配置一个字都没写（闸门那一支只退役旧文件）。
    assert!(!temp.exists("config/launcher.json"), "闸门放行时不该建配置");
    // 旧文件仍然退役（它是真相源之外的东西，留着只会下次再撞一次）。
    assert!(!temp.exists("cache/update/prefs.json"));

    // 反证：源文件**变了**（false → true）⇒ 指纹不同 ⇒ 允许重放，收据被更新。
    temp.write("cache/update/prefs.json", r#"{"auto_check": true}"#);
    let outcome = run(&temp);
    assert_eq!(
        outcome.as_str(),
        "migrated",
        "源内容变了就该重放，实际 {outcome:?}"
    );
    assert_ne!(
        temp.read(&format!("config/{RECEIPT_FILE}")),
        receipt_before,
        "重放要写新收据（指纹变了）"
    );
}

/// **`Refused`：主配置不可写时不动旧文件**（唯一有用户数据风险的分支）。
///
/// 造法：主配置的 `$schema` 写成**更高的 minor**（1.7.0 > 1.6.0）⇒ `SchemaCompat` 判
/// `ReadOnlyPreserveSchema` ⇒ `FileAccess::ReadOnly` ⇒ `can_save()` false。
/// 抓得住：把「写不进去」当成「迁完了」（旧文件被改名，用户的值两边都没有了）。
#[test]
fn a_read_only_launcher_config_never_retires_the_legacy_file() {
    let temp = TempDir::new("m161-refused");
    temp.write(
        "config/launcher.json",
        &json!({
            "$schema": "https://schemas.unlucky-launcher.dev/ull/launcher/1.7.0",
            "schemaVersion": 1
        })
        .to_string(),
    );
    temp.write("cache/update/prefs.json", r#"{"auto_check": false}"#);

    let outcome = run(&temp);
    let Migration::Refused { reason } = &outcome else {
        panic!("只读主配置必须走 Refused，实际 {outcome:?}");
    };
    // 拒绝的理由必须是「先看能不能写」这一条（而不是写到一半才发现 `set` 被拒）：
    // 抓得住把 `can_save()` 守卫删掉这种变异——那条路径的 reason 是「写 update.auto_check 被拒」。
    assert!(
        reason.contains("不可写"),
        "应当在动手前就判定主配置不可写，实际理由：{reason}"
    );
    // 旧文件原样留着（下一次还能再试）。
    assert!(
        temp.exists("cache/update/prefs.json"),
        "写不进去时**不许**退役旧文件——用户的值不能两边都没有"
    );
    assert!(
        !temp.exists("cache/update/prefs.json.migrated"),
        "不许留下「已迁移」的痕迹"
    );
    // 不写收据（没迁成就不该有收据）。
    assert!(!temp.exists(&format!("config/{RECEIPT_FILE}")));
    // 主配置一个字都没改（还是那份 1.7.0）。
    let text = temp.read("config/launcher.json");
    assert!(text.contains("1.7.0"), "{text}");
    assert!(!text.contains("auto_check"), "只读文件不许被写：{text}");
}

/// 真 `$schema`（加载不降级才看得到显式值）。
fn schema_url() -> String {
    use unlucky_core::setting::keys::launcher_version;
    use unlucky_core::setting::schema::{SchemaId, SchemaRef};
    SchemaRef::new(SchemaId::LAUNCHER, launcher_version()).url()
}
