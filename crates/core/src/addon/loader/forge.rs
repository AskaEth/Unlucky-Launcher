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
//! | 只有内联 `versionInfo` + `install{path,filePath}`（installer 里没有 `version.json`） | Forge 1.12.2-14.23.5.2768、1.10.2、1.8.9、1.7.10 | [`LoaderSupport::JsonRoute`]（M330 起） |
//! | 老协议 installer 但连内联 `versionInfo` 都没有 | 未验（没找到实例） | 明确 unsupported |
//!
//! 第一类必须跑 `java -cp … installertools …` 才能产出 `client-slim/extra` 等打补丁产物
//! （processor 的 `data` 变量表里有 `MC_SLIM`/`MC_EXTRA`/`PATCHED`），解析结果因此带上
//! [`LoaderSupport::RequiresInstaller`] 标记 + install profile 本体，交给
//! [`super::processors`]（M35 执行器）真跑；这里的**判定**仍只管「要不要跑 installer」。
//!
//! 后两类的本体 jar 在官方 maven 上**只发布 `-universal` classifier**（无 classifier 路径
//! 官方 404，只有 BMCLAPI 的 302 别名兜底）——补救见 [`body_url::fill_missing_urls`]：
//! URL 带 classifier、磁盘落点保持官方 installer 的原样。形状 A 的本体库有 `downloads`
//! （只是 `url` 空串），形状 B 的库表**连 `downloads` 都没有**（只有 `name` + `url`），
//! 两条都补；本体坐标在形状 A 的顶层 `path`、形状 B 的 `install.path`。
//!
//! # 一条容易踩的坑：processor 的库不进启动 classpath
//!
//! install profile 自带的 `libraries` 是**processor 依赖**（installertools/srgutils…），
//! 它们不属于游戏 classpath。把它们并进 loader 版本 JSON 会让启动多加载一堆无关 jar，
//! 因此这里只记数量进告警、绝不合并；真下载它们的是 [`super::processors`]。

use std::collections::BTreeMap;
use std::fmt;

use serde::de::value::MapAccessDeserializer;
use serde::de::{self, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;

use super::body_url;
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
    ///
    /// 老版 Forge 的 installer 里就这只 jar；官方 maven 的 `-universal` 与它 sha1 相同
    /// （实测 1.10.2 / 1.12.2），所以这里**只留作证据，不做解包**。
    #[serde(default)]
    pub file_path: Option<String>,
    /// 本体的 maven 坐标（`net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10`）。
    ///
    /// 老协议的版本主体在内联 `versionInfo` 里，本体坐标只在这条 `install.path` 上
    /// （顶层 `path` 是 spec 0 才有）——[`build_release`] 要用它认出「哪条库是本体」。
    #[serde(default)]
    pub path: Option<String>,
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
    ///
    /// `1.12.2-14.23.5.2851` 的真 profile 把它写成空**数组**（`"data": [],`，第 16 行），
    /// 而 `#[serde(default)]` 只管缺键不管类型错（会给 `invalid type: sequence`）——
    /// 归一成「空数组 = 空表」见 [`deserialize_datum_map`]。
    #[serde(default, deserialize_with = "deserialize_datum_map")]
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

    /// 是否是老协议 installer（installer 里没有独立 `version.json`，版本主体只能来自内联
    /// `versionInfo` + `install.filePath` 那只 jar）。
    #[must_use]
    pub fn is_legacy_protocol(&self) -> bool {
        self.version_json_path.is_none() && self.install.is_some()
    }

    /// 本体的 maven 坐标：spec 0 在顶层 `path`，老协议在 `install.path`。
    #[must_use]
    pub fn body_coordinate(&self) -> Option<&str> {
        self.path
            .as_deref()
            .or_else(|| self.install.as_ref()?.path.as_deref())
    }
}

/// `data` 的宽松解析：对象照常，**空数组当空表**。
///
/// 真样例：`1.12.2-14.23.5.2851` 的 install_profile.json 第 16 行是 `"data": [],`
/// （上游手误，2852 起改回 `{}`）。`#[serde(default)]` 只在**缺键**时生效，给一个 `[]`
/// 会报 `invalid type: sequence, expected a map`。非空数组**仍然报错**——
/// 那说明这一档协议我们没读懂，静默丢掉变量表会让 processor 拿到空值。
fn deserialize_datum_map<'de, D>(deserializer: D) -> Result<BTreeMap<String, Datum>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct DatumMapVisitor;

    impl<'de> Visitor<'de> for DatumMapVisitor {
        type Value = BTreeMap<String, Datum>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("processor 变量表（对象，或空数组）")
        }

        fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            BTreeMap::deserialize(MapAccessDeserializer::new(map))
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            if seq.next_element::<IgnoredAny>()?.is_some() {
                return Err(de::Error::custom(
                    "processor 变量表是非空数组：这一档 data 形状没见过，不敢当空表处理",
                ));
            }
            Ok(BTreeMap::new())
        }
    }

    deserializer.deserialize_any(DatumMapVisitor)
}

/// 用 install profile + 版本主体组 loader 版本。
///
/// # 版本主体从哪来（两种形状，判据是 installer 里有没有独立 `version.json`）
///
/// - **形状 A**：`embedded`（installer 里的 `version.json`，由 profile 的 `json` 键指路）；
/// - **形状 B**（老重建版，1.12.2 的 355 个版本里 343 个是这一档）：installer 里没有
///   `version.json`，版本主体是 `install_profile.json` 里内联的 `versionInfo`。
///   这一档的本体坐标在 `install.path`（顶层 `path` 是 spec 0 才有），
///   库表里也没有 `downloads`——见 [`body_url::fill_missing_urls`]。
///
/// 两者都缺才是真装不了（返回 `unsupported`）。
///
/// # Errors
///
/// 原版版本对不上、installer 里既没有 `version.json` 也没有内联 `versionInfo`、
/// 生成的清单缺主类时返回错误。
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

    let Some(body) = embedded.or(profile.version_info.as_ref()) else {
        let reason = if profile.is_legacy_protocol() {
            format!(
                "{} {} 的老协议 installer 里既没有 version.json 也没有内联 versionInfo（只有 install.filePath 那只 jar），读不出任何版本声明",
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
            "换成官方 maven 上的正式版本（老版 Forge 的 installer 至少会内联 versionInfo）",
        ));
    };

    let mut version_json = body.clone();
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
    let installer_body = profile.body_coordinate();
    body_url::fill_missing_urls(libraries, maven::base(kind), installer_body);

    Ok(LoaderRelease {
        kind,
        mc_version: mc_version.to_string(),
        parent_version_id: mc_version.to_string(),
        loader_version,
        version_id,
        version_json,
        support,
        installer: None,
        install_profile: Some(profile.clone()),
        warnings,
    })
}
