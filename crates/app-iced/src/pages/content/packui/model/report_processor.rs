//! 加载器安装步骤段（M328）：core 的逐步骤结局 → 要画的那几行。
//!
//! ## 为什么单独一截
//!
//! 这一段与报告的其余部分**来源不同**：三态/缺文件/冲突都在讲「包里的内容怎么了」，
//! 这一段讲「加载器安装器自己跑过哪几步」。于是它有自己的段头、自己的 Java 来源行，逐步骤
//! 还各有结局（跑过 / 跳过）——塞进 [`super::report::Report::warnings`] 里当几行注记会丢结构。
//!
//! ## 取词口径
//!
//! - 段头 / 汇总 / 单步两档都是目录 key（[`Key::PackProcessorTitle`] 一族），界面不写字面量；
//! - **来源不是 slug**：`java_origin` 是 DTO 的枚举，`match` 五个臂**穷尽、无通配**，
//!   core 加一档 ⇒ 这里编译不过（那五条人话就是 M325 的 `PackProcessorJavaFrom*`）；
//! - 耗时是**数字**，单位在文案里（`{elapsed} ms`）；这里只做 `to_string()`，不加单位。

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{
    ImportReportView, JavaOriginView, ProcessorStepStatusView, ProcessorStepView,
};

/// 一条 processor 步骤。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorStepRow {
    /// 序号（从 1 起）
    pub index: u32,
    /// 左列：jar 的 maven 坐标
    pub label: String,
    /// 右列：结局（`执行完成 · 产物 3 个 · 1240 ms` / `跳过 · 产物已就绪`）
    pub note: String,
}

/// 加载器安装步骤那一段（`None` = 这次没有 processor 链，整段不画）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorSection {
    /// 段头（`加载器安装步骤`）
    pub title: String,
    /// 用的哪份 Java 与它的来源（`用的是 Java 17（启动器自己装的）`）
    pub java_line: String,
    /// 汇总（`执行 2 步 · 跳过 1 步`）——**两半都印**，`跳过 0 步` 也是诚实信息
    pub summary: String,
    /// 逐步骤（顺序即执行顺序）
    pub steps: Vec<ProcessorStepRow>,
    /// 空态一句（`steps` 为空时才有——链跑了却一步都没列，界面不许画成空白）
    pub empty: Option<String>,
}

/// 报告 → 处理器段（`None` 原样传下去：没有链就没有这一段）。
pub(super) fn section(report: &ImportReportView, catalog: Catalog) -> Option<ProcessorSection> {
    let processors = report.processors.as_ref()?;
    Some(ProcessorSection {
        title: catalog.text(Key::PackProcessorTitle).to_owned(),
        java_line: catalog.render(
            Key::PackProcessorJava,
            &[
                ("java", &java_label(processors.java_major, &processors.java)),
                ("origin", origin_text(processors.java_origin, catalog)),
            ],
        ),
        summary: catalog.render(
            Key::PackProcessorSummary,
            &[
                ("ran", &processors.executed.to_string()),
                ("skipped", &processors.skipped.to_string()),
            ],
        ),
        steps: processors
            .steps
            .iter()
            .map(|step| step_row(step, catalog))
            .collect(),
        empty: processors
            .steps
            .is_empty()
            .then(|| catalog.text(Key::PackProcessorEmpty).to_owned()),
    })
}

/// `{java}` 那一位：有主版本就印主版本（`17`），探不到才退回可执行文件名。
///
/// 为什么不直接印 `processors.java`（完整路径）：那是几十个字符的宿主路径，一行装不下，
/// 而且对用户毫无信息量——他要知道的是「哪一份 Java」，不是它躺哪个目录。
fn java_label(major: Option<u32>, java: &str) -> String {
    major.map_or_else(
        || java.rsplit(['/', '\\']).next().unwrap_or(java).to_string(),
        |major| major.to_string(),
    )
}

/// 来源 → 那五条人话（**穷尽 match、无通配臂**：core 加一档 ⇒ 这里编译不过）。
fn origin_text(origin: JavaOriginView, catalog: Catalog) -> &'static str {
    match origin {
        JavaOriginView::Instance => catalog.text(Key::PackProcessorJavaFromInstance),
        JavaOriginView::Managed => catalog.text(Key::PackProcessorJavaFromLauncher),
        JavaOriginView::JavaHome => catalog.text(Key::PackProcessorJavaFromEnvVar),
        JavaOriginView::Path => catalog.text(Key::PackProcessorJavaFromPath),
        JavaOriginView::System => catalog.text(Key::PackProcessorJavaFromSystem),
    }
}

/// 一条步骤的右列（两档各说一件事：跳过的步耗时恒为 0，印一个 0 只会误导）。
fn step_row(step: &ProcessorStepView, catalog: Catalog) -> ProcessorStepRow {
    ProcessorStepRow {
        index: step.index,
        label: step.label.clone(),
        note: match step.status {
            ProcessorStepStatusView::Executed => catalog.render(
                Key::PackProcessorStepRan,
                &[
                    ("outputs", &step.outputs.to_string()),
                    // 纯数字毫秒：单位在文案里，不要在这里带上（否则上屏 `1240 ms ms`）。
                    ("elapsed", &step.elapsed_ms.to_string()),
                ],
            ),
            ProcessorStepStatusView::Skipped => {
                catalog.text(Key::PackProcessorStepSkipped).to_owned()
            }
        },
    }
}
