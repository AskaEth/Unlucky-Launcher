//! game 快照 → [`LaunchRequest`] 的适配层。
//!
//! launch 不 import `crate::game` 的**清单类型**（避免对 M14 的模型产生编译期耦合，见
//! `DevDocs/SubAgentsTalk/launch-接口约定.md` §6）：本模块只吃 game 侧**已经算好**的纯数据——
//! `arguments.expand(&env)` 的展开结果（rules 已在 game 求值）、库文件路径、目录布局，
//! 把它们装配成命令行组装所需的结构。
//!
//! 唯一例外是 [`crate::game::capability`] 里的 `&str → bool` 纯谓词：能力位的判据属于
//! 版本 JSON 的语义、必须只有一处实现，抄第二份必然漂移。它不带任何 game 结构体。
//!
//! 三个职责：
//! 1. classpath 过滤 + **缺失库显式记录**（analyse/02 §8.4-3：不再静默跳过，否则只能靠
//!    `NoClassDefFoundError` 事后猜）；
//! 2. features 开关的派生（`has_custom_resolution` / `has_quick_plays_support`）；
//! 3. 组装 `LaunchRequest`（内存/账户/加载器/分辨率/QuickPlay/代理/日志配置）。

use std::path::PathBuf;

use crate::game::capability::library_names_have_vulkan;
use crate::launch::argument::ArgumentEntry;
use crate::launch::error::MissingComponent;
use crate::launch::game_version::GameVersion;
use crate::launch::natives::{NativeJar, NativesEra, NativesPlan};
use crate::launch::request::{
    AuthPlaceholders, FeatureSet, InstanceInfo, JavaSelection, LaunchDirs, LaunchRequest,
    LauncherInfo, Log4jOptions, ManifestArgs, MemoryOptions, ModLoaderKind, OptimizationOptions,
    QuickPlay, RendererOptions, Resolution, SocksProxy, UserArgs,
};

/// 一个待进 classpath 的库（game 侧已按环境过滤）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryInput {
    /// 落盘路径（`GameLayout::library(coord)` 的结果）。
    pub path: PathBuf,
    /// 坐标或展示名（缺失时报错用，例 `org.lwjgl:lwjgl:3.3.1`）。
    pub name: String,
}

impl LibraryInput {
    /// 构造。
    pub fn new(path: impl Into<PathBuf>, name: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            name: name.into(),
        }
    }
}

/// 版本快照里 launch 需要的部分（全部是纯数据，不依赖 game 类型）。
#[derive(Debug, Clone)]
pub struct SnapshotInput<'a> {
    /// 版本 id（`versions/<id>/`）。
    pub version_id: &'a str,
    /// 展示名（缺省用 version_id）。
    pub version_name: Option<&'a str>,
    /// 版本类型（`release` / `snapshot`）。
    pub version_type: Option<&'a str>,
    /// 游戏版本串（门限判断用；缺省用 version_id）。
    pub game_version: Option<&'a str>,
    /// 主类。
    pub main_class: Option<&'a str>,
    /// `arguments.jvm` 展开后的 token。
    pub jvm_arguments: &'a [String],
    /// `arguments.game` 展开后的 token。
    pub game_arguments: &'a [String],
    /// 旧版 `minecraftArguments` 整串。
    pub legacy_arguments: Option<&'a str>,
    /// 待进 classpath 的库（已过滤环境、不含 native）。
    pub libraries: &'a [LibraryInput],
    /// natives 分类器构件（`natives` 表命中的 jar；1.19+ 为空，natives jar 在 classpath 里）。
    pub natives: &'a [NativeJar],
    /// 主 jar 路径。
    pub primary_jar: PathBuf,
    /// 运行目录（`${game_directory}`）。
    pub game_dir: PathBuf,
    /// 资产索引 id。
    pub assets_index: Option<&'a str>,
    /// 原生库目录。
    pub natives_dir: PathBuf,
    /// 库根目录。
    pub library_dir: PathBuf,
    /// 资产根。
    pub assets_root: PathBuf,
    /// 虚拟资产目录（`${game_assets}`；无虚拟化时等于 assets_root）。
    pub game_assets: PathBuf,
}

impl<'a> SnapshotInput<'a> {
    /// 最小构造：只给版本 id 与主类，其余留空（测试/CLI 组装用）。
    pub fn minimal(version_id: &'a str, main_class: &'a str) -> Self {
        Self {
            version_id,
            version_name: None,
            version_type: None,
            game_version: None,
            main_class: Some(main_class),
            jvm_arguments: &[],
            game_arguments: &[],
            legacy_arguments: None,
            libraries: &[],
            natives: &[],
            primary_jar: PathBuf::new(),
            game_dir: PathBuf::new(),
            assets_index: None,
            natives_dir: PathBuf::new(),
            library_dir: PathBuf::new(),
            assets_root: PathBuf::new(),
            game_assets: PathBuf::new(),
        }
    }

    fn resolved_version(&self) -> GameVersion {
        GameVersion::parse(self.game_version.unwrap_or(self.version_id))
    }
}

/// 适配选项（账户/内存/加载器/一键进入等运行期选择）。
#[derive(Debug, Clone, Default)]
pub struct AdapterOptions {
    /// Java 运行时。
    pub java: JavaSelection,
    /// 内存。
    pub memory: MemoryOptions,
    /// 账户占位符。
    pub auth: AuthPlaceholders,
    /// 启动器名（`${launcher_name}`）。
    pub launcher_name: String,
    /// 启动器版本（`${launcher_version}`）。
    pub launcher_version: String,
    /// 模组加载器。
    pub mod_loader: ModLoaderKind,
    /// 分辨率（`Some` 时自动打开 `has_custom_resolution`）。
    pub resolution: Option<Resolution>,
    /// 全屏。
    pub fullscreen: bool,
    /// 一键进入。
    pub quick_play: Option<QuickPlay>,
    /// SOCKS 代理。
    pub proxy: Option<SocksProxy>,
    /// 图形后端。
    pub graphics_backend: Option<String>,
    /// log4j 配置文件（launch 侧注入）。
    pub log4j_config: Option<PathBuf>,
    /// `-Dfile.encoding` 指定值。
    pub encoding: Option<String>,
    /// 界面语言。
    pub language: String,
    /// 额外环境变量。
    pub environment: Vec<(String, String)>,
    /// JVM 自动优化。
    pub optimization: OptimizationOptions,
    /// 渲染器开关。
    pub renderer: RendererOptions,
    /// 用户自定义参数。
    pub user: UserArgs,
    /// 是否要求库文件真实存在。
    ///
    /// - `true`（默认，严格）：缺库/缺主 jar → 记 `AdapterWarning::MissingLibrary` 并填进
    ///   `missing_components`，组装前显式失败（analyse/02 §8.4-3）；
    /// - `false`（宽松，HMCL 行为）：**不检查存在性、不跳过、不告警**，库照进 classpath，
    ///   交给运行时（`NoClassDefFoundError`）——仅在用户明确选“跳过校验”时用。
    pub check_classpath: bool,
}

impl AdapterOptions {
    /// 默认严格模式：库必须存在（analyse/02 §8.4-3）。
    pub fn strict() -> Self {
        Self {
            check_classpath: true,
            language: "zh-CN".to_string(),
            ..Self::default()
        }
    }
}

/// 适配告警（非致命）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterWarning {
    /// 库文件缺失（严格模式下同时会变成 `missing_components` → 组装失败）。
    MissingLibrary {
        /// 坐标/展示名。
        name: String,
        /// 期望路径。
        path: PathBuf,
    },
    /// 既没有展开后的游戏参数，也没有 legacy 整串。
    MissingGameArguments,
    /// 没有资产索引 id（`${assets_index_name}` 会是空串）。
    MissingAssetsIndex,
    /// 主类缺失（组装一定会报错，这里提前给出中文提示）。
    MissingMainClass,
    /// natives 分类器 jar 缺失（严格模式下同时会变成 `missing_components` → 组装失败）。
    MissingNativeJar {
        /// 坐标 + 分类器。
        name: String,
        /// 期望路径。
        path: PathBuf,
    },
}

/// 适配结果。
#[derive(Debug, Clone)]
pub struct AdapterOutput {
    /// 组装输入。
    pub request: LaunchRequest,
    /// 告警。
    pub warnings: Vec<AdapterWarning>,
    /// 实际进 classpath 的条目数（库 + 主 jar）。
    pub classpath_entries: usize,
    /// 缺失库清单。
    pub missing: Vec<MissingComponent>,
}

/// 把快照 + 选项装配成 [`LaunchRequest`]。
pub fn build_request(snapshot: &SnapshotInput<'_>, options: AdapterOptions) -> AdapterOutput {
    let mut warnings = Vec::new();
    let mut missing = Vec::new();
    let mut classpath = Vec::new();
    for library in snapshot.libraries {
        if options.check_classpath && !library.path.is_file() {
            warnings.push(AdapterWarning::MissingLibrary {
                name: library.name.clone(),
                path: library.path.clone(),
            });
            missing.push(MissingComponent::at_path(
                library.name.clone(),
                library.path.clone(),
            ));
            continue;
        }
        if options.check_classpath && classpath.contains(&library.path) {
            continue;
        }
        classpath.push(library.path.clone());
    }
    if !snapshot.primary_jar.as_os_str().is_empty()
        && options.check_classpath
        && !snapshot.primary_jar.is_file()
    {
        let name = snapshot
            .primary_jar
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "client.jar".to_string());
        warnings.push(AdapterWarning::MissingLibrary {
            name: name.clone(),
            path: snapshot.primary_jar.clone(),
        });
        missing.push(MissingComponent::at_path(
            name,
            snapshot.primary_jar.clone(),
        ));
    }
    if snapshot.main_class.is_none() {
        warnings.push(AdapterWarning::MissingMainClass);
    }
    if snapshot.assets_index.is_none() {
        warnings.push(AdapterWarning::MissingAssetsIndex);
    }
    if snapshot.game_arguments.is_empty() && snapshot.legacy_arguments.is_none() {
        warnings.push(AdapterWarning::MissingGameArguments);
    }
    // natives jar 缺失同样属「组件缺失」：解压期才发现会只剩一句 UnsatisfiedLinkError。
    for native in snapshot.natives {
        if options.check_classpath && !native.path.is_file() {
            warnings.push(AdapterWarning::MissingNativeJar {
                name: native.name.clone(),
                path: native.path.clone(),
            });
            missing.push(MissingComponent::at_path(
                native.name.clone(),
                native.path.clone(),
            ));
        }
    }

    let version = snapshot.resolved_version();
    let features = FeatureSet {
        has_custom_resolution: options.resolution.is_some(),
        is_demo_user: false,
        has_quick_plays_support: version.is_at_least("1.20"),
    };
    let manifest = ManifestArgs {
        main_class: snapshot.main_class.map(str::to_string),
        jvm: entries(snapshot.jvm_arguments),
        game: entries(snapshot.game_arguments),
        legacy_arguments: snapshot.legacy_arguments.map(str::to_string),
        has_vulkan_backend: library_names_have_vulkan(
            snapshot
                .libraries
                .iter()
                .map(|library| library.name.as_str()),
        ),
    };

    let classpath_entries =
        classpath.len() + usize::from(!snapshot.primary_jar.as_os_str().is_empty());
    let era = NativesEra::detect(snapshot.jvm_arguments.len(), snapshot.natives.len());
    let natives = NativesPlan {
        jars: snapshot.natives.to_vec(),
        era,
        custom: false,
    };
    let request = LaunchRequest {
        instance: InstanceInfo {
            id: snapshot.version_id.to_string(),
            name: snapshot
                .version_name
                .unwrap_or(snapshot.version_id)
                .to_string(),
            version,
            version_type: snapshot.version_type.unwrap_or("release").to_string(),
        },
        launcher: LauncherInfo {
            name: options.launcher_name,
            version: options.launcher_version,
        },
        java: options.java,
        memory: options.memory,
        manifest,
        dirs: LaunchDirs {
            game_dir: snapshot.game_dir.clone(),
            assets_root: snapshot.assets_root.clone(),
            game_assets: snapshot.game_assets.clone(),
            assets_index: snapshot.assets_index.unwrap_or("legacy").to_string(),
            natives_dir: snapshot.natives_dir.clone(),
            library_dir: snapshot.library_dir.clone(),
            primary_jar: snapshot.primary_jar.clone(),
        },
        natives,
        auth: options.auth,
        user: options.user,
        features,
        resolution: options.resolution,
        quick_play: options.quick_play,
        proxy: options.proxy,
        graphics_backend: options.graphics_backend,
        fullscreen: options.fullscreen,
        mod_loader: options.mod_loader,
        log4j: Log4jOptions {
            enabled: true,
            config_file: options.log4j_config,
            debug: false,
        },
        renderer: options.renderer,
        optimization: options.optimization,
        encoding: options.encoding,
        language: options.language,
        classpath,
        missing_components: missing.clone(),
        extra_jvm: Vec::new(),
        ..LaunchRequest::default()
    };
    AdapterOutput {
        request,
        warnings,
        classpath_entries,
        missing,
    }
}

fn entries(tokens: &[String]) -> Vec<ArgumentEntry> {
    if tokens.is_empty() {
        Vec::new()
    } else {
        vec![ArgumentEntry::plain(tokens.iter().cloned())]
    }
}
