//! stderr 实时进度（原版安装 / 加载器库 / processor 依赖库三段共用）。
//!
//! **M210/D11 的教训**：进度行的数字必须是**已经发生的事实**（每个文件的 [`FileStatus`]），
//! 不能拿「处理到第几个文件」冒充「下载了几个」。旧实现在回调里只累加一个 `done` 就打印
//! `下载 {done}/{total}`，于是重装同版本（3961 个文件全部已就绪、一个字节都没下）时会连打
//! `下载 32/3961 … 3961/3961`——进度条在跑，下载量是 0，用户在读一句假话。
//!
//! 现在分三个计数（下载 / 跳过 / 失败），人读行的措辞随计数走：下载数为 0 时**不出现「下载」
//! 二字**，改说「已就绪 N/M（跳过 N 个）」。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use unlucky_core::game::install::{FileOutcome, FileStatus, ProgressSink};

use crate::output::progress_line;

/// 每处理多少个文件打一行进度（3000 个资源下约 90 行，够用又不刷屏）。
const PROGRESS_EVERY: usize = 32;

/// 进度里最多逐条打印几条失败（其余交给最终错误汇总）。
const MAX_PROGRESS_FAILURES: usize = 5;

/// 一次进度快照：每个字段都是**真实结局**的计数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    /// 已处理文件数（含跳过与失败）。
    pub done: usize,
    /// 计划文件总数。
    pub total: usize,
    /// 本次真的下载并落盘的文件数。
    pub downloaded: usize,
    /// 已存在且校验通过、本次跳过的文件数。
    pub skipped: usize,
    /// 失败文件数。
    pub failed: usize,
}

impl Snapshot {
    /// 人读进度行。
    ///
    /// 下载数为 0 时**不提「下载」**：本次一个字节都没下，再写「下载 0/N」也只是换个说法的假进度。
    pub(crate) fn text(&self) -> String {
        let failure = if self.failed > 0 {
            format!(" / 失败 {}", self.failed)
        } else {
            String::new()
        };
        if self.downloaded == 0 {
            format!(
                "已就绪 {}/{}（跳过 {} 个{failure}）",
                self.done, self.total, self.skipped
            )
        } else {
            format!(
                "下载 {} 个 / 跳过 {} 个（进度 {}/{}{failure}）",
                self.downloaded, self.skipped, self.done, self.total
            )
        }
    }

    /// `--json` 模式的进度事件（字段名稳定，`done`/`total` 保持旧名以兼容脚本）。
    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "event": "progress",
            "done": self.done,
            "total": self.total,
            "downloaded": self.downloaded,
            "skipped": self.skipped,
            "failed": self.failed,
        })
    }
}

/// 计数：回调来自多个下载线程，全部原子累加。
#[derive(Debug, Default)]
struct Tally {
    done: AtomicUsize,
    downloaded: AtomicUsize,
    skipped: AtomicUsize,
    failed: AtomicUsize,
    failure_lines: AtomicUsize,
}

impl Tally {
    /// 记一个结局，返回记完之后的快照。
    fn observe(&self, outcome: &FileOutcome, total: usize) -> Snapshot {
        let done = self.done.fetch_add(1, Ordering::Relaxed) + 1;
        match outcome.status {
            FileStatus::Downloaded => {
                self.downloaded.fetch_add(1, Ordering::Relaxed);
            }
            FileStatus::Skipped => {
                self.skipped.fetch_add(1, Ordering::Relaxed);
            }
            FileStatus::Failed => {
                self.failed.fetch_add(1, Ordering::Relaxed);
            }
        }
        Snapshot {
            done,
            total,
            downloaded: self.downloaded.load(Ordering::Relaxed),
            skipped: self.skipped.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
        }
    }
}

/// 实时进度回调（只写 stderr，不污染 `--json` 的 stdout）。
///
/// 两种形态：人类模式打文本行；`--json` 模式打 **JSONL**（一行一条事件，`event` 字段分流），
/// 这样脚本既能吃 stdout 的最终信封，也能实时跟进而不必解析文本。
///
/// 失败行只打前几条：整源不可达时会一口气失败上千个文件，全打出来既没用又淹没原因。
pub fn progress_sink(enabled: bool, json: bool, total: usize) -> Option<ProgressSink> {
    sink_with(enabled, json, total, Arc::new(progress_line))
}

/// 可注入输出端的构造器（测试据此抓行；生产只有一个调用者 [`progress_sink`]）。
fn sink_with(
    enabled: bool,
    json: bool,
    total: usize,
    out: Arc<dyn Fn(&str) + Send + Sync>,
) -> Option<ProgressSink> {
    if !enabled {
        return None;
    }
    let tally = Arc::new(Tally::default());
    Some(Arc::new(move |outcome: &FileOutcome| {
        let snapshot = tally.observe(outcome, total);
        if let Some(failure) = &outcome.failure {
            let seen = tally.failure_lines.fetch_add(1, Ordering::Relaxed);
            if seen < MAX_PROGRESS_FAILURES {
                emit(
                    json,
                    serde_json::json!({
                        "event": "file_failed",
                        "done": snapshot.done,
                        "total": total,
                        "name": outcome.name,
                        "role": outcome.role.as_str(),
                        "kind": failure.kind.as_str(),
                        "message": failure.message,
                    }),
                    &format!(
                        "失败 {}/{} {}：{}",
                        snapshot.done, total, outcome.name, failure.message
                    ),
                    &out,
                );
            } else if seen == MAX_PROGRESS_FAILURES {
                emit(
                    json,
                    serde_json::json!({
                        "event": "progress_truncated",
                        "suppressed_after": MAX_PROGRESS_FAILURES,
                    }),
                    "（更多失败不再逐条打印，见最终错误汇总）",
                    &out,
                );
            }
            return;
        }
        if snapshot.done.is_multiple_of(PROGRESS_EVERY) || snapshot.done == total {
            emit(json, snapshot.json(), &snapshot.text(), &out);
        }
    }))
}

/// 一条进度：JSONL 模式打序列化后的一行，人类模式打短文本。
fn emit(
    json_mode: bool,
    record: serde_json::Value,
    text: &str,
    out: &Arc<dyn Fn(&str) + Send + Sync>,
) {
    if json_mode {
        let line = record.to_string();
        out(&line);
    } else {
        out(text);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use unlucky_core::download::DownloadErrorKind;
    use unlucky_core::game::install::{FileFailure, FileRole};

    use super::*;

    /// 测试用的输出端：一个可注入的 sink + 它收到的行。
    type Recorder = (Arc<dyn Fn(&str) + Send + Sync>, Arc<Mutex<Vec<String>>>);

    /// 抓行的输出端。
    fn recorder() -> Recorder {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink_lines = Arc::clone(&lines);
        let out: Arc<dyn Fn(&str) + Send + Sync> =
            Arc::new(move |line: &str| sink_lines.lock().expect("锁").push(line.to_string()));
        (out, lines)
    }

    fn outcome(name: &str, status: FileStatus) -> FileOutcome {
        FileOutcome {
            role: FileRole::Asset,
            name: name.to_string(),
            dest: std::path::PathBuf::from(format!("assets/{name}")),
            status,
            bytes: 1234,
            source: Some("cache".to_string()),
            failure: None,
        }
    }

    /// **D11 用例**：全部已存在（重装同版本）⇒ 进度行里不出现「下载」，下载计数为 0。
    ///
    /// 抓得住：把「处理序号」当下载量打印的旧实现（它会打出 `下载 32/96` 这类行）；
    /// 以及任何「下载数 > 0 却说没下载」的表述。抓不住：行里出现的其它夸张词（措辞审查），
    /// 也不管单行是否真的落到 stderr（那是 [`crate::output::progress_line`] 的事）。
    #[test]
    fn all_skipped_progress_never_claims_downloads() {
        const TOTAL: usize = 96; // 3 的倍数，必然跨过 PROGRESS_EVERY
        let (out, lines) = recorder();
        let sink = sink_with(true, false, TOTAL, out).expect("进度开关打开时必须有 sink");
        for index in 0..TOTAL {
            sink(&outcome(&format!("asset-{index}"), FileStatus::Skipped));
        }
        let lines = lines.lock().expect("锁").clone();
        assert!(!lines.is_empty(), "应当有进度行");
        for line in &lines {
            assert!(
                !line.contains("下载"),
                "全部跳过时进度行不许提「下载」：{line}"
            );
        }
        let last = lines.last().expect("至少一行");
        assert!(
            last.contains(&format!("已就绪 {TOTAL}/{TOTAL}"))
                && last.contains(&format!("跳过 {TOTAL}")),
            "全跳过应当说「已就绪 N/N（跳过 N 个）」：{last}"
        );
    }

    /// 同一场景的 `--json` 形态：`downloaded` 必须是 0（脚本据此判断「其实没下」）。
    #[test]
    fn all_skipped_json_progress_reports_zero_downloads() {
        const TOTAL: usize = 32;
        let (out, lines) = recorder();
        let sink = sink_with(true, true, TOTAL, out).expect("进度开关打开时必须有 sink");
        for index in 0..TOTAL {
            sink(&outcome(&format!("lib-{index}"), FileStatus::Skipped));
        }
        let records: Vec<serde_json::Value> = lines
            .lock()
            .expect("锁")
            .iter()
            .map(|line| serde_json::from_str(line).expect("JSONL"))
            .collect();
        let progress = records
            .iter()
            .find(|record| record["event"] == "progress")
            .expect("应当有 progress 事件");
        assert_eq!(progress["downloaded"], 0);
        assert_eq!(progress["skipped"], TOTAL);
        assert_eq!(progress["done"], TOTAL);
        assert_eq!(progress["total"], TOTAL);
    }

    /// 真的有下载时，计数与措辞都跟着事实走（避免「一刀切不打印」这种假修复）。
    #[test]
    fn mixed_outcomes_report_real_counts() {
        let (out, lines) = recorder();
        let sink = sink_with(true, false, 4, out).expect("sink");
        sink(&outcome("client.jar", FileStatus::Downloaded));
        let failed = FileOutcome {
            failure: Some(FileFailure {
                kind: DownloadErrorKind::Network,
                message: "连接被重置".to_string(),
            }),
            ..outcome("b", FileStatus::Failed)
        };
        sink(&failed);
        sink(&outcome("c", FileStatus::Skipped));
        sink(&outcome("d", FileStatus::Skipped));
        let lines = lines.lock().expect("锁").clone();
        // 失败有自己的行（`失败 2/4 b：…`），进度行不拿它冒充下载量。
        assert!(
            lines.iter().any(|line| line.starts_with("失败 2/4 b")),
            "{lines:?}"
        );
        let last = lines.last().expect("至少一行");
        // done == total 时无条件打最后一行。
        assert!(last.contains("下载 1 个"), "{last}");
        assert!(last.contains("跳过 2 个"), "{last}");
        assert!(last.contains("失败 1"), "{last}");
        assert!(last.contains("进度 4/4"), "{last}");
    }
}
