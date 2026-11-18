//! M317：CurseForge 接线的**导入链降级**用例（全离线）。
//!
//! | 判据 | 用例 | 改坏会怎样 |
//! |------|------|-----------|
//! | 缺 Key 是**具名状态**、只让那一条记缺 | [`no_key_is_a_named_state_and_the_rest_of_the_pack_still_imports`] | 改回 `Unsupported` ⇒ 文案/`message_key` 断言红；改成致命 ⇒ 状态红 |
//! | 一条补不出来不挡另一条 | [`one_broken_entry_does_not_block_the_other`] | 「一条失败就整包失败」⇒ 状态断言红 |
//! | 401 / 403 / 429 在导入路径上降级 | [`invalid_key_and_rate_limiting_degrade_one_file_at_a_time`] | 让 429 重试 ⇒ 请求计数红；让 401 致命 ⇒ 状态红 |
//! | Key 不进错误串 / 报告 / Debug | [`the_key_never_reaches_an_error_or_the_report`] | 把 Key 拼进错误串 ⇒ 红 |
//!
//! 替身（假传输层 + 两个假 fetcher）在 `import_curseforge_support.rs`；补全正路在
//! `import_curseforge_api.rs`。

use crate::addon::import::keys::{MISSING_FETCH_FAILED, MISSING_NO_SOURCE};
use crate::addon::import::{import_pack, FetchError, FetchErrorKind, ImportRequest, ImportStatus};
use crate::download::blocking::block_on;

use super::import_curseforge_support::{
    file_json, key, CfApiFetcher, NoCurseFetcher, Scripted, CURSE_BYTES, FABRIC_API, FILE_URL,
    MOD_JSON, TEST_KEY,
};
use super::{fixtures, TempRoot};

/// 没有 Key ⇒ 两条 CF 条目各自记「缺文件 + 缺 Key」，**包内自带内容照装**，整包 `Partial`。
///
/// 这一条走的是 `PackFetcher::curse_file` 的**默认实现**（[`NoCurseFetcher`] 没覆盖它）。
///
/// 抓得住：默认实现改回 `Unsupported` ⇒ 文案与判据断言红；让缺 Key 变成致命 ⇒
/// `ImportStatus::Partial` 断言红（会变 `Failed`）；顺手把包内内容也跳过 ⇒ 装好断言红。
#[test]
fn no_key_is_a_named_state_and_the_rest_of_the_pack_still_imports() {
    let fixture = fixtures::curseforge("cf-m317-nokey");
    let root = TempRoot::new("cf-m317-nokey-out");
    let target = root.join("instance");
    let fetcher = NoCurseFetcher::new();
    let request = ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher);
    let report = block_on(import_pack(&request));

    assert_eq!(
        report.status,
        ImportStatus::Partial,
        "缺 Key 不是失败：{:?} / error={:?}",
        report.status,
        report.error
    );
    assert!(report.error.is_none(), "不许有致命错误：{:?}", report.error);
    assert_eq!(report.missing.len(), 2, "{:?}", report.missing);
    for missing in &report.missing {
        assert_eq!(
            missing.message_key, MISSING_NO_SOURCE,
            "缺 Key 走「补不出来」那一档：{}",
            missing.detail
        );
        assert!(
            missing.detail.contains("API Key"),
            "原因必须说清是缺 Key：{}",
            missing.detail
        );
        assert!(
            !missing
                .detail
                .contains(FetchErrorKind::Unsupported.as_str()),
            "不许说成「这条路走不通」：{}",
            missing.detail
        );
    }
    // 包内自带的内容一件都不能少（这就是「其余文件不受影响」）。
    assert!(target.join("mods/example.jar").is_file());
    assert!(target.join("config/known.toml").is_file());
    assert!(target.join("config/broken.toml").is_file());
    // CF 条目一条都没落地。
    assert!(!target.join("mods/jei-1.20.1-forge-15.2.0.27.jar").exists());
    assert!(!target.join("mods/fabric-api.jar").exists());
}

/// 一条补不出来（站点 404）**不挡**另一条：JEI 装好、fabric-api 记缺，整包 `Partial`。
///
/// 抓得住：把 `curse_file` 的可容忍错误改成致命 ⇒ 状态断言红；两条条目的结果混成一条 ⇒
/// 「jei.jar 已装 + fabric-api 缺」两条断言红。
#[test]
fn one_broken_entry_does_not_block_the_other() {
    let transport = Scripted::new()
        .route("/mods/238222/files/5102025", 200, file_json())
        .route("/mods/238222", 200, MOD_JSON);
    let fetcher = CfApiFetcher::new(transport, Some(key())).with_file(FILE_URL, CURSE_BYTES);
    let fixture = fixtures::curseforge("cf-m317-one-missing");
    let root = TempRoot::new("cf-m317-one-missing-out");
    let target = root.join("instance");
    let request = ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher);
    let report = block_on(import_pack(&request));

    assert_eq!(report.status, ImportStatus::Partial, "{:?}", report.error);
    // 落盘名来自站点给的 `fileName`，目录来自项目的 `classId`（`mods/`）。
    assert_eq!(
        std::fs::read(target.join("mods/jei-1.20.1-forge-15.2.0.27.jar")).expect("jei 必须装上"),
        CURSE_BYTES
    );
    assert!(
        !target.join("mods/fabric-api.jar").exists(),
        "站点没有的那条不许凭空出现"
    );
    let missing = report
        .missing
        .iter()
        .find(|entry| entry.project_id == Some(FABRIC_API.0))
        .expect("fabric-api 应在缺失清单里");
    assert_eq!(missing.message_key, MISSING_FETCH_FAILED);
    assert!(
        missing.detail.contains("404"),
        "原因要带状态码：{}",
        missing.detail
    );
    assert_eq!(report.missing.len(), 1, "{:?}", report.missing);
}

/// Key 无效（401/403）与限流（429）都只让**这一条**记缺，整包照装；且不重试
/// （`RetryPolicy::disabled()` ⇒ 每条只发一次请求）。
///
/// 抓得住：401 当成致命 ⇒ 状态红；429 重试 ⇒ 请求计数红；把 429 归到「缺 Key」⇒ 文案断言红。
#[test]
fn invalid_key_and_rate_limiting_degrade_one_file_at_a_time() {
    let cases = [
        (401u16, "拒绝了这把 Key"),
        (403, "拒绝了这把 Key"),
        (429, "限流"),
    ];
    for (status, expected) in cases {
        let transport = Scripted::new().fallback(status);
        let fetcher = CfApiFetcher::new(transport, Some(key()));
        let fixture = fixtures::curseforge(&format!("cf-m317-status-{status}"));
        let root = TempRoot::new(&format!("cf-m317-status-{status}-out"));
        let target = root.join("instance");
        let request = ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher);
        let report = block_on(import_pack(&request));

        assert_eq!(
            report.status,
            ImportStatus::Partial,
            "HTTP {status} 不许让整包失败：{:?}",
            report.error
        );
        assert_eq!(
            report.missing.len(),
            2,
            "HTTP {status}：{:?}",
            report.missing
        );
        for missing in &report.missing {
            assert_eq!(
                missing.message_key, MISSING_FETCH_FAILED,
                "{}",
                missing.detail
            );
            assert!(
                missing.detail.contains(expected),
                "HTTP {status} 的原因该含「{expected}」：{}",
                missing.detail
            );
            assert!(
                !missing.detail.contains("API Key"),
                "Key 无效 / 限流不是「缺 Key」：{}",
                missing.detail
            );
        }
        // 两条条目各一次请求，不许重试风暴。
        assert_eq!(
            fetcher.transport.requests().len(),
            2,
            "HTTP {status}：401/403 不重试是必须，429 在关闭重试时也只发一次"
        );
        // 包内自带内容照装。
        assert!(target.join("mods/example.jar").is_file());
    }
}

/// Key 不进错误串、不进报告、不进 `Debug`。
///
/// 变异（改坏就该红）：在 `fetch/curse.rs` 的错误里拼上 Key（比如
/// `format!("HTTP 401（key={}）", key.expose())`）⇒ 下面每一条 `contains(TEST_KEY)` 断言红。
#[test]
fn the_key_never_reaches_an_error_or_the_report() {
    let transport = Scripted::new().fallback(401);
    let fetcher = CfApiFetcher::new(transport, Some(key()));
    let fixture = fixtures::curseforge("cf-m317-leak");
    let root = TempRoot::new("cf-m317-leak-out");
    let target = root.join("instance");
    let request = ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher);
    let report = block_on(import_pack(&request));

    // 报告整体（`Debug` 与逐条文案）都不含 Key。
    let dumped = format!("{report:?}");
    assert!(!dumped.contains(TEST_KEY), "报告里出现了 Key：{dumped}");
    for missing in &report.missing {
        assert!(!missing.detail.contains(TEST_KEY), "{}", missing.detail);
    }
    // 错误类型自己的 `Debug` 也不含。
    let error = FetchError::new(FetchErrorKind::NotConfigured, "缺 Key");
    assert!(!format!("{error:?}").contains(TEST_KEY));
    // `ApiKey` 的 `Debug` 是脱敏的（类型层约束，不是某处的自觉）。
    assert_eq!(format!("{:?}", key()), "ApiKey(<redacted>)");
    // 但请求确实带了 Key（否则上面几条「不含」是空转）。
    assert!(fetcher
        .transport
        .requests()
        .iter()
        .all(|(_, headers)| headers.iter().any(|(_, value)| value == TEST_KEY)));
}
