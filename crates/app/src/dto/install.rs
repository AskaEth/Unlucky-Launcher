//! 安装进度 DTO：下载页与新建向导共用的「一个安装任务」视图。
//!
//! 只放聚合量（文件计数 / 字节 / 速度 / 剩余时间）+ 一行状态文案：
//! 逐文件的明细留在 core 的 `InstallReport`，UI 不需要也不该持有。
//! 速度与 ETA 由 host 依据相邻两次字节数差分算（core 的事件只给绝对值）。

use serde::{Deserialize, Serialize};

/// 安装阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallStage {
    /// 解析版本清单 / 规划文件（联网取 metadata）
    Planning,
    /// 下载并校验文件
    Downloading,
    /// 落版本 JSON / 跑安装器
    Installing,
    /// 登记实例
    Registering,
    /// 完成
    Done,
    /// 失败
    Failed,
    /// 用户取消
    Cancelled,
}

impl InstallStage {
    /// 阶段文案。
    pub fn label(self) -> &'static str {
        match self {
            Self::Planning => "解析版本",
            Self::Downloading => "下载文件",
            Self::Installing => "生成版本",
            Self::Registering => "登记实例",
            Self::Done => "已完成",
            Self::Failed => "失败",
            Self::Cancelled => "已取消",
        }
    }

    /// 是否已经收尾（Done/Failed/Cancelled）。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }
}

/// 一次安装的进度快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallProgressView {
    /// 目标版本 id
    pub version: String,
    /// 实例名（默认 = 版本 id）
    pub instance: String,
    /// 加载器徽标文字（`原版` / `Fabric 0.16.5`）
    pub loader: String,
    /// 阶段
    pub stage: InstallStage,
    /// 已完成文件数
    pub files_done: u64,
    /// 文件总数
    pub files_total: u64,
    /// 已下载字节
    pub bytes_done: u64,
    /// 总字节（0 = 未知）
    pub bytes_total: u64,
    /// 当前速度（字节/秒；0 = 未知）
    pub speed_bps: u64,
    /// 剩余秒数（0 = 未知）
    pub eta_secs: u64,
    /// 状态/错误文案
    pub message: String,
}

impl Default for InstallProgressView {
    fn default() -> Self {
        Self::new()
    }
}

impl InstallProgressView {
    /// 规划中的空快照。
    pub fn new() -> Self {
        Self {
            version: String::new(),
            instance: String::new(),
            loader: String::new(),
            stage: InstallStage::Planning,
            files_done: 0,
            files_total: 0,
            bytes_done: 0,
            bytes_total: 0,
            speed_bps: 0,
            eta_secs: 0,
            message: String::new(),
        }
    }

    /// 起一个任务的快照。
    pub fn start(version: &str, instance: &str, loader: &str) -> Self {
        Self {
            version: version.to_string(),
            instance: instance.to_string(),
            loader: loader.to_string(),
            stage: InstallStage::Planning,
            message: "正在解析版本清单…".to_string(),
            ..Self::new()
        }
    }

    /// 整体进度 0.0~1.0（优先文件计数，其次字节；都没有时 0）。
    pub fn ratio(&self) -> f32 {
        if self.files_total > 0 {
            (self.files_done as f32 / self.files_total as f32).clamp(0.0, 1.0)
        } else if self.bytes_total > 0 {
            (self.bytes_done as f32 / self.bytes_total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// 进度是否已知（未知时进度条转不确定态）。
    pub fn is_known(&self) -> bool {
        self.files_total > 0 || self.bytes_total > 0
    }

    /// 速度文案（`1.2 MB/s`；未知给空串）。
    pub fn speed_label(&self) -> String {
        if self.speed_bps == 0 {
            return String::new();
        }
        format!("{}/s", human_bytes(self.speed_bps))
    }

    /// 剩余时间文案（`剩 12s`；未知给空串）。
    pub fn eta_label(&self) -> String {
        if self.eta_secs == 0 {
            return String::new();
        }
        if self.eta_secs >= 60 {
            format!("剩 {}m{:02}s", self.eta_secs / 60, self.eta_secs % 60)
        } else {
            format!("剩 {}s", self.eta_secs)
        }
    }

    /// 文件计数文案（`128/512 文件`）。
    pub fn files_label(&self) -> String {
        if self.files_total == 0 {
            return String::new();
        }
        format!("{}/{} 文件", self.files_done, self.files_total)
    }
}

/// 字节数 → 人读（`1.2 MB`）。
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// 用相邻两次采样算速度与 ETA（`None` = 本次无法计算，保持上次值）。
pub fn derive_speed(
    previous: (u64, std::time::Instant),
    current: (u64, std::time::Instant),
    bytes_total: u64,
) -> Option<(u64, u64)> {
    let elapsed = current.1.saturating_duration_since(previous.1);
    if elapsed.as_millis() < 50 {
        return None;
    }
    let delta = current.0.saturating_sub(previous.0);
    if delta == 0 {
        return None;
    }
    let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    let speed = delta.saturating_mul(1000) / elapsed_ms.max(1);
    let eta = if bytes_total > current.0 && speed > 0 {
        (bytes_total - current.0) / speed
    } else {
        0
    };
    Some((speed, eta))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn ratio_prefers_file_counts() {
        let mut view = InstallProgressView::start("1.20.4", "1.20.4", "原版");
        assert_eq!(view.ratio(), 0.0);
        assert!(!view.is_known());

        view.files_total = 4;
        view.files_done = 1;
        view.bytes_total = 1000;
        view.bytes_done = 900;
        assert!((view.ratio() - 0.25).abs() < f32::EPSILON);
        assert!(view.is_known());
        assert_eq!(view.files_label(), "1/4 文件");
    }

    #[test]
    fn speed_and_eta_labels_are_human_readable() {
        let view = InstallProgressView {
            speed_bps: 1_258_291,
            eta_secs: 75,
            ..InstallProgressView::new()
        };
        assert_eq!(view.speed_label(), "1.2 MB/s");
        assert_eq!(view.eta_label(), "剩 1m15s");
        assert_eq!(InstallProgressView::new().speed_label(), "");
    }

    #[test]
    fn derive_speed_needs_a_measurable_delta() {
        let start = Instant::now();
        let previous = (0, start);
        // 间隔太短 → 不算
        assert!(derive_speed(previous, (500, start + Duration::from_millis(10)), 1000).is_none());
        // 正常差分：1s 下 500 字节 → 500 B/s，剩 500 字节 → 1s
        let (speed, eta) =
            derive_speed(previous, (500, start + Duration::from_secs(1)), 1000).expect("应能算");
        assert_eq!(speed, 500);
        assert_eq!(eta, 1);
    }

    #[test]
    fn terminal_stages() {
        assert!(InstallStage::Done.is_terminal());
        assert!(InstallStage::Failed.is_terminal());
        assert!(!InstallStage::Downloading.is_terminal());
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KB");
    }
}
