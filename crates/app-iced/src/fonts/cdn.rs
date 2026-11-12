//! 真实字体下载器（M168）：把 `unlucky_i18n::font` 的 pin 接到 `unlucky_core::download`。
//!
//! ## 一句话
//!
//! ```text
//! i18n 的 CjkFont.source（常量：URL 候选 + sha256 + 字节数）
//!         ↓ 允许设置改写首选 URL（`ui.font_cdn`）
//! core 的 FontSpec（值）
//!         ↓ core::download::font::fetch_blocking（缓存 → 共享引擎 → 复验 → 原子落盘）
//! Vec<u8>（交给 install 注册进 iced 的字体系统）
//! ```
//!
//! 本模块只做三件适配：常量 → 值、缓存根从哪来（[`crate::platform::data`]）、错误类别翻译。
//! 判据与落盘都不在这里——那两半各有自己的守卫（`i18n::font` 的单测、`core::download::font` 的
//! 单测）。
//!
//! ## 「不联网」的三条路
//!
//! | 情形 | 谁拦住 |
//! |---|---|
//! | 英文环境 / 系统已有字体 | `i18n::font::ensure` 根本不调 [`Fetcher`] |
//! | 同一进程第二次（缓存已在盘上） | `core::download::font` 的缓存分支（零 HTTP） |
//! | 二次启动 | 同上：缓存文件按 `<sha256 前 16 位>` 命名，命中即不联网 |
//!
//! ## 设置改写只换 URL，不换哈希
//!
//! `ui.font_cdn` 允许换成国内镜像（整条 URL）。**哈希与字节数仍是内置那一份**：镜像是同一份内容
//! 的不同出口，给了别的内容就该被拒——宁可不下载，也不把没校验过的字节注册成字体。

use std::cell::Cell;
use std::path::PathBuf;

use unlucky_core::download::font::{self as core_font, FontFetchError, FontSpec};
use unlucky_i18n::font::{CjkFont, FetchError, Source};

use crate::platform::data;

/// 字体下载器（一个进程用一份就够：无状态，除了缓存根）。
#[derive(Debug, Clone)]
pub struct CdnFetcher {
    cache_root: PathBuf,
    spec: FontSpec,
    /// 最近一次 `fetch` 是否**真的**命中缓存（`None` = 还没取过）。
    ///
    /// 为什么不在调用方「取之前看一眼文件在不在」：那个判据会说谎——缓存文件若已损坏，
    /// `core::download::font` 会删掉重下（零网络不成立），而「取之前在不在」仍然是 true。
    /// 复审 r1 次要项点名的就是这个：日志 `from_cache=true` 必须等于「这次没联网」。
    last_from_cache: Cell<Option<bool>>,
}

impl CdnFetcher {
    /// 按 pin 建一个下载器（`cache_root` 由调用方给，便于测试与截图通道用临时根）。
    ///
    /// # Errors
    /// pin 与覆盖设置拼不出自洽的 spec 时返回人话错误（不发请求）。
    pub fn new(font: &CjkFont, cache_root: PathBuf) -> Result<Self, FetchError> {
        Ok(Self {
            cache_root,
            spec: spec_of(font)?,
            last_from_cache: Cell::new(None),
        })
    }

    /// 缓存文件是不是已经在盘上（只 stat，不做校验）。
    ///
    /// 用途只有**诊断**（`[font] cjk-download start … cache_present=`）。它**不是**命中判据：
    /// 真正的命中判定在 `core::download::font`（读出来 + 复验哈希与魔数），而「这次有没有联网」
    /// 只看 [`Self::was_cached`]——坏缓存会被删掉重下，那时这个函数仍然会返回 `true`。
    #[must_use]
    pub fn has_cached(&self) -> bool {
        core_font::cache_path(&self.cache_root, &self.spec).is_file()
    }

    /// 最近一次 [`Fetcher::fetch`] 是否零网络命中缓存（`None` = 还没取过）。
    ///
    /// 状态行与日志的 `from_cache` 只认这个值；`has_cached()` 只是「文件在不在」，两者不可互换。
    #[must_use]
    pub fn was_cached(&self) -> Option<bool> {
        self.last_from_cache.get()
    }

    /// 缓存文件落点（诊断日志用）。
    #[must_use]
    pub fn cache_file(&self) -> PathBuf {
        core_font::cache_path(&self.cache_root, &self.spec)
    }
}

impl unlucky_i18n::font::Fetcher for CdnFetcher {
    fn fetch(&self, _font: &'static CjkFont) -> Result<Vec<u8>, FetchError> {
        let outcome = core_font::fetch_blocking(&self.cache_root, &self.spec).map_err(translate);
        self.last_from_cache
            .set(outcome.as_ref().ok().map(|fetch| fetch.from_cache));
        outcome.map(|fetch| fetch.bytes)
    }
}

/// i18n 的常量 pin + 设置覆盖 → core 的 `FontSpec`。
///
/// # Errors
/// 覆盖值不是 http(s) URL、或 pin 自身不自洽时返回 [`FetchError::Integrity`]。
pub fn spec_of(font: &CjkFont) -> Result<FontSpec, FetchError> {
    let Source::Pinned {
        urls,
        sha256,
        bytes,
    } = font.source;

    let mut candidates: Vec<String> = urls.iter().map(|url| (*url).to_owned()).collect();
    if let Some(override_url) = data::font_cdn_override() {
        // 覆盖值排在最前（用户显式选了它），内置候选留作后备：覆盖值挂了还能回落到默认镜像。
        candidates.insert(0, override_url);
    }

    FontSpec::new(candidates, sha256, bytes)
        .map_err(|error| FetchError::Integrity(error.to_string()))
}

/// core 的三类错误 → i18n 的三类（字符串原样带过去，日志里能看到根因）。
fn translate(error: FontFetchError) -> FetchError {
    match error {
        FontFetchError::Network(message) => FetchError::Network(message),
        FontFetchError::Integrity(message) => FetchError::Integrity(message),
        FontFetchError::Disk(message) => FetchError::Disk(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unlucky_i18n::font::NOTO_SANS_SC;

    /// pin → spec 的搬运：URL 候选、哈希、字节数一个都不许掉。
    ///
    /// 抓得住：①漏搬 `sha256`（下载器就会「只要 200 就落盘」）；②把候选列表截成一条（备用镜像
    /// 静默消失）；③字节数被写成 0（大小上限判据失效）。
    /// 抓不住：`ui.font_cdn` 覆盖值在真机上读到什么（那要数据根 + 配置文件；见 `platform::data`）。
    #[test]
    fn the_pin_survives_the_trip_to_core() {
        let spec = spec_of(&NOTO_SANS_SC).expect("内置 pin 应自洽");
        let Source::Pinned {
            urls,
            sha256,
            bytes,
        } = NOTO_SANS_SC.source;

        assert!(
            spec.urls.len() >= urls.len(),
            "候选不该变少：{:?}",
            spec.urls
        );
        assert_eq!(spec.sha256, sha256);
        assert_eq!(spec.bytes, bytes);
        assert_eq!(
            spec.urls[spec.urls.len() - urls.len()],
            urls[0],
            "内置首选不许被挤掉"
        );
    }

    /// 缓存命中判定只看文件在不在（**不联网**）：一个刚建的空目录 ⇒ `has_cached() == false`。
    #[test]
    fn an_empty_cache_root_is_not_a_hit() {
        let dir = std::env::temp_dir().join(format!("ull-cdn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let fetcher = CdnFetcher::new(&NOTO_SANS_SC, dir.clone()).expect("下载器");
        assert!(!fetcher.has_cached());
        assert_eq!(fetcher.was_cached(), None, "还没取过 ⇒ 不知道");
        assert!(fetcher
            .cache_file()
            .ends_with("cjk-noto-sans-sc-a3041811a78c361b.ttf"));
    }
}
