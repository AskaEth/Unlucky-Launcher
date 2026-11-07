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
