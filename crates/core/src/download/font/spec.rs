//! 「下什么、放哪」：字体 pin 的**值**表示（[`FontSpec`]）与缓存落点（[`cache_path`]）。
//!
//! 从 `font.rs` 拆出只为守 400 行硬线（M168 复审 r1 加了整体时限与 URL 口径之后越线）。
//! 判据（哪些 URL 可用）、复验、落盘编排都在 [`super`] 那边；这里只有数据结构与路径规则。

use std::path::{Path, PathBuf};

use super::{FontFetchError, MAX_FONT_BYTES};

/// 一套字体的「在哪拿、拿到什么」：URL 候选（首选 + 备用镜像）+ 期望 sha256 + 期望字节数。
///
/// 与 `unlucky_i18n::font::Source::Pinned` 是同一份事实的两种表示：i18n 那份是**常量**（零依赖
/// crate 里写死），本结构是**值**（可被设置改写、可在测试里注入假 URL）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontSpec {
    /// URL 候选，按顺序尝试（首选 CDN 挂了才试备用镜像）。
    pub urls: Vec<String>,
    /// 期望 sha256（小写十六进制、64 位）。
    pub sha256: String,
    /// 期望字节数（进度预算 + 大小上限的判据）。
    pub bytes: u64,
}

impl FontSpec {
    /// 构造并做基本自洽检查（URL 非空 / 哈希形态 / 大小上限）。
    ///
    /// # Errors
    /// pin 本身不合法时返回 [`FontFetchError::Integrity`]（**不发请求**）。
    pub fn new(
        urls: impl IntoIterator<Item = impl Into<String>>,
        sha256: impl Into<String>,
        bytes: u64,
    ) -> Result<Self, FontFetchError> {
        let spec = Self {
            urls: urls.into_iter().map(Into::into).collect(),
            sha256: sha256.into().trim().to_ascii_lowercase(),
            bytes,
        };
        spec.check()?;
        Ok(spec)
    }

    /// pin 是否自洽（`super::fetch_blocking*` 与构造器都调它，故 `pub(super)`）。
    pub(super) fn check(&self) -> Result<(), FontFetchError> {
        if self.urls.is_empty() {
            return Err(FontFetchError::Integrity("字体 pin 没有 URL".to_owned()));
        }
        if let Some(bad) = self.urls.iter().find(|url| !is_acceptable_url(url)) {
            return Err(FontFetchError::Integrity(format!(
                "字体 pin 只接受 https（http 仅限回环地址，给单测用）：{bad}"
            )));
        }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(FontFetchError::Integrity(format!(
                "字体 pin 的 sha256 不是 64 位十六进制：{}",
                self.sha256
            )));
        }
        if self.bytes == 0 || self.bytes > MAX_FONT_BYTES {
            return Err(FontFetchError::Integrity(format!(
                "字体 pin 的字节数 {} 不在 (0, {MAX_FONT_BYTES}] 内",
                self.bytes
            )));
        }
        Ok(())
    }

    /// 缓存文件名的哈希段（前 16 位：够区分，又不至于让路径长得没法看）。
    #[must_use]
    fn cache_key(&self) -> &str {
        let end = self.sha256.len().min(16);
        &self.sha256[..end]
    }
}

/// URL 是否可用：**https 一律可以；http 只给回环地址**（单测的回环 HTTP 服务要用）。
///
/// 为什么这条要写进校验而不是只写进注释：字体是从网络下来、直接注册进渲染器的东西。明文 http
/// 在 sha256 钉死的前提下不会导致「内容被换」，但会把「从哪拿」暴露给路上的任何人（可被替换成
/// 任意大文件来耗流量/磁盘，虽然会被大小上限与哈希挡住）。契约写成「非回环必须 https」，
/// 注释与实现就是一个口径。
fn is_acceptable_url(url: &str) -> bool {
    if url.starts_with("https://") {
        return true;
    }
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let host_port = rest.split('/').next().unwrap_or_default();
    let host = host_port
        .rsplit_once(':')
        .filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit()))
        .map_or(host_port, |(host, _)| host);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    matches!(host, "127.0.0.1" | "localhost" | "::1") || host.starts_with("127.")
}

/// 缓存文件落点：`<数据根>/fonts/cjk-noto-sans-sc-<sha256 前 16 位>.ttf`。
///
/// 文件名带哈希段是刻意的：pin 一改（换字体 / 换子集），新文件自然换名，旧文件不会冒充命中。
#[must_use]
pub fn cache_path(cache_root: &Path, spec: &FontSpec) -> PathBuf {
    cache_root
        .join("fonts")
        .join(format!("cjk-noto-sans-sc-{}.ttf", spec.cache_key()))
}
