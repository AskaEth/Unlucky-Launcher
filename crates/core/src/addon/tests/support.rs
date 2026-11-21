//! 测试替身：内存里的远端（零网络）+ 把 M110 的 mrpack 夹具改成"声明与实际一致"的帮助器。
//!
//! # 为什么要改哈希
//!
//! M110 的 mrpack 夹具里 `hashes` 是**形状正确的占位值**（`0123456789abcdef…`），不对应任何
//! 真实字节——那份夹具的用途是"读得出、写得回"，不是"能装上"。导入链要校验"声明与实际
//! 是否相符"，所以：
//!
//! - **成功路径**用 [`mrpack_stub`]：沿用同一份夹具目录，只把 `sha1` 换成真实摘要、把
//!   `downloads` 换成 `stub://` 地址（`sha512` 是占位值不影响：导入只校验 `sha1`，
//!   而 M110 的读者只校验 `sha512` 的**形状**）；
//! - **声明与实际不符**用原始夹具：占位 `sha1` 一定不等于实际字节 ⇒ 必须 `Failed`。
//!
//! 两件事用同一份夹具，比手搓两个包更能说明"判据真的在跑"。

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde_json::json;

use super::super::import::{
    CurseFileMeta, FetchFuture, FetchRequest, FetchedFile, ModrinthFileMeta, PackFetcher,
};
use super::fixtures;
use super::fixtures::Fixture;
use super::{write_file, zip_dir};
use crate::download::verify::digest_bytes;
use crate::download::HashAlgo;

/// 成功路径上 `mods/jei.jar` 的内容。
pub(crate) const JEI_BYTES: &[u8] = b"stub jei jar: not a real jar, just bytes";
/// 成功路径上 `mods/optional.jar` 的内容。
pub(crate) const OPTIONAL_BYTES: &[u8] = b"stub optional";
/// 成功路径上两个文件的下载地址。
pub(crate) const JEI_URL: &str = "stub://mods/jei.jar";
/// 第二个文件的下载地址。
pub(crate) const OPTIONAL_URL: &str = "stub://mods/optional.jar";
/// CF 夹具里两个条目的补全地址（stub）。
pub(crate) const CURSE_URL: &str = "stub://curse/jei.jar";
/// M110 mrpack 夹具里 `mods/jei.jar` 的**占位**下载地址（原始夹具用，配的是假哈希）。
pub(crate) const JEI_PLACEHOLDER_URL: &str =
    "https://cdn.modrinth.com/data/AANobbMI/versions/abc/jei.jar";
/// M110 mrpack 夹具里 `mods/optional.jar` 的**占位**下载地址。
pub(crate) const OPTIONAL_PLACEHOLDER_URL: &str =
    "https://cdn.modrinth.com/data/opt/versions/x/optional.jar";

/// 一个字节的 SHA-1（小写十六进制）。
pub(crate) fn sha1(bytes: &[u8]) -> String {
    digest_bytes(HashAlgo::Sha1, bytes)
}

/// 内存 fetcher：按 URL / (project, file) 查表，写盘后回成功。
pub(crate) struct StubFetcher {
    files: BTreeMap<String, Vec<u8>>,
    curse: BTreeMap<(i64, i64), CurseFileMeta>,
    /// M162：按 sha512 查到的 Modrinth 文件元数据（模拟「站点认得这个文件」）。
    modrinth: BTreeMap<String, ModrinthFileMeta>,
    /// M162：补全时假装「没联网」（与「站点不认得这个摘要」区分开）。
    modrinth_offline: bool,
    /// 被请求过的 URL（断言"没白下载"用）。
    calls: Mutex<Vec<String>>,
    /// 是否像真引擎那样在目标旁边留下续传残片（`.ullpart-*` / `.ullmeta.json`）。
    artifacts: bool,
}

impl StubFetcher {
    /// 空表。
    pub(crate) fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            curse: BTreeMap::new(),
            modrinth: BTreeMap::new(),
            modrinth_offline: false,
            calls: Mutex::new(Vec::new()),
            artifacts: false,
        }
    }

    /// 挂一个可下载的地址。
    pub(crate) fn with_file(mut self, url: &str, bytes: &[u8]) -> Self {
        self.files.insert(url.to_string(), bytes.to_vec());
        self
    }

    /// 挂一条 CurseForge 元数据。
    pub(crate) fn with_curse(mut self, project_id: i64, file_id: i64, meta: CurseFileMeta) -> Self {
        self.curse.insert((project_id, file_id), meta);
        self
    }

    /// 挂一条「按 sha512 反查」的结果（M162 的补全路径）。
    pub(crate) fn with_modrinth(mut self, sha512: &str, meta: ModrinthFileMeta) -> Self {
        self.modrinth.insert(sha512.to_string(), meta);
        self
    }

    /// 让补全路径假装「没联网」（`FetchErrorKind::Offline`）。
    pub(crate) fn offline_modrinth(mut self) -> Self {
        self.modrinth_offline = true;
        self
    }

    /// 像真引擎那样在每次"下载"后留下续传残片与 meta（用来钉"中间产物不许进实例"）。
    pub(crate) fn with_engine_artifacts(mut self) -> Self {
        self.artifacts = true;
        self
    }

    /// 被请求过的 URL 列表。
    pub(crate) fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls 锁").clone()
    }
}

impl PackFetcher for StubFetcher {
    fn curse_file<'a>(
        &'a self,
        project_id: i64,
        file_id: i64,
    ) -> FetchFuture<'a, Result<CurseFileMeta, super::super::import::FetchError>> {
        Box::pin(async move {
            self.curse
                .get(&(project_id, file_id))
                .cloned()
                .ok_or_else(|| {
                    super::super::import::FetchError::new(
                        super::super::import::FetchErrorKind::NotFound,
                        format!("stub 里没有 {project_id}:{file_id}"),
                    )
                })
        })
    }

    fn modrinth_file_by_sha512<'a>(
        &'a self,
        sha512: &'a str,
    ) -> FetchFuture<'a, Result<ModrinthFileMeta, super::super::import::FetchError>> {
        Box::pin(async move {
            if self.modrinth_offline {
                return Err(super::super::import::FetchError::new(
                    super::super::import::FetchErrorKind::Offline,
                    "连不上 api.modrinth.com",
                ));
            }
            self.modrinth.get(sha512).cloned().ok_or_else(|| {
                super::super::import::FetchError::new(
                    super::super::import::FetchErrorKind::NotFound,
                    format!("stub 里没有 sha512={sha512}"),
                )
            })
        })
    }

    fn fetch_file<'a>(
        &'a self,
        request: FetchRequest<'a>,
    ) -> FetchFuture<'a, Result<FetchedFile, super::super::import::FetchError>> {
        Box::pin(async move {
            self.calls
                .lock()
                .expect("calls 锁")
                .push(request.url.to_string());
            let bytes = self.files.get(request.url).cloned().ok_or_else(|| {
                super::super::import::FetchError::new(
                    super::super::import::FetchErrorKind::NotFound,
                    format!("stub 里没有 {}", request.url),
                )
            })?;
            if let Some(parent) = request.dest.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    super::super::import::FetchError::new(
                        super::super::import::FetchErrorKind::Io,
                        error.to_string(),
                    )
                })?;
            }
            std::fs::write(request.dest, &bytes).map_err(|error| {
                super::super::import::FetchError::new(
                    super::super::import::FetchErrorKind::Io,
                    error.to_string(),
                )
            })?;
            if self.artifacts {
                leave_engine_artifacts(request.dest);
            }
            Ok(FetchedFile {
                bytes: bytes.len() as u64,
            })
        })
    }
}

/// 把 M110 的 mrpack 夹具改成"声明与实际一致"，并配好 stub fetcher。
///
/// 只改两处：`files[].hashes.sha1`（换成真实摘要）与 `files[].downloads`（换成 `stub://`）。
pub(crate) fn mrpack_stub(tag: &str) -> (Fixture, StubFetcher) {
    let fixture = fixtures::modrinth(tag);
    let index_path = fixture.dir.join("modrinth.index.json");
    let text = std::fs::read_to_string(&index_path).expect("读夹具 index");
    let mut value: serde_json::Value = serde_json::from_str(&text).expect("解析夹具 index");
    let entries = [
        ("mods/jei.jar", JEI_BYTES, JEI_URL),
        ("mods/optional.jar", OPTIONAL_BYTES, OPTIONAL_URL),
    ];
    let files = value["files"].as_array_mut().expect("夹具 files 是数组");
    for (entry, (path, bytes, url)) in files.iter_mut().zip(entries) {
        assert_eq!(
            entry["path"],
            json!(path),
            "夹具条目顺序变了，本帮助器要跟着改"
        );
        entry["hashes"]["sha1"] = json!(sha1(bytes));
        entry["downloads"] = json!([url]);
        entry["fileSize"] = json!(bytes.len());
    }
    write_file(&index_path, value.to_string().as_bytes());
    zip_dir(&fixture.dir, &fixture.zip);

    let fetcher = StubFetcher::new()
        .with_file(JEI_URL, JEI_BYTES)
        .with_file(OPTIONAL_URL, OPTIONAL_BYTES);
    (fixture, fetcher)
}

/// CF 夹具的 stub 元数据（两个条目都补得出来，落在 `mods/`）。
pub(crate) fn curse_meta(file_name: &str, bytes: &[u8]) -> CurseFileMeta {
    CurseFileMeta {
        file_name: file_name.to_string(),
        url: Some(CURSE_URL.to_string()),
        sha1: Some(sha1(bytes)),
        size: Some(bytes.len() as u64),
        class_id: Some(6),
    }
}

/// 在目标旁边留下与 `core::download::atomic` 同名的续传残片与 meta。
fn leave_engine_artifacts(dest: &std::path::Path) {
    let Some(name) = dest
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
    else {
        return;
    };
    let part = dest.with_file_name(format!("{name}.ullpart-{}-0", std::process::id()));
    let meta = dest.with_file_name(format!("{name}.ullmeta.json"));
    std::fs::write(part, b"partial bytes").expect("写续传残片");
    std::fs::write(meta, b"{}").expect("写续传 meta");
}
