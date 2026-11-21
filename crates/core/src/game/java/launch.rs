//! 启动期 Java 解析的**唯一**入口：候选收集 + 两段式挑（M337 / 台账 P40）。
//!
//! ## 为什么单独一截
//!
//! GUI（`presenter::host::launch::pick_java`）与 CLI（`cli::commands::java_launch::resolve_for_launch`）
//! 以前**各自收集一次**候选：托管根怎么并、`JAVA_HOME` / `PATH` 怎么进、标准安装位置什么时候扫，
//! 两边各写一份。当时结论等价，但那是「同一件事的第二条路」（本仓已收掉三处：安装编排 /
//! 退回自动挑 / 宿主拼英文）。现在只剩这一处：两个调用方都只把 [`JavaResolution`] 包成自己那份
//! DTO（GUI 要一行人读摘要，CLI 要钉死的路径），**没有第二份收集**。
//!
//! ## 两段式：便宜的先挑，扫盘只在必要时（性能纪律，不是风格）
//!
//! 每条候选都要起一个 `java -version` 子进程，标准安装位置（Program Files 系、官方启动器的
//! `runtime/`、注册表）在慢盘上很贵。所以顺序是：
//!
//! 1. **便宜段**：托管目录 + `JAVA_HOME` + `PATH`；
//! 2. 这一段里挑中的那个**确定满足**要求 ⇒ 到此为止，**不进贵段**；
//! 3. 否则把标准安装位置并进来重挑一次（[`resolve_two_stage`]）。
//!
//! 「确定满足」比 [`JavaResolution`] 的 `mismatch.is_none()` 更严，判据是
//! **`conclusive`**：挑中的那个必须**已知主版本**且与要求相符。只判 `mismatch.is_none()` 不够——
//! 探不到主版本号的候选也算「无不符」（不知道就是不知道），而那种候选**可能被扫盘找到的 JDK
//! 打败**。早退判据放宽到 `mismatch.is_none()` 会在那种输入下挑错。
//!
//! 这条判据同时保证两段式与「一次全扫」挑出**同一个**：贵段多出来的来源在
//! [`pick`](super::select::pick) 里排名最低（[`JavaOrigin::System`] 的 rank 是 4），只有**距离更近**
//! 才可能翻盘，而「距离更近」⇔ 便宜段那个不满足要求（[`distance`](super::select::distance)）；同距离
//! 时它排在后面、来源又更低 ⇒ 不变。两段式只省时间，不改结论。
//!
//! ## 显式路径不参与打分
//!
//! 用户在实例里钉了 `launch.javaPath` / 命令行给了 `--java` 时**只探测那一个**（D13：明确的选择
//! 不推翻），收集器不并任何其它来源、也不会因为「探测不到」换一个——探测失败怎么兜底由调用方
//! 决定（GUI 落回版本要求的基准主版本，CLI 留 `major: None` 并钉住那条路径）。

use std::path::{Path, PathBuf};

use super::discover::{dedupe, standard_candidates};
use super::mapping::{check, JavaRequirement};
use super::probe::{discover_managed_roots, probe, system_candidates, JavaOrigin, JavaRuntime};
use super::select::{resolve, JavaResolution};

/// 启动期解析：显式路径 > 托管/环境 > 标准安装位置（惰性扫描）。
///
/// `managed_roots` 由调用方按自己的数据模式/落点求（`location::scan_roots`，一处实现）——
/// 本函数只管「这些根 + 环境 + 扫盘怎么并成候选」，不读设置。
#[must_use]
pub fn resolve_for_launch(
    managed_roots: &[PathBuf],
    required: &JavaRequirement,
    explicit: Option<&Path>,
) -> JavaResolution {
    if let Some(path) = explicit {
        // 明确的选择不参与打分：只探测这一个（探测失败也照样把它交出去）。
        return resolve(&[probe(path, JavaOrigin::Instance)], required);
    }
    let mut cheap = discover_managed_roots(managed_roots);
    cheap.extend(system_candidates());
    resolve_two_stage(&cheap, required, standard_candidates)
}

/// 两段式挑（纯函数）：便宜段先挑，**没有确定满足的候选**才调 `wide` 扫贵段。
///
/// `wide` 只在需要时才被调用——调用次数就是「这一启动要不要付扫盘的代价」，所以它必须是
/// `FnOnce`（也便于用例用计数/断言证明「没进贵段」）。`wide` 里的候选与便宜段合并时走
/// [`dedupe`]，去重保留**先出现**的那个（便宜段在前）⇒ 同一条 `java` 被两支同时报上来时，
/// 保下来的是来源排名更高的那份，与「一次全扫」的结果一致。
#[must_use]
pub fn resolve_two_stage<F>(
    cheap: &[JavaRuntime],
    required: &JavaRequirement,
    wide: F,
) -> JavaResolution
where
    F: FnOnce() -> Vec<JavaRuntime>,
{
    let mut candidates = dedupe(cheap.to_vec());
    let first = resolve(&candidates, required);
    if conclusive(&first, required) {
        return first;
    }
    candidates.extend(wide());
    resolve(&dedupe(candidates), required)
}

/// 「这一份已经可以定下来、不必再扫盘」的判据：挑中的那个**已知主版本**且与要求相符。
///
/// 探不到主版本号的候选不算确定（`mismatch: None` 说的是「不知道」，不是「相符」）。
fn conclusive(resolution: &JavaResolution, required: &JavaRequirement) -> bool {
    resolution
        .runtime
        .as_ref()
        .and_then(|runtime| runtime.major)
        .is_some_and(|major| check(major, required).is_none())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::PathBuf;

    use super::{resolve_for_launch, resolve_two_stage};
    use crate::game::java::mapping::{JavaRequirement, RequirementSource};
    use crate::game::java::probe::{JavaOrigin, JavaRuntime, DEFAULT_MAJOR};

    fn runtime(major: Option<u32>, origin: JavaOrigin, binary: &str) -> JavaRuntime {
        JavaRuntime {
            binary: PathBuf::from(binary),
            origin,
            version: major.map(|major| format!("openjdk version \"{major}.0.1\"")),
            major,
            bits: 64,
            vendor: None,
        }
    }

    fn at_least(minimum: u32) -> JavaRequirement {
        JavaRequirement::new(minimum, None, RequirementSource::Table)
    }

    /// **性能纪律的钉子**（复审/后续重构最容易弄丢的那条）：便宜段里已有「已知主版本且相符」
    /// 的候选 ⇒ **不进贵段**。
    ///
    /// 抓得住：把 [`resolve_two_stage`] 改成「无条件把 `wide` 也并进来」——那时 `wide` 的计数
    /// 变成 1，本节立刻红。这条正是「性能回归不会让任何测试变红」的那个洞：其余用例只断结论，
    /// 全扫与按需扫的结论一模一样。
    #[test]
    fn a_conclusive_cheap_candidate_never_enters_the_expensive_stage() {
        let cheap = [runtime(Some(21), JavaOrigin::Managed, "/jdks/21/bin/java")];
        let calls = Cell::new(0);
        let picked = resolve_two_stage(&cheap, &at_least(21), || {
            calls.set(calls.get() + 1);
            Vec::new()
        });

        assert_eq!(
            calls.get(),
            0,
            "便宜段已有相符的候选，标准安装位置一个字节都不该扫"
        );
        assert_eq!(picked.major(), 21);
        assert!(picked.is_satisfied());
    }

    /// **判据比 `mismatch.is_none()` 更严的那一格**：主版本未知的候选**不算**确定满足。
    ///
    /// 抓得住（变异）：早退判据改回 `resolution.is_satisfied()`（= `mismatch.is_none()`）——
    /// 那时「探不到主版本」被当成「相符」，直接早退，扫盘能找到的那份 Java 21 永远不被选中；
    /// 下面 `assert_eq!(picked.major(), 21)` 与 `runtime.major == Some(Some(21))` 立刻红。
    #[test]
    fn an_unknown_major_does_not_stop_the_search() {
        // 托管目录里那份 `java -version` 解析不出来 ⇒ `major: None`、`mismatch: None`。
        let cheap = [runtime(None, JavaOrigin::Managed, "/jdks/broken/bin/java")];
        let wide = [runtime(
            Some(21),
            JavaOrigin::System,
            "/opt/jdk-21/bin/java",
        )];
        let picked = resolve_two_stage(&cheap, &at_least(21), || wide.to_vec());

        assert_eq!(
            picked.runtime.as_ref().map(|runtime| runtime.major),
            Some(Some(21)),
            "主版本未知不是「相符」：扫盘能给出已知版本的 JDK 时必须以它为准"
        );
        assert_eq!(picked.major(), 21);
        assert!(picked.is_satisfied());
    }

    /// 便宜段全不满足 ⇒ 进贵段，且**贵段真的能翻盘**（这就是两段式存在的意义）。
    #[test]
    fn a_mismatching_cheap_stage_escalates_and_the_expensive_one_can_win() {
        let cheap = [runtime(Some(17), JavaOrigin::JavaHome, "/jdks/17/bin/java")];
        let wide = [runtime(
            Some(21),
            JavaOrigin::System,
            "/opt/jdk-21/bin/java",
        )];
        let picked = resolve_two_stage(&cheap, &at_least(21), || wide.to_vec());

        assert_eq!(picked.major(), 21, "17 太老，扫盘找到的 21 应当胜出");
        assert!(picked.is_satisfied());
    }

    /// 贵段的来源排名最低：同距离时**不许**抢走便宜段已经挑中的那个（两段式与「一次全扫」同一个结论）。
    ///
    /// 抓得住：合并时把 `wide` 放到便宜段**前面**（或按来源重排），或去重时保留后出现的那个
    /// （那时同路径的 17 会以 `System` 的身份留下，下面第二条断言红）。
    #[test]
    fn the_expensive_stage_never_outranks_the_cheap_one_at_the_same_distance() {
        // 要求恰好 21：17 太老（距离 4）、25 太新（距离 4）——**同距离**，胜负只能看来源。
        let exactly = JavaRequirement::new(21, Some(21), RequirementSource::Table);
        let cheap = [runtime(Some(17), JavaOrigin::Managed, "/jdks/17/bin/java")];
        // 同一份 17 被扫盘又报了一次（来源更低）+ 一份同距离的 25。
        let wide = [
            runtime(Some(17), JavaOrigin::System, "/jdks/17/bin/java"),
            runtime(Some(25), JavaOrigin::System, "/opt/jdk-25/bin/java"),
        ];
        let picked = resolve_two_stage(&cheap, &exactly, || wide.to_vec());

        assert_eq!(picked.major(), 17);
        assert_eq!(
            picked.runtime.map(|runtime| runtime.origin),
            Some(JavaOrigin::Managed),
            "同一条 java 被两支同时报上来时，留下的是先出现（来源更高）的那份"
        );
        assert_eq!(
            picked.mismatch.map(|mismatch| mismatch.code()),
            Some("java_too_old"),
            "全都不满足时要老实报不符，别因为扫过盘就当成挑到了"
        );
    }

    /// 显式路径：只探测这一个，不参与打分（也不并任何其它来源）。
    ///
    /// 抓得住：把显式那一支也走两段式——那时这份探测不出来的路径会被扫盘结果顶掉，
    /// 而用户明确钉过的东西不许被推翻（D13）。
    #[test]
    fn an_explicit_path_is_probed_alone() {
        let missing = PathBuf::from("Z:/ull-no-such-java/bin/java.exe");
        let picked = resolve_for_launch(&[], &at_least(21), Some(&missing));

        let runtime = picked
            .runtime
            .as_ref()
            .expect("显式指定的那条路径照样要交出去");
        assert_eq!(runtime.binary, missing);
        assert_eq!(runtime.origin, JavaOrigin::Instance);
        // 探测失败 ⇒ 主版本未知、无不符（不知道就是不知道）：兜底是**调用方**的事。
        assert_eq!(runtime.major, None);
        assert!(picked.is_satisfied());
    }

    /// 一条候选都没有：不编造，也不误报不符（`mismatch: None`）。
    ///
    /// 走 [`resolve_two_stage`] 而不是 `resolve_for_launch`：后者的贵段会扫本机真实的 JDK，
    /// 用例不能挂在「这台机器装了什么」上。
    #[test]
    fn no_candidate_at_all_reports_nothing() {
        let picked = resolve_two_stage(&[], &at_least(21), Vec::new);
        assert!(picked.runtime.is_none());
        assert_eq!(picked.major(), DEFAULT_MAJOR);
        assert!(picked.is_satisfied());
    }
}
