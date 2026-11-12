//! 安装进度 DTO：下载页与新建向导共用的「一个安装任务」视图。
//!
//! 只放聚合量（文件计数 / 字节 / 速度 / 剩余时间）+ 一行状态文案：
//! 逐文件的明细留在 core 的 `InstallReport`，UI 不需要也不该持有。
//! 速度与 ETA 由 host 依据相邻两次字节数差分算（core 的事件只给绝对值）。

use serde::{Deserialize, Serialize};

use crate::dto::ContentFailureKind;

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

/// 一条可只重试的失败的**档位**（M191；与 core 的 `FailureKind` 一一对应）。
///
/// ## 为什么不是一个 `bool` / 一句话
///
/// 「不兼容所选版本/加载器」与「取不到」要用户做的事不同：前者换版本或换加载器，后者检查网络
/// 再试。**两档在类型上分开**，界面才能分组画两段、给两条不同的话；合并成一档之后
/// 「重试」按钮会把一条重试必然失败的项也算进去（M191 的变异守门就钉这一条）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InstallFailureKind {
    /// 站点有这个项目，但目标版本/加载器下没有条目（换版本/加载器才有救）
    Incompatible,
    /// 取不到（没联网 / 项目不存在 / 站点出错 / 下载失败）
    Unreachable,
}

impl InstallFailureKind {
    /// 稳定字符串 id（日志 / 用例断言用，**不要改名**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Incompatible => "incompatible",
            Self::Unreachable => "unreachable",
        }
    }

    /// 两档的固定顺序（界面分组按它排：先「换版本能救」，再「检查网络」）。
    pub const ALL: [Self; 2] = [Self::Incompatible, Self::Unreachable];
}

/// 一条「可只重试」的失败项（M191）：项目键 + 可读标签 + 档位 + core 给的原话。
///
/// 这是界面画「可只重试这 N 个」和分组所需的**全部**信息：项目键就是重试时发出去的那个键
/// （`DependencyPlan::retry_request()` 用的同一份）。
///
/// ## 为什么还要 `label`（M191 r1 复审 P2-1）
///
/// `project` 是站点 id（`P7dR8mSH`），直接画在界面上等于给用户一串哈希。可读名其实都在手边，
/// 只是没进 DTO：根项目的名字就在请求的 `title` 里；失败文件的名字在 `PlannedNode.file.filename`
/// 里（下载失败那种）。`label` 收的正是这两个**语言中立**的字符串（站点标题 / 文件名），
/// **不是** `detail` 那种 core 中文散文——那会踩 i18n 的 R2（core 中文不进界面）。
///
/// `label` 为空是合法且**必须**保留的一档：项目不存在（404）时确实没有名字可显示，
/// 那时界面只画 id（评审认这条兜底是合理的）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallFailureView {
    /// 请求它用的项目键（slug 或 id；重试也用它）
    pub project: String,
    /// 语言中立的可读标签（站点给的项目标题 / 文件名；空 = 拿不到名字）
    pub label: String,
    /// 哪一档
    pub kind: InstallFailureKind,
    /// core 给的人读说明（引擎原话 + 上下文，**不是**界面文案；**不要**画到界面上）
    pub detail: String,
}

impl InstallFailureView {
    /// 构造。
    #[must_use]
    pub fn new(
        project: impl Into<String>,
        label: impl Into<String>,
        kind: InstallFailureKind,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            project: project.into(),
            label: label.into(),
            kind,
            detail: detail.into(),
        }
    }

    /// 界面该画的那一行：有名字就「名字 (id)」，没有就只画 id。
    ///
    /// 界面上**名字优先、id 次要**：名字让人认出是哪个项目，括号里的 id 是给「名字对不上时」
    /// 或排查用的。定在这个方法里而不是视图里，是因为「有名字用名字」这条要有用例钉住。
    #[must_use]
    pub fn line(&self) -> String {
        let label = self.label.trim();
        if label.is_empty() || label == self.project {
            self.project.clone()
        } else {
            format!("{label} ({})", self.project)
        }
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
    /// 失败大类（M162 r1）：`None` = 没失败，或失败来自不产生分类的路径。
    ///
    /// 放在进度快照里而不是只塞进文案：`message` 是给用户看的一句话，"没联网" 与
    /// "这个项目不存在" 该画两种提示，判据不能靠那句话里有没有"网络"两个字。
    pub failure_kind: Option<ContentFailureKind>,
    /// 可只重试的失败项（M191）：空 = 没有失败项 ⇒ 界面**不渲染**那条入口。
    ///
    /// 每条自带档位（[`InstallFailureKind`]），界面按档分组；`project` 就是
    /// `DependencyPlan::retry_request()` 会发出去的那几个项目键——名单由 plan 产出，
    /// 界面不自己拼（拼一份就会与重试实际请求的那几个不一致）。
    pub retry: Vec<InstallFailureView>,
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
            failure_kind: None,
            retry: Vec::new(),
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

    /// 有可只重试的失败项吗（`false` ⇒ 界面**不渲染**那条入口，不是置灰）。
    #[must_use]
    pub fn has_retry(&self) -> bool {
        !self.retry.is_empty()
    }

    /// 某一档的可重试项（两档分开画时用；顺序沿用计划里的顺序）。
    pub fn retry_of(&self, kind: InstallFailureKind) -> impl Iterator<Item = &InstallFailureView> {
        self.retry.iter().filter(move |item| item.kind == kind)
    }

    /// 某一档有几项。
    #[must_use]
    pub fn retry_count(&self, kind: InstallFailureKind) -> usize {
        self.retry_of(kind).count()
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
