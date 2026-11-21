//! 报告里的**一条注记**：级别 + 稳定 key + 人读细节 + 可选路径（M339）。
//!
//! 从 [`super::items`] 搬出来（搬前那边 371 行，`report.rs` 那条 400 行硬线已经贴线）。级别这一维是
//! 「一条注记自己的属性」，与文件 / 冲突 / 缺失那些条目类型不是同一族。

use serde::Serialize;

/// 一条注记的**级别**（M339）。
///
/// **级别是产品判断，不是 key 的属性**：`message_key` 是 i18n 的查找串（稳定字符串，页面按它
/// 查文案），它的前缀（`pack.import.note.…`）与「这条要紧不要紧」没有关系——同一个 `note.`
/// 前缀下既有 `NOTE_LOADER_NOT_INSTALLED`（加载器没装、启动不了 ⇒ 告警），也有
/// `NOTE_REGISTERED`（登记成功 ⇒ 注记）。
///
/// 所以级别**由构造点逐条显式给出**：没有 `Default`、没有「按 key 猜」的兜底，构造函数也只有
/// [`ImportIssue::warning`] / [`ImportIssue::note`] 两个（**没有不带级别的 `new`**），漏给就
/// 编译不过。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportIssueLevel {
    /// 要用户处理：东西没装上 / 可能起不来 / 目标目录状态与用户以为的不同。
    Warning,
    /// 事实陈述：成功确认、有意跳过（格式语义规定）、可选优化。
    Note,
}

/// 一条人读注记（带稳定 key 与级别）。
#[derive(Debug, Clone, Serialize)]
pub struct ImportIssue {
    /// 级别（界面与命令行的分类标题、计数都按它算）。
    pub level: ImportIssueLevel,
    /// 文案 key。
    pub message_key: &'static str,
    /// 人读细节。
    pub message: String,
    /// 相关实例内路径。
    pub path: Option<String>,
}

impl ImportIssue {
    /// 构造一条**告警**（用户要处理）。
    #[must_use]
    pub fn warning(message_key: &'static str, message: impl Into<String>) -> Self {
        Self::of(ImportIssueLevel::Warning, message_key, message.into())
    }

    /// 构造一条**注记**（事实陈述 / 有意跳过）。
    #[must_use]
    pub fn note(message_key: &'static str, message: impl Into<String>) -> Self {
        Self::of(ImportIssueLevel::Note, message_key, message.into())
    }

    /// 两个构造函数共用的那一处（`path` 起点永远是 `None`，由 [`ImportIssue::with_path`] 挂）。
    fn of(level: ImportIssueLevel, message_key: &'static str, message: String) -> Self {
        Self {
            level,
            message_key,
            message,
            path: None,
        }
    }

    /// 挂一条路径。
    #[must_use]
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// 是**告警**吗（界面与命令行分档的唯一判据）。
    #[must_use]
    pub const fn is_warning(&self) -> bool {
        matches!(self.level, ImportIssueLevel::Warning)
    }
}

#[cfg(test)]
mod tests {
    //! 构造点的档位守门（M339）：**判错档要红**，而不是「UI 看起来还行」。
    //!
    //! 只钉有公开入口、且一个入口一个构造点的那几条（`ImportReport` 的方法 + `suffix_note`）：
    //! 这几个入口直接体现「用户要不要动手」，改错档就是用户被误导。
    //!
    //! 抓不住：`plan.rs` / `pipeline.rs` 里那些要真包才走到的构造点（要造整份 `ImportPlan`），
    //! 以及 presenter 自己造的两条（`instance_bound` / `instance_binding_failed`，见那边的用例）。

    use super::*;
    use crate::addon::import::plan::paths::{suffix_note, SuffixVariant};
    use crate::addon::import::report::keys;
    use crate::addon::import::{ImportReport, LoaderTarget, ProcessorSummary, RollbackOutcome};
    use crate::addon::loader::LoaderKind;
    use crate::game::java::JavaOrigin;

    fn target() -> LoaderTarget {
        LoaderTarget {
            kind: LoaderKind::Fabric,
            mc_version: "1.20.1".to_string(),
            loader_version: "0.15.11".to_string(),
            raw: "fabric-loader-0.15.11-1.20.1".to_string(),
            version_id: "fabric-loader-0.15.11-1.20.1".to_string(),
        }
    }

    fn levels(report: &ImportReport) -> Vec<(ImportIssueLevel, &'static str)> {
        report
            .warnings
            .iter()
            .map(|issue| (issue.level, issue.message_key))
            .collect()
    }

    /// 加载器**装好了** = 成功确认 ⇒ 注记；**没装** = 启动不了 ⇒ 告警。
    ///
    /// 变异：把 `report.rs` 里 `note_loader_not_installed` 的 `ImportIssue::warning` 改成
    /// `ImportIssue::note` ⇒ 本条红（第 2 条断言）。
    #[test]
    fn loader_installed_is_a_note_but_not_installed_is_a_warning() {
        let mut report = ImportReport::empty("C:/tmp/instance".into());
        report.note_loader_installed(&target(), "第六跳成功");
        assert_eq!(
            levels(&report),
            vec![(ImportIssueLevel::Note, keys::NOTE_LOADER_INSTALLED)]
        );

        let mut report = ImportReport::empty("C:/tmp/instance".into());
        report.note_loader_not_installed(&target());
        assert_eq!(
            levels(&report),
            vec![(ImportIssueLevel::Warning, keys::NOTE_LOADER_NOT_INSTALLED)],
            "清单要求加载器却没装上：用户不改就启动不了，必须是告警"
        );
    }

    /// 加载器解析期告警与 processor 链告警都进告警档（库缺下载地址 / 产物摘要不符 ⇒ 装不全）。
    #[test]
    fn loader_and_processor_warnings_are_warnings() {
        let mut report = ImportReport::empty("C:/tmp/instance".into());
        report.note_loader_warnings(&["loader 库缺下载地址，已降级".to_string()]);
        report.note_processors(ProcessorSummary {
            java: "C:/jdk/bin/java.exe".to_string(),
            java_origin: JavaOrigin::Managed,
            java_major: Some(17),
            executed: 1,
            skipped: 0,
            steps: Vec::new(),
            warnings: vec!["产物数 3 与计划里的 2 不一致".to_string()],
        });
        assert_eq!(
            levels(&report),
            vec![
                (ImportIssueLevel::Warning, keys::NOTE_LOADER_WARNING),
                (ImportIssueLevel::Warning, keys::NOTE_PROCESSOR_WARNING),
            ]
        );
    }

    /// 取消收尾的**三档**（`ImportReport::cancel` 里的三个构造点）逐条钉住级别、条数与顺序。
    ///
    /// 依据（「照这份报告做决定会不会出错」）：
    ///
    /// | 档 | 事实 | 用户要做什么 | 级别 |
    /// |----|------|-------------|------|
    /// | `attempted == 0` | 落盘之前就停手，目标目录一个字节没动 | 重跑一次即可 | 注记 |
    /// | 回滚干净 | 已回滚 N 项，目录恢复导入前 | 同上 | 注记 |
    /// | 回滚没做完 | 可能留着半包内容 | 得看 `rollback.failed` 并自己收拾，否则会在"看着是新的、其实混着旧文件"的实例上启动 | 告警 |
    ///
    /// 后两档是**同一次取消里的两条**：告警在前、进度注记在后；两条各自独立（把"已取消"那条也判成
    /// 告警，用户会以为两件事都要他动手）。
    ///
    /// 变异（M352 r2 实测）：`report.rs` 里 `attempted == 0` 那处 `ImportIssue::note` 改成 `warning`
    /// ⇒ 第 1 段红；不完整分支末尾那条 `note` 改成 `warning` ⇒ 第 3 段红。
    /// **只用 `contains` / `all(... == Note)` 时这两处都不红**——r2 复审抓到的正是这个空洞。
    #[test]
    fn cancel_outcomes_are_leveled_exactly_three_ways() {
        // ① 落盘之前就停手：目标目录未被写入（`report.rs` 的 `attempted == 0` 分支）。
        let mut report = ImportReport::empty("C:/tmp/instance".into());
        report.cancel(RollbackOutcome {
            attempted: 0,
            failed: Vec::new(),
        });
        assert_eq!(
            levels(&report),
            vec![(ImportIssueLevel::Note, keys::NOTE_CANCELLED)],
            "一个字节都没写过 ⇒ 只有一条「已取消」的事实陈述：{:?}",
            levels(&report)
        );
        assert!(report.rolled_back, "没写过就等于回滚干净");
        assert!(
            report.warnings[0].message.contains("未被写入"),
            "{}",
            report.warnings[0].message
        );

        // ② 回滚干净：恢复 import 之前的样子。
        let mut report = ImportReport::empty("C:/tmp/instance".into());
        report.cancel(RollbackOutcome {
            attempted: 2,
            failed: Vec::new(),
        });
        assert_eq!(
            levels(&report),
            vec![(ImportIssueLevel::Note, keys::NOTE_CANCELLED)],
            "回滚干净只说「已取消、目录恢复原样」：{:?}",
            levels(&report)
        );
        assert!(
            report.warnings[0].message.contains("恢复为导入前的样子"),
            "{}",
            report.warnings[0].message
        );

        // ③ 回滚没做完：告警在前、进度注记在后（两条都要，等级各不相同）。
        let mut report = ImportReport::empty("C:/tmp/instance".into());
        report.cancel(RollbackOutcome {
            attempted: 2,
            failed: vec!["mods/a.jar：删不掉".to_string()],
        });
        assert_eq!(
            levels(&report),
            vec![
                (ImportIssueLevel::Warning, keys::NOTE_ROLLBACK_INCOMPLETE),
                (ImportIssueLevel::Note, keys::NOTE_CANCELLED),
            ],
            "回滚没做完 ⇒ 告警 + 一条「已取消」的注记（条数与顺序都钉住）：{:?}",
            levels(&report)
        );
        assert!(!report.rolled_back, "不许在可能没回干净时宣称一致");
        assert!(
            report.warnings[0].message.contains("mods/a.jar"),
            "没回去的那几项要逐条列出：{}",
            report.warnings[0].message
        );
        assert!(
            report.warnings[1].message.contains("回滚未完整"),
            "{}",
            report.warnings[1].message
        );
    }

    /// 路径后缀注记（`.disabled` / `.old`）是**有意按原后缀保留** ⇒ 注记。
    #[test]
    fn suffix_notes_are_notes() {
        let issue = suffix_note(SuffixVariant::Disabled, "mods/a.jar.disabled").expect("有注记");
        assert_eq!(issue.level, ImportIssueLevel::Note);
        assert_eq!(issue.message_key, keys::NOTE_DISABLED_SUFFIX);
        assert!(!issue.is_warning());
    }
}
