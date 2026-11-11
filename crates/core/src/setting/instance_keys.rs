//! `config/profiles.json` 与 `config/instances/<id>.json` 的键表。
//!
//! 实例模型预告见 `DevDocs/PlanDoc/04-data-and-config.md` §4：
//! 每实例 = 一个 `.minecraft` 目录绑定 + 独立设置覆盖链（全局 → 实例 → 临时启动参数），
//! 实例 id 用稳定 UUID（不用路径——路径会变）。
//!
//! # 实例文件的形状（D20 定案，research/04 §5.1）
//!
//! 实例文件是**稀疏 camelCase 扁平结构**，与 `game::install::instance::InstanceConfig`
//! 的 serde 形状逐键一致（那份结构才是写入者，本文件是它的 schema 白名单视图）：
//!
//! ```jsonc
//! {
//!   "schemaVersion": 1,
//!   "id": "<uuid>",
//!   "versionId": "1.20.1",
//!   "isolation": "custom",          // 缺键 = inherit（跟随全局，不落盘）
//!   "gameDir": "D:/mcs/farm",       // 仅 custom 时有值
//!   "overrides": ["maxMemory"],
//!   "lastModeChange": { "at": "2026-10-30T12:00:00Z", "from": "shared", "to": "isolated",
//!                       "probe": { "isolatedSide": 0, "sharedSide": 12 }, "warned": true }
//! }
//! ```
//!
//! 三条不变量：
//! 1. `isolation` 默认 `inherit` 且**不落盘**——改全局模式对存量实例立即生效
//!    （`document` 的「默认值不进文件」保证这一点，直接避开 PCL2 的默认值固化缺陷）；
//!    显式写 `"isolation": "inherit"` 也合法：`InstanceConfig` 的反序列化把它归一成
//!    「键不存在」（等价语义，不会出现两层读写回环不一致）；
//! 2. `isolation`/`lastModeChange` 由用户（前者）与启动器（后者）维护，**AI 不可写**；
//! 3. 派生的绝对路径不入档：`isolated` 只是「运行目录 = `versions/<versionId>`」的语法糖，
//!    派生实现归 `game::isolation`。

use super::catalog::SettingSchema;
use super::schema::{SchemaId, SchemaVersion};
use super::spec::{AiPolicy, SettingSpec};

/// 实例级 `isolation` 允许值（`inherit` = 键缺失，跟随全局 `instances.mode`）。
pub const ISOLATION_OVERRIDES: &[&str] = &["inherit", "isolated", "shared", "custom"];
/// 切换检测记录里 `from`/`to` 的允许值（`inherit` 不是生效档位，故不在表里）。
pub const ISOLATION_RESOLVED: &[&str] = &["isolated", "shared", "custom"];
/// 实例可覆盖的设置键（enum 白名单，拼错即校验失败而不是静默不生效）。
pub const OVERRIDE_KEYS: &[&str] = &["maxMemory", "javaType", "runningDirectory"];
/// `launch.graphicsBackend` 的允许值（`default` = 不下发 `--graphicsBackend`）。
///
/// 与 `crate::game::launch_overrides::GraphicsBackend::IDS` 必须是同一份集合——setting 层
/// 不依赖 game 层（分层纪律），一致性由本文件的 `graphics_backends_match_the_game_layer` 固定。
pub const GRAPHICS_BACKENDS: &[&str] = &["default", "opengl", "vulkan"];

/// profiles.json 当前 schema 版本。
#[must_use]
pub fn profiles_version() -> SchemaVersion {
    SchemaVersion::new(1, 0, 0)
}

/// instances/<id>.json 当前 schema 版本。
///
/// `1.1.0` 登记 D20 的 `isolation` 档位（`inherit`/`isolated`/`shared`/`custom`）与
/// `lastModeChange` 切换检测键；老文件缺键即默认值，不需要迁移步骤（major 未变）。
#[must_use]
pub fn instance_version() -> SchemaVersion {
    SchemaVersion::new(1, 1, 0)
}

/// `config/profiles.json` 的 schema。
#[must_use]
pub fn profiles_schema() -> SettingSchema {
    SettingSchema::new(SchemaId::PROFILES, profiles_version())
        .with(SettingSpec::text(
            "selected_profile_id",
            "",
            64,
            AiPolicy::audited(),
            "上次选中的实例 id",
        ))
        .with(SettingSpec::any(
            "profiles",
            AiPolicy::locked(),
            "实例档案列表（数据而非配置，由 game 模块维护；AI 不得整体改写）",
        ))
}

/// `config/instances/<id>.json` 的 schema。
#[must_use]
pub fn instance_schema() -> SettingSchema {
    SettingSchema::new(SchemaId::INSTANCE, instance_version())
        .with(SettingSpec::text(
            "id",
            "",
            64,
            AiPolicy::locked(),
            "实例稳定 id（UUID，与文件名一致；不允许改写身份）",
        ))
        .with(SettingSpec::text(
            "versionId",
            "",
            128,
            AiPolicy::audited(),
            "绑定的版本 id（versions/<id>）",
        ))
        .with(SettingSpec::enumeration(
            "isolation",
            "inherit",
            ISOLATION_OVERRIDES,
            AiPolicy::sensitive(),
            "实例隔离档位：inherit（跟随全局 instances.mode，不落盘）/ isolated（运行目录派生 versions/<versionId>）/ shared（共用 .minecraft 根）/ custom（用 gameDir）",
        ))
        .with(SettingSpec::path(
            "gameDir",
            AiPolicy::sensitive(),
            "自定义运行目录（仅 isolation=custom 生效）",
        ))
        .with(SettingSpec::enum_list(
            "overrides",
            OVERRIDE_KEYS,
            AiPolicy::locked(),
            "该实例覆盖了全局哪些设置键（enum 白名单，拼错即校验失败）",
        ))
        // M157：实例级启动参数（`launch` 段，五项全可选、逐项回落到全局）。
        // 作者是 `game::install::instance::InstanceConfig`（serde 形状与这些路径逐键对应），
        // 回落规则与生效值在 `game::launch_overrides`，拼装消费在 `launch::overrides`。
        .with(SettingSpec::text(
            "launch.jvmArgs",
            "",
            1024,
            AiPolicy::sensitive(),
            "实例级 JVM 参数（空白分隔、支持引号）；留空 = 只用全局 java.extra_args（两者叠加，实例优先）",
        ))
        .with(SettingSpec::text(
            "launch.gameArgs",
            "",
            1024,
            AiPolicy::audited(),
            "实例级游戏参数（追加在命令行末段，可覆盖前面一切游戏参数）；留空 = 不追加",
        ))
        .with(SettingSpec::int(
            "launch.memoryMb",
            0,
            (0, 65536),
            AiPolicy::sensitive(),
            "实例级最大堆（MiB）；0/缺键 = 跟随全局 java.default_memory_mb（有效区间 256~65536）",
        ))
        .with(SettingSpec::path(
            "launch.javaPath",
            AiPolicy::sensitive(),
            "实例级 Java 可执行文件；留空 = 跟随全局 java.selected_path 再退到自动挑（托管 > JAVA_HOME > PATH）",
        ))
        .with(SettingSpec::enumeration(
            "launch.graphicsBackend",
            "default",
            GRAPHICS_BACKENDS,
            AiPolicy::audited(),
            "图形后端：default（不下发）/ opengl / vulkan。只对 MC 26.2-snapshot-2+ 下发，老版本会被门槛拦下并给出诊断告警",
        ))
        .with(SettingSpec::text(
            "lastModeChange.at",
            "",
            40,
            AiPolicy::locked(),
            "上次切换隔离档位的时间（RFC-3339，启动器写入）",
        ))
        .with(SettingSpec::enumeration(
            "lastModeChange.from",
            "isolated",
            ISOLATION_RESOLVED,
            AiPolicy::locked(),
            "切换前的生效档位（启动器写入）",
        ))
        .with(SettingSpec::enumeration(
            "lastModeChange.to",
            "isolated",
            ISOLATION_RESOLVED,
            AiPolicy::locked(),
            "切换后的生效档位（启动器写入）",
        ))
        .with(SettingSpec::int(
            "lastModeChange.probe.isolatedSide",
            0,
            (0, 1_000_000_000),
            AiPolicy::locked(),
            "切换时隔离侧（versions/<versionId>）的内容条目数（只探测，不迁移）",
        ))
        .with(SettingSpec::int(
            "lastModeChange.probe.sharedSide",
            0,
            (0, 1_000_000_000),
            AiPolicy::locked(),
            "切换时共享侧（.minecraft 根）的内容条目数（只探测，不迁移）",
        ))
        .with(SettingSpec::bool(
            "lastModeChange.warned",
            false,
            AiPolicy::locked(),
            "该次切换的提示是否已展示过（保证切换后首次启动只提示一次）",
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M157：图形后端词表**只有一份真相源**（`game::launch_overrides::GraphicsBackend`），
    /// setting 层是它的白名单视图——两边脱节就是「写得进、读不出」或「UI 三态少一档」。
    #[test]
    fn graphics_backends_match_the_game_layer() {
        use crate::game::launch_overrides::GraphicsBackend;
        let mut setting: Vec<&str> = GRAPHICS_BACKENDS.to_vec();
        let mut game: Vec<&str> = GraphicsBackend::IDS.to_vec();
        setting.sort_unstable();
        game.sort_unstable();
        assert_eq!(setting, game, "setting 白名单与 game 层词表脱节了");
        for id in GRAPHICS_BACKENDS {
            assert!(
                GraphicsBackend::parse(id).is_some(),
                "`{id}` 在 game 层解析不出来"
            );
        }
    }

    /// 五项键真的进了实例 schema 白名单（且 `default` 之外的后端值被拒）。
    #[test]
    fn launch_section_keys_are_registered() {
        let schema = instance_schema();
        assert!(schema
            .validate("launch.jvmArgs", &serde_json::json!("-XX:+UseG1GC"))
            .is_ok());
        assert!(schema
            .validate("launch.gameArgs", &serde_json::json!("--fullscreen"))
            .is_ok());
        assert!(schema
            .validate("launch.memoryMb", &serde_json::json!(4096))
            .is_ok());
        assert!(schema
            .validate(
                "launch.javaPath",
                &serde_json::json!("D:/jdk21/bin/java.exe")
            )
            .is_ok());
        assert!(schema
            .validate("launch.graphicsBackend", &serde_json::json!("vulkan"))
            .is_ok());
        // 非法后端值：白名单外必须报错（手改文件绕不过校验）。
        assert!(schema
            .validate("launch.graphicsBackend", &serde_json::json!("vulkan2"))
            .is_err());
        // 越界内存：schema 层也拦。
        assert!(schema
            .validate("launch.memoryMb", &serde_json::json!(1_000_000))
            .is_err());
    }
}
