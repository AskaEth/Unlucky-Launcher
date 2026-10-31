//! 六维修复动作的参数类型（`fix_action.rs` 的载荷 + 几个小枚举）。
//!
//! 由 `rules.rs` 用 `#[path]` 声明并统一导出；文件拆分只为守 400 行上限。

use serde::Deserialize;

/// 建议的 JVM 实现。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JavaVendor {
    /// 不限定（HotSpot 优先）。
    #[default]
    Any,
    /// 必须 HotSpot（OpenJ9 等不兼容实现的反面）。
    Hotspot,
}

impl JavaVendor {
    /// 稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::Hotspot => "hotspot",
        }
    }
}

/// 图形侧要修的那一类设置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphicsSetting {
    /// 不指定具体项，只表达「图形相关」。
    #[default]
    Any,
    /// 渲染器（OpenGL 核心/兼容、ANGLE、软渲染）。
    Renderer,
    /// 显卡驱动。
    Driver,
    /// 图形 API（MC 26.2+ 的「默认 / 优先 Vulkan / 优先 OpenGL」三选一）。
    GraphicsApi,
    /// 分辨率 / 资源包分辨率。
    Resolution,
    /// 覆盖层类软件（RTSS、Discord 等）。
    Overlay,
    /// 软件渲染兜底。
    SoftwareRenderer,
}

impl GraphicsSetting {
    /// 稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::Renderer => "renderer",
            Self::Driver => "driver",
            Self::GraphicsApi => "graphics_api",
            Self::Resolution => "resolution",
            Self::Overlay => "overlay",
            Self::SoftwareRenderer => "software_renderer",
        }
    }
}

/// 对 mod 的处理方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModAction {
    /// 移出 mods 目录。
    #[default]
    Remove,
    /// 暂时禁用（挪到 `.disabled`）。
    Disable,
    /// 换一个版本 / 换替代品。
    Replace,
    /// 升级到兼容版本。
    Upgrade,
}

impl ModAction {
    /// 稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Remove => "remove",
            Self::Disable => "disable",
            Self::Replace => "replace",
            Self::Upgrade => "upgrade",
        }
    }
}

/// 要重装/修复的组件。粒度对齐 download/game 侧的组件概念，便于 agent 直接调用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    /// Java 运行时本身。
    Java,
    /// 游戏主 jar。
    GameJar,
    /// 版本清单（version JSON）。
    GameJson,
    /// 未指定具体 loader。
    Loader,
    /// Fabric loader。
    Fabric,
    /// Forge。
    Forge,
    /// NeoForge。
    #[serde(rename = "neoforge")]
    NeoForge,
    /// OptiFine。
    #[serde(rename = "optifine")]
    OptiFine,
    /// natives 原生库。
    Natives,
    /// mods 目录（含「jar 被解压成目录」的修复）。
    Mods,
    /// libraries 库文件。
    Libraries,
    /// assets 资源。
    Assets,
    /// MixinBootstrap 等前置。
    MixinBootstrap,
}

impl ComponentKind {
    /// 稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Java => "java",
            Self::GameJar => "game_jar",
            Self::GameJson => "game_json",
            Self::Loader => "loader",
            Self::Fabric => "fabric",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
            Self::OptiFine => "optifine",
            Self::Natives => "natives",
            Self::Mods => "mods",
            Self::Libraries => "libraries",
            Self::Assets => "assets",
            Self::MixinBootstrap => "mixin_bootstrap",
        }
    }
}

/// 换 Java 的参数。
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeJava {
    /// 建议的大版本（17 / 21 …）。
    #[serde(default)]
    pub required_major: Option<u32>,
    /// 允许的最高大版本（老 Forge 只能用 Java 8 时给 `8`）。
    #[serde(default)]
    pub max_major: Option<u32>,
    /// 从该命名捕获组取 `class file version N`，按 `N - 44` 换算成 Java 大版本。
    #[serde(default)]
    pub class_file_version_group: Option<String>,
    /// JVM 实现偏好。
    #[serde(default)]
    pub vendor: JavaVendor,
    /// 给用户看的补充说明。
    #[serde(default)]
    pub note: Option<String>,
}

/// 调内存的参数。
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdjustMemory {
    /// 建议的 `-Xmx`（MB）；规则不确定时留空，由 UI 按物理内存算。
    #[serde(default)]
    pub suggested_xmx_mb: Option<u32>,
    /// `-Xmx` 的下限（MB）。
    #[serde(default)]
    pub min_xmx_mb: Option<u32>,
    /// 需要 32 位 → 64 位 Java（「Could not reserve enough space」类）。
    #[serde(default)]
    pub need_64bit_java: bool,
    /// 需要用户扩大系统虚拟内存 / 关掉占内存的程序。
    #[serde(default)]
    pub increase_system_swap: bool,
    /// 补充说明。
    #[serde(default)]
    pub note: Option<String>,
}

/// 改图形设置的参数。
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeGraphics {
    /// 要改的设置类别。
    #[serde(default)]
    pub setting: GraphicsSetting,
    /// 建议更新显卡驱动。
    #[serde(default)]
    pub update_driver: bool,
    /// 建议的目标值（如 `854x480`、`OpenGL 兼容模式`）。
    #[serde(default)]
    pub suggested_value: Option<String>,
    /// 补充说明。
    #[serde(default)]
    pub note: Option<String>,
}

/// 删/禁/换 mod 的参数。
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveMod {
    /// modid 列表，允许 `{group}` 模板（由规则命名捕获组代入）。
    #[serde(default)]
    pub mod_ids: Vec<String>,
    /// 处理方式。
    #[serde(default)]
    pub action: ModAction,
    /// 补充说明。
    #[serde(default)]
    pub note: Option<String>,
}

/// 重装/修组件的参数。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reinstall {
    /// 要修的组件。
    pub component: ComponentKind,
    /// 出问题的路径提示（来自规则捕获组或原文）。
    #[serde(default)]
    pub path_hint: Option<String>,
    /// 补充说明。
    #[serde(default)]
    pub note: Option<String>,
}

/// 求助/上报的参数。
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeekHelp {
    /// 建议导出诊断包（日志 + 崩溃报告 + 清单 + 环境）。
    #[serde(default)]
    pub export_diagnostics: bool,
    /// 还需要用户补充什么（「把日志上限调到 20000 行再复现」）。
    #[serde(default)]
    pub needs_more_info: Vec<String>,
    /// 补充说明。
    #[serde(default)]
    pub note: Option<String>,
}
