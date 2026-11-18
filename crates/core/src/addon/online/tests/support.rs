//! 测试替身：录制响应传输层 + 假字节下载器。
//!
//! 两个替身都**只做注入点该做的事**：不校验、不重试、不判断——那些必须在被测代码里，
//! 否则测试就只是「验证桩自己」。

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use sha1::Digest as _;

use crate::addon::online::download::{ArtifactFetcher, FetchArgs};
use crate::addon::online::{OnlineError, OnlineErrorKind, OnlineFuture, Transport};

/// 按 URL 前缀路由的传输层（把录制 JSON 喂给客户端）。
pub(super) struct FixtureTransport {
    routes: BTreeMap<String, Result<String, OnlineError>>,
    seen: Mutex<Vec<String>>,
}

impl FixtureTransport {
    /// 建一个空的路由表。
    pub(super) fn new() -> Self {
        Self {
            routes: BTreeMap::new(),
            seen: Mutex::new(Vec::new()),
        }
    }

    /// 挂一条路由（`prefix` 是 URL 前缀）。
    pub(super) fn with(mut self, prefix: &str, body: &str) -> Self {
        self.routes.insert(prefix.to_string(), Ok(body.to_string()));
        self
    }

    /// 挂一条「没联网」的路由。
    pub(super) fn offline(mut self, prefix: &str) -> Self {
        self.routes.insert(
            prefix.to_string(),
            Err(OnlineError::offline("连不上 api.modrinth.com")),
        );
        self
    }

    /// 挂一条「站点说没有」的路由。
    pub(super) fn missing(mut self, prefix: &str) -> Self {
        self.routes.insert(
            prefix.to_string(),
            Err(OnlineError::new(OnlineErrorKind::NotFound, "HTTP 404")),
        );
        self
    }

    /// 实际请求过的 URL（按顺序）。
    pub(super) fn seen(&self) -> Vec<String> {
        self.seen.lock().expect("锁没中毒").clone()
    }
}

impl Transport for FixtureTransport {
    fn get_text<'a>(&'a self, url: &'a str) -> OnlineFuture<'a, Result<String, OnlineError>> {
        Box::pin(async move {
            self.seen.lock().expect("锁没中毒").push(url.to_string());
            // 最长前缀优先：`/v2/project/sodium` 与 `/v2/project/sodium/version` 不能互相抢。
            let best = self
                .routes
                .iter()
                .filter(|(prefix, _)| url.starts_with(prefix.as_str()))
                .max_by_key(|(prefix, _)| prefix.len())
                .map(|(_, result)| result);
            match best {
                Some(Ok(body)) => Ok(body.clone()),
                Some(Err(error)) => Err(error.clone()),
                None => Err(OnlineError::new(
                    OnlineErrorKind::NotFound,
                    format!("测试没为 {url} 挂路由"),
                )),
            }
        })
    }
}

/// 假字节下载器：把给定字节写到 `dest`，可以安排前几次失败。
pub(super) struct FakeArtifact {
    bytes: Vec<u8>,
    /// 还欠几次「没联网」失败（每尝试一次减一）
    failures: Mutex<u32>,
    /// 实际尝试次数
    attempts: Mutex<u32>,
    /// 故意只写一半（模拟半截文件）
    truncate: bool,
}

impl FakeArtifact {
    /// 正常一份字节。
    pub(super) fn new(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
            failures: Mutex::new(0),
            attempts: Mutex::new(0),
            truncate: false,
        }
    }

    /// 前 `count` 次尝试没联网。
    pub(super) fn failing(mut self, count: u32) -> Self {
        *self.failures.get_mut().expect("锁没中毒") = count;
        self
    }

    /// 只写一半字节（半截文件场景）。
    pub(super) fn truncated(mut self) -> Self {
        self.truncate = true;
        self
    }

    /// 尝试次数。
    pub(super) fn attempts(&self) -> u32 {
        *self.attempts.lock().expect("锁没中毒")
    }
}

impl ArtifactFetcher for FakeArtifact {
    fn fetch<'a>(&'a self, args: FetchArgs<'a>) -> OnlineFuture<'a, Result<u64, OnlineError>> {
        Box::pin(async move {
            *self.attempts.lock().expect("锁没中毒") += 1;
            let mut failures = self.failures.lock().expect("锁没中毒");
            if *failures > 0 {
                *failures -= 1;
                return Err(OnlineError::offline("假装的断网"));
            }
            drop(failures);
            write_bytes(args.dest, &self.bytes, self.truncate)?;
            Ok(if self.truncate {
                self.bytes.len() as u64 / 2
            } else {
                self.bytes.len() as u64
            })
        })
    }
}

fn write_bytes(dest: &Path, bytes: &[u8], truncated: bool) -> Result<(), OnlineError> {
    let mut file = std::fs::File::create(dest).map_err(|error| {
        OnlineError::new(
            OnlineErrorKind::Io,
            format!("建 {} 失败：{error}", dest.display()),
        )
    })?;
    let half = bytes.len() / 2;
    let slice = if truncated { &bytes[..half] } else { bytes };
    file.write_all(slice).map_err(|error| {
        OnlineError::new(
            OnlineErrorKind::Io,
            format!("写 {} 失败：{error}", dest.display()),
        )
    })
}

/// 一段字节的 `(sha1, sha512)`（小写十六进制）。
pub(super) fn hashes_of(bytes: &[u8]) -> (String, String) {
    let sha1 = sha1::Sha1::digest(bytes);
    let sha512 = sha2::Sha512::digest(bytes);
    (
        crate::addon::online::download::to_hex(&sha1),
        crate::addon::online::download::to_hex(&sha512),
    )
}

/// 拼一份 `/v2/project/{id}/version` 响应：一个版本、一个主文件、按给定顺序声明依赖。
///
/// `deps` 的每一项是 `(project_id, dependency_type)`，**顺序照抄站点原文**——顺序无关性由
/// `resolve` 保证（`tests/order.rs` 用它做「夹具逆序也得到同一顺序」的守门）。
pub(super) fn version_json(project: &str, file: &str, deps: &[(&str, &str)]) -> String {
    let dependencies: Vec<String> = deps
        .iter()
        .map(|(id, kind)| format!(r#"{{"project_id":"{id}","dependency_type":"{kind}"}}"#))
        .collect();
    format!(
        r#"[{{"id":"ver-{project}","project_id":"{project}","name":"{project}",
            "version_number":"1.0","version_type":"release","loaders":["fabric"],
            "game_versions":["1.20.1"],
            "files":[{{"hashes":{{}},"url":"https://cdn.modrinth.com/data/{project}/1.0/{file}",
                       "filename":"{file}","size":16,"primary":true}}],
            "dependencies":[{}]}}]"#,
        dependencies.join(",")
    )
}
