//! 断点续传的**保守判定**。
//!
//! 承 analyse/03 §5.4 与设计文档 §4 最后一行：「拒绝『能续传但会坏』场景」是 HMCL 做对的事。
//! 我们把它写成纯函数，判定条件全部显式：
//!
//! 1. 策略不是 `Never`；
//! 2. 中间文件存在且长度 > 0；
//! 3. 有上一次的元数据（URL、摘要、总长、校验器）；
//! 4. URL 与期望摘要都没变；
//! 5. 总长已知且 `0 < 已下 < 总长`；
//! 6. 有 ETag 或 Last-Modified 作为校验器（否则服务器换了内容我们也发现不了）。
//!
//! 任何一条不满足就**从头下**——重下 100MB 也比交付一个坏文件便宜。
//! 另外单独处理 416（`Requested Range Not Satisfiable`）：已下满则直接进入校验，
//! 否则丢弃中间文件重来。

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::atomic::write_atomic;
use super::types::{HashAlgo, ResumePolicy};

/// 上一次下载留下的续传元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeMeta {
    /// 当时请求的 URL（镜像可能换掉，URL 变了就不续传）。
    pub url: String,
    /// 响应 `ETag`。
    pub etag: Option<String>,
    /// 响应 `Last-Modified`。
    pub last_modified: Option<String>,
    /// 响应告知的总长度。
    pub total: Option<u64>,
    /// 摘要算法。
    pub algo: HashAlgo,
    /// 期望摘要。
    pub hash: String,
    /// 保存时间（unix 毫秒）。
    pub saved_at_ms: u64,
}

impl ResumeMeta {
    /// 从磁盘读（不存在/损坏 → `None`）。
    #[must_use]
    pub fn load(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// 原子写。
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        write_atomic(path, &json)
    }

    /// 删除（校验通过或放弃续传时）。
    pub fn remove(path: &Path) {
        let _ = std::fs::remove_file(path);
    }

    /// 本记录可用的校验器。
    #[must_use]
    pub fn validator(&self) -> Validator {
        Validator {
            etag: self.etag.clone(),
            last_modified: self.last_modified.clone(),
        }
    }
}

/// 续传用的校验器（`If-Range` 头的取值来源）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validator {
    /// 强校验器。
    pub etag: Option<String>,
    /// 弱校验器。
    pub last_modified: Option<String>,
}

impl Validator {
    /// `If-Range` 头的值（ETag 优先，其次 Last-Modified）。
    #[must_use]
    pub fn if_range_value(&self) -> Option<&str> {
        self.etag.as_deref().or(self.last_modified.as_deref())
    }
}

/// 不复续传的原因（日志与诊断页用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartReason {
    /// 策略要求从不续传。
    PolicyNever,
    /// 没有中间文件。
    NoPart,
    /// 中间文件是 0 字节。
    EmptyPart,
    /// 没有元数据（无法证明中间文件是同一次下载的产物）。
    NoMeta,
    /// URL 变了（换源后偏移不可信）。
    UrlChanged,
    /// 期望摘要变了（目标文件已经不是同一个东西）。
    HashChanged,
    /// 总长未知（无法判断是否已下满）。
    TotalUnknown,
    /// 已下字节超过总长（中间文件被污染）。
    Oversize,
    /// 没有 ETag/Last-Modified，无法证明远端内容没变。
    NoValidator,
    /// 服务器 416 且未下满。
    RangeUnsatisfiable,
}

impl RestartReason {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PolicyNever => "policy_never",
            Self::NoPart => "no_part",
            Self::EmptyPart => "empty_part",
            Self::NoMeta => "no_meta",
            Self::UrlChanged => "url_changed",
            Self::HashChanged => "hash_changed",
            Self::TotalUnknown => "total_unknown",
            Self::Oversize => "oversize",
            Self::NoValidator => "no_validator",
            Self::RangeUnsatisfiable => "range_unsatisfiable",
        }
    }
}

/// 续传决策。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeDecision {
    /// 可以续传：从 `offset` 起发 `Range` + `If-Range`。
    Resume {
        /// 已下字节数。
        offset: u64,
        /// 校验器。
        validator: Validator,
    },
    /// 从头下载。
    Restart {
        /// 原因。
        reason: RestartReason,
    },
    /// 中间文件已经是完整长度：跳过传输，直接校验。
    Complete {
        /// 文件长度。
        size: u64,
    },
}

impl ResumeDecision {
    /// 是否要丢掉中间文件（`Complete` 不能丢）。
    #[must_use]
    pub const fn should_drop_part(&self) -> bool {
        matches!(self, Self::Restart { .. })
    }
}

/// 判定是否续传（纯函数）。
#[must_use]
pub fn decide(
    policy: ResumePolicy,
    part_len: u64,
    meta: Option<&ResumeMeta>,
    url: &str,
    integrity: Option<(HashAlgo, &str)>,
    expected_size: Option<u64>,
) -> ResumeDecision {
    if policy == ResumePolicy::Never {
        return ResumeDecision::Restart {
            reason: RestartReason::PolicyNever,
        };
    }
    if part_len == 0 {
        return ResumeDecision::Restart {
            reason: RestartReason::EmptyPart,
        };
    }
    let Some(meta) = meta else {
        return ResumeDecision::Restart {
            reason: RestartReason::NoMeta,
        };
    };
    if meta.url != url {
        return ResumeDecision::Restart {
            reason: RestartReason::UrlChanged,
        };
    }
    if let Some((algo, hash)) = integrity {
        if meta.algo != algo || !meta.hash.eq_ignore_ascii_case(hash) {
            return ResumeDecision::Restart {
                reason: RestartReason::HashChanged,
            };
        }
    }
    let Some(total) = meta.total.or(expected_size) else {
        return ResumeDecision::Restart {
            reason: RestartReason::TotalUnknown,
        };
    };
    if part_len > total {
        return ResumeDecision::Restart {
            reason: RestartReason::Oversize,
        };
    }
    if part_len == total {
        return ResumeDecision::Complete { size: total };
    }
    let validator = meta.validator();
    if validator.if_range_value().is_none() {
        return ResumeDecision::Restart {
            reason: RestartReason::NoValidator,
        };
    }
    ResumeDecision::Resume {
        offset: part_len,
        validator,
    }
}

/// 收到 416 后的决策。
#[must_use]
pub fn on_range_unsatisfiable(part_len: u64, total: Option<u64>) -> ResumeDecision {
    match total {
        Some(total) if part_len == total => ResumeDecision::Complete { size: total },
        _ => ResumeDecision::Restart {
            reason: RestartReason::RangeUnsatisfiable,
        },
    }
}
