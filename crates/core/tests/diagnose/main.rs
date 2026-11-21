//! `core::diagnose` 集成测试。
//!
//! 结构对齐 HMCL 的 `CrashReportAnalyzerTest` + `resources/logs`（analyse/02 §7.9）：
//! **每条规则一个样本**（`src/diagnose/rules.d/samples/`），断言「样本命中它自己」。
//! 规则库是数据不是代码，样本就是规则的验收标准——新增规则必须同时提交样本。
//!
//! 跑法：`cargo test -p unlucky-core --test diagnose`

mod collectors_probe;
mod engine_rank;
mod hs_err_parse;
mod loader_reload;
mod rules_samples;

/// 测试共用导入与辅助函数。
pub mod prelude {
    use std::path::{Path, PathBuf};

    pub use unlucky_core::diagnose::collectors::{
        collect_all, CollectionContext, CrashReportCollector, EnvironmentCollector,
        EnvironmentSnapshot, ExitCollector, ExitSignal, ExitType, HsErrCollector,
        LatestLogCollector, LogBuffer, ModRecord, SignalBundle, SignalCollector, SignalKind,
        SignalOrigin,
    };
    pub use unlucky_core::diagnose::engine::{
        Diagnoser, DiagnosisInput, DiagnosisReport, Evidence, Finding, CONCLUSIVE_CONFIDENCE,
    };
    pub use unlucky_core::diagnose::hs_err::{self, FrameKind, HsErrKind, HsErrReport};
    pub use unlucky_core::diagnose::loader::{ReloadOutcome, RuleLoader, RuleSet, EMBEDDED_RULES};
    pub use unlucky_core::diagnose::rules::{
        class_file_to_java_major, ComponentKind, FixAction, FixActionKind, GraphicsSetting,
        JavaVendor, ModAction, RuleCategory, Severity,
    };

    /// 规则目录（`crates/core/src/diagnose/rules.d`）。
    #[must_use]
    pub fn rules_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagnose/rules.d")
    }

    /// 读样本（路径相对 `rules.d/`）。
    pub fn read_sample(relative: &str) -> String {
        let path = rules_dir().join(relative);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("样本 {} 读不到：{error}", path.display()))
    }

    /// 用内嵌规则库跑一段文本。
    pub fn analyze_text(origin: SignalOrigin, text: &str) -> DiagnosisReport {
        let mut input = DiagnosisInput::new();
        input.push_text(origin, text);
        diagnoser().analyze(&input)
    }

    /// 内嵌规则库构造的引擎。
    #[must_use]
    pub fn diagnoser() -> Diagnoser {
        Diagnoser::new(RuleLoader::embedded().rules().to_vec())
    }

    /// 结论 id 列表（断言失败时打印用）。
    #[must_use]
    pub fn ids(report: &DiagnosisReport) -> Vec<&str> {
        report
            .findings
            .iter()
            .map(|finding| finding.rule_id.as_str())
            .collect()
    }

    /// 临时目录（进程 id + 计数器，不引 tempfile 依赖）。
    #[must_use]
    pub fn temp_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("ull-diagnose-{}-{tag}-{index}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("创建临时目录");
        dir
    }
}
