//! M317 CurseForge 接线用例的**替身**（假传输层 + 两个假 fetcher）。
//!
//! 拆出来的理由与 `support.rs` 同一个：两卷用例（`import_curseforge_api` 管客户端补全、
//! `import_curseforge_key` 管导入链上的降级）共用这一套替身，写成两份迟早漂开。
//!
//! 全部内存操作：没有一条用例会打开 socket（`Scripted` 就是一张 路由表 → 响应）。

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::addon::curseforge::{ApiKey, CfError, CfFuture, CfResponse, CfTransport, RetryPolicy};
use crate::addon::import::fetch::curse_meta;
use crate::addon::import::{
    CurseFileMeta, FetchError, FetchErrorKind, FetchFuture, FetchRequest, FetchedFile, PackFetcher,
};

use super::support::sha1;

/// 用例里的「用户自带 Key」（不是任何真实凭据）。
pub(super) const TEST_KEY: &str = "cf-test-key-not-a-real-one";
/// CF 夹具里两个条目的 id（`fixtures::curseforge` 的 `manifest.json`）。
pub(super) const JEI: (i64, i64) = (238_222, 5_102_025);
pub(super) const FABRIC_API: (i64, i64) = (306_612, 4_587_454);
/// 假 CDN 上那条文件的地址（`format!` 与断言共用一份，写两遍会写飘）。
pub(super) const FILE_URL: &str =
    "https://edge.forgecdn.net/files/4455/667/jei-1.20.1-forge-15.2.0.27.jar";
/// 夹具文件的内容（不是真 jar，只是字节）。
pub(super) const CURSE_BYTES: &[u8] = b"stub curse file bytes";

/// 一条文件的响应（字段取自录制的 `fixtures/mod_files.json`，摘要换成夹具字节的**真实** sha1：
/// 导入链会核对声明摘要，占位值会把「补全成功」的用例变成 `Integrity` 失败）。
pub(super) fn file_json() -> String {
    format!(
        r#"{{"data":{{"id":4455667,"modId":238222,"displayName":"jei-1.20.1-forge-15.2.0.27","fileName":"jei-1.20.1-forge-15.2.0.27.jar","releaseType":1,"fileLength":{len},"downloadUrl":"{FILE_URL}","hashes":[{{"value":"{sha1}","algo":1}}],"gameVersions":["1.20.1","Forge","Client"]}}}}"#,
        len = CURSE_BYTES.len(),
        sha1 = sha1(CURSE_BYTES),
    )
}

/// 一条项目的响应（`classId` 决定落盘目录；字段取自录制的 `fixtures/mod_detail.json` 形状）。
pub(super) const MOD_JSON: &str = r#"{"data":{"id":238222,"name":"Just Enough Items","slug":"jei","summary":"View items and recipes","classId":6}}"#;

/// 一次请求的记录：URL + 请求头（用例断言「带了什么」用）。
pub(super) type SeenRequest = (String, Vec<(String, String)>);

/// 内存里的 CF 传输层：按「URL 里最长的匹配片段」回一条脚本化响应，并记下每次请求。
pub(super) struct Scripted {
    routes: Vec<(&'static str, u16, String)>,
    fallback: u16,
    seen: Mutex<Vec<SeenRequest>>,
}

impl Scripted {
    pub(super) fn new() -> Self {
        Self {
            routes: Vec::new(),
            fallback: 404,
            seen: Mutex::new(Vec::new()),
        }
    }

    /// 挂一条路由（`needle` 是 URL 里的片段）。
    pub(super) fn route(
        mut self,
        needle: &'static str,
        status: u16,
        body: impl Into<String>,
    ) -> Self {
        self.routes.push((needle, status, body.into()));
        self
    }

    /// 没命中任何路由时的状态码（缺省 404 = 站点没这个条目）。
    pub(super) fn fallback(mut self, status: u16) -> Self {
        self.fallback = status;
        self
    }

    /// 发出去的全部请求（URL + 请求头）。
    pub(super) fn requests(&self) -> Vec<SeenRequest> {
        self.seen.lock().expect("seen 锁").clone()
    }
}

impl CfTransport for Scripted {
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
            // 最长匹配优先：`/mods/1/files/2` 必须命中文件那条，而不是项目那条。
            let hit = self
                .routes
                .iter()
                .filter(|(needle, ..)| url.contains(needle))
                .max_by_key(|(needle, ..)| needle.len());
            match hit {
                Some((_, status, body)) => Ok(CfResponse::new(*status, None, body.clone())),
                None => Ok(CfResponse::new(self.fallback, None, String::new())),
            }
        })
    }
}

/// 把 `files` 里的字节写到 `request.dest`（两个假 fetcher 共用一份，避免两套写入逻辑漂开）。
fn download_stub(
    files: &BTreeMap<String, Vec<u8>>,
    calls: &Mutex<Vec<String>>,
    request: FetchRequest<'_>,
) -> Result<FetchedFile, FetchError> {
    calls
        .lock()
        .expect("calls 锁")
        .push(request.url.to_string());
    let bytes = files.get(request.url).ok_or_else(|| {
        FetchError::new(
            FetchErrorKind::NotFound,
            format!("stub 里没有 {}", request.url),
        )
    })?;
    if let Some(parent) = request.dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| FetchError::new(FetchErrorKind::Io, error.to_string()))?;
    }
    std::fs::write(request.dest, bytes)
        .map_err(|error| FetchError::new(FetchErrorKind::Io, error.to_string()))?;
    Ok(FetchedFile {
        bytes: bytes.len() as u64,
    })
}

/// 只实现下载、**不实现** `curse_file` 的 fetcher ⇒ `curse_file` 走 trait 的默认实现
/// （= 「没配 Key 的 fetcher」那条真实分支，不是用例自己复制一份措辞）。
pub(super) struct NoCurseFetcher {
    files: BTreeMap<String, Vec<u8>>,
    calls: Mutex<Vec<String>>,
}

impl NoCurseFetcher {
    pub(super) fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl PackFetcher for NoCurseFetcher {
    fn fetch_file<'a>(
        &'a self,
        request: FetchRequest<'a>,
    ) -> FetchFuture<'a, Result<FetchedFile, FetchError>> {
        Box::pin(async move { download_stub(&self.files, &self.calls, request) })
    }
}

/// 走**真**补全路径（`curse_meta` + 脚本化传输层）的 fetcher：用来钉 401/403/429 与「一条不挡另一条」。
pub(super) struct CfApiFetcher {
    pub(super) transport: Scripted,
    key: Option<ApiKey>,
    files: BTreeMap<String, Vec<u8>>,
    calls: Mutex<Vec<String>>,
}

impl CfApiFetcher {
    pub(super) fn new(transport: Scripted, key: Option<ApiKey>) -> Self {
        Self {
            transport,
            key,
            files: BTreeMap::new(),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// 挂一条可下载的字节。
    pub(super) fn with_file(mut self, url: &str, bytes: &[u8]) -> Self {
        self.files.insert(url.to_string(), bytes.to_vec());
        self
    }
}

impl PackFetcher for CfApiFetcher {
    fn curse_file<'a>(
        &'a self,
        project_id: i64,
        file_id: i64,
    ) -> FetchFuture<'a, Result<CurseFileMeta, FetchError>> {
        Box::pin(async move {
            // 关掉重试：用例要的是「一次 429 就降级」，不是真的等退避。
            curse_meta(
                &self.transport,
                self.key.as_ref(),
                RetryPolicy::disabled(),
                project_id,
                file_id,
            )
            .await
        })
    }

    fn fetch_file<'a>(
        &'a self,
        request: FetchRequest<'a>,
    ) -> FetchFuture<'a, Result<FetchedFile, FetchError>> {
        Box::pin(async move { download_stub(&self.files, &self.calls, request) })
    }
}

/// 用例里的那个 Key（每次新建一把，避免共用全局态）。
pub(super) fn key() -> ApiKey {
    ApiKey::new(TEST_KEY).expect("非空 Key")
}
