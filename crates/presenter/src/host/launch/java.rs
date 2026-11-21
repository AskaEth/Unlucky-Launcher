//! 「用哪个 Java」：显式路径（实例 → 全局）> 托管 > JAVA_HOME > PATH（M161 从 `host/launch.rs` 拆出）。
//!
//! 拆出的理由与其它 400 行拆分一致：`host/launch.rs` 到线了，而「挑 Java」本来就是独立一段。
//! 口径没变（见原文件与 `core::game::java` 的文档）：显式路径**先探测**，探测失败按版本要求的
//! 基准主版本兜底——启动路径不该因为探测不到就拒绝启动。
//!
//! M331：这里还要报**挑中的那个 Java 与目标版本的要求差在哪**（[`JavaPick::mismatch`]）——
//! 判断本身是 core 的 `check`（`game::java::mapping::check`），本模块一个字的规则都不重写。
//! 后果见 `host::launch::accept`：不符且用户没显式指定就不启动（D4 口径，CLI 早就是这么做的）。
//!
//! M337（台账 P40）：**候选怎么收**不再在这里写。以前本模块与 CLI 的
//! `commands::java_launch::resolve_for_launch` 各自收一次候选（托管根 / 环境变量 / 标准安装位置
//! 什么时候扫各写一份），结论等价却是「同一件事的第二条路」。现在收集只剩 core 一处
//! （`game::java::launch::resolve_for_launch`：便宜的先挑、没有「已知主版本且相符」的候选才扫盘），
//! 本模块只做两件 GUI 自己的事：求托管根（`location::scan_roots`，与 CLI 同一个函数）、把
//! [`JavaResolution`] 包成界面上那一行摘要 + 交给 adapter 的 [`JavaSelection`]。

use std::path::PathBuf;

use unlucky_core::game::java::{JavaMismatch, JavaRequirement, JavaResolution, DEFAULT_MAJOR};
use unlucky_core::game::GameLayout;
use unlucky_core::launch::request::JavaSelection;

use super::super::env::HostEnv;

/// 选中并用起来的 Java（`summary` 给 UI 一行提示）。
pub struct JavaPick {
    /// 交给 adapter 的选择
    pub selection: JavaSelection,
    /// 人读一行
    pub summary: String,
    /// 目标版本对 Java 的**要求**（与 [`Self::mismatch`] 同一份，来自实例自己那个根）。
    ///
    /// 拦下时界面要说「该版本需要 Java X 及以上」——上面的数字从这里取，不给界面第二次读盘的理由。
    pub required: JavaRequirement,
    /// 与目标版本要求**不符**的说明（`None` = 满足要求，或压根没探测到主版本号）。
    ///
    /// 「拦不拦」不在这里定：显式指定过的 Java 也照样算出不符（用户要能看见「你自己挑的这个
    /// 与版本要求差在哪」），放不放行由 [`unlucky_core::game::java::select::decide`] 判。
    pub mismatch: Option<JavaMismatch>,
    /// **探测到**的主版本号（`None` = 探测不出来，`selection.major` 那个是兜底值）。
    ///
    /// 为什么单独留一格（M349 r2 / 复审 P3-3）：`selection.major` 在探测失败时要给 adapter 一个
    /// 数字（按版本要求兜底），但**建议句不能拿兜底值当事实**——那会平白编出一个「这次用的是
    /// Java 17」。命令行那一侧一直就是「不知道就不说」（`runtime.major?`），这里是同一个口径。
    pub probed_major: Option<u32>,
}

/// 选一个能用的 Java：显式路径（实例 → 全局 `java.selected_path`）> 托管 > JAVA_HOME > PATH。
///
/// `game` 是这个实例**自己那个根**的布局（M274）：版本清单里的 `javaVersion` 只在那个根下，
/// 拿配置根读不到 ⇒ 退回「按 id 的静态表」，而第三方启动器的自定义名版本（`樱落怡然客户端`）
/// `version_parts` 为空 ⇒ `minimum = 8`，一个要 21 的 modpack 会被拉去用 17 跑
/// （M274 r1 复审 P1-2 的探针：实例自己根 minimum=21，配置根读出来 8，实际选中 17）。
///
/// 显式路径**先探测**（`java -version`）：探测失败不报错，按版本要求的基准主版本兜底——
/// 与自动挑的退路同一口径（启动路径不该因为探测不到就拒绝启动）。
///
/// M331：两条分支都算 [`JavaPick::mismatch`]。显式那一支以前只 `probe`、从不与要求比对，
/// 于是「用户自己钉了一个不合适的 JDK」这件事到崩之前一个字都没有。
pub fn pick_java(
    env: &HostEnv,
    game: &GameLayout,
    version_id: &str,
    explicit: Option<&std::path::Path>,
) -> JavaPick {
    let required = unlucky_core::game::java::requirement_for_version(game, version_id);
    // 显式路径只探测那一个（core 那条分支不并任何其它来源）⇒ 托管根都不必求，
    // 少读一次设置、少一次路径解析。
    let roots = if explicit.is_some() {
        Vec::new()
    } else {
        super::super::engine::java_scan_roots(env)
    };
    let resolution = unlucky_core::game::java::resolve_for_launch(&roots, &required, explicit);
    java_pick(resolution, explicit.is_some())
}

/// 解析结果 → 界面上那一行 + 交给 adapter 的选择。
///
/// 三支各有各的兜底（口径见 [`pick_java`] 的文档）：
/// - 显式路径探测失败 ⇒ 主版本落回**版本要求的基准**（启动路径不该因此拒绝启动）；
/// - 自动挑中了一个 ⇒ 主版本缺失时落回 [`DEFAULT_MAJOR`]；
/// - 一个候选都没有 ⇒ 交给 adapter 裸 `java`（PATH 上的那个），主版本按要求的基准。
fn java_pick(resolution: JavaResolution, explicit: bool) -> JavaPick {
    let required = resolution.required.clone();
    let (selection, summary) = match (&resolution.runtime, explicit) {
        (Some(runtime), true) => {
            let major = runtime.major.unwrap_or(required.minimum);
            (
                JavaSelection::new(runtime.binary.clone(), major, runtime.bits),
                format!(
                    "实例指定 Java {}（{}）",
                    runtime.version.clone().unwrap_or_else(|| major.to_string()),
                    runtime.binary.display()
                ),
            )
        }
        (Some(runtime), false) => {
            let major = runtime.major.unwrap_or(DEFAULT_MAJOR);
            let vendor = runtime
                .vendor
                .as_ref()
                .map(|vendor| format!(" · {vendor}"))
                .unwrap_or_default();
            (
                JavaSelection::new(runtime.binary.clone(), major, runtime.bits),
                format!(
                    "Java {}（{}{vendor}）",
                    runtime.version.clone().unwrap_or_else(|| major.to_string()),
                    runtime.origin.category(),
                ),
            )
        }
        (None, _) => {
            let major = required.minimum;
            (
                JavaSelection::new(
                    PathBuf::from(unlucky_core::game::java::java_binary_name()),
                    major,
                    64,
                ),
                format!("PATH 上的 Java（未探测到版本，按 Java {major} 启动）"),
            )
        }
    };
    // 探测到的主版本号：**在任何兜底之前**取（M349 r2 / 复审 P3-3）。
    let probed_major = resolution
        .runtime
        .as_ref()
        .and_then(|runtime| runtime.major);
    JavaPick {
        selection,
        summary,
        required,
        // 不符口径在 core 的 `resolve` 里：探不到主版本号的候选不判「不符」（不知道就是不知道）。
        mismatch: resolution.mismatch,
        probed_major,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use unlucky_core::game::java::{
        JavaOrigin, JavaRequirement, JavaResolution, JavaRuntime, RequirementSource, DEFAULT_MAJOR,
    };

    use super::java_pick;

    /// **复审 P3-3**：探测不到主版本时，`selection.major` 可以有兜底值（adapter 要一个数字），
    /// 但 [`super::JavaPick::probed_major`] **必须是 `None`**——建议句绝不拿兜底值当事实。
    ///
    /// 抓得住：把 `probed_major` 写成 `Some(selection.major)`（或让建议改用 `selection.major`）
    /// ——那时一个探测失败的候选会让界面说「这次用的是 Java 17」，而命令行的口径是「不知道就不说」。
    #[test]
    fn an_unprobed_java_reports_no_major_even_though_selection_falls_back() {
        let broken =
            JavaRuntime::unknown(PathBuf::from("/jdks/broken/bin/java"), JavaOrigin::Managed);
        let resolution = JavaResolution {
            runtime: Some(broken),
            required: JavaRequirement::new(8, None, RequirementSource::Table),
            mismatch: None,
        };
        let pick = java_pick(resolution, false);
        assert_eq!(pick.probed_major, None, "探测不到就是不知道，不给建议");
        assert_eq!(
            pick.selection.major, DEFAULT_MAJOR,
            "adapter 那边仍要一个数字（兜底值）——这两件事不能混"
        );
    }
}
