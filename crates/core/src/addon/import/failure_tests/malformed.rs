//! M323：**格式/结构坏 ≠ 摘要不符**。
//!
//! | 用例 | 改回/删掉哪一处就红 |
//! |------|----------------------|
//! | [`an_unreadable_response_is_tolerated_and_never_named_integrity`] | `fetch.rs` 的 `From<OnlineError>` 里 `OnlineErrorKind::Malformed` 那一行（改回 `Integrity` 即红） |
//! | [`a_pack_whose_manifest_is_not_json_is_named_pack_metadata`] | `pack` 侧 detect 的「清单在场但读不出来 ⇒ metadata」语义 |
//!
//! # 为什么后一条是"核查"而不是"修复"
//!
//! 复审那条观察（「`Malformed` 被归成 `Integrity`」）说的是「格式坏被贴了摘要标签」。核下来：
//! **本地包**这条路上它不成立——包自己的 JSON 解析不了走的是 `pack_metadata`（「清单损坏」），
//! 从来不是 `import_integrity`。真正贴错的是**远端响应**那一档：`fetch.rs` 的
//! `From<OnlineError>` 曾把 `OnlineErrorKind::Malformed`（JSON 坏 / 不是 UTF-8 / 服务器忽略
//! Range）并进 `Integrity`，于是引擎原话「服务器不支持分段请求」在报告里成了
//! 「摘要与清单声明不符」。两条用例各钉一头，谁把两条路并回去谁红。

use super::super::fetch::{
    FetchError, FetchErrorKind, FetchFuture, FetchRequest, FetchedFile, PackFetcher,
};
use super::super::report::ImportStatus;
use super::super::{import_pack, keys, ImportRequest, MissingReason};
use super::{mrpack, mrpack_with_index, Entry};
use crate::addon::online::OnlineError;
use crate::download::blocking::block_on;
use crate::download::{DownloadError, DownloadErrorKind};

/// 「响应读不懂」那一条的地址。
const UNREADABLE_URL: &str = "stub://mods/unreadable.jar";

/// 照**引擎收尾的真实形状**造一条「响应读不懂」的错误，同样走生产链路
/// （`DownloadError → OnlineError → FetchError`）。
///
/// 为什么用叶子 `RangeUnsupported`：它是生产链路上唯一能把 `OnlineErrorKind::Malformed` 带到
/// 导入层的成因（`online/transport.rs` 的映射；其余 `Malformed` 来自站点 JSON 解析，那条路在
/// `complete.rs` 里被计成「补不出」，从不整包失败）。M323 之前它被并进 `Integrity`，于是这里
/// 的真原因（站点不支持分段请求）在报告里成了「摘要与清单声明不符」。
fn engine_malformed_error(url: &str) -> FetchError {
    let engine = DownloadError::all_sources_failed(
        format!("1 个候选源全部失败：{url}"),
        vec![format!("{url}: 服务器忽略 Range 回了 200（不支持分段）")],
        [DownloadErrorKind::RangeUnsupported],
        None,
    );
    FetchError::from(OnlineError::from(engine))
}

/// 只回「响应读不懂」的 fetcher（不走 `ScriptedFetcher` 那张按 URL 分派的表：本用例只有这一条）。
struct UnreadableFetcher;

impl PackFetcher for UnreadableFetcher {
    fn fetch_file<'a>(
        &'a self,
        request: FetchRequest<'a>,
    ) -> FetchFuture<'a, Result<FetchedFile, FetchError>> {
        Box::pin(async move { Err(engine_malformed_error(request.url)) })
    }
}

/// 读不懂响应 ⇒ **不是** integrity（M323）。
///
/// 判据（复审那条观察的行为级版本）：那一条的 kind 不是 `import_integrity`，原因文案说的是
/// 「这一条拿不到」+ 引擎原话，而不是「摘要与清单声明不符」。
#[test]
fn an_unreadable_response_is_tolerated_and_never_named_integrity() {
    // 映射本身就是判据的第一道：`OnlineErrorKind::Malformed` 不许落进 `Integrity`。
    assert_eq!(
        FetchError::from(OnlineError::malformed("响应不是合法 JSON")).kind(),
        FetchErrorKind::Malformed,
        "站点响应读不懂与「摘要不符」是两件事，不许并档"
    );

    let declared = "0".repeat(40);
    let (root, pack) = mrpack(
        "unreadable-response",
        &[Entry {
            path: "mods/unreadable.jar",
            url: UNREADABLE_URL,
            sha1: &declared,
            size: 42,
        }],
        &[],
    );
    let target = root.join("instance");

    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target).with_fetcher(&UnreadableFetcher),
    ));

    assert!(
        report.error.is_none(),
        "读不懂响应不是「内容与清单不符」，不许整包失败：{:?}",
        report
            .error
            .as_ref()
            .map(|error| (error.code(), error.message().to_string()))
    );
    assert_eq!(
        report.status,
        ImportStatus::Partial,
        "这是「这一条拿不到」，其余照装"
    );
    let missing = report
        .missing
        .first()
        .expect("这一条要逐条记账，不许静默丢");
    assert_eq!(missing.reason, MissingReason::FetchFailed);
    assert_eq!(missing.message_key, keys::MISSING_FETCH_FAILED);
    assert!(
        missing.detail.contains("不支持分段"),
        "要说得出真原因（引擎原话）：{}",
        missing.detail
    );
    assert!(
        !missing.detail.contains("摘要与清单声明不符"),
        "不许把取数/解析问题说成摘要问题：{}",
        missing.detail
    );
}

/// 判据核查（M323）：「包自己的清单解析不了」现在报的是**清单损坏**，不是摘要不符。
///
/// 这条用例把"复审那条观察在本地包这条路上不成立"钉成事实：`pack` 侧把「清单在场但读不出来」
/// 判成 `PackError::metadata`（见 `pack/format.rs::detect` 的错误语义），导入层只透传成
/// `ImportErrorKind::Pack(Metadata)`。谁哪天把解析失败往 `Integrity` 上并，这里就红。
#[test]
fn a_pack_whose_manifest_is_not_json_is_named_pack_metadata() {
    let (root, pack) = mrpack_with_index("broken-manifest", b"{ \"files\": [ oops", &[]);
    let target = root.join("instance");

    let report = block_on(import_pack(&ImportRequest::new(&pack, &target)));

    assert_eq!(report.status, ImportStatus::Failed, "包坏了必须整包失败");
    let error = report.error.as_ref().expect("具名错误");
    assert_eq!(
        error.code(),
        "pack_metadata",
        "清单读不出来是「清单损坏」，不是 integrity：{}",
        error.message()
    );
    assert!(
        error.message().contains("JSON") || error.message().contains("清单"),
        "要说出是清单/解析的问题：{}",
        error.message()
    );
    assert!(!target.exists(), "失败后目标目录必须零残留");
}
