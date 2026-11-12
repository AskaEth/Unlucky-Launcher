//! 快照仓库：不可变读、单写者草稿、发布即指针交换。

use std::sync::Arc;

use unlucky_core::game::error::GameError;
use unlucky_core::game::snapshot::{Snapshot, Store};

#[test]
fn snapshot_survives_draft_publish() {
    let store = Store::new(vec!["1.20.1".to_owned()]);
    let before = store.snapshot();

    let mut draft = store.open_draft().expect("首个草稿应可打开");
    draft.edit().push("1.20.2".to_owned());
    let published = draft.commit();

    // 旧快照不变（读端遍历时不会看到半新半旧的状态）。
    assert_eq!(before.get().len(), 1);
    assert_eq!(published.get().len(), 2);
    assert_eq!(store.snapshot().get().len(), 2);
}

#[test]
fn only_one_draft_at_a_time() {
    let store = Store::new(0u32);
    let draft = store.open_draft().expect("首个草稿应可打开");

    match store.open_draft() {
        Err(GameError::DraftBusy) => {}
        other => panic!("第二个草稿必须被拒绝，得到 {other:?}"),
    }

    drop(draft);
    let reopened = store.open_draft();
    assert!(reopened.is_ok(), "放弃草稿后写权必须释放");
}

#[test]
fn dropping_draft_discards_changes() {
    let store = Store::new(vec![1u32]);
    {
        let mut draft = store.open_draft().expect("草稿应可打开");
        draft.edit().push(2);
    }
    assert_eq!(store.snapshot().get(), &vec![1u32]);
}

#[test]
fn replace_publishes_whole_table() {
    let store = Store::new(vec!["old".to_owned()]);
    let snapshot = store.replace(vec!["new".to_owned()]).expect("替换应成功");
    assert_eq!(snapshot.get(), &vec!["new".to_owned()]);
    assert_eq!(store.snapshot().get(), &vec!["new".to_owned()]);
}

#[test]
fn clones_share_one_allocation() {
    let store = Store::new(vec![1u32]);
    let first = store.snapshot();
    let second = first.clone();

    assert!(Arc::ptr_eq(&first.shared(), &second.shared()));
    assert_eq!(first, second);

    let published = store.replace(vec![2u32]).expect("替换应成功");
    assert!(!Arc::ptr_eq(&first.shared(), &published.shared()));
    assert_eq!(first.get(), &vec![1u32], "旧快照保持旧内容");
}

#[test]
fn snapshot_new_wraps_value() {
    let snapshot = Snapshot::new("payload".to_owned());
    assert_eq!(snapshot.get(), "payload");
    assert_eq!(snapshot.clone().get(), "payload");
}
