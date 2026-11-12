//! 断点续传保守判定的单测：矩阵覆盖 + 416 处理。

use unlucky_core::download::resume::{
    decide, on_range_unsatisfiable, RestartReason, ResumeDecision, ResumeMeta, Validator,
};
use unlucky_core::download::types::{HashAlgo, ResumePolicy};

const URL: &str = "https://bmclapi2.bangbang93.com/versions/1.20.1/client.jar";
const HASH: &str = "0123456789abcdef0123456789abcdef01234567";

fn meta(url: &str) -> ResumeMeta {
    ResumeMeta {
        url: url.to_string(),
        etag: Some("\"abc\"".to_string()),
        last_modified: None,
        total: Some(1_000),
        algo: HashAlgo::Sha1,
        hash: HASH.to_string(),
        saved_at_ms: 1,
    }
}

fn resume_ok(part_len: u64, meta: &ResumeMeta) -> ResumeDecision {
    decide(
        ResumePolicy::Conservative,
        part_len,
        Some(meta),
        URL,
        Some((HashAlgo::Sha1, HASH)),
        Some(1_000),
    )
}

#[test]
fn resumes_when_everything_matches() {
    let decision = resume_ok(400, &meta(URL));
    assert_eq!(
        decision,
        ResumeDecision::Resume {
            offset: 400,
            validator: Validator {
                etag: Some("\"abc\"".to_string()),
                last_modified: None,
            },
        }
    );
}

#[test]
fn never_policy_and_missing_files_restart() {
    let meta = meta(URL);
    assert_eq!(
        decide(
            ResumePolicy::Never,
            400,
            Some(&meta),
            URL,
            Some((HashAlgo::Sha1, HASH)),
            Some(1_000)
        ),
        ResumeDecision::Restart {
            reason: RestartReason::PolicyNever
        }
    );
    assert_eq!(
        resume_ok(0, &meta),
        ResumeDecision::Restart {
            reason: RestartReason::EmptyPart
        }
    );
    // 没有元数据：无法证明中间文件是同一次下载的产物。
    assert_eq!(
        decide(
            ResumePolicy::Conservative,
            400,
            None,
            URL,
            Some((HashAlgo::Sha1, HASH)),
            Some(1_000)
        ),
        ResumeDecision::Restart {
            reason: RestartReason::NoMeta
        }
    );
}

#[test]
fn changed_url_or_hash_restarts() {
    assert_eq!(
        decide(
            ResumePolicy::Conservative,
            400,
            Some(&meta("https://other.example/client.jar")),
            URL,
            Some((HashAlgo::Sha1, HASH)),
            Some(1_000)
        ),
        ResumeDecision::Restart {
            reason: RestartReason::UrlChanged
        }
    );
    assert_eq!(
        decide(
            ResumePolicy::Conservative,
            400,
            Some(&meta(URL)),
            URL,
            Some((HashAlgo::Sha1, "ffffffffffffffffffffffffffffffffffffffff")),
            Some(1_000)
        ),
        ResumeDecision::Restart {
            reason: RestartReason::HashChanged
        }
    );
}

#[test]
fn unknown_total_or_oversize_restarts() {
    let mut unknown = meta(URL);
    unknown.total = None;
    assert_eq!(
        decide(
            ResumePolicy::Conservative,
            400,
            Some(&unknown),
            URL,
            None,
            None
        ),
        ResumeDecision::Restart {
            reason: RestartReason::TotalUnknown
        }
    );

    assert_eq!(
        resume_ok(2_000, &meta(URL)),
        ResumeDecision::Restart {
            reason: RestartReason::Oversize
        }
    );
}

#[test]
fn complete_part_skips_transfer() {
    assert_eq!(
        resume_ok(1_000, &meta(URL)),
        ResumeDecision::Complete { size: 1_000 }
    );
}

#[test]
fn missing_validator_restarts() {
    let mut weak = meta(URL);
    weak.etag = None;
    weak.last_modified = None;
    assert_eq!(
        resume_ok(400, &weak),
        ResumeDecision::Restart {
            reason: RestartReason::NoValidator
        }
    );
    // Last-Modified 单独也能当校验器。
    weak.last_modified = Some("Wed, 21 Oct 2015 07:28:00 GMT".to_string());
    assert!(matches!(
        resume_ok(400, &weak),
        ResumeDecision::Resume { offset: 400, .. }
    ));
}

#[test]
fn expected_size_can_stand_in_for_missing_total() {
    let mut unknown = meta(URL);
    unknown.total = None;
    let decision = decide(
        ResumePolicy::Conservative,
        400,
        Some(&unknown),
        URL,
        Some((HashAlgo::Sha1, HASH)),
        Some(1_000),
    );
    assert!(matches!(
        decision,
        ResumeDecision::Resume { offset: 400, .. }
    ));
}

#[test]
fn range_not_satisfiable_handling() {
    // 已下满：直接进入校验，不要白删一个好文件。
    assert_eq!(
        on_range_unsatisfiable(1_000, Some(1_000)),
        ResumeDecision::Complete { size: 1_000 }
    );
    // 没下满却 416：中间文件不可信，丢掉重来。
    let decision = on_range_unsatisfiable(400, Some(1_000));
    assert_eq!(
        decision,
        ResumeDecision::Restart {
            reason: RestartReason::RangeUnsatisfiable
        }
    );
    assert!(decision.should_drop_part());
    // 不允许丢掉「已完成」的中间文件。
    assert!(!ResumeDecision::Complete { size: 1 }.should_drop_part());
}

#[test]
fn resume_meta_roundtrip_and_removal() {
    let dir = std::env::temp_dir().join(format!("ull-resume-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("client.jar.ullpart.ullmeta.json");

    let original = meta(URL);
    original.save(&path).expect("写元数据失败");
    assert_eq!(ResumeMeta::load(&path), Some(original));

    ResumeMeta::remove(&path);
    assert!(ResumeMeta::load(&path).is_none());

    let _ = std::fs::remove_dir_all(&dir);
}
