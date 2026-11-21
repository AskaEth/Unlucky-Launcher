//! 分块路径的续传判定（`chunk_resume`，M293）。
//!
//! 分块路径不能复用单连接那套「长度 = 已下」的判定：它的中间文件被 `set_len(total)` 预分配过，
//! 长度恒等于总长（`resume::decide` 会判成 `Complete`，然后整文件校验把一份没下完的文件判死）。
//! 这里钉住的是**另一套条件**：URL / 总长 / 摘要 / 校验器全部对上，而且每条记录都要精确命中
//! 本次分块规划里的一段；过了判定还要能按记录重算出同样的摘要（`verify_records`），
//! 否则一律从头再来。

use unlucky_core::download::chunk_resume::{
    decide as chunk_decide, remaining, verify_records, ChunkDecision,
};
use unlucky_core::download::chunked::{plan, ByteRange};
use unlucky_core::download::resume::{ChunkRecord, RestartReason, ResumeMeta};
use unlucky_core::download::types::{HashAlgo, ResumePolicy};
use unlucky_core::download::verify::digest_bytes;

use super::{meta, HASH, URL};

fn chunk_payload(len: usize) -> Vec<u8> {
    (0..len).map(|index| (index % 251) as u8).collect()
}

fn record(payload: &[u8], range: ByteRange) -> ChunkRecord {
    let bytes = &payload[range.start as usize..=range.end as usize];
    ChunkRecord {
        start: range.start,
        end: range.end,
        sha1: digest_bytes(HashAlgo::Sha1, bytes),
    }
}

fn with_chunks(base: ResumeMeta, chunks: Vec<ChunkRecord>) -> ResumeMeta {
    ResumeMeta {
        chunks: Some(chunks),
        ..base
    }
}

/// 判定「应当从头再来」，返回原因。
fn chunk_restart(
    part_len: u64,
    meta: Option<&ResumeMeta>,
    url: &str,
    total: u64,
    planned: &[ByteRange],
) -> RestartReason {
    match chunk_decide(
        ResumePolicy::Conservative,
        part_len,
        meta,
        url,
        Some((HashAlgo::Sha1, HASH)),
        total,
        planned,
    ) {
        ChunkDecision::Restart { reason } => reason,
        other => panic!("应当判从头再来，实得 {other:?}"),
    }
}

#[test]
fn chunk_resume_reuses_only_segments_that_match_the_plan() {
    let payload = chunk_payload(1_000);
    let planned = plan(1_000, 4);
    let done = vec![record(&payload, planned[0]), record(&payload, planned[2])];
    let meta = with_chunks(meta(URL), done.clone());

    let decision = chunk_decide(
        ResumePolicy::Conservative,
        1_000,
        Some(&meta),
        URL,
        Some((HashAlgo::Sha1, HASH)),
        1_000,
        &planned,
    );
    assert_eq!(
        decision,
        ChunkDecision::Reuse { done: done.clone() },
        "身份全部对上时应复用，且只复用记录过的段"
    );
    assert_eq!(
        remaining(&planned, &done),
        vec![planned[1], planned[3]],
        "本次要抓的应当是规划里没落盘的那几段"
    );

    // 全部落完 ⇒ 没有要抓的段（下一次尝试只做整文件校验）。
    let all: Vec<ChunkRecord> = planned
        .iter()
        .map(|range| record(&payload, *range))
        .collect();
    assert!(remaining(&planned, &all).is_empty());
}

#[test]
fn chunk_resume_restart_matrix() {
    let planned = plan(1_000, 4);
    let good = with_chunks(
        meta(URL),
        vec![ChunkRecord {
            start: planned[0].start,
            end: planned[0].end,
            sha1: HASH.to_string(),
        }],
    );

    // 策略禁止续传。
    assert_eq!(
        match chunk_decide(
            ResumePolicy::Never,
            1_000,
            Some(&good),
            URL,
            Some((HashAlgo::Sha1, HASH)),
            1_000,
            &planned
        ) {
            ChunkDecision::Restart { reason } => reason,
            other => panic!("实得 {other:?}"),
        },
        RestartReason::PolicyNever
    );
    // 没有中间文件 / 没有元数据 / 元数据里没有分块清单（单连接写的那种）。
    assert_eq!(
        chunk_restart(0, Some(&good), URL, 1_000, &planned),
        RestartReason::NoPart
    );
    assert_eq!(
        chunk_restart(1_000, None, URL, 1_000, &planned),
        RestartReason::NoMeta
    );
    assert_eq!(
        chunk_restart(1_000, Some(&meta(URL)), URL, 1_000, &planned),
        RestartReason::NoMeta,
        "没有分块清单的元数据证明不了乱序段各自到哪"
    );
    // 换了源 / 换了目标文件（摘要变）/ 换了大小。
    assert_eq!(
        chunk_restart(
            1_000,
            Some(&good),
            "https://other.example/x.jar",
            1_000,
            &planned
        ),
        RestartReason::UrlChanged
    );
    let changed_hash = ResumeMeta {
        hash: "f".repeat(40),
        ..good.clone()
    };
    assert_eq!(
        chunk_restart(1_000, Some(&changed_hash), URL, 1_000, &planned),
        RestartReason::HashChanged
    );
    assert_eq!(
        chunk_restart(1_000, Some(&good), URL, 2_000, &plan(2_000, 4)),
        RestartReason::SizeChanged,
        "总长变了就是另一个文件，旧段的偏移全部作废"
    );
    // 没有校验器：服务器无法回答「内容变了没有」。
    let weak = ResumeMeta {
        etag: None,
        last_modified: None,
        ..good.clone()
    };
    assert_eq!(
        chunk_restart(1_000, Some(&weak), URL, 1_000, &planned),
        RestartReason::NoValidator
    );
    // 段边界漂移：半段、重复、越界都不行。
    let half = with_chunks(
        meta(URL),
        vec![ChunkRecord {
            start: 0,
            end: 100,
            sha1: HASH.to_string(),
        }],
    );
    assert_eq!(
        chunk_restart(1_000, Some(&half), URL, 1_000, &planned),
        RestartReason::ChunkLayout,
        "记录必须是规划里原样的一段"
    );
    let first = ChunkRecord {
        start: planned[0].start,
        end: planned[0].end,
        sha1: HASH.to_string(),
    };
    let duplicated = with_chunks(meta(URL), vec![first.clone(), first]);
    assert_eq!(
        chunk_restart(1_000, Some(&duplicated), URL, 1_000, &planned),
        RestartReason::ChunkLayout
    );
    // 换了连接数 ⇒ 规划变了 ⇒ 旧段对不上号（从头再来，而不是硬拼）。
    let reconfigured = with_chunks(
        meta(URL),
        vec![ChunkRecord {
            start: planned[0].start,
            end: planned[0].end,
            sha1: HASH.to_string(),
        }],
    );
    assert_eq!(
        chunk_restart(1_000, Some(&reconfigured), URL, 1_000, &plan(1_000, 8)),
        RestartReason::ChunkLayout
    );
}

#[test]
fn chunk_verify_rejects_bytes_that_do_not_match_the_record() {
    let dir = std::env::temp_dir().join(format!("ull-chunk-verify-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("jdk.zip.ullpart");

    let payload = chunk_payload(1_000);
    let planned = plan(1_000, 4);
    let records = vec![record(&payload, planned[0]), record(&payload, planned[1])];
    std::fs::write(&path, &payload).expect("写中间文件失败");
    assert_eq!(
        verify_records(&path, &records).expect("字节与记录一致时必须通过"),
        planned[0].len() + planned[1].len(),
        "返回的是可复用的字节数"
    );

    // 盘上那一段被改过（断电半截写 / 坏盘 / 别的源写进来的字节）。
    let mut broken = payload.clone();
    broken[planned[1].start as usize + 3] ^= 0xff;
    std::fs::write(&path, &broken).expect("写中间文件失败");
    assert!(
        verify_records(&path, &records).is_err(),
        "摘要对不上必须判「证明不了」，不能拿旧字节当好的"
    );

    // 盘上根本不够长（记录说写完了，其实只有开头几个字节）。
    std::fs::write(&path, &payload[..10]).expect("写中间文件失败");
    assert!(verify_records(&path, &records).is_err());

    let _ = std::fs::remove_dir_all(&dir);
}
