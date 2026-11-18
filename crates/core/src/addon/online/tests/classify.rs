//! 引擎错误 → 在线内容源错误分类（r1 P1-1 的守卫）。
//!
//! 这一族的价值全在「经过真的 `From<DownloadError>`」：直接给 `OnlineError` 造一个
//! `NotFound` 什么也证明不了——本轮修的是**引擎把叶子成因折叠掉**之后上层的映射，
//! 所以输入必须是引擎真会产出的那种错误（`AllSourcesFailed` + `leaf`）。

use crate::addon::import::FetchErrorKind;
use crate::addon::online::{OnlineError, OnlineErrorKind};
use crate::download::{DownloadError, DownloadErrorKind};

#[test]
fn an_all_sources_failed_with_a_404_leaf_is_not_found() {
    // 引擎在「所有候选源都回 404」时的真实形状（`download::bytes` 的收尾）。
    let error = DownloadError::new(
        DownloadErrorKind::AllSourcesFailed,
        "所有候选源都取不到文本资源：https://api.modrinth.com/v2/version_file/...",
    )
    .with_suppressed(vec!["official: HTTP 404".to_string()])
    .with_leaf(DownloadErrorKind::NotFound);
    let online: OnlineError = error.into();
    assert_eq!(
        online.kind(),
        OnlineErrorKind::NotFound,
        "项目不存在 / id 拼错必须报 NotFound，不能画成「未联网」"
    );
    assert!(!online.is_offline());
}

#[test]
fn an_all_sources_failed_without_a_leaf_stays_conservatively_offline() {
    let online: OnlineError =
        DownloadError::new(DownloadErrorKind::AllSourcesFailed, "全部候选源都失败").into();
    assert_eq!(online.kind(), OnlineErrorKind::NotConnected);
    assert!(online.is_offline());
}

#[test]
fn leaf_kinds_map_one_to_one() {
    let cases = [
        (DownloadErrorKind::NotFound, OnlineErrorKind::NotFound),
        (DownloadErrorKind::ServerError, OnlineErrorKind::Server),
        (DownloadErrorKind::Timeout, OnlineErrorKind::NotConnected),
        (DownloadErrorKind::Network, OnlineErrorKind::NotConnected),
        (DownloadErrorKind::Integrity, OnlineErrorKind::Integrity),
        (DownloadErrorKind::Io, OnlineErrorKind::Io),
        (
            DownloadErrorKind::InvalidRequest,
            OnlineErrorKind::Unsupported,
        ),
        (
            DownloadErrorKind::RangeUnsupported,
            OnlineErrorKind::Malformed,
        ),
    ];
    for (leaf, expected) in cases {
        let error =
            DownloadError::new(DownloadErrorKind::AllSourcesFailed, "全部失败").with_leaf(leaf);
        let online: OnlineError = error.into();
        assert_eq!(online.kind(), expected, "叶子 {} 映射错了", leaf.as_str());
    }
}

#[test]
fn a_single_source_failure_uses_its_own_kind() {
    // 单源直失败（没有 AllSourcesFailed 这一层）：不该看 leaf，本来就没有。
    let online: OnlineError = DownloadError::new(DownloadErrorKind::NotFound, "HTTP 404").into();
    assert_eq!(online.kind(), OnlineErrorKind::NotFound);
}

#[test]
fn a_404_leaf_is_not_treated_as_offline_by_the_importer_too() {
    // 导入侧（补全）用的是另一套分类：这里只钉「404 与没联网不同类」。
    //
    // 走**生产链路**（`DownloadError → OnlineError → FetchError`）：导入链的错误一律经
    // `online::OnlineError` 中转（`DownloadEngineFetcher` → `download_verified` → 这里）。
    // 曾经 `import::fetch` 另有一份 `From<DownloadError>`，那条路生产代码零调用（M319 复审
    // P2-2：修在了没人跑的那份上）；现在只有一条路，本用例钉的就是它。
    let not_found: crate::addon::import::FetchError = OnlineError::from(
        DownloadError::new(DownloadErrorKind::AllSourcesFailed, "全部失败")
            .with_leaf(DownloadErrorKind::NotFound),
    )
    .into();
    assert_eq!(not_found.kind(), FetchErrorKind::NotFound);
    assert!(not_found.kind().is_tolerable());

    let offline: crate::addon::import::FetchError = OnlineError::from(
        DownloadError::new(DownloadErrorKind::AllSourcesFailed, "全部失败")
            .with_leaf(DownloadErrorKind::Network),
    )
    .into();
    assert_eq!(offline.kind(), FetchErrorKind::Offline);
    assert!(
        offline.kind().is_tolerable(),
        "没联网仍然是「这条文件搞不到」，实例照装"
    );
}

/// 逐源摘要要活着到**导入侧**（M319 复审 P2-2 的判据）。
///
/// 用户看到的 `detail` 是 `FetchError::message()`，而它在生产链路上经过两次搬运
/// （`OnlineError::from(DownloadError)` → `From<OnlineError> for FetchError`），两次都只搬
/// `message`。所以 `suppressed` 必须在**第一站**就接进 message；那段「校验不符：期望 … 实得 …」
/// 一旦在这里丢掉，报告就只剩「全部失败 + 一个地址」，用户看不到「为什么」。
#[test]
fn the_per_source_digest_detail_survives_into_the_importer() {
    let engine = DownloadError::all_sources_failed(
        "1 个候选源全部失败：https://cdn.example/mods/a.jar",
        vec!["cdn.example: 校验不符：期望 0000…0 实得 ffff…f".to_string()],
        [DownloadErrorKind::Integrity],
        None,
    );
    let fetch: crate::addon::import::FetchError = OnlineError::from(engine).into();

    assert_eq!(
        fetch.kind(),
        FetchErrorKind::Integrity,
        "分类也要经得起这一跳"
    );
    assert!(
        fetch
            .message()
            .contains("校验不符：期望 0000…0 实得 ffff…f"),
        "逐源摘要在 OnlineError 那一站被吞了：{}",
        fetch.message()
    );
    assert!(
        fetch.message().contains("各源：cdn.example"),
        "至少要看得出这条摘要是哪个源说的：{}",
        fetch.message()
    );
}
