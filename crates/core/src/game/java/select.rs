//! 「启动选哪个 Java」的**纯**优先链（D13）。
//!
//! 优先级：实例覆盖（`--java` / 档案 `javaPath`）> 托管目录里**满足要求**的 JDK >
//! `JAVA_HOME` > `PATH`。实例覆盖由调用方单独判（用户明确指定了就不参与打分），
//! 这里只处理「一堆已探测的候选 + 一个要求」的挑选。
//!
//! 挑选规则（比「机械按来源排序」更贴近用户预期）：
//!
//! 1. **满足要求的候选优先**——1.12.2 只能用 Java 8，托管的 21 再新也不能选；
//! 2. 同档内按来源：托管 > `JAVA_HOME` > `PATH`（D13：托管目录是启动器的地盘，
//!    用户 PATH 上的 java 可能是另一个 JDK 的残留）；
//! 3. 全都不满足时仍给一个（按来源优先取最好那个），并报不符——**提示而不是拒绝**。
//!
//! 抽成纯函数是为了可测：测试注入「托管 21 + PATH 8」这类组合，不必真装 JDK。

use super::mapping::{check, JavaMismatch, JavaRequirement};
use super::probe::{JavaRuntime, DEFAULT_MAJOR};

/// 一次启动的 Java 选择结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaResolution {
    /// 选中的运行时（一个候选都没有时 `None`）。
    pub runtime: Option<JavaRuntime>,
    /// 目标版本对 Java 的要求。
    pub required: JavaRequirement,
    /// 不符说明（`None` = 满足要求，或压根没探测到 Java）。
    pub mismatch: Option<JavaMismatch>,
}

impl JavaResolution {
    /// 生效的主版本号（没有运行时按缺省值）。
    #[must_use]
    pub fn major(&self) -> u32 {
        self.runtime
            .as_ref()
            .map_or(DEFAULT_MAJOR, JavaRuntime::major_or_default)
    }

    /// 是否满足要求（没有运行时视为「不知道」，返回 `true`）。
    #[must_use]
    pub fn is_satisfied(&self) -> bool {
        self.mismatch.is_none()
    }
}

/// 从候选里挑一个：满足要求的优先，同档内按来源优先序。
#[must_use]
pub fn pick<'a>(
    candidates: &'a [JavaRuntime],
    required: &JavaRequirement,
) -> Option<&'a JavaRuntime> {
    candidates.iter().min_by_key(|runtime| {
        let ok = runtime
            .major
            .is_some_and(|major| required.satisfied_by(major));
        // 先排「不满足」（`ok=false` 的 key 大），再排来源优先序。
        (!ok, runtime.origin.rank())
    })
}

/// 挑 + 判不符。
#[must_use]
pub fn resolve(candidates: &[JavaRuntime], required: &JavaRequirement) -> JavaResolution {
    let runtime = pick(candidates, required).cloned();
    let mismatch = runtime
        .as_ref()
        .and_then(|runtime| runtime.major)
        .and_then(|major| check(major, required));
    JavaResolution {
        runtime,
        required: required.clone(),
        mismatch,
    }
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use crate::game::java::mapping::{JavaRequirement, RequirementSource};
    use crate::game::java::probe::{JavaOrigin, JavaRuntime, DEFAULT_MAJOR};

    fn runtime(major: u32, origin: JavaOrigin) -> JavaRuntime {
        JavaRuntime {
            binary: std::path::PathBuf::from(format!("java{major}")),
            origin,
            version: Some(format!("openjdk version \"{major}.0.1\"")),
            major: Some(major),
            bits: 64,
            vendor: None,
        }
    }

    fn at_least(minimum: u32) -> JavaRequirement {
        JavaRequirement::new(minimum, None, RequirementSource::Table)
    }

    #[test]
    fn satisfying_managed_runtime_beats_mismatching_system_java() {
        let candidates = [
            runtime(8, JavaOrigin::Path),
            runtime(21, JavaOrigin::Managed),
        ];
        let picked = resolve(&candidates, &at_least(17));
        assert_eq!(picked.major(), 21);
        assert!(picked.is_satisfied());
    }

    #[test]
    fn satisfying_system_java_beats_mismatching_managed() {
        // 1.12.2 只能 Java 8：托管的 21 不满足，PATH 上的 8 满足 → 选 8。
        let required = JavaRequirement::new(8, Some(8), RequirementSource::Table);
        let candidates = [
            runtime(21, JavaOrigin::Managed),
            runtime(8, JavaOrigin::Path),
        ];
        let picked = resolve(&candidates, &required);
        assert_eq!(picked.major(), 8);
        assert!(picked.is_satisfied());
    }

    #[test]
    fn managed_wins_when_both_satisfy() {
        let candidates = [
            runtime(21, JavaOrigin::Path),
            runtime(17, JavaOrigin::JavaHome),
            runtime(17, JavaOrigin::Managed),
        ];
        assert_eq!(resolve(&candidates, &at_least(17)).major(), 17);
    }

    #[test]
    fn java_home_beats_path_when_both_satisfy() {
        let candidates = [
            runtime(21, JavaOrigin::Path),
            runtime(21, JavaOrigin::JavaHome),
        ];
        let picked = resolve(&candidates, &at_least(21));
        assert_eq!(
            picked.runtime.map(|runtime| runtime.origin),
            Some(JavaOrigin::JavaHome)
        );
    }

    #[test]
    fn no_candidate_keeps_the_default_and_reports_no_mismatch() {
        let picked = resolve(&[], &at_least(21));
        assert_eq!(picked.major(), DEFAULT_MAJOR);
        assert!(picked.runtime.is_none());
        assert!(picked.is_satisfied());
    }

    #[test]
    fn mismatch_is_reported_when_nothing_fits() {
        let candidates = [runtime(8, JavaOrigin::Path)];
        let picked = resolve(&candidates, &at_least(21));
        assert_eq!(picked.major(), 8);
        let mismatch = picked.mismatch.expect("应报不符");
        assert_eq!(mismatch.code(), "java_too_old");
    }
}
