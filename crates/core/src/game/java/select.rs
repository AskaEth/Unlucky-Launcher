//! 「启动选哪个 Java」的**纯**优先链（D13）。
//!
//! 优先级：实例覆盖（`--java` / 档案 `javaPath`）> 托管目录里**满足要求**的 JDK >
//! `JAVA_HOME` > `PATH`。实例覆盖由调用方单独判（用户明确指定了就不参与打分），
//! 这里只处理「一堆已探测的候选 + 一个要求」的挑选。
//!
//! 挑选规则（比「机械按来源排序」更贴近用户预期）：
//!
//! 1. **满足要求的候选优先**——1.12.2 配 Forge/LiteLoader 时上限是 8，托管的 21 再新也不能选；
//! 2. 全都不满足时按**版本距离**（[`distance`]）取最接近的那个，同距离再按来源：
//!    托管 > `JAVA_HOME` > `PATH`（D13：托管目录是启动器的地盘，用户 PATH 上的 java
//!    可能是另一个 JDK 的残留）。距离优先而非来源优先是 D4：托管里只有 25、`JAVA_HOME`
//!    是 21 时，1.12.2 该被推上更接近的 21，而不是机械地选「来源排名更高」的托管 25；
//! 3. 挑完还要决定**要不要拦**（[`decide`]）：不符且用户没显式指定 Java 就不启动。
//!    「显式」只认 [`explicit_java`] 那两个来源（实例层 / 命令行），**全局默认不算**（M334）。
//!
//! 抽成纯函数是为了可测：测试注入「托管 21 + PATH 8」这类组合，不必真装 JDK。

use std::path::Path;

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

/// 候选主版本号与要求区间的**距离**（`0` = 满足要求）。
///
/// D4：主版本号是离散档位，用户装的 `17` 与 `25` 对「只能用 8」的 1.12.2（+ LaunchWrapper 系
/// 加载器）都是错的，但 `17` 只差 9 档、`25` 差 17 档——真机上「离得近」的那个起得来的概率明显
/// 更高（老版本对新运行时的容忍度随版本差递减）。
#[must_use]
pub fn distance(major: u32, required: &JavaRequirement) -> u32 {
    if major < required.minimum {
        return required.minimum - major;
    }
    match required.maximum {
        Some(maximum) if major > maximum => major - maximum,
        _ => 0,
    }
}

/// 从候选里挑一个：先按「离要求多远」，同距离再按来源优先序。
///
/// 探不到主版本号的候选排最后（不知道就不能算“近”）。
#[must_use]
pub fn pick<'a>(
    candidates: &'a [JavaRuntime],
    required: &JavaRequirement,
) -> Option<&'a JavaRuntime> {
    candidates.iter().min_by_key(|runtime| {
        let distance = runtime
            .major
            .map_or(u32::MAX, |major| distance(major, required));
        // 先排「距离远」，再排来源优先序（托管 > JAVA_HOME > PATH）。
        (distance, runtime.origin.rank())
    })
}

/// 启动前的取舍（D4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchVerdict {
    /// 照常启动（满足要求，或用户显式指定了 Java——那是他的明确选择）。
    Proceed,
    /// 拦下：不满足要求且没人明确指定，起不来是必然的，那句警告没有信息量。
    Block(JavaMismatch),
}

impl LaunchVerdict {
    /// 需要拦下时返回不符说明。
    #[must_use]
    pub fn block(self) -> Option<JavaMismatch> {
        match self {
            Self::Proceed => None,
            Self::Block(mismatch) => Some(mismatch),
        }
    }
}

/// 不满足要求时是「只告警」还是「拦住」（D4 的取舍，理由见 [`LaunchVerdict`]）。
///
/// 判据只有两条：不符的方向不重要（两个方向都必然起不来），**是不是用户自己选的**才重要。
#[must_use]
pub fn decide(mismatch: Option<JavaMismatch>, explicitly_chosen: bool) -> LaunchVerdict {
    match mismatch {
        Some(mismatch) if !explicitly_chosen => LaunchVerdict::Block(mismatch),
        _ => LaunchVerdict::Proceed,
    }
}

/// 「用户**显式指定**了要用哪个 Java」的判据（M334 / 台账 P38）。
///
/// 只有两个来源算显式，其余一律不算：
///
/// | 来源 | 例子 | 算不算 |
/// |---|---|---|
/// | **实例层**钉的 `launch.javaPath` | 用户在「实例 → 启动参数 → Java 路径」里填过 | 算 |
/// | **命令行**给的 `--java` | 用户这一次启动明确要试那个 JDK | 算 |
/// | **全局默认** `java.selected_path` | 用户在设置里「设为默认」过一次 | **不算** |
///
/// 全局默认为什么不算：它说的是「没别的主意时用这个」，不是「这一条实例就要这个」。把它算成
/// 显式的话，凡是在设置里指定过一次全局默认 Java 的用户（很常见），D4 那道闸对**所有**实例
/// 都失效——点 1.12.2（+ LaunchWrapper 系加载器）照常起、必崩，用户只看到「退出码 1」。
/// 这就是台账 P38 那个洞。
///
/// **纯函数**：不吃环境、不读设置、不读盘。「生效值是哪一份」由调用方按三级回落求
/// （[`crate::game::launch_overrides::resolve`]），本函数只答「算不算用户自己的选择」。
/// 全局默认**故意不进签名**：进来的只能是「实例层」与「命令行」这两个来源。
#[must_use]
pub fn explicit_java(instance_layer: Option<&Path>, command_line: Option<&Path>) -> bool {
    instance_layer.is_some() || command_line.is_some()
}

/// 「全局默认跑不了这个版本」时该不该**退回自动挑**（M334 / 复审 P2-2）。
///
/// 「用哪个 Java」的优先级：**实例层钉的** → **全局默认**（只在它满足要求时） → 自动挑。
/// 退回自动挑要同时成立：没有命令行 `--java`/档案 `javaPath`、实例层没钉 `launch.javaPath`、
/// 确实有生效值、而它判下来不符。
///
/// 默认是「没别的主意时用这个」，不是「这条实例就得用它」：设过全局默认 21 的用户点开要 25 的
/// 版本，装完 25 再点启动若还拿 21 去撞就永远停在同一个框里。GUI 与 CLI **共用这一个函数**——
/// 退回逻辑写两遍就是把「同一功能两条路径」从判据搬到挑 Java 上（复审 r2 P2-2）。
#[must_use]
pub fn fall_back_to_auto(
    instance_layer: Option<&Path>,
    command_line: Option<&Path>,
    effective: Option<&Path>,
    mismatched: bool,
) -> bool {
    !explicit_java(instance_layer, command_line) && effective.is_some() && mismatched
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
    use super::{decide, distance, explicit_java, fall_back_to_auto, resolve, LaunchVerdict};
    use crate::game::java::mapping::{JavaMismatch, JavaRequirement, RequirementSource};
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
        // 1.12.2 + Forge/LiteLoader 上限是 8：托管的 21 不满足，PATH 上的 8 满足 → 选 8。
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
    fn unsatisfiable_candidates_are_ranked_by_distance_not_by_origin() {
        // 1.12.2 + Forge/LiteLoader（min=max=8）：托管的 25 与 JAVA_HOME 的 17 都不满足要求。
        // 按来源排会选托管 25（rank 1 < 2），按版本距离应选 17（距离 9 vs 17）。
        let required = JavaRequirement::new(8, Some(8), RequirementSource::Table);
        let candidates = [
            runtime(25, JavaOrigin::Managed),
            runtime(17, JavaOrigin::JavaHome),
        ];
        assert_eq!(
            resolve(&candidates, &required).major(),
            17,
            "全不满足时应按版本距离选 17，而不是按来源选托管的 25"
        );
    }

    #[test]
    fn too_old_candidates_are_ranked_by_distance() {
        // 需要 21+：托管 8 与 JAVA_HOME 17 都太老（距离 13 vs 4）。
        let candidates = [
            runtime(8, JavaOrigin::Managed),
            runtime(17, JavaOrigin::JavaHome),
        ];
        assert_eq!(
            resolve(&candidates, &at_least(21)).major(),
            17,
            "都太老时应选最接近的那个，而不是托管目录里最老的那个"
        );
    }

    #[test]
    fn mismatch_is_reported_when_nothing_fits() {
        let candidates = [runtime(8, JavaOrigin::Path)];
        let picked = resolve(&candidates, &at_least(21));
        assert_eq!(picked.major(), 8);
        let mismatch = picked.mismatch.expect("应报不符");
        assert_eq!(mismatch.code(), "java_too_old");
    }

    #[test]
    fn distance_is_zero_inside_the_interval_and_positive_outside() {
        let legacy = JavaRequirement::new(8, Some(8), RequirementSource::Table);
        assert_eq!(distance(8, &legacy), 0);
        assert_eq!(distance(17, &legacy), 9);
        assert_eq!(distance(25, &legacy), 17);
        let modern = at_least(21);
        assert_eq!(distance(21, &modern), 0);
        assert_eq!(distance(25, &modern), 0);
        assert_eq!(distance(17, &modern), 4);
        // 区间 8–16（假想）：16 与 9 都满足，只有 17 超出一档。
        let band = JavaRequirement::new(8, Some(16), RequirementSource::Table);
        assert_eq!(distance(16, &band), 0);
        assert_eq!(distance(17, &band), 1);
    }

    #[test]
    fn unknown_majors_rank_last() {
        let mut unknown = runtime(17, JavaOrigin::Managed);
        unknown.major = None;
        let candidates = [unknown, runtime(25, JavaOrigin::JavaHome)];
        let picked = resolve(&candidates, &at_least(21));
        assert_eq!(
            picked.runtime.map(|runtime| runtime.major),
            Some(Some(25)),
            "探不到主版本号的候选不能因为来源排名高就胜出"
        );
    }

    #[test]
    fn mismatch_blocks_only_when_nobody_chose_that_java() {
        let mismatch = JavaMismatch::TooNew {
            maximum: 8,
            actual: 17,
        };
        assert_eq!(
            decide(Some(mismatch), false),
            LaunchVerdict::Block(mismatch)
        );
        assert_eq!(decide(Some(mismatch), true), LaunchVerdict::Proceed);
        assert_eq!(decide(None, false), LaunchVerdict::Proceed);
        assert_eq!(LaunchVerdict::Proceed.block(), None);
        assert_eq!(LaunchVerdict::Block(mismatch).block(), Some(mismatch));
        // 太老同样拦：装错方向不会改变「起不来」这个结局。
        let too_old = JavaMismatch::TooOld {
            required: 21,
            actual: 17,
        };
        assert_eq!(decide(Some(too_old), false).block(), Some(too_old));
    }

    /// **M334 / P38 的核心判据**：全局默认 `java.selected_path` **不算**显式指定。
    ///
    /// 抓得住：把「全局默认」也算进显式（`explicit_java` 收到实例层的 `None` 却返回 `true`，
    /// 或调用方把三级回落后的生效值当成「实例层」传进来）——那时下面这句 `assert!` 红，
    /// 而只要它红了，1.12.2 在设过全局默认的机器上就再也不会被拦。
    ///
    /// 抓不住：调用方**传错参数**这件事本身（本函数看不出 `instance_layer` 是实例层还是
    /// 全局层）。那一条由 product-path 用例兜
    /// （`presenter::host::launch::m334_tests::a_global_default_alone_still_blocks`）。
    #[test]
    fn a_global_default_is_not_explicit() {
        // 「用户在设置里点过一次『设为默认』」的真实形状：全局 `java.selected_path` 有值，
        // 而实例层没钉、命令行也没给 —— 进到本判据的就是 `(None, None)`。
        // 全局那一份**故意不进签名**：判据在调用方就把全局层丢掉了，这里断言的是「丢掉之后
        // 不许有人把它补回来」。
        assert!(
            !explicit_java(None, None),
            "只设了全局默认 Java ⇒ 不算用户显式指定（否则 D4 那道闸对所有实例失效）"
        );
    }
    /// 判据的两个来源各自成立，且**只有**这两个（四种组合全覆盖）。
    #[test]
    fn explicit_java_covers_exactly_two_sources() {
        let pinned = std::path::PathBuf::from("/jdks/8-temurin/bin/java");
        let flag = std::path::PathBuf::from("/jdks/17-temurin/bin/java");
        assert!(
            explicit_java(Some(&pinned), None),
            "实例层钉了 launch.javaPath ⇒ 显式"
        );
        assert!(explicit_java(None, Some(&flag)), "命令行给了 --java ⇒ 显式");
        assert!(explicit_java(Some(&pinned), Some(&flag)), "两个都给 ⇒ 显式");
        assert!(!explicit_java(None, None), "都没给 ⇒ 不是显式");
    }
    /// 「退回自动挑」的全部 16 种组合（四维各两值）：只有「没人指定 + 有生效值 + 不符」退。
    /// GUI 与 CLI 各调一次 [`fall_back_to_auto`] ⇒ 这张表同时守着两条路径（复审 r2 P2-2）。
    #[test]
    fn fall_back_to_auto_has_exactly_one_winning_combination() {
        let (pin, flag, eff) = (
            std::path::Path::new("/jdks/8/bin/java"),
            std::path::Path::new("/jdks/17/bin/java"),
            std::path::Path::new("/jdks/21/bin/java"),
        );
        let mut winners = Vec::new();
        for instance in [None, Some(pin)] {
            for command in [None, Some(flag)] {
                for effective in [None, Some(eff)] {
                    for mismatched in [false, true] {
                        if fall_back_to_auto(instance, command, effective, mismatched) {
                            let shape = (instance.is_some(), command.is_some());
                            winners.push((shape, effective.is_some(), mismatched));
                        }
                    }
                }
            }
        }
        assert_eq!(
            winners,
            vec![((false, false), true, true)],
            "只有「没人指定 + 有生效值 + 不符」才退回；别的组合都算用户自己选的"
        );
    }
}
