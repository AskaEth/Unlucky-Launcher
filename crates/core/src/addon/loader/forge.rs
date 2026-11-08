//! NeoForge / Forge：installer 内的 install profile → loader 版本 JSON。
//!
//! 依据 `DevDocs/analyse/05-modloader-modpack.md` §1.2/§1.3：Forge 有两代 installer 协议，
//! NeoForge 是 fork（早期与 Forge 完全同构）。判据也在那儿：install profile 有没有
//! `spec` / processors、有没有独立的 `version.json`。maven 坐标与版本表在 [`super::maven`]。
//!
//! # 本模块的取舍（为什么只做一部分）
//!
//! 这两家都**不发**launcher 直接可用的 profile，只发 installer jar；能纯 json 化到什么程度
//! 取决于 installer 声明的处理器：
//!
//! | install profile 形状 | 实测样例 | 本模块结论 |
//! |----------------------|----------|-----------|
//! | `spec: 1` + 有 client 侧 processors | Forge 1.20.1-47.2.0、NeoForge 21.1.80 | [`LoaderSupport::RequiresInstaller`] |
//! | `spec: 0` + 无 processors + 内嵌 `version.json` | Forge 1.12.2-14.23.5.2860 | [`LoaderSupport::JsonRoute`] |
//! | 只有内联 `versionInfo`（+ `install.filePath` 解包） | 1.12.2 之前的老重建版 | 明确 unsupported |
//!
//! 第一类必须跑 `java -cp … installertools …` 才能产出 `client-slim/extra` 等打补丁产物
//! （processor 的 `data` 变量表里有 `MC_SLIM`/`MC_EXTRA`/`PATCHED`），解析结果因此带上
//! [`LoaderSupport::RequiresInstaller`] 标记 + install profile 本体，交给
//! [`super::processors`]（M35 执行器）真跑；这里的**判定**仍只管「要不要跑 installer」。
//!
//! 第二类的本体 jar 在 version.json 里 `url` 是空串，且**官方 maven 只发布 `-universal`
//! classifier**（无 classifier 路径官方 404，只有 BMCLAPI 的 302 别名兜底）——
//! 补救见 [`fill_missing_urls`]，URL 带 classifier、磁盘落点保持官方 installer 的原样。
//!
//! # 一条容易踩的坑：processor 的库不进启动 classpath
//!
//! install profile 自带的 `libraries` 是**processor 依赖**（installertools/srgutils…），
//! 它们不属于游戏 classpath。把它们并进 loader 版本 JSON 会让启动多加载一堆无关 jar，
//! 因此这里只记数量进告警、绝不合并；真下载它们的是 [`super::processors`]。

use std::collections::BTreeMap;

use serde::Deserialize;

use super::maven;
use super::plan::{LoaderRelease, LoaderSupport};
use super::{LoaderError, LoaderKind};
use crate::game::library::Library;
use crate::game::manifest::VersionJson;

pub use super::processors::spec::{Datum, Processor};

/// 老协议 installer 的 `install` 段（`filePath` 指向内嵌的 universal jar）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyInstall {
    /// 内嵌 jar 在 installer 里的相对路径。
    #[serde(default)]
    pub file_path: Option<String>,
}

/// installer 里的 `install_profile.json`（只解析我们需要的键，其余忽略）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProfile {
    /// 协议代号（`1` = processor 时代）。
    #[serde(default)]
    pub spec: Option<i64>,
    /// 组件名（`forge` / `NeoForge`）。
    #[serde(default)]
    pub profile: Option<String>,
    /// installer 自己的版本串（`1.20.1-forge-47.2.0`）。
    #[serde(default)]
    pub version: Option<String>,
    /// 目标原版版本。
    #[serde(default)]
    pub minecraft: Option<String>,
    /// 内嵌版本 JSON 在 installer 里的路径（通常是 `/version.json`）。
    #[serde(default, rename = "json")]
    pub version_json_path: Option<String>,
    /// universal jar 的 maven 坐标（`net.minecraftforge:forge:…`，spec 0 才有）。
    #[serde(default)]
    pub path: Option<String>,
    /// processor 列表。
    #[serde(default)]
    pub processors: Vec<Processor>,
    /// processor 的变量表（`{MAPPINGS}` / `{MC_SLIM}` 之类占位的数据源）。
    #[serde(default)]
    pub data: BTreeMap<String, Datum>,
    /// processor / universal jar 依赖（**不是**启动 classpath，别合并）。
    #[serde(default)]
    pub libraries: Vec<Library>,
    /// 老协议：内嵌 jar 解包信息。
    #[serde(default)]
    pub install: Option<LegacyInstall>,
    /// 老协议：内联的版本 JSON 片段。
    #[serde(default, rename = "versionInfo")]
    pub version_info: Option<VersionJson>,
}

impl InstallProfile {
    /// client 侧 processor 数量。
    #[must_use]
    pub fn client_processors(&self) -> usize {
        self.processors
            .iter()
            .filter(|processor| processor.applies_to_client())
            .count()
    }

    /// 是否是老协议 installer（内联 `versionInfo` + 解包 universal jar）。
    #[must_use]
    pub fn is_legacy_protocol(&self) -> bool {
        self.version_json_path.is_none() && (self.install.is_some() || self.version_info.is_some())
    }
}

/// 用 install profile + 内嵌版本 JSON 组 loader 版本。
///
/// `embedded` 是 installer 内 `version.json` 的内容（读不到就是 `None`）。
///
/// # Errors
///
/// 原版版本对不上、installer 里没有 `version.json`、生成的清单缺主类时返回错误。
pub fn build_release(
    kind: LoaderKind,
    mc_version: &str,
    maven_version: &str,
    profile: &InstallProfile,
    embedded: Option<&VersionJson>,
) -> Result<LoaderRelease, LoaderError> {
    if let Some(declared) = profile.minecraft.as_deref() {
        if declared != mc_version {
            return Err(LoaderError::metadata(format!(
                "installer 声明的原版是 {declared}，与目标的 {mc_version} 不符（选错了加载器版本）"
            ))
            .with_hint("`--loader-version` 要和 `install <version>` 的原版版本配套"));
        }
    }
    let loader_version = maven::display_loader_version(kind, mc_version, maven_version);
    let version_id = kind.version_id(mc_version, &loader_version);
    let mut warnings = Vec::new();

    let Some(embedded) = embedded else {
        let reason = if profile.is_legacy_protocol() {
            format!(
                "{} {} 走老协议 installer（内联 versionInfo + 解包 universal jar），本期不做解包路线",
                kind.label(),
                loader_version
            )
        } else {
            format!(
                "{} {} 的 installer 里没有可读的 version.json",
                kind.label(),
                loader_version
            )
        };
        return Err(LoaderError::unsupported(reason).with_hint(
            "这段协议要动 installer 内的 jar，属后续 mission；可先用官方启动器装好该加载器",
        ));
    };

    let mut version_json = embedded.clone();
    let mut support = LoaderSupport::JsonRoute;
    let client_processors = profile.client_processors();
    if client_processors > 0 {
        support = LoaderSupport::RequiresInstaller {
            reason: format!(
                "{} {} 的 installer 声明了 {client_processors} 个 client 侧 processor（拆包/映射/打补丁），必须先外调 java 执行",
                kind.label(),
                loader_version
            ),
        };
        warnings.push(format!(
            "install profile 里另有 {} 个 processor 依赖（installertools 等）：它们不进启动 classpath，只在安装期给执行器用",
            profile.libraries.len()
        ));
    }

    version_json.id = Some(version_id.clone());
    version_json.name = Some(format!("{mc_version} · {} {loader_version}", kind.label()));
    version_json.inherits_from = Some(mc_version.to_string());
    version_json.jar = Some(mc_version.to_string());

    if version_json
        .main_class
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err(LoaderError::metadata(format!(
            "{} {} 的 version.json 没有 mainClass",
            kind.label(),
            loader_version
        )));
    }

    let libraries = version_json.libraries.get_or_insert_with(Vec::new);
    let installer_body = profile.path.as_deref();
    for library in libraries.iter_mut() {
        fill_missing_urls(library, maven::base(kind), installer_body);
    }

    Ok(LoaderRelease {
        kind,
        mc_version: mc_version.to_string(),
        loader_version,
        version_id,
        version_json,
        support,
        installer: None,
        install_profile: Some(profile.clone()),
        warnings,
    })
}

/// spec 0 的 version.json 里，forge 本体库的 `url` 常是空串（jar 原本内嵌在 installer 里），
/// 于是"补一个 maven 地址"就能直取，省掉解包 installer。
///
/// ⚠️ 本体 jar 的**发布 classifier 是 `universal`**：installer 内嵌的条目叫
/// `maven/…/forge-<ver>.jar`（无 classifier），但官方 maven 只托管
/// `forge-<ver>-universal.jar`——无 classifier 的那条路径**官方 404**
/// （实测 1.12.2-14.23.5.2860：`-universal.jar` 200 / 4466148 字节，sha1 与 version.json 一致；
/// 无 classifier 路径仅靠 BMCLAPI 的 302 别名兜底）。
/// 因此这里**只改 URL**：给 installer 声明的本体构件追加 `-universal`；
/// `artifact.path`（磁盘落点）保持官方 installer 的原样，启动 classpath 与官方布局一致。
fn fill_missing_urls(library: &mut Library, base: &str, installer_body: Option<&str>) {
    if library
        .url
        .as_deref()
        .is_none_or(|url| url.trim().is_empty())
    {
        library.url = Some(if base.ends_with('/') {
            base.to_string()
        } else {
            format!("{base}/")
        });
    }
    // 先算"是不是 installer 声明的本体"（要读 library 的坐标），再拿 artifact 的可变借用。
    let is_body = is_installer_body(library, installer_body);
    let root = library.url.clone().unwrap_or_else(|| base.to_string());
    let Some(artifact) = library
        .downloads
        .as_mut()
        .and_then(|downloads| downloads.artifact.as_mut())
    else {
        return;
    };
    if artifact.has_url() {
        return;
    }
    let Some(path) = artifact.path.clone() else {
        return;
    };
    let remote_path = if is_body {
        with_universal_classifier(&path)
    } else {
        path
    };
    artifact.url = Some(if root.ends_with('/') {
        format!("{root}{remote_path}")
    } else {
        format!("{root}/{remote_path}")
    });
}

/// 这条库是不是 install profile 的 `path` 声明的本体构件（`group:artifact:version`，无 classifier）。
fn is_installer_body(library: &Library, installer_body: Option<&str>) -> bool {
    let Some(body) = installer_body else {
        return false;
    };
    let mut parts = body.split(':');
    let (Some(group), Some(artifact)) = (parts.next(), parts.next()) else {
        return false;
    };
    library.coordinate().is_ok_and(|coordinate| {
        coordinate.classifier.is_none()
            && coordinate.group == group
            && coordinate.artifact == artifact
    })
}

/// `…/forge-1.12.2-14.23.5.2860.jar` → `…/forge-1.12.2-14.23.5.2860-universal.jar`。
fn with_universal_classifier(path: &str) -> String {
    match path.strip_suffix(".jar") {
        Some(stem) => format!("{stem}-universal.jar"),
        None => path.to_string(),
    }
}
