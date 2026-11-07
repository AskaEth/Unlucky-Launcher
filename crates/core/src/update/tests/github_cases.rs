//! GitHub Releases 源：固定清单地址、产物 URL 解析、渠道只有 stable、与 `DirSource` 对拍。
//!
//! 全程离线：假 HTTP 服务起在 `127.0.0.1` 上，`base` 指向它——没有一个用例访问 github.com
//! 或 gh-proxy（那部分只能真联网验收，见 mission 报告的「没验的部分」）。

use std::fs;
use std::sync::Arc;

use crate::download::mirrors::SOURCE_GH_PROXY;
use crate::download::{DownloadEngine, RegionProfile, SOURCE_OFFICIAL};
use crate::update::github::{DEFAULT_BASE, DEFAULT_OWNER, DEFAULT_REPO, MANIFEST_ASSET};
use crate::update::{
    Channel, DirSource, GitHubReleaseSource, ReleaseManifest, Target, UpdateChecker, UpdateError,
    UpdateErrorKind, UpdateSource, Version,
};

use super::support::{artifact, engine_for, manifest, release, FakeHttp, FakeResponse, TempDir};

const PAYLOAD: &[u8] = b"github release payload";

/// 假服务上清单所在路径（与本源拼出来的地址逐字符对齐）。
fn manifest_path() -> String {
    format!("/{DEFAULT_OWNER}/{DEFAULT_REPO}/releases/latest/download/{MANIFEST_ASSET}")
}

/// 按 D49 命名规则的产物文件名。
fn asset_name() -> &'static str {
    "unlucky-launcher_0.6.0_windows_x64.exe"
}

/// 假服务上产物所在路径。
fn asset_path() -> String {
    format!(
        "/{DEFAULT_OWNER}/{DEFAULT_REPO}/releases/latest/download/{}",
        asset_name()
    )
}

/// 一份可用的 stable 清单（产物 url 就是 D49 文件名）。
fn stable_manifest_json() -> String {
    manifest(
        "stable",
        &[release(
            "0.6.0",
            r#","notes":"修掉启动崩溃""#,
            &[artifact("win-x64", "portable-exe", asset_name(), PAYLOAD)],
        )],
    )
}

/// 解析上面那份清单（多数字段级断言都用它）。
fn parsed_stable() -> ReleaseManifest {
    ReleaseManifest::from_json(&stable_manifest_json()).expect("清单应可解析")
}

fn target() -> Target {
    Target::parse("win-x64").expect("目标平台写法合法")
}

fn source_with(engine: Arc<DownloadEngine>, base: &str) -> GitHubReleaseSource {
    GitHubReleaseSource::new(engine).with_base(base)
}

/// 本地假服务 + 指向它的源（直连模式：不套 gh-proxy，请求落在 127.0.0.1 上）。
fn local_source(tmp: &TempDir, server: &FakeHttp) -> GitHubReleaseSource {
    source_with(engine_for(tmp, RegionProfile::direct(0)), &server.base())
}

#[test]
fn manifest_url_is_the_fixed_latest_download_path() {
    let tmp = TempDir::new("gh-url");
    let source = GitHubReleaseSource::new(engine_for(&tmp, RegionProfile::direct(0)));
    assert_eq!(source.owner(), DEFAULT_OWNER);
    assert_eq!(source.repo(), DEFAULT_REPO);
    assert_eq!(source.base(), DEFAULT_BASE);
    let expected = format!(
        "{DEFAULT_BASE}/{DEFAULT_OWNER}/{DEFAULT_REPO}/releases/latest/download/{MANIFEST_ASSET}"
    );
    assert_eq!(
        source.manifest_url(Channel::Stable).as_deref(),
        Some(expected.as_str()),
        "D50 定死的固定地址，一个字符都不许飘"
    );
    assert_eq!(
        source.source_id(),
        format!("github:{DEFAULT_OWNER}/{DEFAULT_REPO}")
    );

    // fork / 自建镜像仓：owner/repo 可换，路径形状不变。
    let fork = GitHubReleaseSource::with_repo(
        engine_for(&tmp, RegionProfile::direct(0)),
        "someone",
        "ull-fork",
    )
    .with_base("http://127.0.0.1:1/");
    assert_eq!(
        fork.manifest_url(Channel::Stable).as_deref(),
        Some("http://127.0.0.1:1/someone/ull-fork/releases/latest/download/manifest.json"),
        "base 末尾斜杠要去掉，别拼出双斜杠"
    );
    assert_eq!(fork.source_id(), "github:someone/ull-fork");
}

#[test]
fn channels_other_than_stable_publish_nothing() {
    let tmp = TempDir::new("gh-channels");
    let server = FakeHttp::start(vec![(
        manifest_path().as_str(),
        FakeResponse::json(&stable_manifest_json()),
    )]);
    let source = local_source(&tmp, &server);

    for channel in [Channel::Beta, Channel::Alpha, Channel::Nightly] {
        assert!(
            source.manifest_url(channel).is_none(),
            "{channel} 不该有清单地址"
        );
        let manifest = source.fetch_manifest(channel).expect("没开通道不是错误");
        assert!(
            manifest.is_none(),
            "{channel} 应返回 Ok(None)（不发布即无更新）"
        );
    }
    assert!(
        server.hits().is_empty(),
        "非 stable 渠道一次请求都不该发（实际：{:?}）",
        server.hits()
    );
}

#[test]
fn artifact_url_appends_the_release_download_path() {
    let tmp = TempDir::new("gh-artifact-url");
    let source = GitHubReleaseSource::new(engine_for(&tmp, RegionProfile::direct(0)));
    let artifact = &parsed_stable().releases[0].artifacts[0];

    let url = source.artifact_url(artifact).expect("应能拼出地址");
    assert_eq!(
        url,
        format!(
            "{DEFAULT_BASE}/{DEFAULT_OWNER}/{DEFAULT_REPO}/releases/latest/download/{}",
            asset_name()
        )
    );

    // `./name` / `/name` 与裸文件名等价（清单是人手写的）。
    for written in [format!("./{}", asset_name()), format!("/{}", asset_name())] {
        let mut tweaked = artifact.clone();
        tweaked.url = written;
        assert_eq!(source.artifact_url(&tweaked).expect("应可解析"), url);
    }

    // 已经写了完整地址 → 原样使用（清单可以临时指向别处）。
    let mut absolute = artifact.clone();
    absolute.url = "https://example.invalid/dist/ull.exe".to_string();
    assert_eq!(
        source.artifact_url(&absolute).expect("绝对地址原样"),
        "https://example.invalid/dist/ull.exe"
    );

    // 其它协议不认（那是别的源的活）。
    let mut unsupported = artifact.clone();
    unsupported.url = "ftp://example.invalid/ull.exe".to_string();
    assert_eq!(
        source
            .artifact_url(&unsupported)
            .expect_err("ftp 不是本源能取的")
            .kind,
        UpdateErrorKind::UnsupportedSource
    );
}

#[test]
fn fetches_and_parses_the_manifest_from_the_base() {
    let tmp = TempDir::new("gh-manifest-ok");
    let server = FakeHttp::start(vec![(
        manifest_path().as_str(),
        FakeResponse::json(&stable_manifest_json()),
    )]);
    let source = local_source(&tmp, &server);

    let manifest = source
        .fetch_manifest(Channel::Stable)
        .expect("应取到清单")
        .expect("清单存在");
    assert_eq!(manifest.channel, Channel::Stable);
    assert_eq!(manifest.releases.len(), 1);
    assert_eq!(manifest.releases[0].version.to_string(), "0.6.0");
    assert_eq!(server.hits(), vec![manifest_path()]);
}

#[test]
fn missing_manifest_in_the_release_is_not_an_error() {
    // 最新 release 没挂 manifest.json → 404 → 当成「还没发布」，不是红字。
    let tmp = TempDir::new("gh-manifest-404");
    let server = FakeHttp::start(vec![]);
    let source = local_source(&tmp, &server);

    let outcome = source
        .fetch_manifest(Channel::Stable)
        .expect("404 不是错误");
    assert!(outcome.is_none(), "取不到清单 = 还没发过版");
    assert_eq!(
        server.hits(),
        vec![manifest_path()],
        "确实问过一次，不是压根没发请求"
    );
}

#[test]
fn broken_json_is_reported_as_invalid_manifest() {
    let tmp = TempDir::new("gh-manifest-broken");
    let server = FakeHttp::start(vec![(
        manifest_path().as_str(),
        FakeResponse::json("{ not json"),
    )]);
    let source = local_source(&tmp, &server);

    let error = source
        .fetch_manifest(Channel::Stable)
        .expect_err("坏 JSON 必须报错");
    assert_eq!(error.kind, UpdateErrorKind::InvalidManifest);
    assert!(
        error.message.contains(&server.base()),
        "错误里要带出来源地址，日志才定位得到（实际：{}）",
        error.message
    );
    assert!(!error.is_retryable(), "清单坏重试多少次都一样");
}

#[test]
fn illegal_manifest_fields_are_reported() {
    let tmp = TempDir::new("gh-manifest-illegal");
    // size 为 0（发布脚本写漏）——字段层面的非法，走同一条 InvalidManifest 分类。
    let json = r#"{"channel":"stable","releases":[{"version":"0.6.0","artifacts":[{"target":"win-x64","format":"portable-exe","url":"ull.exe","size":0,"sha256":"0000000000000000000000000000000000000000000000000000000000000000"}]}]}"#;
    let server = FakeHttp::start(vec![(manifest_path().as_str(), FakeResponse::json(json))]);
    let source = local_source(&tmp, &server);

    let error = source
        .fetch_manifest(Channel::Stable)
        .expect_err("size 为 0 必须报错");
    assert_eq!(error.kind, UpdateErrorKind::InvalidManifest);
}

#[test]
fn manifest_self_reporting_another_channel_is_a_mismatch() {
    let tmp = TempDir::new("gh-manifest-mismatch");
    let server = FakeHttp::start(vec![(
        manifest_path().as_str(),
        FakeResponse::json(&manifest("beta", &[])),
    )]);
    let source = local_source(&tmp, &server);

    let error = source
        .fetch_manifest(Channel::Stable)
        .expect_err("渠道不符必须报错");
    assert_eq!(error.kind, UpdateErrorKind::ChannelMismatch);
}

#[test]
fn server_errors_are_network_errors() {
    let tmp = TempDir::new("gh-manifest-500");
    let server = FakeHttp::start(vec![(manifest_path().as_str(), FakeResponse::status(500))]);
    let source = local_source(&tmp, &server);

    let error = source
        .fetch_manifest(Channel::Stable)
        .expect_err("5xx 必须报错");
    assert_eq!(error.kind, UpdateErrorKind::Network);
    assert!(error.is_retryable(), "网络抖动值得隔一会儿再试");
}

#[test]
fn downloads_the_artifact_through_the_engine() {
    let tmp = TempDir::new("gh-artifact-download");
    let server = FakeHttp::start(vec![(asset_path().as_str(), FakeResponse::bin(PAYLOAD))]);
    let source = local_source(&tmp, &server);
    let dest = tmp.join("staging/package.exe");

    let written = source
        .fetch_artifact(&parsed_stable().releases[0].artifacts[0], &dest)
        .expect("应下载成功");
    assert_eq!(written, PAYLOAD.len() as u64);
    assert_eq!(fs::read(&dest).expect("目标文件应存在"), PAYLOAD);
    assert_eq!(
        server.hits(),
        vec![asset_path()],
        "包体走引擎，请求地址就是解析出来的那条"
    );
}

#[test]
fn missing_artifact_is_reported_as_source_missing() {
    let tmp = TempDir::new("gh-artifact-404");
    let server = FakeHttp::start(vec![]);
    let source = local_source(&tmp, &server);

    let error: UpdateError = source
        .fetch_artifact(
            &parsed_stable().releases[0].artifacts[0],
            &tmp.join("staging/package.exe"),
        )
        .expect_err("产物不存在必须报错");
    assert_eq!(error.kind, UpdateErrorKind::SourceMissing);
    assert!(!error.is_retryable(), "产物没上传，重试多少次都一样");
}

#[test]
fn mirror_first_region_puts_the_gh_proxy_in_front() {
    // 镜像策略不新造：国内（镜像优先）环境下 gh-proxy 前缀由引擎的镜像表 + 地区档案决定。
    let tmp = TempDir::new("gh-mirror");
    let source = GitHubReleaseSource::new(engine_for(&tmp, RegionProfile::mirror_first(0)));
    let artifact_url = source
        .artifact_url(&parsed_stable().releases[0].artifacts[0])
        .expect("应能拼出地址");

    let planned = source.candidates(&artifact_url);
    assert!(
        planned.len() >= 2,
        "镜像优先至少要有镜像与官方两条候选（实际 {planned:?}）"
    );
    assert_eq!(planned[0].source, SOURCE_GH_PROXY);
    assert_eq!(
        planned[0].url,
        format!("https://gh-proxy.com/{artifact_url}")
    );
    assert!(
        planned
            .iter()
            .any(|candidate| candidate.source == SOURCE_OFFICIAL && candidate.url == artifact_url),
        "官方地址仍要留作兜底（实际 {planned:?}）"
    );

    // 直连模式不套前缀：只剩官方一条。
    let direct = GitHubReleaseSource::new(engine_for(&tmp, RegionProfile::direct(0)));
    let planned_direct = direct.candidates(&artifact_url);
    assert_eq!(planned_direct.len(), 1);
    assert_eq!(planned_direct[0].source, SOURCE_OFFICIAL);

    // 清单地址同样吃到 gh-proxy（国内拿不到 releases/latest/download 的正是清单这一刻）。
    let manifest_url = source.manifest_url(Channel::Stable).expect("stable 有清单");
    let planned_manifest = source.candidates(&manifest_url);
    assert_eq!(planned_manifest[0].source, SOURCE_GH_PROXY);
    assert_eq!(
        planned_manifest[0].url,
        format!("https://gh-proxy.com/{manifest_url}")
    );
}

#[test]
fn agrees_with_dir_source_on_the_same_manifest() {
    // 对拍：同一份清单、同一个当前版本、同一个目标平台，两个源的判定必须一模一样。
    let tmp = TempDir::new("gh-dir-parity");
    let json = stable_manifest_json();

    // 本地目录源：清单与产物都在临时目录里（产物 url 是相对文件名）。
    tmp.write(
        &format!("{}.json", Channel::Stable.as_str()),
        json.as_bytes(),
    );
    tmp.write(asset_name(), PAYLOAD);
    let dir = DirSource::new(tmp.path());

    // GitHub 源：同一份 JSON 由假服务提供，产物 url 解析成 release 产物地址。
    let server = FakeHttp::start(vec![
        (manifest_path().as_str(), FakeResponse::json(&json)),
        (asset_path().as_str(), FakeResponse::bin(PAYLOAD)),
    ]);
    let github = local_source(&tmp, &server);

    let current = Version::parse("0.5.0").expect("版本号合法");
    let from_dir = UpdateChecker::new(dir, target())
        .check(&current, Channel::Stable)
        .expect("本地源应能判定");
    let from_github = UpdateChecker::new(github, target())
        .check(&current, Channel::Stable)
        .expect("GitHub 源应能判定");

    assert!(
        from_dir.is_available(),
        "基准用例应当有可用更新（实际 {from_dir:?}）"
    );
    assert_eq!(
        from_github, from_dir,
        "同一份清单，两个源选出的版本/产物/紧急度必须一致"
    );
}
