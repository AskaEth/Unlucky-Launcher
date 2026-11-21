//! M317：CurseForge 检索**接线 + 退役**的守门用例（全离线：喂录制的检索夹具给假传输层）。
//!
//! # 这一卷回答三个问题
//!
//! 1. **旧实现真的没了**：[`the_retired_direct_request_path_is_gone`] 扫 `sources.rs` 的源码，
//!    请求头 / 端点拼装 / 手写解析一个都不许再出现；[`the_delegation_to_core_is_still_there`]
//!    反过来钉「委托那几行还在」（不然「删掉」与「换回来」都测不出来）。
//!    **针在 `retired_curseforge_needles.rs`**（P28-2：针与断言分居两个文件，删/注释被检查的
//!    源码不会连针一起删）。
//! 2. **同一夹具下新旧结果一致**：[`the_same_fixture_yields_the_same_hits`] 把
//!    `core/src/addon/curseforge/fixtures/search_mods.json`（录制的 CF 响应）喂进新路径，逐字段
//!    断言 DTO——期望值就是退役前那份手解析对同一份 JSON 的产出（每个字段的来源写在
//!    `sources::cf_hit_of` 的表里）。
//! 3. **Key 的形状没变**：缺 Key 仍是一个「不可用」失败、且**一个请求都不发**；
//!    401 的失败说明里没有 Key（它只出现在请求头里）。

use std::sync::Mutex;

use super::retired_curseforge_needles::{FORBIDDEN, REQUIRED, SOURCE};
use crate::dto::{
    ContentFailureKind, ContentHit, ContentKind, ContentQuery, ContentSort, ContentSource,
};
use crate::host::addon::sources::{api_key, cf_available, curseforge_page};
use unlucky_core::addon::curseforge::{ApiKey, CfError, CfFuture, CfResponse, CfTransport};

/// 录制的 CF 检索响应（与 core 的客户端用例同一份夹具：新旧实现比的是同一批字节）。
pub(super) const SEARCH_FIXTURE: &str =
    include_str!("../../../../../core/src/addon/curseforge/fixtures/search_mods.json");

/// 用例里的「用户自带 Key」（不是任何真实凭据）。
const TEST_KEY: &str = "cf-presenter-test-key";

/// 一次请求的记录：URL + 请求头。
pub(super) type SeenRequest = (String, Vec<(String, String)>);

/// 内存传输层：回一份固定正文，并记下每次请求（URL + 请求头）。
pub(super) struct Recorded {
    status: u16,
    body: String,
    seen: Mutex<Vec<SeenRequest>>,
}

impl Recorded {
    pub(super) fn with_body(body: &str) -> Self {
        Self {
            status: 200,
            body: body.to_string(),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn with_status(status: u16) -> Self {
        Self {
            status,
            body: String::new(),
            seen: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn requests(&self) -> Vec<SeenRequest> {
        self.seen.lock().expect("seen 锁").clone()
    }
}

impl CfTransport for Recorded {
    fn get<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
    ) -> CfFuture<'a, Result<CfResponse, CfError>> {
        Box::pin(async move {
            self.seen
                .lock()
                .expect("seen 锁")
                .push((url.to_string(), headers.to_vec()));
            Ok(CfResponse::new(self.status, None, self.body.clone()))
        })
    }
}

pub(super) fn query() -> ContentQuery {
    ContentQuery {
        kind: ContentKind::Mod,
        source: ContentSource::Curseforge,
        text: "sodium".to_string(),
        game_version: Some("1.20.1".to_string()),
        loader: Some(crate::dto::LoaderKind::Fabric),
        sort: ContentSort::Relevance,
        offset: 0,
        limit: 20,
    }
}

fn key() -> ApiKey {
    ApiKey::new(TEST_KEY).expect("非空 Key")
}

// --------------------------------------------------------------- 退役：源码级守卫

/// `sources.rs` 里**不再出现**自带请求头、手拼端点与手写 JSON 解析。
///
/// 变异（改坏就该红）：
/// - 把 `("x-api-key", key)` 那一行**注释回去**（`// ("x-api-key", key)`）⇒ `x-api-key` 断言红
///   （注释里的针同样是「这段代码还在这个文件里」）；
/// - 把旧的手解析搬回来（`serde_json::from_str` / `latestFilesIndexes`）⇒ 对应断言红；
/// - 把端点常量搬回来（`api.curseforge.com`）⇒ 对应断言红。
#[test]
fn the_retired_direct_request_path_is_gone() {
    let hits: Vec<String> = FORBIDDEN
        .iter()
        .filter(|(needle, _)| SOURCE.contains(needle))
        .map(|(needle, why)| format!("{needle:?}（{why}）"))
        .collect();
    assert!(
        hits.is_empty(),
        "sources.rs 里还有退役前的东西：{}",
        hits.join(" / ")
    );
}

/// 委托给 core 的那几行**必须还在**（删掉/掏空 ⇒ 这一条红）。
///
/// 变异：把 `curseforge_page` 换成 `Err(ContentFailure::unsupported(..))`（旧实现的「未接通」）
/// ⇒ `CurseForge::with_key` / `.search(` 两条断言红。
#[test]
fn the_delegation_to_core_is_still_there() {
    let missing: Vec<String> = REQUIRED
        .iter()
        .filter(|(needle, _)| !SOURCE.contains(needle))
        .map(|(needle, why)| format!("{needle:?}（{why}）"))
        .collect();
    assert!(
        missing.is_empty(),
        "sources.rs 少了委托给 core 的东西：{}",
        missing.join(" / ")
    );
}

// --------------------------------------------------------------- 等价：同一夹具

/// 同一份录制的 CF 响应，退役前后的 DTO **逐字段相同**。
///
/// 期望值来自退役前那份手解析（`curseforge_hit`）：`id.to_string()` / `slug` / `name` / `summary` /
/// `authors[0].name` / `downloadCount` / `dateModified` 的日期 / `latestFilesIndexes[]` 的
/// `gameVersion` 与 `modLoader`（1 Forge / 4 Fabric / 5 Quilt / 6 NeoForge）/ `logo.url`。
///
/// 抓得住：CF 的 `categories` 混进 loaders（画面上会多出「fabric」这种**分类标签**来源、
/// 或者少一个 neoforge）⇒ `loaders` 断言红；`totalCount` 取成 `resultCount` ⇒ `total` 断言红；
/// 图标取 `avatarUrl` 之类别的字段 ⇒ `icon` 断言红。
#[test]
fn the_same_fixture_yields_the_same_hits() {
    let transport = Recorded::with_body(SEARCH_FIXTURE);
    let key = key();
    let (hits, total) = curseforge_page(&transport, Some(&key), &query()).expect("解析录制夹具");

    assert_eq!(total, 12_345, "总数取 pagination.totalCount");
    assert_eq!(hits.len(), 2, "{hits:?}");
    assert_eq!(
        hits[0],
        ContentHit {
            id: "394468".to_string(),
            slug: "sodium".to_string(),
            title: "Sodium".to_string(),
            summary: "Modern rendering engine and client-side optimization mod".to_string(),
            author: "jellysquid3".to_string(),
            downloads: 42_123_456,
            updated: "2026-02-03".to_string(),
            source: ContentSource::Curseforge,
            // 站点顺序 + 去重：fabric（4）、neoforge（6）各一次（第三个索引的 fabric 是重复项）。
            game_versions: vec!["1.21.1".to_string(), "1.20.1".to_string()],
            loaders: vec!["fabric".to_string(), "neoforge".to_string()],
            icon: Some("https://media.forgecdn.net/avatars/sodium.png".to_string()),
        }
    );
    assert_eq!(
        hits[1],
        ContentHit {
            id: "238222".to_string(),
            slug: "jei".to_string(),
            title: "Just Enough Items".to_string(),
            summary: "View items and recipes".to_string(),
            author: "mezz".to_string(),
            downloads: 312_345_678,
            updated: "2026-01-20".to_string(),
            source: ContentSource::Curseforge,
            game_versions: vec!["1.20.1".to_string()],
            loaders: vec!["forge".to_string()],
            icon: None,
        }
    );

    // 请求的 URL 必须与退役前手拼的那一条逐字节相同（参数顺序也一致）。
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    let (url, headers) = &requests[0];
    assert_eq!(
        url,
        "https://api.curseforge.com/v1/mods/search?gameId=432&classId=6&index=0&pageSize=20\
&sortField=1&sortOrder=desc&searchFilter=sodium&gameVersion=1.20.1"
    );
    // 加载器过滤**不发**（旧实现没发过：这一刀只换传输与解析，不外扩检索行为）。
    assert!(
        !url.contains("modLoaderType"),
        "加了加载器过滤会改变检索结果：{url}"
    );
    // Key 只在请求头里。
    assert!(headers
        .iter()
        .any(|(name, value)| name == "x-api-key" && value == TEST_KEY));
    assert!(!url.contains(TEST_KEY), "Key 不许进 URL：{url}");
}

// --------------------------------------------------------------- 缺 Key 与 401

/// 没配 Key ⇒ **不发请求**、失败分类仍是「不可用」（与退役前同一档），说明里说清是缺 Key。
///
/// 抓得住：让「没 Key」偷偷发一次请求（先发再判）⇒ 请求计数红；把它归到 `Other`
/// （页面会失去「去填 Key」那句话）⇒ 分类断言红。
#[test]
fn missing_key_is_unavailable_without_touching_the_network() {
    let transport = Recorded::with_body(SEARCH_FIXTURE);
    let failure = curseforge_page(&transport, None, &query()).expect_err("没 Key 就是不可用");

    assert_eq!(failure.kind, ContentFailureKind::Unsupported);
    assert!(
        failure.detail.contains("API Key"),
        "要说清缺的是什么：{}",
        failure.detail
    );
    assert!(
        transport.requests().is_empty(),
        "缺 Key 的判定必须发生在发请求之前：{:?}",
        transport.requests()
    );
}

/// 401 的失败说明里没有 Key（它只出现在请求头里），分类走 `dto::content::curseforge` 那张表
/// （Key 无效 = 「去改配置」，与缺 Key 同档 `Unsupported`）。
///
/// 变异（改坏就该红）：在 core 的错误里拼上 Key，或在这里把它转进 `detail`
/// ⇒ 下面 `contains(TEST_KEY)` 断言红。
#[test]
fn a_rejected_key_never_leaks_into_the_failure_detail() {
    let transport = Recorded::with_status(401);
    let key = key();
    let failure = curseforge_page(&transport, Some(&key), &query()).expect_err("401 是失败");

    assert_eq!(failure.kind, ContentFailureKind::Unsupported);
    assert!(
        !failure.detail.contains(TEST_KEY),
        "Key 泄漏进了错误串：{}",
        failure.detail
    );
    assert!(!format!("{failure:?}").contains(TEST_KEY));
    // 请求确实带了 Key（否则上面那条「不含」是空转）。
    assert!(transport
        .requests()
        .iter()
        .any(|(_, headers)| headers.iter().any(|(_, value)| value == TEST_KEY)));
}

// --------------------------------------------------------------- 「有没有 Key」的判据（M316 r1 P2-1）

/// 设置里那个键（与 `sources.rs` 用的同一个常量，不另写一份字符串）。
const KEY_SETTING: &str = unlucky_core::setting::keys::online::CURSEFORGE_API_KEY;
/// 兜底那一支读的环境变量名。
const KEY_ENV: &str = "ULL_CF_API_KEY";

/// 环境变量是**进程级**的：本卷里动它的用例串行跑（`cargo test` 默认多线程）。
pub(super) static KEY_ENV_LOCK: Mutex<()> = Mutex::new(());

/// 临时数据根（`Drop` 即删，panic 展开也删）。
pub(super) struct Root(std::path::PathBuf);

impl Root {
    pub(super) fn new(tag: &str) -> Self {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("ull-cfkey-{tag}-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ULL").join("config")).expect("建临时数据根");
        Self(dir)
    }

    pub(super) fn env(&self) -> crate::host::env::HostEnv {
        crate::host::env::HostEnv::at_root(&self.0)
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 环境变量的值守卫：离开作用域（含 panic 展开）时**恢复原值**，不污染同进程的其它用例。
pub(super) struct KeyEnv {
    previous: Option<String>,
}

impl KeyEnv {
    /// `Some` = 设成这个值，`None` = 删掉这个变量。
    pub(super) fn set(value: Option<&str>) -> Self {
        let previous = std::env::var(KEY_ENV).ok();
        match value {
            Some(value) => std::env::set_var(KEY_ENV, value),
            None => std::env::remove_var(KEY_ENV),
        }
        Self { previous }
    }
}

impl Drop for KeyEnv {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var(KEY_ENV, value),
            None => std::env::remove_var(KEY_ENV),
        }
    }
}

/// 把设置写进 `<root>/.ULL/config/launcher.json`（走真实的 `SettingStore`，不手搓 JSON）。
pub(super) fn write_setting(root: &Root, value: &str) {
    let env = root.env();
    let mut store = env.store();
    store
        .set(KEY_SETTING, serde_json::json!(value))
        .expect("写 CF Key");
    store.save().expect("落盘");
}

/// 「有没有 Key」这个 `bool` 看的是**值**，不是「这个键出现过」。
///
/// 三段判据（缺一段这条 decision 就没守住）：
/// 1. 出厂（设置空、环境变量空）⇒ `false`；
/// 2. 设置里是**空白串** ⇒ 仍 `false`（`trim` + 过滤空串那两行，去掉就该红）；
/// 3. 设置里是真值 ⇒ `true`，且**不需要**环境变量。
///
/// 抓得住：`api_key` 改成「设置里有这个键就 `Some`」（不看值）⇒ 第 2 段红；
/// `cf_available` 改成恒 `true`（或改成只看 `curseforge_ready()` 的环境变量）⇒ 第 1 段红。
#[test]
fn cf_available_follows_the_settings_key() {
    let _lock = KEY_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _env = KeyEnv::set(None);
    let root = Root::new("settings");
    let env = root.env();

    assert!(!cf_available(&env), "出厂没有 Key，不许说「可用」");
    assert!(api_key(&env).is_none());

    write_setting(&root, "   ");
    let env = root.env();
    assert!(!cf_available(&env), "空白 Key 不是 Key");
    assert!(api_key(&env).is_none(), "空白 Key 不许变成 Some");

    write_setting(&root, "settings-only-key");
    let env = root.env();
    assert!(cf_available(&env), "设置里填了 Key 就必须可用");
    assert!(api_key(&env).is_some());
}

/// 环境变量只是**兜底**：空白串仍算「没配」，真值才算有（与设置那一支同一套 `trim` 判据）。
///
/// 抓得住：宿主侧自己重写一遍滤空（绕过 core 的 `resolve_key`）⇒ 第 1 段红；整支不接 ⇒ 第 2 段红。
#[test]
fn the_env_fallback_counts_only_a_real_value() {
    let _lock = KEY_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = Root::new("env");
    let env = root.env();

    {
        let _blank = KeyEnv::set(Some("   "));
        assert!(!cf_available(&env), "空白环境变量不是 Key");
        assert!(api_key(&env).is_none());
    }
    {
        let _real = KeyEnv::set(Some("env-only-key"));
        assert!(cf_available(&env), "环境变量兜底必须生效");
        assert!(api_key(&env).is_some());
    }
}
