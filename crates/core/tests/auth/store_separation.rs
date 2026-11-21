//! 存储契约：**分文件 / 原子写 / 写序 / 容错 / 孤儿清理**。
//!
//! 这些是「凭据红线」的落盘侧证据：元数据文件里永远搜不到 token（可安全备份/分享），
//! 凭据文件只在需要发请求时才读，且写坏了也不会让账户列表整体打不开。

use std::path::Path;

use unlucky_core::auth::account::{AccountKind, AccountMeta, Credentials};
use unlucky_core::auth::microsoft::MicrosoftSession;
use unlucky_core::auth::offline::offline_meta;
use unlucky_core::auth::{AccountStore, Secret};

use crate::support::temp_dir;

const MC_TOKEN: &str = "mc-access-token-value";
const LIVE_REFRESH: &str = "live-refresh-token-value";

fn microsoft_entry(created_at_ms: u64) -> (AccountMeta, Credentials) {
    let meta = AccountMeta::new(
        AccountKind::Microsoft,
        "0f8fad5bd9cb469fa16570867728950e",
        "Steve",
        created_at_ms,
    );
    let session = MicrosoftSession::new(
        "0f8fad5bd9cb469fa16570867728950e",
        "Steve",
        Secret::new(MC_TOKEN),
        Secret::new(LIVE_REFRESH),
        86_400,
        1_000,
        None,
    );
    (meta, Credentials::Microsoft(session))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("读文件")
}

fn tmp_leftovers(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .expect("列目录")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".tmp"))
        .collect()
}

#[test]
fn metadata_and_credentials_live_in_separate_files() {
    let dir = temp_dir("sep");
    let mut store = AccountStore::open(&dir);
    let (meta, credentials) = microsoft_entry(1);
    store.insert(meta, credentials).expect("插入");
    store.save().expect("落盘");

    let accounts_text = read(&store.paths().accounts_file());
    let credentials_text = read(&store.paths().credentials_file());

    // 元数据：有身份、有轨道，但**没有任何 token**。
    assert!(accounts_text.contains("0f8fad5bd9cb469fa16570867728950e"));
    assert!(accounts_text.contains("\"kind\": \"microsoft\""));
    assert!(!accounts_text.contains(MC_TOKEN), "{accounts_text}");
    assert!(!accounts_text.contains(LIVE_REFRESH), "{accounts_text}");
    // 凭据：有 token、有保护级别标记，但**没有**可公开的展示字段。
    assert!(credentials_text.contains(MC_TOKEN));
    assert!(credentials_text.contains(LIVE_REFRESH));
    assert!(credentials_text.contains("\"protection\": \"plain-v1\""));
    // 凭据文件里没有元数据那份「账户列表」结构（两个文件的 schema 各管各的）。
    assert!(
        !credentials_text.contains("\"accounts\""),
        "{credentials_text}"
    );

    // 原子写不留 tmp 垃圾。
    assert!(tmp_leftovers(&dir).is_empty(), "残留临时文件");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn roundtrip_keeps_credentials_intact() {
    let dir = temp_dir("roundtrip");
    let mut store = AccountStore::open(&dir);
    let (meta, credentials) = microsoft_entry(1);
    let id = meta.id.clone();
    store.insert(meta, credentials.clone()).expect("插入");
    store.save().expect("落盘");

    let reopened = AccountStore::open(&dir);
    assert_eq!(reopened.accounts().len(), 1);
    assert!(reopened.warnings().is_empty(), "{:?}", reopened.warnings());
    assert_eq!(reopened.credentials(&id), Some(&credentials));
    let placeholders = match reopened.credentials(&id).expect("凭据") {
        Credentials::Microsoft(session) => session.to_placeholders().expect("占位符"),
        other => panic!("应是微软凭据，实际 {other:?}"),
    };
    assert_eq!(placeholders.player_name, "Steve");
    assert_eq!(placeholders.access_token, MC_TOKEN);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn credentials_are_written_before_metadata() {
    let dir = temp_dir("order");
    let mut store = AccountStore::open(&dir);
    store
        .insert(offline_meta("First", 1).expect("甲"), Credentials::Offline)
        .expect("插入甲");
    store.save().expect("首次落盘");

    // 把凭据文件的位置换成目录 → 凭据写必定失败。
    let credentials_path = store.paths().credentials_file();
    std::fs::remove_file(&credentials_path).expect("删凭据文件");
    std::fs::create_dir_all(&credentials_path).expect("建同名目录");

    store
        .insert(offline_meta("Second", 2).expect("乙"), Credentials::Offline)
        .expect("插入乙");
    let error = store.save().expect_err("凭据写不进去时 save 必须失败");
    assert_eq!(error.code(), "store");

    // 关键断言：元数据没有被先写（写序正确才有的结果）。
    let accounts_text = read(&store.paths().accounts_file());
    assert!(!accounts_text.contains("Second"), "{accounts_text}");
    assert!(tmp_leftovers(&dir).is_empty(), "残留临时文件");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn corrupt_record_is_skipped_not_fatal() {
    let dir = temp_dir("corrupt");
    let store = AccountStore::open(&dir);
    let good = offline_meta("Good", 1).expect("合法元数据");
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "accounts": [
            { "id": good.id.as_str(), "kind": "offline",
              "profileId": good.profile_id, "profileName": "Good", "createdAtMs": 1 },
            { "id": "account:zzzz", "kind": "offline" },
            { "id": "account:0123456789abcdef0123456789abcdef", "kind": "unknown-track" }
        ]
    });
    std::fs::write(
        store.paths().accounts_file(),
        serde_json::to_string_pretty(&payload).expect("序列化"),
    )
    .expect("写元数据");

    let reopened = AccountStore::open(&dir);
    assert_eq!(reopened.accounts().len(), 1, "坏记录必须被跳过");
    assert_eq!(reopened.accounts()[0].display_name(), "Good");
    assert_eq!(reopened.warnings().len(), 2, "{:?}", reopened.warnings());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn broken_metadata_file_still_allows_empty_store() {
    let dir = temp_dir("broken");
    let store = AccountStore::open(&dir);
    std::fs::write(store.paths().accounts_file(), "{ this is not json").expect("写坏文件");
    let reopened = AccountStore::open(&dir);
    assert!(reopened.is_empty());
    assert_eq!(reopened.warnings().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn orphan_credentials_are_ignored_and_cleaned_on_save() {
    let dir = temp_dir("orphan");
    let store = AccountStore::open(&dir);
    let orphan_id = "account:00000000000000000000000000000000";
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "protection": "plain-v1",
        "entries": [{ "accountId": orphan_id, "data": { "kind": "offline" } }]
    });
    std::fs::write(
        store.paths().credentials_file(),
        serde_json::to_string_pretty(&payload).expect("序列化"),
    )
    .expect("写凭据");

    let reopened = AccountStore::open(&dir);
    assert!(reopened
        .warnings()
        .iter()
        .any(|line| line.contains("没有对应账户")));
    let unknown = unlucky_core::auth::AccountId::parse(orphan_id).expect("id");
    assert!(reopened.credentials(&unknown).is_none());
    reopened.save().expect("落盘");
    let text = read(&reopened.paths().credentials_file());
    assert!(!text.contains(orphan_id), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unknown_protection_level_is_reported() {
    let dir = temp_dir("protection");
    let store = AccountStore::open(&dir);
    std::fs::write(
        store.paths().credentials_file(),
        serde_json::to_string_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "protection": "hmcl-obfuscated-v1",
            "entries": []
        }))
        .expect("序列化"),
    )
    .expect("写凭据");
    let reopened = AccountStore::open(&dir);
    assert!(reopened
        .warnings()
        .iter()
        .any(|line| line.contains("保护级别")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn remove_drops_credential_entry_on_disk() {
    let dir = temp_dir("remove");
    let mut store = AccountStore::open(&dir);
    store
        .insert(offline_meta("Keep", 1).expect("甲"), Credentials::Offline)
        .expect("甲");
    let (meta, credentials) = microsoft_entry(2);
    let id = meta.id.clone();
    store.insert(meta, credentials).expect("乙");
    store.save().expect("落盘");

    store.remove("Steve").expect("删乙");
    store.save().expect("再落盘");
    let text = read(&store.paths().credentials_file());
    assert!(!text.contains(id.as_str()), "{text}");

    let reopened = AccountStore::open(&dir);
    assert_eq!(reopened.accounts().len(), 1);
    assert_eq!(reopened.accounts()[0].display_name(), "Keep");
    let _ = std::fs::remove_dir_all(&dir);
}
