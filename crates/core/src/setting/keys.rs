//! `config/launcher.json` 的键表（全局设置）。
//!
//! 键名与默认值对齐 `DevDocs/PlanDoc/04-data-and-config.md` §2-§3 与 `05/06/07/08` 各专题；
//! AI 可写 + 敏感分级按 `02-ai-agent-design.md` §3.1 的边界落：**密钥类一律 locked**。

use super::catalog::SettingSchema;
use super::schema::{SchemaId, SchemaVersion};
use super::spec::{AiPolicy, SettingSpec};

/// `core.data_mode` 允许值。
pub const DATA_MODES: &[&str] = &["portable", "standard", "custom"];
/// `download.region` 允许值。
pub const REGIONS: &[&str] = &["auto", "cn", "global"];
/// `download.mirror_preference` 允许值。
pub const MIRROR_PREFERENCES: &[&str] = &["auto", "official", "mirror"];
/// `download.proxy_mode` 允许值。
pub const PROXY_MODES: &[&str] = &["none", "system", "manual"];
/// `ai.provider` 允许值。
pub const AI_PROVIDERS: &[&str] = &["openai_compatible", "anthropic", "ollama", "disabled"];
/// `ui.language` 允许值。
pub const LANGUAGES: &[&str] = &["zh_cn", "en_us"];
/// `appearance.theme_mode` 允许值（D17 定案：auto/light/dark）。
pub const THEME_MODES: &[&str] = &["auto", "light", "dark"];
/// `ui.theme` 的旧允许值（该键已废弃，仅读兼容，不再落盘）。
pub const LEGACY_THEMES: &[&str] = &["system", "metro_light", "metro_dark"];
/// 旧 `ui.theme` 值 → 新 `appearance.theme_mode` 值（tower 键冲突裁决的读兼容映射）。
pub const THEME_ALIASES: &[(&str, &str)] = &[
    ("system", "auto"),
    ("metro_light", "light"),
    ("metro_dark", "dark"),
];
/// `log.level` 允许值。
pub const LOG_LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];
/// `instances.mode` 允许值（D20 定案：默认隔离，用户可手动切共享）。
pub const INSTANCE_MODES: &[&str] = &["isolated", "shared"];
/// `java.installRoot` 未设置时的哨兵（空串 = 按数据目录模式派生落点）。
pub const JAVA_INSTALL_ROOT_UNSET: &str = "";
/// `java.vendorPriority` 的允许值（D23：四家发行版，顺序即优先级）。
///
/// 与 `crate::game::java::vendor::VENDOR_IDS` 必须是同一份集合——setting 层不依赖 game 层
/// （分层纪律），一致性由 `tests/setting/java_install_keys.rs` 的契约测试固定。
pub const JDK_VENDORS: &[&str] = &["temurin", "oracle", "microsoft", "zulu"];

/// launcher.json 当前 schema 版本。
///
/// `1.1.0` 登记 D20 实例隔离键（`instances.mode` 等）。
/// `1.2.0` 登记 D23 的 JDK 落点与发行版优先级键（`java.installRoot` / `java.vendorPriority`）：
/// 同样只推 minor、不强制迁移——[`super::document`] 的「默认值不落盘」让老文件缺键即取默认，
/// 迁移链（按 major 走）不需要新步骤；`1.1.0 → 1.2.0` 的差异只体现在文件里出现新键之后。
/// `1.3.0` 登记 D17 的「外观（高级）」P1 键（`appearance.bg_image*` / `panel_alpha` / `glass` /
/// `accent_color`）：同一套纯增量语义——出厂全默认（高级特性全关）即等于老文件缺键。
/// `1.4.0` 登记 P4 的外观下层三键（`appearance.window_opacity` / `titlebar_mode` /
/// `palette_source`，M91 · D36/D37/D38）：仍然只推 minor、不加迁移步骤——三者的出厂值
/// （不透明 / auto / 内置色卡）正是「老文件没有这些键」时的等价物。
/// `1.5.0` 登记 M116 的窗口圆角策略（`appearance.window_corner`，出厂 `system`）：
/// 同一套纯增量语义——**新读旧**：老文件缺该键 ⇒ 读默认 `system` ⇒ 跟随系统，与写进文件后行为一致。
///
/// **旧读新（方向相反，措辞必须精确）**：1.4.0 的客户端读 1.5.0 的文件时，
/// [`super::schema::SchemaCompat::check_url`] 走 `found.minor > expected.minor` →
/// `ReadOnlyPreserveSchema` → [`super::schema::SchemaCompat::access`] → `FileAccess::ReadOnly`。
/// 也就是说**整份配置对它只读**（值照读、`store.set` 一律 `SettingError::ReadOnly`、
/// `save` 跳过），不是「只是少一档」。这是 minor 提升的既定代价——也正是这条键必须推 minor
/// 而不是偷偷塞进 1.4.0 的原因：回退到旧版本会出现「设置改不动」，而原因只在
/// `$schema` 那一行里，不看版本号无从排查。
#[must_use]
pub fn launcher_version() -> SchemaVersion {
    SchemaVersion::new(1, 5, 0)
}

/// `config/launcher.json` 的 schema。
#[must_use]
pub fn launcher_schema() -> SettingSchema {
    SettingSchema::new(SchemaId::LAUNCHER, launcher_version())
        .with(SettingSpec::enumeration(
            "core.data_mode",
            "portable",
            DATA_MODES,
            AiPolicy::sensitive(),
            "数据目录模式：portable（exe 旁）/ standard（系统标准目录）/ custom（自定义）",
        ))
        .with(SettingSpec::path(
            "core.data_custom_root",
            AiPolicy::sensitive(),
            "custom 模式的根目录（必须是绝对路径）",
        ))
        .with(SettingSpec::path(
            "core.launcher_home",
            AiPolicy::sensitive(),
            "显式覆盖启动器数据目录（.ULL 的等价物）",
        ))
        .with(SettingSpec::path(
            "core.game_home",
            AiPolicy::sensitive(),
            "游戏目录（.minecraft），与数据目录分离指定",
        ))
        // D20（research/04 §5.1）：只留一个全局默认档，实例级 override 在
        // `instance_keys::instance_schema`（`isolation`），派生逻辑在 `game::isolation`。
        .with(SettingSpec::enumeration(
            "instances.mode",
            "isolated",
            INSTANCE_MODES,
            AiPolicy::sensitive(),
            "实例默认隔离模式：isolated（运行目录派生 versions/<版本 id>，D20 默认）/ shared（所有实例共用 .minecraft 根）",
        ))
        .with(SettingSpec::int(
            "download.concurrency",
            8,
            (1, 256),
            AiPolicy::silent(),
            "下载并发数",
        ))
        .with(SettingSpec::int(
            "download.retry_limit",
            3,
            (0, 10),
            AiPolicy::silent(),
            "单个文件重试次数",
        ))
        .with(SettingSpec::enumeration(
            "download.region",
            "auto",
            REGIONS,
            AiPolicy::audited(),
            "下载地区判定：auto（时区+语言+探测）/ cn / global",
        ))
        .with(SettingSpec::enumeration(
            "download.mirror_preference",
            "auto",
            MIRROR_PREFERENCES,
            AiPolicy::audited(),
            "镜像偏好：auto（健康度评分）/ official（官方源优先）/ mirror（镜像优先）",
        ))
        .with(SettingSpec::enumeration(
            "download.proxy_mode",
            "none",
            PROXY_MODES,
            AiPolicy::sensitive(),
            "代理模式：none / system / manual",
        ))
        .with(SettingSpec::text(
            "download.proxy_url",
            "",
            512,
            AiPolicy::sensitive(),
            "manual 代理地址（如 http://127.0.0.1:7890）",
        ))
        .with(SettingSpec::path(
            "java.selected_path",
            AiPolicy::sensitive(),
            "用户选定的 Java 可执行文件路径",
        ))
        .with(SettingSpec::bool(
            "java.auto_install",
            true,
            AiPolicy::audited(),
            "缺 Java 时自动下载（D13 基准 JDK17）",
        ))
        // D23：托管 JDK 的落点三模式。语义在 `game::java::location`：
        // 未设置（空串）→ 按 `core.data_mode` 派生（Portable 数据根 / Standard 系统目录）；
        // 设置了 → 覆盖前两者，按 `<vendor>/<version>` 分层。
        .with(SettingSpec::text(
            "java.installRoot",
            JAVA_INSTALL_ROOT_UNSET,
            512,
            AiPolicy::sensitive(),
            "托管 JDK 的自定义安装根（留空 = 按数据目录模式：portable 进数据根、standard 进 %APPDATA%\\Java）；设置后覆盖两者，JDK 落在 <root>/<发行方>/<版本>/",
        ))
        // D23：发行版回退链的用户优先级。空数组 = 按地区默认序
        // （国内 Temurin→Oracle→Microsoft→Zulu；国外 Temurin→Microsoft→Oracle→Zulu）。
        .with(SettingSpec::enum_list(
            "java.vendorPriority",
            JDK_VENDORS,
            AiPolicy::audited(),
            "JDK 发行版优先级（temurin/oracle/microsoft/zulu）；留空按地区默认序，只写几家则其余按默认序补齐",
        ))
        .with(SettingSpec::int(
            "java.default_memory_mb",
            2048,
            (512, 65536),
            AiPolicy::silent(),
            "默认分配给游戏的最大堆内存（MB）",
        ))
        .with(SettingSpec::text(
            "java.extra_args",
            "",
            1024,
            AiPolicy::sensitive(),
            "附加 JVM 参数（原样拼到命令行）",
        ))
        .with(SettingSpec::bool(
            "ai.enabled",
            true,
            AiPolicy::silent(),
            "启用内置 AI agent（关闭后规则引擎诊断仍可用）",
        ))
        .with(SettingSpec::enumeration(
            "ai.provider",
            "openai_compatible",
            AI_PROVIDERS,
            AiPolicy::sensitive(),
            "LLM 提供方：openai_compatible（含本地 llama.cpp/DeepSeek/Qwen）/ anthropic / ollama / disabled",
        ))
        .with(SettingSpec::text(
            "ai.base_url",
            "",
            512,
            AiPolicy::sensitive(),
            "LLM 端点 base_url（OpenAI 兼容形态）",
        ))
        .with(SettingSpec::text(
            "ai.model",
            "",
            128,
            AiPolicy::audited(),
            "模型名（如 gpt-4o-mini / deepseek-chat）",
        ))
        .with(SettingSpec::bool(
            "ai.yolo",
            false,
            AiPolicy::sensitive(),
            "自动批准全部工具调用（审计照记，白名单不变）",
        ))
        .with(SettingSpec::new(
            "ai.api_key",
            super::spec::ValueKind::Text { max_len: 4096 },
            serde_json::Value::Null,
            AiPolicy::locked(),
            "LLM API 密钥——AI 永不读写的凭据（D9）",
        ))
        // M39（P3 MCP server，02 §0「传输定论」）：对外 agent 工具服务。
        // 三个开关默认「关 / 只读」——外部客户端拿到的是与内置客户端**同一注册表**的工具，
        // 但写权限必须用户显式打开（02 §1「同一注册表、同一安全策略、同一审计日志」）。
        .with(SettingSpec::bool(
            "agent.mcp.enabled",
            false,
            AiPolicy::sensitive(),
            "对外 MCP server 总开关（127.0.0.1 上的 Streamable HTTP；stdio 形态按需显式启动）",
        ))
        .with(SettingSpec::int(
            "agent.mcp.port",
            47823,
            (1024, 65535),
            AiPolicy::sensitive(),
            "MCP server 的 Streamable HTTP 端口（只绑 127.0.0.1）",
        ))
        .with(SettingSpec::bool(
            "agent.mcp.allow_write",
            false,
            AiPolicy::sensitive(),
            "允许外部 MCP 客户端调用写工具（默认只读；敏感键仍受分级确认与 YOLO 规则约束）",
        ))
        .with(SettingSpec::text(
            "agent.mcp.token",
            "",
            256,
            AiPolicy::locked(),
            "MCP server 本地访问令牌（Authorization: Bearer；空 = 不校验，仅绑 127.0.0.1）",
        ))
        .with(SettingSpec::enumeration(
            "ui.language",
            "zh_cn",
            LANGUAGES,
            AiPolicy::audited(),
            "界面语言",
        ))
        .with(SettingSpec::enumeration(
            "appearance.theme_mode",
            "auto",
            THEME_MODES,
            AiPolicy::silent(),
            "主题模式：auto（跟随系统）/ light（Metro 浅色）/ dark（Metro 深色）",
        ))
        // D17「外观（高级）」P1 四件（09 §101-104 / §111）：**默认全部关闭**（出厂 Metro 纯色）。
        // 分级一律 audited（Medium）：全部是纯视觉、可一键复原的键，没有安全面（不碰凭据/网络/
        // 数据根），谈不上 High 级确认；但 AI 改过什么要留审计痕迹，故不用 silent。
        .with(SettingSpec::bool(
            "appearance.bg_image_enabled",
            false,
            AiPolicy::audited(),
            "自定义窗口背景图开关（默认关；开启时设置页给出对比度提示）",
        ))
        .with(SettingSpec::text(
            "appearance.bg_image",
            "",
            512,
            AiPolicy::audited(),
            "背景图路径（PNG；空 = 未设置。Slint 默认解码器只带 PNG）",
        ))
        .with(SettingSpec::enumeration(
            "appearance.bg_fill",
            "cover",
            super::appearance::BG_FILLS,
            AiPolicy::audited(),
            "背景图填充模式：cover（铺满裁切）/ contain（完整留底色）/ tile（平铺）",
        ))
        .with(SettingSpec::float(
            "appearance.bg_dim",
            0.35,
            (
                super::appearance::BG_DIM_MIN,
                super::appearance::BG_DIM_MAX,
            ),
            AiPolicy::audited(),
            "背景图叠加暗化层强度（越大前景越清晰；09 §101 的对比度保底手法）",
        ))
        .with(SettingSpec::float(
            "appearance.panel_alpha",
            1.0,
            (super::appearance::PANEL_ALPHA_MIN, 1.0),
            AiPolicy::audited(),
            "全局面板半透明度（卡片/导航/AI 栏统一透明度；1.0 = 出厂不透明）",
        ))
        .with(SettingSpec::bool(
            "appearance.glass",
            false,
            AiPolicy::audited(),
            "玻璃拟态卡片（背景图预生成模糊层作卡片底；Slint 无 backdrop-filter 的替代）",
        ))
        .with(SettingSpec::text(
            "appearance.accent_color",
            "",
            9,
            AiPolicy::audited(),
            "自定义强调色 #RRGGBB（空 = 出厂 #0078D4；accent 令牌全域生效）",
        ))
        // M91（P4 / D36·D37·D38）：「外观下层」三键——只登记取值与范围，判定与落地都在 UI 侧。
        // 分级同 D17 那批（audited/Medium）：纯视觉、可一键复原，但要留审计痕迹。
        .with(SettingSpec::float(
            "appearance.window_opacity",
            1.0,
            (super::appearance::WINDOW_ALPHA_MIN, 1.0),
            AiPolicy::audited(),
            "整窗透明度 0.08~1.0（1.0 = 完全不透明；下限 0.08 是为了不把窗口写到穿透，D36）",
        ))
        .with(SettingSpec::enumeration(
            "appearance.titlebar_mode",
            "auto",
            super::appearance::TITLEBAR_MODES,
            AiPolicy::audited(),
            "标题栏三态：auto（按合成器判定）/ always（总是自绘）/ never（合成器已装饰），默认 auto（D37）",
        ))
        .with(SettingSpec::enumeration(
            "appearance.palette_source",
            "preset",
            super::appearance::PALETTE_SOURCES,
            AiPolicy::audited(),
            "配色来源档（D38 三级）：background-image（背景图取色）/ wallpaper（桌面壁纸取色）/ preset（内置色卡，默认）",
        ))
        // M116：窗口外框圆角策略。只有两档（system / square）——系统不暴露圆角半径，
        // 「固定 8/12/16」只能是自绘圆角，而自绘圆角在无边框 + 整窗 α 上要裁内容层，
        // 会连标题栏三键与缩放热区一起裁掉（用户定案 2026-11-03）。
        .with(SettingSpec::enumeration(
            "appearance.window_corner",
            "system",
            super::appearance::WINDOW_CORNERS,
            AiPolicy::audited(),
            "窗口圆角：system（跟随系统：Win11/Server 2025 走 DWM，Win10 保持直角）/ square（总是直角），默认 system",
        ))
        // 已废弃：D17 定案 appearance.theme_mode 为准（旧值读兼容映射，写永远走新键）。
        .with(
            SettingSpec::new(
                "ui.theme",
                super::spec::ValueKind::Enum(LEGACY_THEMES),
                serde_json::Value::Null,
                AiPolicy::retired(),
                "【已废弃】改用 appearance.theme_mode；旧值读兼容：system→auto / metro_light→light / metro_dark→dark",
            )
            .deprecated("appearance.theme_mode", THEME_ALIASES),
        )
        .with(SettingSpec::float(
            "ui.scale",
            1.0,
            (0.75, 2.0),
            AiPolicy::silent(),
            "界面缩放系数",
        ))
        .with(SettingSpec::int(
            "ui.window.width",
            1280,
            (320, 16384),
            AiPolicy::silent(),
            "主窗口宽度（逻辑像素）",
        ))
        .with(SettingSpec::int(
            "ui.window.height",
            800,
            (200, 16384),
            AiPolicy::silent(),
            "主窗口高度（逻辑像素）",
        ))
        .with(SettingSpec::enumeration(
            "log.level",
            "info",
            LOG_LEVELS,
            AiPolicy::audited(),
            "启动器日志级别",
        ))
}
