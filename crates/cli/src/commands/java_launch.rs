//! 启动期的 Java 解析（`ull launch` 用）：实例覆盖 > 托管匹配 > 系统。
//!
//! 从 `commands/java.rs` 拆出来（那边还要放 list/install/vendors，超 400 行硬上限）。
//! 这里的职责只有两条：①按 D13 的优先链挑一个 Java；②把结果整理成 `--json` 块与告警行。
//! 真正「谁优先」的判定在 `core::game::java::probe` / `select`（纯函数），本文件只做装配。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use unlucky_core::game::java::{self, JavaMismatch, JavaOrigin, JavaRequirement, JavaRuntime};

use crate::context::Ctx;

/// 启动期的 Java 解析结果。
#[derive(Debug, Clone)]
pub struct LaunchJava {
    /// 喂给 `PlanInput.java` 的显式覆盖；`None` = 让 `snapshot::prepare` 按系统兜底。
    pub override_path: Option<PathBuf>,
    /// 实际选中的运行时。
    pub runtime: Option<JavaRuntime>,
    /// 目标版本对 Java 的要求。
    pub requirement: JavaRequirement,
    /// 不符说明（只提示，不阻断）。
    pub mismatch: Option<JavaMismatch>,
}

impl LaunchJava {
    /// `--json` 的 `java` 块（app 层 GUI 弹窗 P1 后直接吃这份数据）。
    #[must_use]
    pub fn to_json(&self) -> Value {
        let runtime = self.runtime.as_ref();
        json!({
            "binary": runtime.map(|runtime| runtime.binary.display().to_string()),
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
    /// 两类：①用户指定的 Java 探测不出来（路径写错了，静默退回默认版本会很难查）；
    /// ②Java 与版本要求不符（D13：提示，不阻断）。
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
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
        warnings
    }

    /// 人读一行（Java 来源 + 要求）。
    #[must_use]
    pub fn summary(&self) -> String {
        let source = self
            .runtime
            .as_ref()
            .map_or("未探测到", |runtime| runtime.origin.as_str());
        let version = self
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.major)
            .map_or_else(|| "未知".to_string(), |major| major.to_string());
        format!(
            "Java {version}（{source}）· 该版本要求 {}",
            self.requirement.describe()
        )
    }
}

/// 解析启动用的 Java：实例覆盖 > 托管匹配 > 系统。
///
/// 覆盖优先级的求值顺序写在 `core::game::java::probe`（纯函数，可注入候选）；
/// 这里只负责「实例覆盖单独成路」与「把结果钉成路径」。
#[must_use]
pub fn resolve_for_launch(ctx: &Ctx, version_id: &str, explicit: Option<&Path>) -> LaunchJava {
    let requirement = java::requirement_for_version(&ctx.game(), version_id);
    let (runtime, mismatch) = match explicit {
        Some(path) => {
            let runtime = java::probe(path, JavaOrigin::Instance);
            let mismatch = runtime
                .major
                .and_then(|major| java::check(major, &requirement));
            (Some(runtime), mismatch)
        }
        None => {
            // 先看托管 + 环境变量；都不满足要求才去扫盘（别为了一次启动白跑 N 次 java -version）。
            // 托管区可能不止一个根（Portable 与 Standard/Custom 并存，D23）。
            let roots = super::java::managed_roots(ctx);
            let mut candidates = java::discover_managed_roots(&roots);
            candidates.extend(java::system_candidates());
            let mut resolution = java::resolve(&candidates, &requirement);
            if resolution.mismatch.is_some() {
                candidates.extend(java::standard_candidates());
                resolution = java::resolve(&java::discover::dedupe(candidates), &requirement);
            }
            (resolution.runtime, resolution.mismatch)
        }
    };
    // 托管 JDK 要钉死路径（用户 PATH 上的 java 可能正是我们要避开的那一个）；
    // 系统来源交回 `snapshot::prepare` 的 `selection()` 兜底，避免重复实现一遍环境判断。
    let override_path = match &runtime {
        Some(runtime) if runtime.origin == JavaOrigin::Managed => Some(runtime.binary.clone()),
        _ => explicit.map(Path::to_path_buf),
    };
    LaunchJava {
        override_path,
        runtime,
        requirement,
        mismatch,
    }
}
