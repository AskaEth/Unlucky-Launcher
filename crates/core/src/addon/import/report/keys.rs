//! 文案 key 常量表（**稳定字符串**，M109/M113 的页面按这些字符串查本地化文案）。
//!
//! core 不出现面向用户的成品文案（诊断信息除外）；三态/阶段/缺失/冲突/注记各自带一个 key，
//! 逐条原因也在报告条目里（`message_key` 字段）。
//!
//! `ALL` 是"没有死常量"的守门用表：`addon/tests/import_report.rs` 会核对
//! ①常量定义集合 == `ALL`（不多不少）；②每个常量除了定义与 `ALL` 里的列举之外，至少还有
//! 一处真实引用。任何一条不满足就失败——避免这张表腐烂成"看着齐全、其实没人用"。

/// 阶段：读取清单。
pub const STAGE_PROBE: &str = "pack.import.stage.probe";
/// 阶段：解压覆盖内容。
pub const STAGE_EXTRACT: &str = "pack.import.stage.extract";
/// 阶段：补全缺失文件。
pub const STAGE_FETCH: &str = "pack.import.stage.fetch";
/// 阶段：落地（rename / journal）。
pub const STAGE_COMMIT: &str = "pack.import.stage.commit";
/// 阶段：登记实例。
pub const STAGE_REGISTER: &str = "pack.import.stage.register";

/// 状态：成功。
pub const STATUS_SUCCESS: &str = "pack.import.status.success";
/// 状态：部分成功。
pub const STATUS_PARTIAL: &str = "pack.import.status.partial";
/// 状态：失败（是否回滚干净看 `ImportReport::rolled_back`）。
pub const STATUS_FAILED: &str = "pack.import.status.failed";
/// 状态：**用户取消**（不是失败，`error` 为空）。
pub const STATUS_CANCELLED: &str = "pack.import.status.cancelled";

/// 注记：`server-overrides/` 客户端不解压。
pub const NOTE_SERVER_OVERRIDES_SKIPPED: &str = "pack.import.note.server_overrides_skipped";
/// 注记：`dependencies` 里不认识的键（`parchment` 之类）。
pub const NOTE_UNKNOWN_DEPENDENCY: &str = "pack.import.note.unknown_dependency";
/// 注记：`env.client=unsupported` 的条目被跳过。
pub const NOTE_ENV_UNSUPPORTED: &str = "pack.import.note.env_unsupported";
/// 注记：路径以 `.disabled` 结尾（禁用态，按原后缀落地）。
pub const NOTE_DISABLED_SUFFIX: &str = "pack.import.note.disabled_suffix";
/// 注记：路径以 `.old` 结尾（只识别与保留，不加语义）。
pub const NOTE_OLD_SUFFIX: &str = "pack.import.note.old_suffix";
/// 注记：`env.client=optional`（"可选"不等于"禁用"，照装）。
pub const NOTE_OPTIONAL_FILE: &str = "pack.import.note.optional_file";
/// 注记：清单里有多个加载器，只按 primary/第一条组计划。
pub const NOTE_LOADER_MULTIPLE: &str = "pack.import.note.loader_multiple";
/// 注记：实例已登记。
pub const NOTE_REGISTERED: &str = "pack.import.note.registered";
/// 注记：从 `instance.cfg` 取到了内存设置。
pub const NOTE_INSTANCE_CFG_MEMORY: &str = "pack.import.note.instance_cfg_memory";
/// 注记：基线文件读不出来（本次按"没有基线"处理）。
pub const NOTE_BASELINE_UNREADABLE: &str = "pack.import.note.baseline_unreadable";
/// 注记：基线属于**另一个整合包**（本次按"没有基线"处理）。
pub const NOTE_BASELINE_PACK_MISMATCH: &str = "pack.import.note.baseline_pack_mismatch";
/// 注记：提交前清掉了下载引擎的中间产物（`.ullpart-*` / `.ullmeta.json`）。
pub const NOTE_ENGINE_ARTIFACTS_DROPPED: &str = "pack.import.note.engine_artifacts_dropped";
/// 注记：回滚**未完整完成**（逐项列失败，`rolled_back` 为 false）。
pub const NOTE_ROLLBACK_INCOMPLETE: &str = "pack.import.note.rollback_incomplete";
/// 注记：用户取消，管线在检查点停下（说明停在哪一步、已处理的条目未落地）。
pub const NOTE_CANCELLED: &str = "pack.import.note.cancelled";
/// 注记：同包另一条声明遮蔽了这个路径（说清谁赢、两份内容是否相同）。
pub const NOTE_OVERRIDE_SHADOWED: &str = "pack.import.note.override_shadowed";
/// 注记：这些远端条目预检判不了（补全前没有路径 / 有路径但缺 sha1 声明），无法预判冲突。
pub const NOTE_REMOTE_PATH_UNKNOWN: &str = "pack.import.note.remote_path_unknown";
/// 注记：这些远端条目按 sha512 从 Modrinth 补全了 sha1（补不出的另有计数）。
pub const NOTE_REMOTE_COMPLETED: &str = "pack.import.note.remote_completed";

/// 缺失：调用方没给 fetcher（离线导入）。
pub const MISSING_OFFLINE: &str = "pack.import.missing.offline";
/// 缺失：条目本身补不出下载地址（CF `files[]` 缺 API 客户端 / `downloads` 为空）。
pub const MISSING_NO_SOURCE: &str = "pack.import.missing.no_source";
/// 缺失：下载/补全尝试过但失败（404 / 网络）。
pub const MISSING_FETCH_FAILED: &str = "pack.import.missing.fetch_failed";

/// 冲突：用户改过这个文件。
pub const CONFLICT_USER_MODIFIED: &str = "pack.import.conflict.user_modified";
/// 冲突：用户删过这个文件。
pub const CONFLICT_USER_DELETED: &str = "pack.import.conflict.user_deleted";
/// 冲突：磁盘上本来就有同名内容（没有基线可判）。
pub const CONFLICT_PRE_EXISTING: &str = "pack.import.conflict.pre_existing";

/// MultiMC 不支持的 component（逐项列出）。
pub const REJECTED_COMPONENT: &str = "pack.import.rejected.component";
/// 致命失败后已完成回滚（目标目录零残留）。
pub const ROLLBACK_DONE: &str = "pack.import.rollback.done";

/// 全部 key（守门断言"没有死常量"用）。
pub const ALL: &[&str] = &[
    STAGE_PROBE,
    STAGE_EXTRACT,
    STAGE_FETCH,
    STAGE_COMMIT,
    STAGE_REGISTER,
    STATUS_SUCCESS,
    STATUS_PARTIAL,
    STATUS_FAILED,
    STATUS_CANCELLED,
    NOTE_SERVER_OVERRIDES_SKIPPED,
    NOTE_UNKNOWN_DEPENDENCY,
    NOTE_ENV_UNSUPPORTED,
    NOTE_DISABLED_SUFFIX,
    NOTE_OLD_SUFFIX,
    NOTE_OPTIONAL_FILE,
    NOTE_LOADER_MULTIPLE,
    NOTE_REGISTERED,
    NOTE_INSTANCE_CFG_MEMORY,
    NOTE_BASELINE_UNREADABLE,
    NOTE_BASELINE_PACK_MISMATCH,
    NOTE_ENGINE_ARTIFACTS_DROPPED,
    NOTE_ROLLBACK_INCOMPLETE,
    NOTE_CANCELLED,
    NOTE_OVERRIDE_SHADOWED,
    NOTE_REMOTE_PATH_UNKNOWN,
    NOTE_REMOTE_COMPLETED,
    MISSING_OFFLINE,
    MISSING_NO_SOURCE,
    MISSING_FETCH_FAILED,
    CONFLICT_USER_MODIFIED,
    CONFLICT_USER_DELETED,
    CONFLICT_PRE_EXISTING,
    REJECTED_COMPONENT,
    ROLLBACK_DONE,
];
