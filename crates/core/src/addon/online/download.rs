//! 「取到下载地址」之后的最后一步：**下载 → 复核 sha1/sha512 → 才算数**。
//!
//! # 三条硬要求（任务书原文）
//!
//! 1. **走既有分块下载引擎与镜像策略**：生产实现 [`EngineArtifactFetcher`] 只把请求翻译成
//!    [`DownloadRequest`]（`ResourceKind::ModFile` ⇒ 引擎按大文件分块下载，镜像展开、地区模式、
//!    健康度、退避全在引擎里；Modrinth 的 CDN 不在 `mirrors.json` 的规则里，所以只有国内环境
//!    真的命中规则时才会换源，不会瞎改写）；
//! 2. **校验 sha1/sha512 后再写入**：引擎只认 sha1（`Integrity` 只有 Sha1/Sha256 两档），
//!    所以 sha1 交给引擎流式校验、sha512 由本模块**落盘后复核**；任何一项不符都删掉目标文件
//!    （`download` 模块的 `HashAlgo` 没有 Sha512 是事实，改它超出 M162 的 scope，已登记 finding）；
//! 3. **失败可重试且不留半截文件**：每次尝试前先删目标文件，尝试次数 [`RETRY_ATTEMPTS`]；
//!    半截文件本身由引擎的 `PartFile` 守卫（`<name>.ullpart-<pid>-<seq>` + 原子改名）兜住，
//!    本模块不另造临时目录范式。
//!
//! # 为什么摘要复核不信任 fetcher
//!
//! [`ArtifactFetcher`] 是注入点（测试喂假字节）。如果「校验」放在某个具体实现里，测试注入的
//! 实现就成了「不校验也能过」的后门——而生产实现与测试实现走的是同一段 [`download_verified`]。
//! 所以复核在**外层**：读一遍落盘文件，同时算 sha1 与 sha512，与声明逐字比对。

use std::io::Read;
use std::path::{Path, PathBuf};

use sha1::Digest as _;
use sha2::Sha512;

use crate::download::config::DEFAULT_MAX_ATTEMPTS_PER_SOURCE;
use crate::download::{DownloadEngine, DownloadRequest, Integrity, ResourceKind};

use super::modrinth::HashAlgorithm;
use super::transport::{OnlineError, OnlineErrorKind, OnlineFuture};

/// 本模块外层「整条下载」的预算（含首次）：最多取几遍文件。
///
/// # 它**不是**引擎的每源尝试次数，所以不引用 `download::config` 的常量（M205 判定）
///
/// 数值恰好都是 3，但数的不是同一件事。三条判据，任一条都足够说明「合并成一处」会把语义拍平：
///
/// 1. **层级不同，而且是相乘**：引擎那层在 [`ArtifactFetcher`] 里面，按**源**数（镜像展开后每个候选
///    各有一份预算，见 `DownloadEngine::attempts_for`；源在冷却期时还会缩成 1 次恢复探测）。本模块这
///    层只数「取一遍 + 复核一遍」，每数一次都要重新走一遍引擎 —— 最坏情况下的真实请求数 ≈
///    [`RETRY_ATTEMPTS`] × 引擎每源预算，合并成一个数等于把最坏请求数变成平方级。
/// 2. **管的失败类型不同**：sha512 不符与字节数不符只有本模块知道（引擎只认 sha1，`Integrity` 没有
///    Sha512 档），这类失败引擎**不会**重试，必须由这一层重取 —— 这一层的预算里有一档是引擎看不见的。
/// 3. **改它的性质不同**：`download.retry_limit` 是用户可见的设置（0~10、进 JSON schema，也就是说引擎
///    那个默认值会因为**用户的**配置而变）；这里的 3 是内容库元数据请求的产品口径，没有设置键，也不该
///    被某个设置项的出厂默认牵着走。
///
/// **边界**：真实请求数 = [`RETRY_ATTEMPTS`]（外层）× 引擎每源预算（`download.retry_limit`，出厂
/// [`DEFAULT_MAX_ATTEMPTS_PER_SOURCE`]）。**出厂档**是 3 × 3 = 9；用户把 `retry_limit` 调到 N 时是
/// 3 × N（schema 上限 `MAX_ATTEMPTS_PER_SOURCE` = 10 ⇒ 30）——**9 只是出厂档的数，不是事实上限**，
/// 这正是本仓反复吃亏的「把出厂值说成上限」。下面两条注解级断言（编译期求值）钉的也只是**出厂档**：
/// 出厂默认想改，就得先读到这段判据，而不是被一次「统一常量」的顺带改动悄悄翻倍。
pub const RETRY_ATTEMPTS: u32 = 3;

/// 外层预算被改了吗（改之前先读 [`RETRY_ATTEMPTS`] 的 doc：它不是引擎的每源次数）。
const _: () = assert!(
    RETRY_ATTEMPTS == 3,
    "外层下载预算（出厂档 3）变了：它不是 download::config 的每源尝试次数，改它要单独一个决定"
);

/// **出厂档**最坏请求数（外层 3 × 引擎每源出厂 3）——两个「3」不共享来源，乘积才是事实。
/// 用户把 `download.retry_limit` 调大后真实最坏是 3 × N，与本断言无关（那头是运行时配置，编译期钉不了）。
const _: () = assert!(
    RETRY_ATTEMPTS as u64 * DEFAULT_MAX_ATTEMPTS_PER_SOURCE as u64 == 9,
    "出厂档最坏真实请求数变了（外层 3 × 引擎每源出厂 3 = 9；用户把 retry_limit 调到 N 时是 3 × N）：先读 doc"
);

/// 一次下载请求（只含「引擎需要什么」，不含任何站点字段）。
#[derive(Debug, Clone, Copy)]
pub struct FetchArgs<'a> {
    /// 逻辑下载地址（官方源；镜像展开在引擎里）
    pub url: &'a str,
    /// 落盘路径（父目录由调用方保证，或由本模块建）
    pub dest: &'a Path,
    /// 期望 SHA-1（形状不对当「没给」，见 [`well_formed`]）
    pub sha1: Option<&'a str>,
    /// 期望字节数
    pub size: Option<u64>,
}

/// 「把 URL 下到 dest」的注入点（测试喂假字节）。
pub trait ArtifactFetcher: Send + Sync {
    /// 下到 `dest`，返回落盘字节数。
    fn fetch<'a>(&'a self, args: FetchArgs<'a>) -> OnlineFuture<'a, Result<u64, OnlineError>>;
}

/// 生产实现：交给 `core::download` 的引擎。
#[derive(Debug, Clone, Copy)]
pub struct EngineArtifactFetcher<'a> {
    engine: &'a DownloadEngine,
}

impl<'a> EngineArtifactFetcher<'a> {
    /// 绑定引擎。
    #[must_use]
    pub const fn new(engine: &'a DownloadEngine) -> Self {
        Self { engine }
    }
}

impl ArtifactFetcher for EngineArtifactFetcher<'_> {
    fn fetch<'a>(&'a self, args: FetchArgs<'a>) -> OnlineFuture<'a, Result<u64, OnlineError>> {
        Box::pin(async move {
            let mut request =
                DownloadRequest::new(ResourceKind::ModFile, args.url, args.dest.to_path_buf());
            if let Some(sha1) = args.sha1.filter(|hex| HashAlgorithm::Sha1.accepts(hex)) {
                request.integrity = Some(Integrity::sha1(sha1));
            }
            request.expected_size = args.size;
            let outcome = self.engine.download(request).await?;
            Ok(outcome.bytes)
        })
    }
}

/// 一个文件的摘要（读一遍盘同时算两个）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Digest {
    /// SHA-1（小写十六进制）
    pub sha1: String,
    /// SHA-512（小写十六进制）
    pub sha512: String,
    /// 字节数
    pub bytes: u64,
}

/// 一次成功下载的产物。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Downloaded {
    /// 落盘路径
    pub path: PathBuf,
    /// 字节数
    pub bytes: u64,
    /// 落盘内容的摘要（**实测**，不是声明）
    pub digest: Digest,
}

/// 下载并复核（sha1 **与** sha512，给了哪个就校验哪个）。
///
/// # Errors
///
/// - 声明的摘要是 `Some` 但形状不对 ⇒ [`OnlineErrorKind::Unsupported`]，**不发请求**；
/// - 没有下载地址 ⇒ `Unsupported`；
/// - 重试用尽仍是网络/站点/摘要问题 ⇒ 最后一次的错误（且目标文件已被删掉）；
/// - 本地 IO ⇒ `Io`。
pub async fn download_verified(
    fetcher: &dyn ArtifactFetcher,
    url: &str,
    sha1: Option<&str>,
    sha512: Option<&str>,
    size: Option<u64>,
    dest: &Path,
) -> Result<Downloaded, OnlineError> {
    if url.trim().is_empty() {
        return Err(OnlineError::unsupported("没有可用下载地址"));
    }
    // 两档摘要**各自**过形状检查：坏掉的那一档丢掉，用另一档校验（mrpack 的权威摘要是
    // sha512，CLI/整合包都可能给出一个人写坏的 sha1——为此整条不装是过分惩罚）。
    // 但如果**声明过**摘要却一档都用不上，就必须失败：那是「没有任何校验」的伪装。
    let declared = sha1.is_some() || sha512.is_some();
    let sha1 = usable_hash(sha1, HashAlgorithm::Sha1);
    let sha512 = usable_hash(sha512, HashAlgorithm::Sha512);
    if declared && sha1.is_none() && sha512.is_none() {
        return Err(OnlineError::unsupported(
            "声明的 sha1/sha512 都不是合法摘要，无法校验（拒绝在「没有校验」的情况下落盘）",
        ));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            OnlineError::new(
                OnlineErrorKind::Io,
                format!("建目录 {} 失败：{error}", parent.display()),
            )
        })?;
    }

    let mut last: Option<OnlineError> = None;
    for attempt in 0..RETRY_ATTEMPTS {
        // 每次尝试前清掉目标：上一轮的半截/坏内容不许参与本轮判定。
        remove_quiet(dest);
        let fetched = fetcher
            .fetch(FetchArgs {
                url,
                dest,
                sha1,
                size,
            })
            .await;
        let bytes = match fetched {
            Ok(bytes) => bytes,
            Err(error) if retryable(error.kind()) && attempt + 1 < RETRY_ATTEMPTS => {
                last = Some(error);
                continue;
            }
            Err(error) => {
                remove_quiet(dest);
                return Err(error);
            }
        };
        if let Some(expected) = size {
            if expected != bytes {
                last = Some(OnlineError::integrity(format!(
                    "字节数与声明不符：声明 {expected}，实际 {bytes}"
                )));
                remove_quiet(dest);
                continue;
            }
        }
        let digest = match digest_file(dest) {
            Ok(digest) => digest,
            Err(error) => {
                // 与其它失败路径一致：读不出来就不许把文件留在盘上。
                remove_quiet(dest);
                return Err(error);
            }
        };
        if let Some(expected) = sha1 {
            if !equals_hex(expected, &digest.sha1) {
                last = Some(OnlineError::integrity(format!(
                    "SHA-1 与声明不符（声明 {expected}，实际 {}）",
                    digest.sha1
                )));
                remove_quiet(dest);
                continue;
            }
        }
        if let Some(expected) = sha512 {
            if !equals_hex(expected, &digest.sha512) {
                last = Some(OnlineError::integrity(format!(
                    "SHA-512 与声明不符（声明 {expected}，实际 {}）",
                    digest.sha512
                )));
                remove_quiet(dest);
                continue;
            }
        }
        return Ok(Downloaded {
            path: dest.to_path_buf(),
            bytes,
            digest,
        });
    }
    remove_quiet(dest);
    Err(last.unwrap_or_else(|| {
        OnlineError::new(
            OnlineErrorKind::NotConnected,
            "下载重试用尽（没有拿到任何字节）",
        )
    }))
}

/// 读一个文件同时算 sha1 与 sha512。
///
/// # Errors
/// 读盘失败返回 [`OnlineErrorKind::Io`]。
pub fn digest_file(path: &Path) -> Result<Digest, OnlineError> {
    let mut file = std::fs::File::open(path).map_err(|error| {
        OnlineError::new(
            OnlineErrorKind::Io,
            format!("读 {} 失败：{error}", path.display()),
        )
    })?;
    let mut sha1 = sha1::Sha1::new();
    let mut sha512 = Sha512::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut bytes = 0u64;
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            OnlineError::new(
                OnlineErrorKind::Io,
                format!("读 {} 失败：{error}", path.display()),
            )
        })?;
        if read == 0 {
            break;
        }
        bytes += read as u64;
        sha1.update(&buffer[..read]);
        sha512.update(&buffer[..read]);
    }
    Ok(Digest {
        sha1: to_hex(&sha1.finalize()),
        sha512: to_hex(&sha512.finalize()),
        bytes,
    })
}

/// 摘要形状合法吗（长度 + 十六进制）。
///
/// 这一档摘要能用吗：形状合法就原样返回（不复制、不归一，大小写不敏感比较交给
/// [`equals_hex`]），形状不合法就当「没给」。
///
/// 为什么丢掉而不是报错：`sha1` 与 `sha512` 是同一个文件的两档校验，**任一**都能证明内容；
/// 一档写坏不该让整条装不上（r1 顺手项）。两档都不可用时由 `download_verified` 统一拒绝。
fn usable_hash(raw: Option<&str>, algorithm: HashAlgorithm) -> Option<&str> {
    raw.map(str::trim)
        .filter(|hex| !hex.is_empty())
        .filter(|hex| algorithm.accepts(hex))
}

/// 十六进制比较（大小写不敏感：声明侧可能大写）。
fn equals_hex(expected: &str, actual: &str) -> bool {
    expected.len() == actual.len() && expected.eq_ignore_ascii_case(actual)
}

/// 这个错误值得再试一次吗（网络/站点/内容不符；**404 与自身不自洽一律不重试**）。
fn retryable(kind: OnlineErrorKind) -> bool {
    matches!(
        kind,
        OnlineErrorKind::NotConnected | OnlineErrorKind::Server | OnlineErrorKind::Integrity
    )
}

fn remove_quiet(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            eprintln!("[online] 删 {} 失败：{error}", path.display());
        }
    }
}

/// 小写十六进制（不引 `hex` 依赖；与 `download::verify::to_hex` 同一写法）。
#[must_use]
pub fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}
