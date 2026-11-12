//! 启动期的 Java 解析（`ull launch` 用）：实例覆盖 > 托管匹配 > 系统。
//!
//! 从 `commands/java.rs` 拆出来（那边还要放 list/install/vendors，超 400 行硬上限）。
//! 这里的职责是三条：①按 D13 的优先链挑一个 Java；②把选中的那个**钉成路径**交给装配
//! （D4：校验过的就是启动用的，不许装配期再重选一遍）；③把结果整理成 `--json` 块与告警行。
//! 真正「谁优先」的判定在 `core::game::java::probe` / `select`（纯函数），本文件只做装配。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use unlucky_core::game::java::{self, JavaMismatch, JavaOrigin, JavaRequirement, JavaRuntime};

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
            "requirement_source": self.requirement.source.as_str(),
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
/// 候选收集有代价（托管扫盘、`java -version` 子进程），所以顺序是「先便宜的、不满足才扫盘」：
/// 托管 + `PATH`/`JAVA_HOME` 不满足要求时，才把标准安装位置（Program Files 系、
/// 官方启动器的 `runtime/`、注册表）并进来重挑一次。
#[must_use]
pub fn resolve_for_launch(ctx: &Ctx, version_id: &str, explicit: Option<&Path>) -> LaunchJava {
    let requirement = java::requirement_for_version(&ctx.game(), version_id);
    let Some(path) = explicit else {
        // 托管区可能不止一个根（Portable 与 Standard/Custom 并存，D23）。
        let roots = super::java::managed_roots(ctx);
        let mut candidates = java::discover_managed_roots(&roots);
        candidates.extend(java::system_candidates());
        let planned = plan_java(&candidates, &requirement);
        if planned.mismatch.is_none() {
            return planned;
        }
        candidates.extend(java::standard_candidates());
        return plan_java(&java::discover::dedupe(candidates), &requirement);
    };
    // 用户自己指定了 Java：只探测这一个，不参与打分（D13：明确的选择不推翻）。
    plan_java(&[java::probe(path, JavaOrigin::Instance)], &requirement)
}

/// 纯装配：从候选里挑一个，并把它**钉成路径**（候选可注入，测试不必真装 JDK）。
#[must_use]
pub fn plan_java(candidates: &[JavaRuntime], requirement: &JavaRequirement) -> LaunchJava {
    let resolution = java::resolve(candidates, requirement);
    // D4：`override_path` 就是 D4 校验过的那个运行时——五种来源一个口径，
    // 不分「托管要钉、环境变量可以放它再挑一次」。
    let override_path = resolution
        .runtime
        .as_ref()
        .map(|runtime| runtime.binary.clone());
    LaunchJava {
        override_path,
        runtime: resolution.runtime,
        requirement: requirement.clone(),
        mismatch: resolution.mismatch,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use unlucky_core::game::java::RequirementSource;

    use super::{plan_java, JavaOrigin, JavaRequirement, JavaRuntime};

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
}
