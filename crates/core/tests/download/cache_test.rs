//! 内容寻址缓存的单测：两级分片、记录信任（命中不重算）、损坏检测、索引持久化。

mod support;

use std::time::Duration;

use unlucky_core::download::cache::ContentCache;
use unlucky_core::download::types::{HashAlgo, Integrity};

const NOW: u64 = 5_000_000;

fn cache_dir(tag: &str) -> std::path::PathBuf {
    support::temp_dir(tag)
}

#[test]
fn sharded_path_follows_layout() {
    let dir = cache_dir("cache-path");
    let cache = ContentCache::open(&dir);
    let integrity = Integrity::sha1("abcdef0123456789abcdef0123456789abcdef01");
    let path = cache.path_for(integrity.algo, &integrity.hex);
    assert!(path.ends_with("sha1/ab/abcdef0123456789abcdef0123456789abcdef01"));
    assert!(path.starts_with(cache.root()));
    // SHA-256 单独一棵子树。
    let sha256_path = cache.path_for(HashAlgo::Sha256, "00112233445566778899aabbccddeeff");
    assert!(sha256_path.ends_with("sha256/00/00112233445566778899aabbccddeeff"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn insert_then_lookup_hits_without_rehashing() {
    let dir = cache_dir("cache-hit");
    let cache = ContentCache::open(&dir);

    let payload = b"hello unlucky launcher".to_vec();
    let integrity = Integrity::sha1(support::sha1_hex(&payload));
    let source = dir.join("download.bin");
    std::fs::write(&source, &payload).unwrap();

    let cached = cache
        .insert(&source, &integrity, NOW)
        .expect("写入缓存失败");
    assert!(cached.is_file());
    assert_eq!(cache.len(), 1);

    // 第二次命中：size/mtime 未变 → 记录信任，直接返回（不重算摘要）。
    let hit = cache.lookup(&integrity, NOW + 10).expect("应命中缓存");
    assert_eq!(hit, cached);

    // 文件被删 → 记录作废。
    std::fs::remove_file(&cached).unwrap();
    assert!(cache.lookup(&integrity, NOW + 20).is_none());
    assert_eq!(cache.len(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn truncated_cache_file_is_a_miss() {
    let dir = cache_dir("cache-truncate");
    let cache = ContentCache::open(&dir);

    let payload = vec![7_u8; 256];
    let integrity = Integrity::sha1(support::sha1_hex(&payload));
    let source = dir.join("payload.bin");
    std::fs::write(&source, &payload).unwrap();
    let cached = cache.insert(&source, &integrity, NOW).unwrap();

    // 半损坏（长度变了）必须立刻视为未命中。
    std::fs::write(&cached, &payload[..128]).unwrap();
    assert!(cache.lookup(&integrity, NOW + 1).is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn metadata_change_forces_a_rehash() {
    let dir = cache_dir("cache-rehash");
    let cache = ContentCache::open(&dir);

    let payload = vec![3_u8; 128];
    let integrity = Integrity::sha1(support::sha1_hex(&payload));
    let source = dir.join("data.bin");
    std::fs::write(&source, &payload).unwrap();
    let cached = cache.insert(&source, &integrity, NOW).unwrap();
    // 硬链接/复制都可能让源文件与缓存同 inode，先断开再改缓存内容。
    let _ = std::fs::remove_file(&source);

    // 等 mtime 变化（部分文件系统时间戳精度 10ms）。
    std::thread::sleep(Duration::from_millis(30));
    let mut tampered = payload.clone();
    tampered[0] = 9;
    std::fs::write(&cached, &tampered).unwrap();

    // 内容变了 → 摘要不符 → 记录作废。
    assert!(cache.lookup(&integrity, NOW + 100).is_none());
    assert_eq!(cache.len(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lookup_verified_rejects_wrong_content_with_same_size() {
    let dir = cache_dir("cache-verified");
    let cache = ContentCache::open(&dir);

    let payload = vec![1_u8; 64];
    let integrity = Integrity::sha1(support::sha1_hex(&payload));
    // 直接放到缓存位但内容是错的（模拟外部篡改 / 镜像写坏）。
    let path = cache.path_for(integrity.algo, &integrity.hex);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, vec![2_u8; 64]).unwrap();
    cache.register(&path, &integrity, NOW).unwrap();

    assert!(
        cache.lookup_verified(&integrity, NOW + 1).is_none(),
        "强制校验必须发现内容不符"
    );

    // 内容对了就能通过。
    std::fs::write(&path, &payload).unwrap();
    cache.register(&path, &integrity, NOW).unwrap();
    assert!(cache.lookup_verified(&integrity, NOW + 2).is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn index_persists_across_reopen() {
    let dir = cache_dir("cache-persist");
    let payload = vec![5_u8; 32];
    let integrity = Integrity::sha1(support::sha1_hex(&payload));

    {
        let cache = ContentCache::open(&dir);
        let source = dir.join("src.bin");
        std::fs::write(&source, &payload).unwrap();
        cache.insert(&source, &integrity, NOW).unwrap();
        cache.persist().expect("持久化索引失败");
        assert_eq!(cache.stats().entries, 1);
        assert_eq!(cache.stats().total_bytes, 32);
    }

    let reopened = ContentCache::open(&dir);
    assert_eq!(reopened.len(), 1);
    assert!(reopened.lookup(&integrity, NOW + 1).is_some());

    let _ = std::fs::remove_dir_all(&dir);
}
