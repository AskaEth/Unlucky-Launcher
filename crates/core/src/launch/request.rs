//! 启动请求：命令行组装的输入模型（纯数据，无 IO，便于快照测试）。
//!
//! 与 `crate::game` / `crate::download` 的映射关系见
//! `DevDocs/SubAgentsTalk/launch-接口约定.md` §6：launch 阶段只消费本结构，
//! 由薄适配层把 game 的快照搬进来，避免 launch 对 manifest 类型产生编译期依赖。

use std::path::PathBuf;

use crate::launch::argument::ArgumentEntry;
use crate::launch::error::MissingComponent;
use crate::launch::game_version::GameVersion;
use crate::launch::natives::NativesPlan;

/// 实例信息。
#[derive(Debug, Clone, Default)]
pub struct InstanceInfo {
    /// 实例/版本 id（稳定 UUID 或版本目录名）。
    pub id: String,
    /// 展示名（`${version_name}`）。
    pub name: String,
    /// 游戏版本号（分支门限用）。
    pub version: GameVersion,
    /// 版本类型（`release` / `snapshot`；`${version_type}`）。
    pub version_type: String,
}

/// 启动器自身信息（`${launcher_name}` / `${launcher_version}`）。
#[derive(Debug, Clone, Default)]
pub struct LauncherInfo {
    /// 名称。
    pub name: String,
    /// 版本。
    pub version: String,
}

/// 选定的 Java 运行时。
#[derive(Debug, Clone, Default)]
pub struct JavaSelection {
    /// `java` 可执行文件路径。
    pub binary: PathBuf,
    /// 主版本号（8 / 17 / 21 / 25 …）。
    pub major: u32,
    /// 位数（32 / 64）。
    pub bits: u32,
}

impl JavaSelection {
    /// 构造。
    pub fn new(binary: impl Into<PathBuf>, major: u32, bits: u32) -> Self {
        Self {
            binary: binary.into(),
            major,
            bits,
        }
    }

    /// 是否 64 位 JVM。
    pub fn is_64bit(&self) -> bool {
        self.bits == 64
    }
}

/// 内存设置（MiB）。
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryOptions {
    /// 初始堆 `-Xms`。
    pub min_mb: Option<u32>,
    /// 最大堆 `-Xmx`。
    pub max_mb: Option<u32>,
}

/// 版本清单里的参数（`arguments.jvm` / `arguments.game` / 旧版 `minecraftArguments`）。
#[derive(Debug, Clone, Default)]
pub struct ManifestArgs {
    /// 主类。
    pub main_class: Option<String>,
    /// 现代清单的 JVM 参数（带 rules）。
    pub jvm: Vec<ArgumentEntry>,
    /// 现代清单的游戏参数（带 rules）。
    pub game: Vec<ArgumentEntry>,
    /// 旧版清单的整串游戏参数（1.12 及更早）。
    pub legacy_arguments: Option<String>,
    /// 版本库表里带 LWJGL 的 Vulkan 绑定（`org.lwjgl:lwjgl-vulkan`）。
    ///
    /// 只作「无法排序的版本」（周快照）的 `--graphicsBackend` 兜底判据与 UI 提示；
    /// 可解析版本的下发门限仍是 `26.2-snapshot-2`（见
    /// [`graphics_backend_allowed`](crate::launch::game_args::graphics_backend_allowed)）。
    pub has_vulkan_backend: bool,
}

impl ManifestArgs {
    /// 是否走旧版参数模板（只有 `minecraftArguments`，没有 `arguments`）。
    pub fn is_legacy(&self) -> bool {
        self.jvm.is_empty() && self.legacy_arguments.is_some()
    }
}

/// 目录布局。
#[derive(Debug, Clone, Default)]
pub struct LaunchDirs {
    /// 运行目录（`${game_directory}`，同时是子进程的 CWD）。
    ///
    /// D20：这个值是**隔离覆盖链的派生结果**（`game::isolation::derive_game_dir`：
    /// 临时参数 > 实例 `isolation` > 全局 `instances.mode`）。launch 侧只消费、不派生，
    /// 也不读配置——路径语义的真相在 `core::game`。
    pub game_dir: PathBuf,
    /// 资产根。
    pub assets_root: PathBuf,
    /// 重建后的虚拟资产目录（`${game_assets}`）。
    pub game_assets: PathBuf,
    /// 资产索引 id（`${assets_index_name}`）。
    pub assets_index: String,
    /// 原生库目录（按 Java 平台选择，`analyse/02` §4.6）。
    pub natives_dir: PathBuf,
    /// 库根目录。
    pub library_dir: PathBuf,
    /// 主 jar。
    pub primary_jar: PathBuf,
}

/// 账户占位符值。
#[derive(Debug, Clone, Default)]
pub struct AuthPlaceholders {
    /// 玩家名。
    pub player_name: String,
    /// 无连字符 UUID。
    pub uuid: String,
    /// 访问令牌（落盘前必须脱敏）。
    pub access_token: String,
    /// `msa` / `legacy`。
    pub user_type: String,
    /// `user_properties` JSON 串。
    pub user_properties: String,
}

/// 用户自定义参数。
#[derive(Debug, Clone, Default)]
pub struct UserArgs {
    /// 最高优先级 JVM 参数（不解析转义，会读 `@argfile`）。
    pub override_jvm: Vec<String>,
    /// 普通 JVM 参数（整体按“默认值”处理）。
    pub jvm: Vec<String>,
    /// 追加在最后、可覆盖前面一切的 game 参数。
    pub game: Vec<String>,
    /// 额外环境变量（用户自定义，最后覆盖）。
    pub environment: Vec<(String, String)>,
}

/// 清单参数 rules 求值所需的特性开关。
#[derive(Debug, Clone, Copy, Default)]
pub struct FeatureSet {
    /// 用户指定了分辨率（`--width/--height` 的开关）。
    pub has_custom_resolution: bool,
    /// 演示模式。
    pub is_demo_user: bool,
    /// 支持 QuickPlay（1.20+）。
    pub has_quick_plays_support: bool,
}

impl FeatureSet {
    /// 按特性名查询（未知特性为 `None`）。
    pub fn get(&self, name: &str) -> Option<bool> {
        match name {
            "has_custom_resolution" => Some(self.has_custom_resolution),
            "is_demo_user" => Some(self.is_demo_user),
            "has_quick_plays_support" => Some(self.has_quick_plays_support),
            _ => None,
        }
    }
}

/// 窗口分辨率。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution {
    /// 宽。
    pub width: u32,
    /// 高。
    pub height: u32,
}

/// 一键进入目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickPlay {
    /// 直入存档。
    Singleplayer {
        /// 存档目录名。
        world: String,
    },
    /// 直入服务器。
    Multiplayer {
        /// `host:port` 形式地址。
        address: String,
    },
    /// 直入 Realms。
    Realms {
        /// Realm id。
        realm_id: String,
    },
}

/// SOCKS 代理。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocksProxy {
    /// 主机。
    pub host: String,
    /// 端口。
    pub port: u16,
    /// 用户名。
    pub username: Option<String>,
    /// 密码。
    pub password: Option<String>,
}

/// 模组加载器种类（决定 Forge 专属参数与环境变量）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModLoaderKind {
    /// 原版。
    #[default]
    Vanilla,
    /// Forge。
    Forge,
    /// NeoForge。
    NeoForge,
    /// Fabric。
    Fabric,
    /// Quilt。
    Quilt,
    /// LiteLoader。
    LiteLoader,
    /// OptiFine（独立安装器）。
    OptiFine,
}

impl ModLoaderKind {
    /// 是否使用 FML/LaunchWrapper（决定 `-Dfml.ignore*` 参数）。
    pub fn is_fml_like(self) -> bool {
        matches!(self, Self::Forge | Self::NeoForge | Self::LiteLoader)
    }
}

/// Log4j 注入选项。
#[derive(Debug, Clone)]
pub struct Log4jOptions {
    /// 是否注入日志配置（HMCL：版本 ≥1.7 即注入，探测失败按 1.7 兜底）。
    pub enabled: bool,
    /// 配置文件路径（`-Dlog4j.configurationFile=`）。
    pub config_file: Option<PathBuf>,
    /// 调试日志变体。
    pub debug: bool,
}

impl Default for Log4jOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            config_file: None,
            debug: false,
        }
    }
}

/// 渲染器相关开关（`analyse/02` §4.1 #17、§4.10）。
#[derive(Debug, Clone, Default)]
pub struct RendererOptions {
    /// 软件渲染（Mesa/llvmpipe）。
    pub software: bool,
    /// Windows 上 mesa 的 native 目录。
    pub mesa_native_dir: Option<PathBuf>,
    /// macOS 上的 Vulkan ICD JSON。
    pub vulkan_icd: Option<PathBuf>,
}

/// JVM 自动优化选项。
#[derive(Debug, Clone)]
pub struct OptimizationOptions {
    /// 是否注入自动优化参数（用户可关）。
    pub enabled: bool,
    /// 物理内存 MiB（决定 CodeCache 一组参数的开关）。
    pub physical_memory_mb: Option<u64>,
}

impl Default for OptimizationOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            physical_memory_mb: None,
        }
    }
}

/// 一次启动的完整输入。
#[derive(Debug, Clone, Default)]
pub struct LaunchRequest {
    /// 实例信息。
    pub instance: InstanceInfo,
    /// 启动器信息。
    pub launcher: LauncherInfo,
    /// Java 运行时。
    pub java: JavaSelection,
    /// 内存。
    pub memory: MemoryOptions,
    /// 清单参数。
    pub manifest: ManifestArgs,
    /// 目录布局。
    pub dirs: LaunchDirs,
    /// natives 解压计划（`dirs.natives_dir` 为目标目录，`jars` 为分类器构件清单）。
    ///
    /// 解压发生在**组装期**（`Launcher::dry_run`/`spawn` 的「环境准备」段），
    /// 见 [`crate::launch::natives`] 与 `SubAgentsTalk/launch-natives-解压与时代分界.md`。
    pub natives: NativesPlan,
    /// 账户。
    pub auth: AuthPlaceholders,
    /// 用户参数。
    pub user: UserArgs,
    /// 特性开关。
    pub features: FeatureSet,
    /// 分辨率（`None` 表示不指定）。
    pub resolution: Option<Resolution>,
    /// 一键进入。
    pub quick_play: Option<QuickPlay>,
    /// SOCKS 代理。
    pub proxy: Option<SocksProxy>,
    /// 图形后端（`default` / `opengl` / `vulkan`）。
    ///
    /// 下发门限见 [`graphics_backend_allowed`](crate::launch::game_args::graphics_backend_allowed)：
    /// 可解析版本 ≥ `26.2-snapshot-2`，周快照另按版本库表的能力位判。
    pub graphics_backend: Option<String>,
    /// 全屏。
    pub fullscreen: bool,
    /// 进程优先级（POSIX `nice -n`，Windows 忽略）。
    pub nice: Option<i32>,
    /// 命令包装器（`optirun`、`prime-run` 等）。
    pub wrapper: Vec<String>,
    /// `-javaagent:` 列表。
    pub java_agents: Vec<String>,
    /// 登录源提供的 JVM 参数（authlib-injector）。
    pub auth_jvm_args: Vec<String>,
    /// 登录源提供的游戏参数。
    pub auth_game_args: Vec<String>,
    /// 模组加载器。
    pub mod_loader: ModLoaderKind,
    /// Log4j 注入。
    pub log4j: Log4jOptions,
    /// 渲染器开关。
    pub renderer: RendererOptions,
    /// JVM 自动优化。
    pub optimization: OptimizationOptions,
    /// `-Dfile.encoding` 指定值（`COMPAT` 表示跟随系统）。
    pub encoding: Option<String>,
    /// 界面语言（`${language}`）。
    pub language: String,
    /// 已过滤为“确实存在”的 classpath（主 jar 由组装器追加到末位）。
    pub classpath: Vec<PathBuf>,
    /// 组件就绪检查发现的缺失项（非空则组装直接失败）。
    pub missing_components: Vec<MissingComponent>,
    /// 额外注入的 JVM 参数（品牌/特性钩子，`appendJvmArgs` 的替代者）。
    pub extra_jvm: Vec<String>,
}
