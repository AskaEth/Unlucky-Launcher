//! 下载引擎：多源健康度、指数退避、分层下载（大文件分块）、边下边算、原子落盘。
//!
//! 设计定稿：`DevDocs/PlanDoc/05-download-and-mirrors.md`（决策 D7）。
//! 镜像清单随更新可刷新（BMCLAPI / TUNA / USTC / SJTU / gh-proxy），
//! 地区自动判定 = 时区 + 语言 + 网络探测组合信号，可手动覆盖。
//!
//! 与其它模块的契约：`DownloadRequest` / 进度事件的类型需与 game、agent 对齐
//! （`DevDocs/SubAgentsTalk/README.md` 的对齐点）。
//!
//! # 公共面
//!
//! 调用方只需要三件套：构造 [`DownloadRequest`] → [`DownloadEngine::download`] →
//! 消费 [`ProgressEvent`]。「用哪个镜像、要不要续传、开几条连接、失败重试几次」
//! 全部由引擎内部决定，调用方既不拼镜像 URL 也不写重试循环。
//!
//! 其余构件（镜像表 / 地区档案 / 健康度 / 内容寻址缓存 / 续传元数据）都作为
//! 可注入对象暴露出来：设置层要把它们接进 schema，诊断页要直接读它们的状态，
//! 测试要能把它们换成确定性的假件。
//!
//! # 分层
//!
//! ```text
//! engine        编排：缓存直取 → 候选规划 → 限流 → 候选外循环 / 退避内循环
//! transfer      单连接尝试（续传 + 流式校验）
//! transfer_chunk 多连接分块编排
//! chunked       分块规划 / Range 语义 / 单块抓取
//! mirrors       镜像清单 + URL 前缀重写      adoptium  JDK 院校镜像
//! region        时区 + 语言 + 探测 → 直连/镜像优先
//! health        源 × 资源类型的健康度记忆
//! cache         内容寻址缓存（记录信任）      atomic    落盘守卫 / 原子写
//! resume        保守续传判定                  verify    流式摘要
//! progress      事件流                        error     错误分类
//! shared        进程级共享：一个数据根一个引擎（跨请求复用连接池/限流/健康度）
//! ```
//!
//! 宿主侧**不要**每次请求现建引擎：`download::shared::engine_for(<cache 根>)` 拿到的是进程内
//! 共享的那个，连接池、信号量限流、健康度记忆因此跨请求累积（M165）。
//!
//! 落盘守卫（[`PartFile`]）中间文件名是 `<name>.ullpart-<session>-<seq>`：`session` 是
//! 进程号，`seq` 是进程内序号。同一数据根跑两个启动器进程时不会互相搬走对方的中间文件
//! （M35 真机：并行两次 `ull install` 各失败数百个资源，`os error 2`）；旧续传材料由
//! `PartFile::for_dest` 收养，单进程语义不变。
//!
//! 依赖纪律：本模块只用 `reqwest` / `tokio` / `futures-util` / `sha1` / `sha2` /
//! `serde` / `serde_json`，不触碰任何 UI 框架（`crates/core` 的硬约束）。

// 对外可用的构件（设置层 / 诊断页 / 其它模块直接消费）。
pub mod adoptium;
pub mod backoff;
pub mod blocking;
pub mod cache;
pub mod chunked;
pub mod config;
pub mod engine;
pub mod error;
pub mod font;
pub mod health;
pub mod jdk_sources;
pub mod mirrors;
pub mod progress;
pub mod region;
pub mod region_system;
pub mod resume;
pub mod shared;
pub mod types;
pub mod verify;

// 内部实现细节：只在 download 模块内（含 crate 内）可见，不承诺兼容性。
pub(crate) mod atomic;
pub(crate) mod bytes;
pub(crate) mod health_store;
pub(crate) mod http;
pub(crate) mod transfer;
pub(crate) mod transfer_chunk;

pub use self::adoptium::{AdoptiumArtifact, JdkFlavor};
pub use self::atomic::{PartFile, ADOPT_MIN_AGE, META_SUFFIX, ORPHAN_MAX_AGE, PART_SUFFIX};
pub use self::backoff::{BackoffPolicy, Jitter};
pub use self::blocking::block_on;
pub use self::cache::{CacheRecord, CacheStats, ContentCache};
pub use self::chunked::{ByteRange, ChunkedConfig, RangeSupport};
pub use self::config::EngineConfig;
pub use self::engine::DownloadEngine;
pub use self::error::{DownloadError, DownloadErrorKind};
pub use self::font::{
    cache_path, fetch_blocking, FontFetch, FontFetchError, FontSpec, MAX_FONT_BYTES,
};
pub use self::health::{HealthKey, HealthRegistry, SourceHealth};
pub use self::jdk_sources::{MsOpenJdkTable, OracleMirror, ZuluTable};
pub use self::mirrors::{direct_source_id, AdoptiumMirror, GhProxy, MirrorTable, PrefixRule};
pub use self::progress::{ProgressEmitter, ProgressEvent, ProgressThrottle};
pub use self::region::{RegionMode, RegionProfile, RegionTrigger, Signals};
pub use self::region_system::detect_signals;
pub use self::resume::{RestartReason, ResumeDecision, ResumeMeta};
pub use self::types::{
    Candidate, DownloadId, DownloadOutcome, DownloadRequest, HashAlgo, Integrity, ResourceKind,
    ResumePolicy, SOURCE_OFFICIAL,
};
pub use self::verify::Hasher;
