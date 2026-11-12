//! 「下载一个 JDK」的作业与状态（M197）。
//!
//! 用户原话：「自动下载 jdk17 作为基准版本，后续在设置中提供多版本可供用户下载。
//! （国内用户走 github release 记得配置加速镜像）」——本文件负责**真下**：选好大版本之后
//! 起一个后台线程跑 `core::game::java::install`（查版本 → 回退链 → 下载引擎 → 解压 → 探测），
//! 界面按快照读进度。
//!
//! ## 为什么又是「后台线程 + 快照」
//!
//! 与 M175 的扫描同一条理由（`store.rs` 里记了完整推导）：本 crate 的执行器是 `thread-pool`，
//! 而一次 JDK 下载要的是**进度（百分比）+ 失败换源原因 + 不卡 UI**。快照的代价是一把短锁 +
//! 每拍一次比较；好处是窗口关掉之后不会往死掉的 channel 里写。
//!
//! ## 进度与「失败退避原因」从哪来
//!
//! 引擎的 [`ProgressEmitter`] 会把**每一次尝试**都吐出来：
//!
//! | 事件 | 界面拿它说什么 |
//! |---|---|
//! | `Started { total_bytes }` | 分母（未知就不显示百分比，不做假进度） |
//! | `Progress { downloaded }` | 百分比 |
//! | `SourceFailed { source, kind, attempt, retry_in_ms }` | 「清华 TUNA 失败（服务器报错）；2 秒后重试第 2 次」 |
//! | `SourceSwitched { from, to }` | 「换源：A → B」 |
//! | `Completed { source }` | 「命中的源」（**装完之后要能说出来**） |
//! | `Verifying` | 校验阶段 |
//!
//! `SourceFailed` 里的 `reason` 是 core 的中文句子，**只进 stderr 日志**；界面按结构化的
//! `kind` 选 i18n 文案（M175 r2 P3-3 的同一条纪律：core 的句子不进双语界面）。
//!
//! ## 为什么不复用 `download::shared::engine_for`
//!
//! `engine_for` 给的是 `Arc<DownloadEngine>`，而 `with_progress` 要**拿走**引擎（`self`）。
//! 要让共享引擎带「这一次作业的」进度观察者，得把引擎内部的 `progress` 改成可插拔的观察者
//! 名单——那是引擎侧的范围（M193 的镜像/并发那一摊），本 mission 不碰。代价写在
//! [`engine::engine_with`] 上：这一次作业有一条自己的连接池，但**盘上的**缓存与健康度仍然共用。
//!
//! ## 同时只允许一个
//!
//! [`Slot::start`] 的判据是「线程还活着」（`running`）。理由与扫描的取消位一致：两个作业抢同一个
//! 归档缓存目录会互相搬走对方的中间文件（`PartFile` 的会话命名只隔进程，不隔线程）。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use unlucky_core::download::blocking::block_on;
use unlucky_core::download::{DownloadErrorKind, ProgressEmitter, ProgressEvent};
use unlucky_core::game::java::{
    install, InstallRequest, InstalledJdk, JavaError, JavaErrorKind, JavaInstallRoot, JdkVendor,
};

/// 推荐判据（`java.auto_install` 的消费者）与引擎装配各自一个文件，见模块文档。
mod engine;
mod recommend;
mod refusal;

pub use engine::{gh_proxy_of_config, GhProxy};
pub use recommend::recommend;
pub use refusal::Refusal;

#[cfg(test)]
mod tests;

/// 一次作业的阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// 查版本（还没开始下字节）
    Preparing,
    /// 下归档
    Downloading,
    /// 校验摘要
    Verifying,
    /// 解压 + 探测（下载已经完成）
    Extracting,
}

/// 一条「退避 / 换源」记录（界面上那一行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// 某个源失败（`retry_in_ms = 0` 表示不再重试、直接换源）
    Retry {
        /// 源标识（`tuna` / `github` / `huawei` …）
        source: String,
        /// 结构化失败分类（文案层按它选 i18n key）
        kind: DownloadErrorKind,
        /// 第几次尝试（从 1 起，给人看）
        attempt: u32,
        /// 下一次重试的等待（毫秒）
        retry_in_ms: u64,
    },
    /// 换源
    Switched {
        /// 原源
        from: String,
        /// 新源
        to: String,
    },
}

/// 正在跑的作业。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    /// 目标大版本
    pub major: u32,
    /// 阶段
    pub phase: Phase,
    /// 已下载字节
    pub downloaded: u64,
    /// 总字节（`None` = 服务器没给长度，不显示百分比）
    pub total: Option<u64>,
    /// 当前 / 命中的源
    pub source: Option<String>,
    /// 最近一条换源 / 退避记录
    pub last: Option<Event>,
}

impl Job {
    fn new(major: u32) -> Self {
        Self {
            major,
            phase: Phase::Preparing,
            downloaded: 0,
            total: None,
            source: None,
            last: None,
        }
    }

    /// 百分比（`None` = 分母未知）。
    #[must_use]
    pub fn percent(&self) -> Option<u32> {
        let total = self.total.filter(|total| *total > 0)?;
        let percent = self.downloaded.saturating_mul(100) / total;
        Some(u32::try_from(percent.min(100)).unwrap_or(100))
    }
}

/// 一次失败（`kind` 是 core 的稳定分类，文案层自己翻）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// 目标大版本
    pub major: u32,
    /// core 的错误分类
    pub kind: JavaErrorKind,
    /// 最后试到的源（查版本阶段就失败时没有）
    pub source: Option<String>,
}

/// 一次成功。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Success {
    /// 目标大版本
    pub major: u32,
    /// 实际装上的发行版（展示名）
    pub vendor: &'static str,
    /// 托管目录
    pub root: String,
    /// `bin/java[.exe]`
    pub binary: String,
    /// 命中的下载源（`None` = 复用已装，没走网络）
    pub source: Option<String>,
    /// 是否完全复用了已装的那一份
    pub reused: bool,
}

/// 「这一次作业试过的一个源」（失败态逐条列出来：哪个源、为什么）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Try {
    /// 源标识
    pub source: String,
    /// 结构化失败分类
    pub kind: DownloadErrorKind,
}

/// 界面读的那份快照。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// 正在跑的作业（`None` = 没在跑）
    pub job: Option<Job>,
    /// 这一次作业试过的源（按首次失败顺序，同一源只留一条）
    pub tries: Vec<Try>,
    /// 上一次失败
    pub failure: Option<Failure>,
    /// 上一次成功
    pub success: Option<Success>,
}

impl Report {
    /// 记一跳失败（同一源只留第一条：界面要的是「试过哪些」，不是重试次数）。
    fn record_try(&mut self, source: &str, kind: DownloadErrorKind) {
        if self.tries.iter().any(|known| known.source == source) {
            return;
        }
        self.tries.push(Try {
            source: source.to_string(),
            kind,
        });
    }
}

/// 一次作业的入参（app 层算好：装到哪、按什么顺序、缓存放哪）。
#[derive(Debug, Clone)]
pub struct Request {
    /// 目标大版本
    pub major: u32,
    /// 下载缓存根（`<home>/cache`，与其它下载共用）
    pub cache_root: PathBuf,
    /// 托管落点（`JavaInstallRoot::resolve` 的结果）
    pub install_root: JavaInstallRoot,
    /// 用户优先级（`java.vendorPriority`；空 = 地区默认序）
    pub priority: Vec<JdkVendor>,
    /// `download.gh_proxy`（`Builtin` = 跟随内置默认；`Off` = 关；`Prefix` = 覆盖前缀）
    pub gh_proxy: GhProxy,
}

/// 作业槽：快照 + 「线程还活着」。
///
/// 内部是 `Arc`：`State` 会被整体替换（hydrate / 截图种子），而作业线程要活得比某一次赋值更久。
#[derive(Debug, Clone, Default)]
pub struct Slot {
    inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    report: Mutex<Report>,
    running: AtomicBool,
}

impl Slot {
    /// 当前快照。
    #[must_use]
    pub fn report(&self) -> Report {
        self.inner
            .report
            .lock()
            .map(|report| report.clone())
            .unwrap_or_default()
    }

    /// 有没有作业在跑。
    #[must_use]
    pub fn busy(&self) -> bool {
        self.inner.running.load(Ordering::SeqCst)
    }

    /// 发起一次安装（已在跑 ⇒ 什么都不做，返回 `false`）。
    pub fn start(&self, request: Request) -> bool {
        if self.inner.running.swap(true, Ordering::SeqCst) {
            return false;
        }
        if let Ok(mut report) = self.inner.report.lock() {
            *report = Report {
                job: Some(Job::new(request.major)),
                tries: Vec::new(),
                failure: None,
                success: None,
            };
        }
        let slot = self.clone();
        std::thread::spawn(move || slot.run(request));
        true
    }

    /// 线程体：挂进度发射器 → 跑 install → 把结论写回快照。
    fn run(self, request: Request) {
        let (emitter, mut events) = ProgressEmitter::channel();
        let feed = self.clone();
        // 抽干事件的那条线：`blocking_recv` 在普通线程上不 panic（tokio 只禁异步上下文里调）。
        let drain = std::thread::spawn(move || {
            while let Some(event) = events.blocking_recv() {
                feed.apply(event);
            }
        });
        let engine = engine::engine_with(&request.cache_root, emitter, &request.gh_proxy);
        let install_request = InstallRequest::new(request.major).with_priority(request.priority);
        let result = block_on(install(&engine, &request.install_root, install_request));
        // 引擎丢掉 ⇒ 发射器丢掉 ⇒ 抽干线程读到 `None` 自然退出。
        drop(engine);
        let _ = drain.join();
        self.finish(result, request.major);
        self.inner.running.store(false, Ordering::SeqCst);
    }

    /// 一条引擎事件 → 快照（纯函数式：界面只读结论）。
    fn apply(&self, event: ProgressEvent) {
        let Ok(mut report) = self.inner.report.lock() else {
            return;
        };
        // 先记「这一跳试过哪个源、为什么失败」——失败态要在界面上逐条列出来。
        if let ProgressEvent::SourceFailed { source, kind, .. } = &event {
            report.record_try(source, *kind);
        }
        let Some(job) = report.job.as_mut() else {
            return;
        };
        match event {
            ProgressEvent::Started { total_bytes, .. } => {
                job.phase = Phase::Downloading;
                job.total = total_bytes;
            }
            ProgressEvent::Progress {
                downloaded,
                total_bytes,
                ..
            } => {
                job.phase = Phase::Downloading;
                job.downloaded = downloaded;
                if total_bytes.is_some() {
                    job.total = total_bytes;
                }
            }
            ProgressEvent::SourceFailed {
                source,
                kind,
                reason,
                attempt,
                retry_in_ms,
                ..
            } => {
                // core 的原句只进日志：界面要双语，不能直接吐中文句子。
                eprintln!("[java] 源 {source} 失败（{}）：{reason}", kind.as_str());
                job.source = Some(source.clone());
                job.last = Some(Event::Retry {
                    source,
                    kind,
                    attempt: attempt.saturating_add(1),
                    retry_in_ms,
                });
            }
            ProgressEvent::SourceSwitched { from, to, .. } => {
                job.source = Some(to.clone());
                job.last = Some(Event::Switched { from, to });
            }
            ProgressEvent::Verifying { .. } => job.phase = Phase::Verifying,
            ProgressEvent::Completed { source, .. } => {
                job.phase = Phase::Extracting;
                job.source = Some(source);
            }
            ProgressEvent::Failed { kind, message, .. } => {
                eprintln!("[java] 下载失败（{}）：{message}", kind.as_str());
            }
        }
    }

    /// install 的结论 → 快照。
    fn finish(&self, result: Result<InstalledJdk, JavaError>, major: u32) {
        let Ok(mut report) = self.inner.report.lock() else {
            return;
        };
        let source = report.job.as_ref().and_then(|job| job.source.clone());
        report.job = None;
        match result {
            Ok(installed) => {
                report.failure = None;
                report.success = Some(Success {
                    major: installed.major,
                    vendor: installed.vendor.display(),
                    root: installed.root.display().to_string(),
                    binary: installed.binary().display().to_string(),
                    source: installed.downloaded.clone().or(source),
                    reused: installed.reused(),
                });
            }
            Err(error) => {
                eprintln!(
                    "[java] Java {major} 安装失败（{}）：{}",
                    error.kind().as_str(),
                    error.message()
                );
                report.success = None;
                report.failure = Some(Failure {
                    major,
                    kind: error.kind(),
                    source,
                });
            }
        }
    }

    /// 截图种子用：把快照钉成某一档（只给 `--screenshot` 走，见 `seed.rs`）。
    ///
    /// 产品路径不调它：真进度只能来自 [`Slot::run`] 那条线程。
    pub fn seed_report(&self, report: Report) {
        if let Ok(mut slot) = self.inner.report.lock() {
            *slot = report;
        }
    }
}
