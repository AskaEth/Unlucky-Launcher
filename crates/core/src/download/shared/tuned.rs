//! 带调优取引擎：把「灌设置」与「取引擎」绑成一次调用（M203）。
//!
//! [`super::set_tuning`] 与 [`super::engine_for`] 是两处显式同步点，宿主少调前一步就会**静默**
//! 拿到出厂参数的引擎——CLI 的 `install` / `java` / `pack` 此前各自 `DownloadEngine::new(
//! EngineConfig::default(), …)`，用户在设置里配的并发 / 重试 / gh-proxy 一概不生效。
//! 把它们绑成一次调用，漏调就成了类型错误而不是「跑起来但用出厂值」。
//!
//! 现有 [`super::engine_for`] 的签名与行为**不动**（presenter 走 `set_tuning` + `engine_for`
//! 两步，语义相同，见 `presenter::host::settings::download::apply_tuning`）。

use std::path::Path;
use std::sync::Arc;

use super::super::config::DownloadTuning;
use super::super::engine::DownloadEngine;
use super::{engine_for, set_tuning};

/// 先灌该数据根的调优，再取它的共享引擎（调优没变时复用的是同一个引擎）。
///
/// 幂等：值没变时 `set_tuning` 一行不动，所以「每次取引擎前都调」不会白扔连接池。
#[must_use]
pub fn engine_with_tuning(
    cache_root: impl AsRef<Path>,
    tune: DownloadTuning,
) -> Arc<DownloadEngine> {
    let root = cache_root.as_ref();
    let _ = set_tuning(root, tune);
    engine_for(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::config::{DEFAULT_MAX_ATTEMPTS_PER_SOURCE, DEFAULT_MAX_CONCURRENCY};
    use std::path::PathBuf;

    fn root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ull-shared-tuned-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建缓存根");
        dir
    }

    /// **调优与取引擎必须是一步**：调 `engine_with_tuning` 就该拿到吃这套参数的引擎。
    ///
    /// 抓得住的变异：把 `engine_with_tuning` 里的 `set_tuning(...)` 删掉（只留 `engine_for`）⇒
    /// 探针停在出厂 8，红。抓不住的：设置键 → 调优那一段（在 CLI 侧，`cli/src/download/tests.rs`）。
    #[test]
    fn the_tuning_reaches_the_engine_it_returns() {
        let dir = root("reach");
        let engine =
            engine_with_tuning(&dir, DownloadTuning::from_settings(Some(3), Some(2), None));
        assert_eq!(engine.config().max_concurrency, 3);
        assert_eq!(engine.concurrency_probe(), 3, "闸门真开了 3 张许可");
        assert_eq!(engine.config().max_attempts_per_source, 2);

        // 出厂调优与「没灌过」等价：不该因为这次调用就把引擎参数写坏。
        let plain = engine_with_tuning(&dir, DownloadTuning::default());
        assert_eq!(plain.config().max_concurrency, DEFAULT_MAX_CONCURRENCY);
        assert_eq!(
            plain.config().max_attempts_per_source,
            DEFAULT_MAX_ATTEMPTS_PER_SOURCE
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 调优变了就要换引擎（`set_tuning` 的那半个承诺在这里也成立）。
    ///
    /// 抓得住的变异：`engine_with_tuning` 只 `engine_for` 不看调优的变更 ⇒ 交回旧引擎（`ptr_eq` 红）。
    #[test]
    fn a_changed_tuning_yields_a_rebuilt_engine() {
        let dir = root("rebuild");
        let first = engine_with_tuning(&dir, DownloadTuning::from_settings(Some(2), None, None));
        let same = engine_with_tuning(&dir, DownloadTuning::from_settings(Some(2), None, None));
        assert!(Arc::ptr_eq(&first, &same), "同值不该重建");
        let next = engine_with_tuning(&dir, DownloadTuning::from_settings(Some(5), None, None));
        assert!(!Arc::ptr_eq(&first, &next), "改了并发必须重建");
        assert_eq!(next.concurrency_probe(), 5);
        std::fs::remove_dir_all(&dir).ok();
    }
}
