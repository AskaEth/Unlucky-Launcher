//! LiteLoader：官方 `versions.json` → **从零合成**一份 launchwrapper 版本 JSON。
//!
//! 依据（A 级：官方数据 + 官方构建脚本，不查二手转述）：
//! - 数据源 `https://dl.liteloader.com/versions/versions.json`（2026-11 实测 200 / 49 KB，
//!   16 个原版版本）；
//! - HMCL 的同类实现只做一件事（`HMCLCore/.../liteloader/LiteLoaderInstallTask.java`）：
//!   `mainClass = launchwrapper` + `--tweakClass <record.tweakClass>` + 「记录自带库 + 本体 jar」，
//!   **不跑任何外部进程**；本模块按同样的协议独立实现，没有抄它的代码。
//!
//! # 四条与上游形状有关的硬事实（都实测过，别按直觉改）
//!
//! 1. **分支键是 md5，不是版本号**：`artefacts."com.mumfrey:liteloader"` 下的键是 md5，
//!    真正的版本号在记录的 `version` 字段（`1.7.10_04`）。按 `version` 去重（同一版本会挂
//!    两个 md5 键），`latest` 分支要跳过。
//! 2. **目录名是原版版本，不是 loader 版本**：release 直链形如
//!    `<repo>/com/mumfrey/liteloader/1.7.10/liteloader-1.7.10_04.jar`（`1.7.10` 是原版）。
//! 3. **1.12.2 这类版本只有 `snapshots` 分支**（上游 16 个版本里 8 个如此，含最常用的 1.12.2）。
//!    记录的 `file`（`liteloader-1.12.2-SNAPSHOT.jar`）**下不动**，仓库里只有带时间戳的
//!    `liteloader-1.12.2-<timestamp>-<build>-release.jar`；要拼出直链必须再读一次
//!    `maven-metadata.xml`（`<timestamp>` + `<buildNumber>`）。见 [`snapshot_artifact_url`]。
//! 4. **分支级 `libraries` 不是要装的库**：那是 ForgeGradle 开发环境用的（1.12.2 的
//!    `snapshots.libraries` 里挂着 `org.spongepowered:mixin`）。要进游戏 classpath 的是
//!    **记录级** `libraries`（`launchwrapper` + `asm-all`），与 HMCL 的取法一致。
//!
//! # 库基址为什么按 group 分流
//!
//! 记录里的库大多不写 `url`，上游默认（Mojang 库仓）**实测 404**：`org.ow2.asm:asm-all:5.2`
//! 在 `libraries.minecraft.net` 上不存在（5.0.3 存在），而 `net.minecraft:launchwrapper:*`
//! 只在 Mojang 库仓有（Central / Forge maven 都 404）。所以 `net.minecraft` 走 Mojang 默认基址、
//! 其余走 Maven Central（实测 206）。这条分流有单测钉住，别把两侧弄反。

use std::collections::BTreeMap;

use super::plan::{LoaderRelease, LoaderSupport};
use super::{LoaderError, LoaderKind};
use crate::game::arguments::{Argument, Arguments};
use crate::game::library::{Library, LibraryDownloads};
use crate::game::manifest::{DownloadInfo, VersionJson};
use crate::game::version::ReleaseType;

/// 官方版本表。
pub const VERSION_LIST_URL: &str = "https://dl.liteloader.com/versions/versions.json";

/// 本体 jar 与 release 分支的仓库根：**直连官方**——`bmclapi-liteloader` 改写规则已
/// `enabled: false`（镜像侧实测 404，关闭依据见 `download/mirrors.json`）。库走 [`CENTRAL_REPO`]。
pub const DEFAULT_REPO: &str = "https://dl.liteloader.com/versions/";

/// 非 Mojang 库的基址：Mojang 库仓里没有 `asm-all:5.2`（实测 404），Central 有（实测 206）。
pub const CENTRAL_REPO: &str = "https://repo1.maven.org/maven2/";

/// 快照分支的仓库根——**固定这一个**，不跟着 `repo.url` 走。
///
/// 上游每个原版版本的 `repo.url` 有两种形态（`…/versions/` 与 `…/repo/`），而**快照只发布在
/// snapshots 仓**：1.10.2 的 `repo.url` 是 `http://dl.liteloader.com/repo/`，按它拼
/// `…/repo/com/mumfrey/liteloader/1.10.2-SNAPSHOT/maven-metadata.xml` 实测会 302 到
/// `…/repositories/liteloader/…` 再 404；而
/// `https://repo.mumfrey.com/content/repositories/snapshots/com/mumfrey/liteloader/1.10.2-SNAPSHOT/maven-metadata.xml`
/// 是 200。HMCL 的构建脚本同样把这个快照仓**写死**（`UpdateLiteLoaderVersionList.SNAPSHOT_FILE`，
/// 注释还引了它自己的 issue #3147）。1.12.2 的 `repo.url` 恰好就是这条路径，所以只测 1.12.2
/// 看不出这个问题——单测专门钉了 1.10.2。
pub const SNAPSHOT_REPO: &str = "https://repo.mumfrey.com/content/repositories/snapshots/";

/// launchwrapper 主类（LiteLoader 靠它做 tweak 注入）。
pub const LAUNCHWRAPPER_MAIN: &str = "net.minecraft.launchwrapper.Launch";

/// 记录没带 launchwrapper 时的兜底版本（1.12.2 的记录带的是 1.12）。
const LAUNCHWRAPPER_FALLBACK: &str = "1.12";

/// 本体库坐标的 group / artifact。
const LOADER_GROUP: &str = "com.mumfrey";
/// 本体库坐标的 artifact 名。
const LOADER_ARTIFACT: &str = "liteloader";

mod versions;

pub use versions::{parse_versions, snapshot_artifact_url};

/// 一条可安装的 LiteLoader 版本（已归一化，不含 `latest` 别名分支）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiteLoaderEntry {
    /// 版本号（`1.7.10_04` / `1.12.2-SNAPSHOT`）
    pub version: String,
    /// 记录所属的原版版本（拼目录与文件名用）
    pub mc_version: String,
    /// 是否快照分支（只有快照时要再读 maven-metadata 才拼得出直链）
    pub snapshot: bool,
    /// tweak 类（`com.mumfrey.liteloader.launch.LiteLoaderTweaker`）
    pub tweak_class: String,
    /// 记录级库（**不含**本体 jar，本体由 [`build_release`] 补）
    pub libraries: Vec<Library>,
    /// release 分支的直链（快照分支为 `None`）
    pub url: Option<String>,
    /// 快照分支的 maven 目录（`…/com/mumfrey/liteloader/1.12.2-SNAPSHOT/`）
    pub snapshot_dir: Option<String>,
}

/// 默认版本：优先非快照（稳定），没有则取最新的那条。
#[must_use]
pub fn default_entry(entries: &[LiteLoaderEntry]) -> Option<&LiteLoaderEntry> {
    entries
        .iter()
        .find(|entry| !entry.snapshot)
        .or_else(|| entries.first())
}

/// 按版本号找一条（大小写不敏感）。
#[must_use]
pub fn find_entry<'a>(
    entries: &'a [LiteLoaderEntry],
    version: &str,
) -> Option<&'a LiteLoaderEntry> {
    entries
        .iter()
        .find(|entry| entry.version.eq_ignore_ascii_case(version.trim()))
}

/// 合成 loader 版本 JSON（**纯函数**：直链由调用方给，快照的直链要先去查 maven-metadata）。
///
/// 拼出来的清单只有四件事：主类 = launchwrapper、`--tweakClass` 进 `arguments.game`、
/// 库 = 记录库 + 本体 jar、`inheritsFrom` = 父版本（缺省原版）。
///
/// **`arguments.game` 而不是 `minecraftArguments`**：继承合并里 `minecraftArguments` 是
/// **子覆盖父**（[`crate::game::inherit::merge`]），写它会把父链（Forge 1.12.2 的
/// `--tweakClass FMLTweaker`）整串顶掉，Forge 当场不加载；`arguments.game` 才是父在前、子在后
/// **追加**，且启动时排在 legacy 参数之后（[`crate::launch::game_args`] 第 24/25 项）。
///
/// # Errors
///
/// 记录缺 `tweakClass`、或没有本体直链时返回 metadata 错误（没有它这份清单装上也起不来）。
pub fn build_release(
    mc_version: &str,
    parent_version_id: &str,
    entry: &LiteLoaderEntry,
    artifact_url: &str,
) -> Result<LoaderRelease, LoaderError> {
    if entry.tweak_class.trim().is_empty() {
        return Err(LoaderError::metadata(format!(
            "LiteLoader {} 的记录没有 tweakClass：这份清单装上也起不来",
            entry.version
        ))
        .with_hint("换一个 LiteLoader 版本，或核对 dl.liteloader.com 的版本表"));
    }
    if artifact_url.trim().is_empty() {
        return Err(LoaderError::metadata(format!(
            "LiteLoader {} 没有可下载的本体 jar 地址",
            entry.version
        )));
    }

    let version_id = LoaderKind::LiteLoader.version_id(mc_version, &entry.version);
    let mut libraries = entry.libraries.clone();
    libraries.push(loader_library(entry, artifact_url));

    let mut warnings = Vec::new();
    if parent_version_id != mc_version {
        warnings.push(format!(
            "LiteLoader {} 叠在 {parent_version_id} 上（父链不是原版 {mc_version}）：tweakClass 追加在父链参数之后，父链自己的 tweakClass 不动",
            entry.version
        ));
    }
    if !libraries
        .iter()
        .any(|library| library.name.starts_with("net.minecraft:launchwrapper:"))
    {
        libraries.push(Library {
            name: format!("net.minecraft:launchwrapper:{LAUNCHWRAPPER_FALLBACK}"),
            ..Library::default()
        });
        warnings.push(
            "LiteLoader 的记录没带 launchwrapper：已补 1.12（Launch 主类就靠它）".to_string(),
        );
    }

    let version_json = VersionJson {
        id: Some(version_id.clone()),
        name: Some(format!("{mc_version} · LiteLoader {}", entry.version)),
        inherits_from: Some(parent_version_id.to_string()),
        // 客户端 jar 恒在原版那层（父链与 loader 层都不带 jar）。
        jar: Some(mc_version.to_string()),
        main_class: Some(LAUNCHWRAPPER_MAIN.to_string()),
        minecraft_arguments: None,
        arguments: Some(Arguments {
            game: vec![
                Argument::Plain("--tweakClass".to_string()),
                Argument::Plain(entry.tweak_class.clone()),
            ],
            jvm: Vec::new(),
        }),
        version_type: Some(if entry.snapshot {
            ReleaseType::Snapshot
        } else {
            ReleaseType::Release
        }),
        libraries: Some(libraries),
        ..VersionJson::default()
    };

    Ok(LoaderRelease {
        kind: LoaderKind::LiteLoader,
        mc_version: mc_version.to_string(),
        parent_version_id: parent_version_id.to_string(),
        loader_version: entry.version.clone(),
        version_id,
        version_json,
        support: LoaderSupport::JsonRoute,
        installer: None,
        install_profile: None,
        warnings,
    })
}

/// 本体 jar 的那一条库：直链绝对地址（快照的路径与 maven 坐标不一致，不能靠坐标推导）。
fn loader_library(entry: &LiteLoaderEntry, artifact_url: &str) -> Library {
    Library {
        name: format!("{LOADER_GROUP}:{LOADER_ARTIFACT}:{}", entry.version),
        url: Some(DEFAULT_REPO.to_string()),
        downloads: Some(LibraryDownloads {
            artifact: Some(DownloadInfo {
                id: None,
                sha1: None,
                size: None,
                url: Some(artifact_url.to_string()),
                path: None,
            }),
            classifiers: BTreeMap::new(),
        }),
        ..Library::default()
    }
}
