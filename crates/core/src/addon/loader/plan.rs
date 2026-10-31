//! 加载器安装计划：一份 loader 版本 JSON + 需要补下载的库清单。
//!
//! 计划层与网络层分开：网络在 [`super::meta`]，这里只做**纯计算**——把已经解析好的
//! loader 版本 JSON 交给 [`crate::game::install::libraries`] 展开成待下载文件，再包成
//! M27 的 [`InstallPlan`]（落盘/跳过判定/原子写版本 JSON 全部沿用同一套实现）。
//!
//! 为什么不直接调 [`InstallPlan::build`]：那是"装一个完整原版版本"的入口（客户端 jar +
//! 资源索引 + 逐对象），而 loader 层**只有库**——jar 与资源都由 `inheritsFrom` 的原版提供。
//! 直接组 [`InstallPlan`] 的字段既省掉一堆"没有 jar"的告警，又不复制任何路径规则。

use std::path::PathBuf;

use serde_json::{json, Value};

use super::forge::InstallProfile;
use super::LoaderKind;
use crate::game::arguments::Environment;
use crate::game::install::libraries;
use crate::game::install::{InstallPlan, PlanFile};
use crate::game::layout::GameLayout;
use crate::game::manifest::VersionJson;

/// 加载器版本的可用路线（判据是"能不能不跑 java 就装出来"，不是加载器名字）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoaderSupport {
    /// 纯元数据/纯下载路线：不跑任何外部进程。
    JsonRoute,
    /// 官方 installer 声明了必须先跑的 processors（拆包/映射），走 M35 执行器外调 java。
    RequiresInstaller {
        /// 人读原因（含该版本声明的 processors 数量与作用）。
        reason: String,
    },
}

impl LoaderSupport {
    /// 稳定字符串 id（JSON 输出用）。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::JsonRoute => "json_route",
            Self::RequiresInstaller { .. } => "requires_installer",
        }
    }

    /// 是否可真正安装（两条路线都走到了：processors 走 M35 执行器）。
    #[must_use]
    pub const fn is_installable(&self) -> bool {
        true
    }

    /// 是否要外调 java 跑 installer processors。
    #[must_use]
    pub const fn requires_installer(&self) -> bool {
        matches!(self, Self::RequiresInstaller { .. })
    }

    /// JSON 视图。
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::JsonRoute => json!({"mode": self.as_str(), "requires_installer": false}),
            Self::RequiresInstaller { reason } => {
                json!({"mode": self.as_str(), "requires_installer": true, "reason": reason})
            }
        }
    }
}

/// 一次解析出来的加载器版本（网络层产物，计划层的输入）。
#[derive(Debug, Clone)]
pub struct LoaderRelease {
    /// 加载器种类。
    pub kind: LoaderKind,
    /// 目标原版版本（`inheritsFrom` 的父版本 id）。
    pub mc_version: String,
    /// 加载器自身版本号（`0.15.11` / `47.2.0` / `21.1.80`）。
    pub loader_version: String,
    /// 生成的 loader 版本 id（`1.20.1-fabric-0.15.11`，即 `versions/<id>/` 的目录名）。
    pub version_id: String,
    /// loader 版本 JSON（`inheritsFrom` 指向原版）。
    pub version_json: VersionJson,
    /// 可走哪条路线。
    pub support: LoaderSupport,
    /// installer jar 的磁盘路径（M35 执行器的输入；Fabric 系为 `None`）。
    pub installer: Option<PathBuf>,
    /// installer 里读出来的 install profile（执行器要 `processors` / `data` / `libraries`）。
    pub install_profile: Option<InstallProfile>,
    /// 解析期告警（不致命：例如 profile 缺 loader 库时我们补了一条）。
    pub warnings: Vec<String>,
}

impl LoaderRelease {
    /// 是否可真正安装。
    #[must_use]
    pub const fn is_installable(&self) -> bool {
        self.support.is_installable()
    }

    /// 是否要外调 java 跑 installer processors。
    #[must_use]
    pub const fn requires_installer(&self) -> bool {
        self.support.requires_installer()
    }

    /// 需要 installer 时的原因（`JsonRoute` 返回 `None`）。
    #[must_use]
    pub fn installer_reason(&self) -> Option<&str> {
        match &self.support {
            LoaderSupport::JsonRoute => None,
            LoaderSupport::RequiresInstaller { reason } => Some(reason),
        }
    }

    /// JSON 视图（`--dry-run` 与诊断共用）。
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "kind": self.kind.as_str(),
            "label": self.kind.label(),
            "mc_version": self.mc_version,
            "loader_version": self.loader_version,
            "version_id": self.version_id,
            "inherits_from": self.version_json.inherits_from,
            "jar": self.version_json.jar,
            "main_class": self.version_json.main_class,
            "libraries": self.version_json.libraries.as_ref().map_or(0, Vec::len),
            "support": self.support.to_json(),
            "warnings": self.warnings,
        })
    }
}

/// [`LoaderInstallPlan::build`] 的输入。
pub struct LoaderPlanInput<'a> {
    /// 游戏目录布局（`versions/` `libraries/` 的路径规则）。
    pub layout: &'a GameLayout,
    /// 已解析的加载器版本。
    pub release: &'a LoaderRelease,
    /// 规则求值环境（OS / 架构 / 特性开关）。
    pub environment: &'a Environment,
    /// 原版的 `versions/<mc>/<mc>.json` 是否已在盘上。
    pub vanilla_installed: bool,
}

/// 加载器安装计划：M27 的 [`InstallPlan`] + 加载器自己的上下文。
///
/// 执行期直接 `execute(&engine, &plan.install, …)`：那份计划里的 `version_json` 就是
/// 带 `inheritsFrom` 的 loader 层，`execute` 会在**全部库就绪后**原子写到
/// `versions/<loader-id>/<loader-id>.json`（半成品不写，沿 M27 的纪律）。
#[derive(Debug, Clone)]
pub struct LoaderInstallPlan {
    /// 加载器版本上下文。
    pub release: LoaderRelease,
    /// 待执行的下载计划（库 + 版本 JSON 落盘）。
    pub install: InstallPlan,
    /// 是否还需要先装原版（原版已装则 `false`，真装时会跳过）。
    pub vanilla_install_needed: bool,
}

impl LoaderInstallPlan {
    /// 组计划。
    ///
    /// 缺下载地址的库降级成告警（见 [`Self::summary_json`] 的 `warnings`），不硬失败——
    /// 与 M27「宁可给出可修复的骨架，也不静默假装成功」一致。
    #[must_use]
    pub fn build(input: LoaderPlanInput<'_>) -> Self {
        let release = input.release;
        let (files, library_warnings) = libraries::plan(
            input.layout,
            &release.version_id,
            release
                .version_json
                .libraries
                .as_deref()
                .unwrap_or_default(),
            input.environment,
        );

        let mut warnings = release.warnings.clone();
        warnings.extend(library_warnings);
        if let Some(reason) = release.installer_reason() {
            warnings.push(format!("需要外调 java 跑 installer processors：{reason}"));
        }
        if !input.vanilla_installed {
            warnings.push(format!(
                "原版 {} 尚未安装：执行时会先装原版再叠加载器",
                release.mc_version
            ));
        }

        let install = InstallPlan {
            version_id: release.version_id.clone(),
            // jar 来自原版层（生成的 loader JSON 已把 `jar` 钉成原版 id）。
            jar_id: release.mc_version.clone(),
            game_root: input.layout.root().to_path_buf(),
            version_json_path: input.layout.version_json(&release.version_id),
            files,
            version_json: release.version_json.clone(),
            prefetched: Vec::new(),
            warnings,
        };

        Self {
            release: release.clone(),
            install,
            vanilla_install_needed: !input.vanilla_installed,
        }
    }

    /// 计划里的待下载文件。
    #[must_use]
    pub fn files(&self) -> &[PlanFile] {
        &self.install.files
    }

    /// 告警（含"原版未装""本期不能真装"）。
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.install.warnings
    }

    /// 汇总 JSON（`--dry-run` 的主载荷）。
    #[must_use]
    pub fn summary_json(&self) -> Value {
        let mut summary = self.install.summary_json();
        if let Some(object) = summary.as_object_mut() {
            object.insert("loader".to_string(), self.release.to_json());
            object.insert(
                "vanilla_install_needed".to_string(),
                json!(self.vanilla_install_needed),
            );
        }
        summary
    }
}
