//! 下载错误：三态分类（同源重试 / 立刻换源 / 整体失败）+ 失败摘要。
//!
//! core 不引 `thiserror`（P0 阶段少一个依赖），手写 `Display`/`Error` 足够。

use std::fmt;

use serde::{Deserialize, Serialize};

/// 错误分类：决定「同源重试」「立刻换源」还是「整体失败」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadErrorKind {
    /// 请求本身不合法（不重试）。
    InvalidRequest,
    /// 404/410/403：该源没有这个资源，立刻换源。
    NotFound,
    /// 5xx / 429：同源退避重试。
    ServerError,
    /// 超时：同源退避重试，多次后换源。
    Timeout,
    /// 连接/传输层错误。
    Network,
    /// 校验不符：同源重试一次后换源（可能是镜像内容不一致）。
    Integrity,
    /// 服务器不支持 Range（内部用于退化单连接，不向上暴露为失败）。
    RangeUnsupported,
    /// 本地 IO 错误（不重试）。
    Io,
    /// 所有候选源都失败。
    AllSourcesFailed,
}

impl DownloadErrorKind {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::NotFound => "not_found",
            Self::ServerError => "server_error",
            Self::Timeout => "timeout",
            Self::Network => "network",
            Self::Integrity => "integrity",
            Self::RangeUnsupported => "range_unsupported",
            Self::Io => "io",
            Self::AllSourcesFailed => "all_sources_failed",
        }
    }

    /// 是否值得在同源上退避重试。
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::ServerError | Self::Timeout | Self::Network | Self::Integrity
        )
    }
}

/// 下载错误。
#[derive(Debug, Clone)]
pub struct DownloadError {
    /// 分类。
    pub kind: DownloadErrorKind,
    /// 出错的源（整体失败时为 `None`）。
    pub source: Option<String>,
    /// 人类可读描述。
    pub message: String,
    /// 已经尝试的次数（含本次）。
    pub attempts: usize,
    /// 其它候选源的失败摘要，便于诊断「到底哪一层挂了」。
    pub suppressed: Vec<String>,
    /// **叶子成因**：整体失败（[`DownloadErrorKind::AllSourcesFailed`]）之前，各候选源到底
    /// 是怎么挂的。
    ///
    /// 为什么需要它：候选源循环结束后只剩一条 `AllSourcesFailed`，上层（在线内容源、
    /// 导入补全、自更新）拿到它就无法区分「这个项目不存在（404）」与「没联网」——
    /// 前者要提示换关键词，后者要提示检查网络。`suppressed` 里虽然有文字，但从错误串里
    /// 反解状态码是靠文案猜，改一句提示就失效。
    ///
    /// 汇总规则（[`leaf_of`]）：全 404 ⇒ `NotFound`；出现 `Network`/`Timeout` ⇒ `Network`；
    /// 全 5xx ⇒ `ServerError`；混合按「网络错 > 5xx > 404」取最可诊断的那个。
    /// `None` = 没有汇总过（单源直接失败，或调用方自己构造的错误）。
    pub leaf: Option<DownloadErrorKind>,
}

impl DownloadError {
    /// 构造错误。
    #[must_use]
    pub fn new(kind: DownloadErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            source: None,
            message: message.into(),
            attempts: 0,
            suppressed: Vec::new(),
            leaf: None,
        }
    }

    /// 标注出错的源。
    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// 标注尝试次数。
    #[must_use]
    pub fn with_attempts(mut self, attempts: usize) -> Self {
        self.attempts = attempts;
        self
    }

    /// 汇总其它源的失败摘要。
    #[must_use]
    pub fn with_suppressed(mut self, suppressed: Vec<String>) -> Self {
        self.suppressed = suppressed;
        self
    }

    /// 标注整体失败之前的叶子成因。
    #[must_use]
    pub const fn with_leaf(mut self, leaf: DownloadErrorKind) -> Self {
        self.leaf = Some(leaf);
        self
    }

    /// 最可诊断的成因：有叶子就用叶子，否则用自身的 kind。
    ///
    /// 上层（`addon::online::transport` / `addon::import::fetch`）靠这一处判定，不各自
    /// 再写一遍「AllSourcesFailed 到底算哪一类」。
    #[must_use]
    pub fn effective_kind(&self) -> DownloadErrorKind {
        if self.kind == DownloadErrorKind::AllSourcesFailed {
            return self.leaf.unwrap_or(self.kind);
        }
        self.kind
    }
}

/// 从各候选源的失败里汇总出**最可诊断**的叶子成因。
///
/// 规则（与 `online::transport` 的映射表配套，判据只此一处）：
///
/// | 各源的情况 | 叶子 |
/// |------------|------|
/// | 全部 404/410/403（`NotFound`） | `NotFound` |
/// | 出现 `Network` / `Timeout` | `Network` |
/// | 全部 5xx（`ServerError`） | `ServerError` |
/// | 其它（`Integrity` / `Io` / `InvalidRequest`） | 该条自己的 kind（后出现的赢） |
/// | 混合了 404 与 5xx | `ServerError`（比「不存在」更可诊断） |
///
/// `kinds` 为空时返回 `AllSourcesFailed`（调用方据此保留原 kind）。
#[must_use]
pub fn leaf_of(kinds: impl IntoIterator<Item = DownloadErrorKind>) -> DownloadErrorKind {
    let mut leaf = DownloadErrorKind::AllSourcesFailed;
    let mut rank = 0u8;
    for kind in kinds {
        let (candidate, weight) = match kind {
            DownloadErrorKind::Network | DownloadErrorKind::Timeout => {
                (DownloadErrorKind::Network, 4)
            }
            DownloadErrorKind::ServerError => (DownloadErrorKind::ServerError, 3),
            DownloadErrorKind::Integrity => (DownloadErrorKind::Integrity, 3),
            DownloadErrorKind::Io => (DownloadErrorKind::Io, 3),
            DownloadErrorKind::InvalidRequest => (DownloadErrorKind::InvalidRequest, 3),
            DownloadErrorKind::NotFound => (DownloadErrorKind::NotFound, 1),
            DownloadErrorKind::RangeUnsupported | DownloadErrorKind::AllSourcesFailed => (kind, 2),
        };
        if weight >= rank {
            rank = weight;
            leaf = candidate;
        }
    }
    leaf
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.source {
            Some(source) => write!(
                f,
                "[{}] {}（源 {}）",
                self.kind.as_str(),
                self.message,
                source
            ),
            None => write!(f, "[{}] {}", self.kind.as_str(), self.message),
        }
    }
}

impl std::error::Error for DownloadError {}

#[cfg(test)]
mod tests {
    use super::{leaf_of, DownloadError, DownloadErrorKind};

    /// 四档叶子成因（tower 定案的门禁）：全 404 / 有网络错 / 全 5xx / 混合。
    #[test]
    fn leaf_keeps_the_most_diagnosable_cause() {
        use DownloadErrorKind::{Network, NotFound, ServerError, Timeout};
        assert_eq!(leaf_of([NotFound, NotFound]), NotFound, "全 404 ⇒ NotFound");
        assert_eq!(
            leaf_of([Network, NotFound]),
            Network,
            "有网络错 ⇒ Network（哪怕另一源是 404）"
        );
        assert_eq!(
            leaf_of([ServerError, ServerError]),
            ServerError,
            "全 5xx ⇒ ServerError"
        );
        assert_eq!(
            leaf_of([NotFound, ServerError]),
            ServerError,
            "混合 404 与 5xx ⇒ 取更可诊断的那个"
        );
        assert_eq!(leaf_of([Timeout, NotFound]), Network, "超时归到 Network");
        assert_eq!(
            leaf_of([]),
            DownloadErrorKind::AllSourcesFailed,
            "一条都没有 ⇒ 保留原 kind"
        );
    }

    /// 整体失败时上层靠 `effective_kind()` 判定；单源失败时就是它自己的 kind。
    #[test]
    fn effective_kind_prefers_the_leaf_only_for_all_sources_failed() {
        let leaf = DownloadError::new(DownloadErrorKind::AllSourcesFailed, "全部失败")
            .with_leaf(DownloadErrorKind::NotFound);
        assert_eq!(leaf.effective_kind(), DownloadErrorKind::NotFound);
        assert_eq!(
            leaf.kind,
            DownloadErrorKind::AllSourcesFailed,
            "kind 本身不改"
        );

        let bare = DownloadError::new(DownloadErrorKind::AllSourcesFailed, "全部失败");
        assert_eq!(
            bare.effective_kind(),
            DownloadErrorKind::AllSourcesFailed,
            "没有叶子 ⇒ 原样返回（上层再决定当哪一类）"
        );

        let single = DownloadError::new(DownloadErrorKind::NotFound, "404");
        assert_eq!(single.effective_kind(), DownloadErrorKind::NotFound);
    }
}
