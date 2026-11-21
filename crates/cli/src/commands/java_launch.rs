//! 启动期的 Java 解析（`ull launch` 用）：实例覆盖 > 托管匹配 > 系统。
//!
//! 从 `commands/java.rs` 拆出来（那边还要放 list/install/vendors，超 400 行硬上限）。
//! 这里的职责是三条：①把实例自己那个根的**要求**交给 core 的收集器、拿到选中的那个 Java
//! （`core::game::java::launch::resolve_for_launch`）；②把选中的那个**钉成路径**交给装配
//! （D4：校验过的就是启动用的，不许装配期再重选一遍）；③把结果整理成 `--json` 块与告警行。
//! 候选**怎么收**（托管根 / 环境变量 / 标准安装位置的扫法）与「谁优先」（`select`）都在 core，
//! 本文件只做装配。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use unlucky_core::game::java::{
    self, JavaMismatch, JavaRequirement, JavaRuntime, LegacyAdvisory, LegacyAdvisoryTier,
};
use unlucky_i18n::Key;

use crate::context::Ctx;

/// 启动期的 Java 解析结果。
#[derive(Debug, Clone)]
pub struct LaunchJava {
    /// 交给 `PlanInput.java` 的**钉死路径**：D4 校验过的那个 java。
    ///
    /// 五个来源一视同仁（M220 修的就是这条）：以前只有 `managed` 会钉住，
    /// `JAVA_HOME` / `PATH` / 标准安装位置的命中会被 `snapshot::prepare` 的
    /// `selection()` 按环境**再挑一遍**——于是 D4 判过的那个不是真启动的那个。
    /// `None` 只可能是「一个候选都没探到」，此时只能让 `snapshot::prepare` 兜底。
    pub override_path: Option<PathBuf>,
    /// 实际选中的运行时。
    pub runtime: Option<JavaRuntime>,
    /// 目标版本对 Java 的要求。
    pub requirement: JavaRequirement,
    /// 不符说明（`None` = 满足要求，或压根没探测到 Java）。
    ///
    /// `--dry-run` 只把它当告警打出来；真启动时先过 [`unlucky_core::game::java::select::decide`]：
    /// 不符且没人显式指定 Java 就直接拦下、不 spawn（D4）。
    pub mismatch: Option<JavaMismatch>,
    /// 「老版本**原版** + 高版本 Java」的一句建议（M348/M349）：**不拦**，只是提示。
    ///
    /// 判据全在 core（[`unlucky_core::game::java::legacy_advisory_facts`]）：版本是能认出来的
    /// `<1.13`、要求里没有上限（有上限 = LaunchWrapper 系，那走 `TooNew` 的硬拦）、**探测到**的
    /// Java 满足下限且更高。`None` 的其余情形一律不提——没有证据的组合不猜。
    ///
    /// **这句话由目录渲染**（M349 r2 / 复审 P2-2）：与界面同一个 key
    /// （`JavaAdvisoryMeasured` / `JavaAdvisoryUnmeasured`），core 只给事实与证据数字。
    pub advisory: Option<String>,
    /// 建议的**证据档位**稳定 id（`measured` / `unmeasured`；`--json` 的 `java.advisory_tier`）。
    ///
    /// 那句人读的话是散文，改措辞就会漂；「这一档有没有实测过」是稳定事实，脚本要能读它。
    pub advisory_tier: Option<&'static str>,
}

impl LaunchJava {
    /// `--json` 的 `java` 块（app 层 GUI 弹窗 P1 后直接吃这份数据）。
    ///
    /// `launched` 是装配之后**真正要执行的** java 路径（`plan.request.java.binary`）。
    /// 它作为参数传进来而不是在这里自己算：读的人不该去猜「`binary` 是不是真跑的那个」，
    /// 所以两个事实并排给出（`binary` = D4 校验过的，`launch_binary` = 装配落地的）。
    #[must_use]
    pub fn to_json(&self, launched: &Path) -> Value {
        let runtime = self.runtime.as_ref();
        json!({
            "binary": runtime.map(|runtime| runtime.binary.display().to_string()),
            "launch_binary": launched.display().to_string(),
            "major": runtime.and_then(|runtime| runtime.major),
            "bits": runtime.map(|runtime| runtime.bits),
            "source": runtime.map(|runtime| runtime.origin.as_str()),
            "vendor": runtime.and_then(|runtime| runtime.vendor.clone()),
            "required": self.requirement.describe(),
            "required_major": self.requirement.minimum,
            "required_max_major": self.requirement.maximum,
            "requirement_source": self.requirement.source.as_str(),
            // M348：老版本原版 + 高版本 Java ⇒ 一句建议（不拦）。GUI 这一版不吃这个字段，
            // 命令行照打一行（见 `launch.rs` 的 `建议:` 行）；放进来是为了脚本也能读到。
            "advisory": self.advisory,
            "advisory_tier": self.advisory_tier,
            "mismatch": self.mismatch.map(|mismatch| json!({
                "code": mismatch.code(),
                "message": mismatch.message(),
            })),
        })
    }

    /// 告警行（dry-run 与真启动都输出）。
    ///
    /// `launched` 是装配之后真正要执行的 java 路径，用来兜一条「钉路径失效」的自检。
    ///
    /// 三类：①用户指定的 Java 探测不出来（路径写错了，静默退回默认版本会很难查）；
    /// ②Java 与版本要求不符——`--dry-run` 到此为止只提示；真启动会先过 `select::decide`，
    /// 只有「用户自己指定了 Java」时才会带着这条告警继续起（D4）；
    /// ③装配落地的不是 D4 校验过的那个（钉路径没生效，M220 的回归闸）。
    #[must_use]
    pub fn warnings(&self, launched: &Path) -> Vec<String> {
        let mut warnings = Vec::new();
        if let Some(runtime) = &self.runtime {
            if runtime.major.is_none() {
                warnings.push(format!(
                    "探测不到 {} 的 `java -version` 输出（路径可能不对）",
                    runtime.binary.display()
                ));
            }
        }
        if let Some(mismatch) = self.mismatch {
            warnings.push(mismatch.message());
        }
        if let Some(unpinned) = self.pin_mismatch(launched) {
            warnings.push(unpinned);
        }
        warnings
    }

    /// 装配落地的 java 与 D4 校验过的那个是否一致（不一致要当场喊出来）。
    fn pin_mismatch(&self, launched: &Path) -> Option<String> {
        match (&self.runtime, self.override_path.as_deref()) {
            (Some(_), Some(validated)) if validated == launched => None,
            (Some(_), Some(validated)) => Some(format!(
                "装配用的 Java（{}）与校验过的（{}）不一致：这次启动没有走 D4 判过的那个",
                launched.display(),
                validated.display()
            )),
            // 选出了一个 java 却没钉住路径 ⇒ 装配期会按环境再挑一遍，D4 判的就不是真跑的那个。
            (Some(runtime), None) => Some(format!(
                "选中了 {}（{}）却没钉住路径，装配期按环境重选成了 {}：这一份没有经过 D4 校验",
                runtime.binary.display(),
                runtime.origin.as_str(),
                launched.display()
            )),
            (None, _) => Some(format!(
                "没探测到任何 Java，装配期按环境兜底用了 {}（这一份没有经过 D4 校验）",
                launched.display()
            )),
        }
    }

    /// 人读一行：Java 版本 + 来源 + **具体可执行文件** + 该版本的要求。
    ///
    /// 路径必须写出来（M220）：五个来源都钉路径之后，「用的是哪个 java」不能再靠猜
    /// ——同机装两个 21 的时候，只有路径能把两个 Jar 头日志对上的问题说清。
    #[must_use]
    pub fn summary(&self) -> String {
        let (version, origin) = match &self.runtime {
            Some(runtime) => (
                runtime
                    .major
                    .map_or_else(|| "未知".to_string(), |major| major.to_string()),
                format!("{} · {}", runtime.origin.as_str(), runtime.binary.display()),
            ),
            None => ("未知".to_string(), "未探测到".to_string()),
        };
        format!(
            "Java {version}（{origin}）· 该版本要求 {}",
            self.requirement.describe()
        )
    }
}

/// 解析启动用的 Java：实例覆盖 > 托管匹配 > 系统。
///
/// 候选**怎么收**全在 core（[`unlucky_core::game::java::launch::resolve_for_launch`]，M337/P40）：
/// 显式路径只探测那一个；否则托管 + `JAVA_HOME`/`PATH` 先挑，没有「已知主版本且相符」的候选才扫
/// 标准安装位置（Program Files 系、官方启动器的 `runtime/`、注册表）。本函数只做两件事：把实例
/// 自己那个根的**要求**交给它、把结果钉成路径。
///
/// 以前这里自己收一遍候选，与 GUI 的 `pick_java` 是「同一件事的第二条路」（结论当时等价，
/// 但两处的早退判据差一点：`mismatch.is_none()` 在探不到主版本时也成立，而那种候选可能被扫盘
/// 找到的 JDK 打败）。现在收集只剩 core 那一处，两个调用方的差别只有各自的 DTO。
#[must_use]
pub fn resolve_for_launch(ctx: &Ctx, version_id: &str, explicit: Option<&Path>) -> LaunchJava {
    let requirement = java::requirement_for_version(&ctx.game(), version_id);
    planned_java(
        java::resolve_for_launch(&super::java::managed_roots(ctx), &requirement, explicit),
        Some(version_id),
    )
}

/// 装配：从候选里挑一个，并把它**钉成路径**（候选可注入，测试不必真装 JDK）。
#[must_use]
pub fn plan_java(candidates: &[JavaRuntime], requirement: &JavaRequirement) -> LaunchJava {
    // 测试口没有版本 id ⇒ 不给建议：建议要判版本年代（`legacy_advisory_facts` 的第一条前提）。
    planned_java(java::resolve(candidates, requirement), None)
}

/// 一次解析结果 → CLI 那份 DTO（`override_path` 就是 D4 校验过的那个运行时）。
fn planned_java(resolution: java::JavaResolution, version_id: Option<&str>) -> LaunchJava {
    // D4：`override_path` 就是 D4 校验过的那个运行时——五种来源一个口径，
    // 不分「托管要钉、环境变量可以放它再挑一次」。
    let override_path = resolution
        .runtime
        .as_ref()
        .map(|runtime| runtime.binary.clone());
    // M348/M349：老版本原版 + 高版本 Java 的建议（不拦）；主版本探不到就不说（与 GUI 同口径）。
    let facts = version_id.and_then(|version_id| {
        let major = resolution
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.major)?;
        java::legacy_advisory_facts(version_id, &resolution.required, major)
    });
    let advisory = facts.as_ref().map(advisory_text);
    let advisory_tier = facts.as_ref().map(|facts| facts.tier.as_str());
    LaunchJava {
        override_path,
        runtime: resolution.runtime,
        requirement: resolution.required,
        mismatch: resolution.mismatch,
        advisory,
        advisory_tier,
    }
}

/// core 的**事实** → 人读那一句：**与界面同一个 key**（M349 r2 / 复审 P2-2）。
///
/// CLI 取中文档那份（`crate::catalog`），界面取当前语言那份；字面只有目录里一份，
/// 证据数字由 core 的 facts 带过来——改常量两侧一起变。
fn advisory_text(facts: &LegacyAdvisory) -> String {
    let key = match facts.tier {
        LegacyAdvisoryTier::Measured => Key::JavaAdvisoryMeasured,
        LegacyAdvisoryTier::Unmeasured => Key::JavaAdvisoryUnmeasured,
    };
    let min = facts.minimum.to_string();
    let actual = facts.actual.to_string();
    let measured_java = facts.measured_java.to_string();
    crate::catalog::CATALOG.render(
        key,
        &[
            ("version", &facts.version_id),
            ("min", &min),
            ("actual", &actual),
            ("measured_version", facts.measured_version),
            ("measured_java", &measured_java),
        ],
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use unlucky_core::game::java::{JavaOrigin, RequirementSource};

    use super::{plan_java, JavaRequirement, JavaRuntime};

    fn runtime(major: u32, origin: JavaOrigin) -> JavaRuntime {
        JavaRuntime {
            binary: PathBuf::from(format!("/jdks/{}/{major}/bin/java", origin.as_str())),
            origin,
            version: Some(format!("openjdk version \"{major}.0.1\"")),
            major: Some(major),
            bits: 64,
            vendor: None,
        }
    }

    /// 五种来源都必须把选中项的路径钉住（M220 的核心不变量）。
    ///
    /// 抓得住：退回「只有 `managed` 才钉路径」——那时 `JAVA_HOME` / `PATH` / `system` /
    /// `instance` 四种来源的 `override_path` 会变成 `None`，装配期按环境再挑一遍，
    /// D4 校验过的那个就不是真启动的那个了。
    #[test]
    fn every_origin_pins_the_selected_binary() {
        for origin in [
            JavaOrigin::Instance,
            JavaOrigin::Managed,
            JavaOrigin::JavaHome,
            JavaOrigin::Path,
            JavaOrigin::System,
        ] {
            let chosen = runtime(21, origin);
            let planned = plan_java(
                std::slice::from_ref(&chosen),
                &JavaRequirement::new(17, None, RequirementSource::Table),
            );
            assert_eq!(planned.runtime.as_ref(), Some(&chosen), "{origin:?}");
            assert_eq!(
                planned.override_path,
                Some(chosen.binary.clone()),
                "{origin:?} 的命中必须钉成路径，不能留给装配期重选"
            );
        }
    }

    /// 钉的是**选中那个**，不是候选里第一个/来源排名最高的那个。
    ///
    /// 抓得住：把 `override_path` 写成 `candidates.first()`，或者钉住一个与
    /// `runtime` 不同的运行时（D4 判 A 却启动 B）。
    #[test]
    fn the_pinned_binary_is_the_one_that_satisfies_the_requirement() {
        let legacy = runtime(8, JavaOrigin::JavaHome);
        let modern = runtime(21, JavaOrigin::Managed);
        let planned = plan_java(
            &[modern, legacy.clone()],
            &JavaRequirement::new(8, Some(8), RequirementSource::Table),
        );
        assert_eq!(planned.runtime.map(|runtime| runtime.major), Some(Some(8)));
        assert_eq!(planned.override_path, Some(legacy.binary));
        assert!(planned.mismatch.is_none());
    }

    /// 一个候选都没有时不编造路径（`None` = 让装配期按环境兜底），也不误报不符。
    #[test]
    fn no_candidate_pins_nothing() {
        let planned = plan_java(
            &[],
            &JavaRequirement::new(21, None, RequirementSource::Table),
        );
        assert!(planned.runtime.is_none());
        assert_eq!(planned.override_path, None);
        assert!(planned.mismatch.is_none());
    }

    use unlucky_core::game::java::{
        legacy_advisory_facts, requirement_for, LegacyAdvisoryTier, MEASURED_LEGACY_JAVA,
        MEASURED_LEGACY_VERSION,
    };
    use unlucky_i18n::Key;

    use super::{advisory_text, LaunchJava};
    /// 目录里那句建议，用 core 给的五个数字渲染（**期望**的那一句）。
    fn expected(key: Key, facts: &unlucky_core::game::java::LegacyAdvisory) -> String {
        let min = facts.minimum.to_string();
        let actual = facts.actual.to_string();
        let measured_java = facts.measured_java.to_string();
        crate::catalog::CATALOG.render(
            key,
            &[
                ("version", &facts.version_id),
                ("min", &min),
                ("actual", &actual),
                ("measured_version", facts.measured_version),
                ("measured_java", &measured_java),
            ],
        )
    }

    /// **复审 r3 P3-1**：`advisory_tier` 两档都真的进 `--json`（`tests/java_test.rs` 已贴线，
    /// 判据放这里）。
    #[test]
    fn the_json_carries_the_tier_for_both_kinds_of_advice() {
        let path = std::path::Path::new("/jdks/unused/bin/java");
        let with =
            |required: unlucky_core::game::java::JavaRequirement,
             facts: Option<&unlucky_core::game::java::LegacyAdvisory>| LaunchJava {
                override_path: None,
                runtime: None,
                requirement: required,
                mismatch: None,
                advisory: facts.map(advisory_text),
                advisory_tier: facts.map(|facts| facts.tier.as_str()),
            };
        for (version, major, tier) in [
            (MEASURED_LEGACY_VERSION, MEASURED_LEGACY_JAVA, "measured"),
            ("1.7.10", 21, "unmeasured"),
        ] {
            let required = requirement_for(version);
            let facts = legacy_advisory_facts(version, &required, major).expect("有建议");
            let json = with(required, Some(&facts)).to_json(path);
            assert_eq!(json["advisory_tier"], tier, "{json}");
            assert_eq!(json["advisory"], advisory_text(&facts), "{json}");
        }
        let json = with(requirement_for("1.20.1"), None).to_json(path);
        assert!(
            json["advisory"].is_null() && json["advisory_tier"].is_null(),
            "{json}"
        );
    }

    /// **复审 r2 P2-2 的判据**：命令行那句**就是目录里那条 key**（两档各一条）。抓得住：把
    /// `advisory_text` 换回 `format!` 自己拼——两处 `assert_eq!` 立刻红。
    #[test]
    fn the_advisory_sentence_comes_from_the_catalog_key() {
        let measured = legacy_advisory_facts(
            MEASURED_LEGACY_VERSION,
            &requirement_for(MEASURED_LEGACY_VERSION),
            MEASURED_LEGACY_JAVA,
        )
        .expect("实测那一档");
        assert_eq!(measured.tier, LegacyAdvisoryTier::Measured);
        assert_eq!(
            advisory_text(&measured),
            expected(Key::JavaAdvisoryMeasured, &measured)
        );

        let unmeasured =
            legacy_advisory_facts("1.7.10", &requirement_for("1.7.10"), 21).expect("没测过那一档");
        assert_eq!(unmeasured.tier, LegacyAdvisoryTier::Unmeasured);
        assert_eq!(
            advisory_text(&unmeasured),
            expected(Key::JavaAdvisoryUnmeasured, &unmeasured)
        );
        let texts = [advisory_text(&measured), advisory_text(&unmeasured)];
        assert_ne!(texts[0], texts[1], "两档必须说两句不同的话");
        let measured_java = format!("Java {MEASURED_LEGACY_JAVA}");
        for text in &texts {
            assert!(text.contains(MEASURED_LEGACY_VERSION), "{text}");
            assert!(text.contains(&measured_java), "{text}");
            assert!(!text.contains("已支持"), "{text}");
        }
    }
}
