//! 「老版本**原版** + 高版本 Java」的那一句建议（M348 的判据，M349 拆成两档）。
//!
//! 从 `mismatch` 拆出来的理由有两条：那边贴着 400 行硬线；而「建议」与「不符」本来是两件事——
//! `check` 判的是**拦不拦**，这里判的是**要不要说一句**，谁都不该替对方说话。
//!
//! ## 为什么不是 `JavaMismatch::TooNew`
//!
//! `<1.13` 的上限来自 LaunchWrapper 系加载器（Forge / LiteLoader / OptiFine），原版没有这条
//! 约束（判据见 [`uses_launch_wrapper`](super::mapping::uses_launch_wrapper)）。M347 实测：
//! 原版 1.12.2 在 Java 17 上走到主菜单——**进世界未验**。所以文案也按这个强度写：
//! 不写「已支持 Java 17」，也不写「一定能玩」（那两句在目录里，`catalog/java_guard.rs`）。
//!
//! ## 两档证据强度（M349）
//!
//! 判据能区分到的那一档才说那一档，**不编阈值**（文案本身不在 core：两句人读的话在
//! `unlucky-i18n` 的 `JavaAdvisoryMeasured` / `JavaAdvisoryUnmeasured`，命令行与界面读同一批 key，
//! 证据数字由 [`LegacyAdvisory`] 带过去）：
//!
//! | 档 | 谁 | 说什么 |
//! |---|---|---|
//! | [`LegacyAdvisoryTier::Measured`] | 我们真跑过的那个版本（[`MEASURED_LEGACY_VERSION`]） | 「可能能启动」，并把证据强度写出来 |
//! | [`LegacyAdvisoryTier::Unmeasured`] | 同一条放宽下的其余 `<1.13` 原版 | 「这一档没实测过」，**直接建议**装低版本 Java |
//!
//! 为什么不按 `<1.9` / `1.9~1.12.2` 那样切：仓里没有支撑那个边界的判据。HMCL 的强制约束里
//! `<1.13` 只有两条（`LAUNCH_WRAPPER` 锁加载器、`VANILLA_LINUX_JAVA_8` 锁 Linux），都在 1.12
//! 结束、都不在原版 Windows 口径上；`diagnose/rules.d/jvm.yaml` 的 `jvm.jdk_9` 说的也是
//! LaunchWrapper 那一族（M348 已硬拦）。凭空写一个 1.9 等于把「不知道」说成「知道」。
//! 而「实测过没有」是**仓里真实存在的事实**，按它分档不需要任何新知识。
//!
//! ## 判据（四个前提，缺一不可）
//!
//! 1. 版本 id 是能解析出来的 `<1.13` 正式版（周快照与魔改名不提示——认不出年代就别乱说）；
//! 2. 要求里**没有上限**（有上限说明是 LaunchWrapper，那时该走 `TooNew` 的硬拦）；
//! 3. 实际主版本满足要求（不满足时 `check` 已经给出不符，别两句话说同一件事）；
//! 4. 实际主版本**高于**下限（本来就该用 8 的时候没什么可建议的）。

use super::mapping::{is_pre_1_13_release, version_parts, JavaRequirement};

/// 我们**真做过实测**的那个版本（M347：原版 1.12.2 + Java 17 走到主菜单，进世界未验）。
///
/// 它是 [`LegacyAdvisoryTier`] 唯一的分档依据：只有这一组有证据，其余一律按「没测过」说。
pub const MEASURED_LEGACY_VERSION: &str = "1.12.2";

/// 那一组实测里用的 Java 主版本（M347：原版 1.12.2 + **Java 17** 到主菜单）。
///
/// 与 [`MEASURED_LEGACY_VERSION`] 一起构成「证据」：改了它，命令行与界面说的数字一起变
/// （这两句文案只有 `unlucky-i18n` 里那一份，两边读同一批 key）。
pub const MEASURED_LEGACY_JAVA: u32 = 17;

/// 建议的**证据强度**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyAdvisoryTier {
    /// 这一组（版本 + 更高主版本 Java）我们真跑过：到主菜单，进世界未验。
    Measured,
    /// 同一条判据下的其余 `<1.13` 原版：**一次都没实测过**，所以才直接建议换 Java。
    Unmeasured,
}

impl LegacyAdvisoryTier {
    /// 稳定字符串 id（`--json` 的 `java.advisory_tier`；**不要改名**）。
    ///
    /// 接在 `ull launch --json` 上是为了脚本：`advisory` 那句是人读的话，改措辞不该让脚本崩，
    /// 而「这一档有没有实测过」是稳定事实（用例见 `cli/tests/java_test.rs`）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Measured => "measured",
            Self::Unmeasured => "unmeasured",
        }
    }
}

/// 一条建议的**事实**：哪个版本、要求多少、实际多少、证据到哪一档、证据是哪一组。
///
/// **这里一个字的散文都没有**（M349 r2 / 复审 P2-2）：两句人读文案在 `unlucky-i18n` 的
/// `JavaAdvisoryMeasured` / `JavaAdvisoryUnmeasured` 里，命令行与界面读**同一批 key**；本结构只负责
/// 判据、档位与五个数字。后两个数字（`measured_version` / `measured_java`）是
/// [`MEASURED_LEGACY_VERSION`] / [`MEASURED_LEGACY_JAVA`] 的**唯一出口**——目录里只有占位符，
/// 改常量两边同时变。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyAdvisory {
    /// 目标版本 id（原样，可能是 `1.7.10` 这种三段式）。
    pub version_id: String,
    /// 该版本按设计要用的 Java 主版本。
    pub minimum: u32,
    /// 这次挑中的 Java 主版本。
    pub actual: u32,
    /// 证据强度。
    pub tier: LegacyAdvisoryTier,
    /// 证据里那个**实测过的版本**（文案的 `{measured_version}`）。
    pub measured_version: &'static str,
    /// 证据里那个**实测用的 Java 主版本**（文案的 `{measured_java}`）。
    pub measured_java: u32,
}

/// 判据 → 一条建议的**事实**（`None` = 什么都不说）。
///
/// **命令行与 GUI 共用这一个函数**（M349）：「要不要说」与「哪一档」只在这里判一次，
/// 两个出口拿到的数字必然相同。
#[must_use]
pub fn legacy_advisory_facts(
    version_id: &str,
    required: &JavaRequirement,
    actual: u32,
) -> Option<LegacyAdvisory> {
    if !is_pre_1_13_release(version_id)
        || required.maximum.is_some()
        || !required.satisfied_by(actual)
        || actual <= required.minimum
    {
        return None;
    }
    Some(LegacyAdvisory {
        version_id: version_id.to_string(),
        minimum: required.minimum,
        actual,
        tier: tier_of(version_id),
        measured_version: MEASURED_LEGACY_VERSION,
        measured_java: MEASURED_LEGACY_JAVA,
    })
}

/// 这个版本在不在「实测过」那一档（见模块文档「两档证据强度」）。
fn tier_of(version_id: &str) -> LegacyAdvisoryTier {
    let measured = version_parts(MEASURED_LEGACY_VERSION);
    if version_parts(version_id) == measured {
        LegacyAdvisoryTier::Measured
    } else {
        LegacyAdvisoryTier::Unmeasured
    }
}

#[cfg(test)]
mod tests {
    use super::{
        legacy_advisory_facts, LegacyAdvisoryTier, MEASURED_LEGACY_JAVA, MEASURED_LEGACY_VERSION,
    };
    use crate::game::java::mapping::{requirement_for, JavaRequirement};

    /// **M348 的核心判据①**：原版 `<1.13` 没有上限 ⇒ 高版本 Java 只出建议，不是不符。
    ///
    /// 抓得住：把上限又写回静态表（`requirement_for("1.12.2").maximum = Some(8)`）——
    /// 那时 `check(17)` 会返回 `TooNew`，`mismatch` 那条用例同时红；这里则连建议都没有。
    #[test]
    fn vanilla_legacy_gets_advice_not_a_mismatch() {
        let vanilla = requirement_for("1.12.2");
        assert_eq!(vanilla.maximum, None, "原版 1.12.2 不该有上限");
        let advisory = legacy_advisory_facts("1.12.2", &vanilla, 17).expect("要有一条建议");
        assert_eq!(advisory.minimum, 8);
        assert_eq!(advisory.actual, 17);
        // 文案不在这里（core 只出事实，M349 r2）：句子在目录里，两壳读同一批 key。
    }

    /// 建议的四个前提各自成立才算数：认不出年代 / 达到上限被拦 / 本来就用 8 时都不提示。
    #[test]
    fn the_advisory_only_speaks_when_all_four_conditions_hold() {
        let vanilla = requirement_for("1.12.2");
        // ① 周快照 / 魔改名认不出年代 ⇒ 不提示。
        assert_eq!(legacy_advisory_facts("20w14a", &vanilla, 17), None);
        assert_eq!(legacy_advisory_facts("我的整合包", &vanilla, 17), None);
        // ② 实际就是 8 ⇒ 没什么可建议的。
        assert_eq!(legacy_advisory_facts("1.12.2", &vanilla, 8), None);
        // ③ 不符时由 `check` 说话（这里造一个太老的需求）。
        let too_new_requirement = JavaRequirement::table(21, None);
        assert_eq!(
            legacy_advisory_facts("1.12.2", &too_new_requirement, 17),
            None
        );
        // ④ ≥1.13 没有这条建议（1.16.5 也不在「老版本原版」的实测范围内）。
        let modern = requirement_for("1.16.5");
        assert_eq!(legacy_advisory_facts("1.16.5", &modern, 17), None);
    }

    /// **有上限 ⇒ 这条建议不开口**（那是 LaunchWrapper 的地盘，由 `TooNew` 硬拦）。
    #[test]
    fn launch_wrapper_requirements_never_get_the_advisory() {
        let loader = JavaRequirement::table(8, Some(8));
        assert_eq!(
            legacy_advisory_facts("1.12.2-forge-14.23.5.2860", &loader, 17),
            None
        );
    }

    /// **M349 判据**：只有实测过的那个版本算 `Measured`，其余 `<1.13` 原版一律 `Unmeasured`。
    ///
    /// 抓得住：把 `tier_of` 改成恒 `Measured`（1.7.10 也敢说「可能能启动」）——后一条断言红；
    /// 改成恒 `Unmeasured`（连实测过的那一档都不敢说）——前一条断言红。
    #[test]
    fn only_the_measured_version_speaks_as_measured() {
        let vanilla = requirement_for(MEASURED_LEGACY_VERSION);
        let measured =
            legacy_advisory_facts(MEASURED_LEGACY_VERSION, &vanilla, 17).expect("1.12.2");
        assert_eq!(measured.tier, LegacyAdvisoryTier::Measured);
        assert_eq!(measured.minimum, 8);
        assert_eq!(measured.actual, 17);

        let old = legacy_advisory_facts("1.7.10", &requirement_for("1.7.10"), 21).expect("1.7.10");
        assert_eq!(old.tier, LegacyAdvisoryTier::Unmeasured);
        assert_eq!(old.minimum, 8);
        assert_eq!(old.actual, 21);
        // 认不出的版本不给档（`None`）；`1.12.2-pre1` 这种后缀不改变年代判定。
        assert_eq!(legacy_advisory_facts("20w14a", &vanilla, 17), None);
        assert_eq!(
            legacy_advisory_facts("1.12.2-pre1", &vanilla, 17)
                .expect("预发布还是 1.12.2")
                .tier,
            LegacyAdvisoryTier::Measured
        );
    }

    /// **证据数字只有一个来源**（M349 r2 / 复审 P2-2）：两条建议都把
    /// [`MEASURED_LEGACY_VERSION`] / [`MEASURED_LEGACY_JAVA`] 原样带出来。
    ///
    /// 抓得住：把这两个字段改成写死的字面（或干脆不给）——目录里的 `{measured_version}` /
    /// `{measured_java}` 就没人填，命令行与界面同时露馅（两侧的用例都断言「输出里含常量」）。
    #[test]
    fn the_evidence_numbers_come_from_the_facts_not_from_the_catalog() {
        let vanilla = requirement_for(MEASURED_LEGACY_VERSION);
        for (version, actual) in [("1.12.2", 17), ("1.7.10", 21)] {
            let facts = legacy_advisory_facts(version, &vanilla, actual).expect(version);
            assert_eq!(facts.measured_version, MEASURED_LEGACY_VERSION);
            assert_eq!(facts.measured_java, MEASURED_LEGACY_JAVA);
        }
        // 稳定 id 接在 `--json` 上（P3-1），两个值都成了契约的一部分。
        assert_eq!(LegacyAdvisoryTier::Measured.as_str(), "measured");
        assert_eq!(LegacyAdvisoryTier::Unmeasured.as_str(), "unmeasured");
    }
}
