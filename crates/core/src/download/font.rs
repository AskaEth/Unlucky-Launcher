//! OFL 中文字体的取回（D28 后半）：**缓存命中 → 共享引擎下载 → 复验 → 原子落盘**。
//!
//! ## 一段话
//!
//! `unlucky_i18n::font` 只做判据与「该下载什么」（URL 候选 + sha256 + 字节数），真正把字节弄到
//! 盘上的活在这里：
//!
//! ```text
//! <数据根>/fonts/cjk-noto-sans-sc-<sha256 前 16 位>.ttf
//!        已有且哈希 + 魔数都对 ⇒ 直接返回（零网络）
//!        否则 ⇒ 共享引擎的 download_bytes（进内存）
//!             ⇒ 自己再算一遍 sha256、再看一眼是不是 sfnt、再看大小上限
//!             ⇒ 全过才 write_atomic 落盘 ⇒ 返回字节
//! ```
//!
//! ## 为什么用「内存下载」而不是「边下边写文件」
//!
//! 一张 17 MiB 的字体本来就要整份进内存（注册进字体系统要 `Vec<u8>`），而**先复验再落盘**让
//! 「哈希不符」这条判据简单到不可能出错：盘上要么没有这个文件，要么它是校验过的。文件下载路径
//! （`DownloadEngine::download`）会先写 `<dest>.ullpart-*` 再换名，中间态与「拒绝落盘」混在一起，
//! 对这一份内容寻址的小资源没有收益。
//!
//! ## 三条判据（都有守卫，见 `tests.rs`）
//!
//! 1. **缓存命中不联网**：命中就 `from_cache`，一条 HTTP 都不发（同进程第二次、二次启动都一样）；
//!    哈希不符的旧文件会被删掉重下（自愈），不冒充命中、也不永远挡路。
//! 2. **sha256 不符拒绝落盘**：复验在任何写盘动作**之前**；不符 ⇒ 正式路径上什么都没有。
//! 3. **大小上限**：[`MAX_FONT_BYTES`]。pin 声称超过上限的直接拒（不发请求）；实得字节超上限的
//!    也拒。挡的是「CDN 被换成一个大文件」这类事故，不是省流量。
//!
//! ## 失败分类为什么是三类
//!
//! 上层（设置页那一行）只需决定「说什么话」：网络类可重试、校验类是内容不对、磁盘类得腾空间。
//! 再多分类对用户没有区别。
//!
//! ## 与 M165 共享引擎的关系
//!
//! **不另写 HTTP 通路**：请求一律走 [`super::shared::engine_for`] 那个进程级引擎（连接池 / 限流 /
//! 健康度 / 候选源都复用）。本模块只额外提供「字体这个资源特有的三件事」：内容寻址的缓存文件名、
//! 复验、大小上限。

use std::path::Path;
use std::time::{Duration, Instant};

use super::atomic::write_atomic;
use super::blocking::block_on;
use super::error::DownloadErrorKind;
use super::shared;
use super::types::{DownloadRequest, HashAlgo, ResourceKind};
use super::verify;

/// 一套字体允许的最大字节数（32 MiB）。
///
/// 实测参考：Noto Sans SC 可变字体（wght 100–900，整个简体字符集）16.95 MiB；再大的「字体」基本
/// 可以判断不是给 UI 用的字体。上限的用处不是省流量，是**挡住被换掉的 CDN 把几十上百 MB 的东西
/// 写进数据根**。
pub const MAX_FONT_BYTES: u64 = 32 * 1024 * 1024;

/// **非最后一个候选**的单次尝试时限（不是连接超时，也不是空闲超时）。
///
/// 为什么必须有：引擎的 `read_timeout` 是 30 s **空闲**超时，一条每秒只挤几 KB、但一直在动的
/// 连接永远不会被判失败。复审实测过这条路径（同一台机器、同一份文件）：
///
/// | 路径 | 结果 |
/// |---|---|
/// | Fastly 的 IPv6 边缘 | 60 s 只到 4.0 MB（≈67 KB/s，17.7 MB 要 ≈4 分钟） |
/// | 同一域名的 IPv4 边缘 | 1.2 s 下完（≈15 MB/s） |
///
/// 60 s 是「明显不对劲就换源」的位置：健康线路（实测 1.1~16.6 s）根本用不到它。
///
/// ## 为什么**最后一个**候选不吃这个时限（复审 r2 P2-3 / r3 P2-3）
///
/// 每个候选都从 0 字节重下（不做续传），所以「单次上限」直接就是一条**吞吐门槛**：
/// `17 772 300 B / 预算`。历史两版都踩在这里：90 s ⇒ 门槛 197 KB/s；120 s + 总 360 s ⇒
/// 第一个候选吃掉 120 s 后最后一个只剩 240 s ⇒ 门槛 **72.3 KiB/s**，而 r1 实测那条 IPv6 是
/// 67~77 KiB/s——**67 KiB/s 需要 259 s > 240 s，仍然必失败**（复审 r3 就是这么把上一版的
/// 「49 KB/s」拆穿的：那个数是「第一个候选瞬间失败」的情形，不是这条改动要救的场景）。
///
/// 现在的规则（见 [`fetch_blocking_with_limits`]）：前 n-1 个候选各拿
/// `min(ATTEMPT_DEADLINE, 剩余)`，**最后一个候选拿整个剩余总预算**。
///
/// ## 三个数的完整算术（2 个候选的常见情形）
///
/// | 情形 | 最后一个候选拿到的预算 | 门槛 = 17 772 300 B / 预算 |
/// |---|---|---|
/// | 第一个候选瞬间失败 | 420 s | **42.3 KiB/s** |
/// | 第一个候选爬满 60 s 被切 | 360 s | **49.4 KiB/s** |
///
/// 两者都低于 r1 实测最慢的 67 KiB/s（需要 259 s），于是「慢而当当能下完」的线路仍有**一次**
/// 老老实实下完的机会。**超时归入网络档**（用户视角是「太慢」，不是「内容不对」）。
pub const ATTEMPT_DEADLINE: Duration = Duration::from_secs(60);

/// 整条取回（所有候选加起来）的总时限，也是**最后一个候选**能拿到的预算上限。
///
/// 取值依据见 [`ATTEMPT_DEADLINE`] 的算术表：420 s 让「第一个候选爬满 60 s」这种最差情形
/// 仍给最后一个候选 360 s（49.4 KiB/s 门槛）。
///
/// 代价：最坏情形下用户要在设置页看着「正在获取中文字体…（16.9 MB）」等 7 分钟。这是刻意的
/// 取舍——**慢而最终成功**远好过**快而必然失败**（后者要用户自己再来一次，结果一样）。
pub const TOTAL_DEADLINE: Duration = Duration::from_secs(420);

/// 字体取回的失败分类（三层，见模块文档「失败分类为什么是三类」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontFetchError {
    /// 网络层：连不上 / 超时 / 非 2xx / 所有候选源都失败。
    Network(String),
    /// 内容层：sha256 不符 / 不是字体文件 / 超出大小上限。
    Integrity(String),
    /// 本地磁盘层：建目录、写文件、换名失败。
    Disk(String),
}

impl FontFetchError {
    /// 稳定类别名（`network` / `integrity` / `disk`；日志与 i18n 金表按它选词，**不要改名**）。
    #[must_use]
    pub const fn class(&self) -> &'static str {
        match self {
            Self::Network(_) => "network",
            Self::Integrity(_) => "integrity",
            Self::Disk(_) => "disk",
        }
    }

    /// 人类可读描述（进日志的一句）。
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Network(message) | Self::Integrity(message) | Self::Disk(message) => message,
        }
    }
}

impl std::fmt::Display for FontFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.class(), self.message())
    }
}

impl std::error::Error for FontFetchError {}

impl From<super::error::DownloadError> for FontFetchError {
    fn from(error: super::error::DownloadError) -> Self {
        // 用叶子成因：`download_bytes` 的整体失败会带上它（全 404 → NotFound，出现网络错 → Network，
        // 校验不符 → Integrity）。少了叶子这一层，哈希不符会被读成「没联网」。
        let message = error.to_string();
        match error.effective_kind() {
            DownloadErrorKind::Io => Self::Disk(message),
            DownloadErrorKind::Integrity | DownloadErrorKind::InvalidRequest => {
                Self::Integrity(message)
            }
            _ => Self::Network(message),
        }
    }
}

/// 一次取回的产物。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFetch {
    /// 字体字节（交给调用方注册进字体系统）。
    pub bytes: Vec<u8>,
    /// 是否零网络命中缓存。
    pub from_cache: bool,
    /// 成功的源标识（`cache` / 引擎给的候选源名）。
    pub source: String,
    /// 耗时。
    pub elapsed: Duration,
}

/// 取一套字体（**阻塞**：内部走进程共享运行时）。
///
/// 顺序：pin 自洽检查 → 缓存复验 → 逐个 URL 走共享引擎 → 复验 + 魔数 + 上限 → 原子落盘。
///
/// # Errors
/// 见 [`FontFetchError`]。失败**不是**致命错误：调用方应当降级回系统字体 / 英文界面。
pub fn fetch_blocking(cache_root: &Path, spec: &FontSpec) -> Result<FontFetch, FontFetchError> {
    fetch_blocking_with_limits(cache_root, spec, ATTEMPT_DEADLINE, TOTAL_DEADLINE)
}

/// 同 [`fetch_blocking`]，但两个时限由调用方给（单测用它把 60 s / 420 s 压成几百毫秒）。
///
/// 为什么把时限做成参数而不是读环境变量：这是**行为**判据，不是诊断开关——测试要能证明
/// 「一条只爬不死的连接会在时限处被打断并换源」，而 90 s 的用例没人会跑。
///
/// # Errors
/// 同 [`fetch_blocking`]；超时归 [`FontFetchError::Network`]（文案里带「超时」二字）。
pub fn fetch_blocking_with_limits(
    cache_root: &Path,
    spec: &FontSpec,
    attempt_deadline: Duration,
    total_deadline: Duration,
) -> Result<FontFetch, FontFetchError> {
    let started = Instant::now();
    spec.check()?;

    let final_path = cache_path(cache_root, spec);
    if let Some(bytes) = read_verified(&final_path, spec)? {
        return Ok(FontFetch {
            bytes,
            from_cache: true,
            source: "cache".to_owned(),
            elapsed: started.elapsed(),
        });
    }

    let engine = shared::engine_for(cache_root);
    let mut network_error: Option<FontFetchError> = None;
    let mut integrity_error: Option<FontFetchError> = None;

    for (index, url) in spec.urls.iter().enumerate() {
        // 总时限先看一眼：已经用完了就不必再开一条注定超时的连接。
        let remaining = total_deadline.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            network_error = Some(FontFetchError::Network(format!(
                "取字体超过总时限 {} s（已试 {index} / {} 个候选；这是「太慢」，不是校验失败）",
                total_deadline.as_secs(),
                spec.urls.len()
            )));
            break;
        }
        // 前 n-1 个候选：`min(单次上限, 剩余)`；**最后一个候选**拿整个剩余总预算。
        // 为什么对最后一个放宽：每个候选都从 0 字节重下，「单次上限」就是一条吞吐门槛，
        // 卡死最后一个等于把「慢但能下完」判成永久失败（复审 r2 P2-3 的算术，见 ATTEMPT_DEADLINE）。
        let is_last = index + 1 == spec.urls.len();
        let budget = if is_last {
            remaining
        } else {
            attempt_deadline.min(remaining)
        };
        let request = DownloadRequest::new(ResourceKind::Other, url.clone(), &final_path)
            .with_sha256(&spec.sha256)
            .with_size(spec.bytes);

        let attempt = block_on(async {
            // `tokio::time::timeout` 必须在**运行时上下文里**构造（它一创建就要注册计时器；
            // 在外面构造会 panic「there is no reactor running」）。所以整段包进 async 块。
            tokio::time::timeout(budget, engine.download_bytes(request)).await
        });
        let outcome = match attempt {
            Ok(outcome) => outcome,
            Err(_elapsed) => {
                network_error = Some(FontFetchError::Network(format!(
                    "下载字体超时（单次上限 {} ms，总上限 {} ms）：{url}",
                    budget.as_millis(),
                    total_deadline.as_millis()
                )));
                continue;
            }
        };

        match outcome {
            Ok(bytes) => {
                if bytes.len() as u64 > MAX_FONT_BYTES {
                    integrity_error = Some(FontFetchError::Integrity(format!(
                        "字体超过大小上限：{} > {MAX_FONT_BYTES}",
                        bytes.len()
                    )));
                    continue;
                }
                // 引擎的流式校验与这一道是**两处独立**的判据：这里同时看哈希与魔数，且都在写盘之前。
                let actual = verify::digest_bytes(HashAlgo::Sha256, &bytes);
                if !verify::matches(&spec.sha256, &actual) {
                    integrity_error = Some(FontFetchError::Integrity(format!(
                        "字体 sha256 不符：期望 {} 实得 {actual}（{url}）",
                        spec.sha256
                    )));
                    continue;
                }
                if !looks_like_sfnt(&bytes) {
                    integrity_error = Some(FontFetchError::Integrity(format!(
                        "取回的不是字体文件（sfnt 魔数不符）：{url}"
                    )));
                    continue;
                }
                write_atomic(&final_path, &bytes).map_err(|error| {
                    FontFetchError::Disk(format!(
                        "字体落盘失败（{}）：{error}",
                        final_path.display()
                    ))
                })?;
                return Ok(FontFetch {
                    bytes,
                    from_cache: false,
                    source: url.clone(),
                    elapsed: started.elapsed(),
                });
            }
            Err(error) => {
                let mapped = FontFetchError::from(error);
                match mapped {
                    FontFetchError::Integrity(_) => integrity_error = Some(mapped),
                    _ => network_error = Some(mapped),
                }
            }
        }
    }

    // 报错时优先报「内容不对」：那说明源给了不该给的东西，比「连不上」更需要人看一眼。
    Err(integrity_error
        .or(network_error)
        .unwrap_or_else(|| FontFetchError::Network("字体 pin 没有可用 URL".to_owned())))
}

/// 读缓存并复验（`Ok(None)` = 不存在或校验不过，**且坏文件已被删掉**）。
fn read_verified(path: &Path, spec: &FontSpec) -> Result<Option<Vec<u8>>, FontFetchError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(FontFetchError::Disk(format!(
                "读字体缓存失败（{}）：{error}",
                path.display()
            )))
        }
    };

    let actual = verify::digest_bytes(HashAlgo::Sha256, &bytes);
    if !verify::matches(&spec.sha256, &actual) || !looks_like_sfnt(&bytes) {
        // 坏缓存自愈：删掉，交给下载路径重来（不把坏文件当命中，也不让它一直挡路）。
        let _ = std::fs::remove_file(path);
        return Ok(None);
    }
    Ok(Some(bytes))
}

/// 是不是一个 sfnt 字体文件（TTF / OTF / TTC）。
///
/// 只认魔数 + 表目录读得出来：够挡住「CDN 回了一个 HTML 错误页」这类事故，**不**做字形覆盖校验
/// （那要 `ttf-parser`，core 的依赖纪律不允许为一个判据引它）。
#[must_use]
pub fn looks_like_sfnt(bytes: &[u8]) -> bool {
    if bytes.len() < 12 {
        return false;
    }
    let tag = &bytes[..4];
    if tag == b"ttcf" {
        // 字体集合：只要长度说得过去就收（成员表的细节不值得在这里解析）。
        return bytes.len() >= 16;
    }
    if !matches!(tag, b"\x00\x01\x00\x00" | b"OTTO" | b"true") {
        return false;
    }
    let tables = u16::from_be_bytes([bytes[4], bytes[5]]);
    tables > 0 && bytes.len() >= 12 + usize::from(tables) * 16
}

mod spec;

pub use spec::{cache_path, FontSpec};

#[cfg(test)]
#[path = "font/tests.rs"]
mod tests;
