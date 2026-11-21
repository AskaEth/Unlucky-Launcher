//! 加载器安装器（第六跳 processor 链）的逐步骤结局（M328）。
//!
//! ## 为什么这一层曾经是空的
//!
//! core 的报告里 `processors` 一直是全的（M321 起每一步都留一条，与 `.with_steps` 实时回调
//! 无关），缺的是**这一层**：[`super::ImportReportView`] 没有 processor 字段，界面于是拿到
//! 一份「装完了」却说不出一共跑了几步、哪一步是跳过的报告。本文件把那一段逐字段搬过边界。
//!
//! ## 来源为什么是枚举而不是 slug 串
//!
//! 界面要为「用的哪份 Java」取一句人话（[`JavaOriginView`] 五档 → `PackProcessorJavaFrom*`
//! 五条）。若这里传 core 的 slug 串，界面就得用字符串 `match` 取词：core 哪天加一档，界面
//! **悄悄不印或印出 slug** 而不报错。换成枚举后，壳层的 `match` 少一个臂就编译不过。
//!
//! `--json` 那边不受影响：脚本读的仍是 core 的稳定 id（`managed` / `JAVA_HOME` / `PATH` /
//! `system` / `instance`），本模块只服务宿主与界面。

use serde::{Deserialize, Serialize};

/// 一步的结局（core 的 `StepStatus` 两档逐条对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessorStepStatusView {
    /// 真的起了一次 java。
    Executed,
    /// 产物已就绪（SHA-1 命中），整步跳过。
    Skipped,
}

impl ProcessorStepStatusView {
    /// 全部两档（`from_slug` 的遍历源）。
    pub const ALL: [ProcessorStepStatusView; 2] = [
        ProcessorStepStatusView::Executed,
        ProcessorStepStatusView::Skipped,
    ];

    /// 稳定 slug（与 core 的 `StepStatus::as_str` 逐字相同，测试用它对齐）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Executed => "executed",
            Self::Skipped => "skipped",
        }
    }

    /// 从 core 的结局 id 反解。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.slug() == slug)
    }
}

/// 一个 Java 运行时的来源（core 的 `JavaOrigin` 五档逐条对应）。
///
/// 与 [`super::MissingReasonView`] 同一条纪律：**枚举**而非 slug 串，界面侧的取词
/// `match` 因此是穷尽的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JavaOriginView {
    /// 实例覆盖（`--java` 参数或实例档案的 `javaPath`）。
    Instance,
    /// 托管目录（启动器自己装的）。
    Managed,
    /// `JAVA_HOME`。
    JavaHome,
    /// `PATH` 上的 `java`。
    Path,
    /// 标准安装位置扫出来的。
    System,
}

impl JavaOriginView {
    /// 全部五档（`from_slug` 的遍历源）。
    pub const ALL: [JavaOriginView; 5] = [
        JavaOriginView::Instance,
        JavaOriginView::Managed,
        JavaOriginView::JavaHome,
        JavaOriginView::Path,
        JavaOriginView::System,
    ];

    /// 稳定 slug（与 core 的 `JavaOrigin::as_str` 逐字相同）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Instance => "instance",
            Self::Managed => "managed",
            Self::JavaHome => "JAVA_HOME",
            Self::Path => "PATH",
            Self::System => "system",
        }
    }

    /// 从 core 的 id 反解（`--json` 里读回来的那条路）。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|origin| origin.slug() == slug)
    }
}

/// 一条 processor 步骤（跑过的每一步都有一条，含跳过的）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessorStepView {
    /// 序号（从 1 起，与 core 的报告同源）。
    pub index: u32,
    /// jar 的 maven 坐标（`net.minecraftforge:installertools:1.3.0` 之类）。
    pub label: String,
    /// 结局。
    pub status: ProcessorStepStatusView,
    /// 产物个数。
    pub outputs: u32,
    /// 耗时（**毫秒，纯数字**）：界面的单位写在文案里（`{elapsed} ms`），
    /// 这里带单位就等于上屏 `1200 ms ms`。
    pub elapsed_ms: u64,
}

/// 加载器安装器那一段的汇总（`None` = 这次没有 processor 链）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessorSummaryView {
    /// 实际用的 Java 可执行文件（原样，可能很长；界面按主版本取词）。
    pub java: String,
    /// Java 来源（人读那行按它取词，**不上屏 slug**）。
    pub java_origin: JavaOriginView,
    /// Java 主版本（探测不到时 `None`）。
    pub java_major: Option<u32>,
    /// 真的起过 java 的步数。
    pub executed: u32,
    /// 产物已就绪而整步跳过的步数。
    pub skipped: u32,
    /// 逐步骤结果（顺序即执行顺序）。
    pub steps: Vec<ProcessorStepView>,
    /// 这一步链自己的告警（同一条也会进 [`super::ImportReportView::warnings`]）。
    pub warnings: Vec<String>,
}
