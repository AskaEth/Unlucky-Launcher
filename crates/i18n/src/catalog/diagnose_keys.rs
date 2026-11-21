//! `rule id` → 诊断文案键的**接线表**（M392 `DIAGI18N`；M395 追加引擎内置结论）。
//!
//! ## 为什么单独一个文件
//!
//! 键名是机械推导的（`diagnose.title.<rule id>` / `diagnose.hint.<rule id>`，见 [`super::diagnose`]），
//! 但**运行时拿到的是字符串**（`Diagnosis.cause_key` 或规则 id），而 [`Key`] 是枚举——两者之间
//! 必须有一张表。读者也不同：`diagnose.rs` 是译文，本文件是 id → 变体的接线。塞在一起会到
//! 460 行，贴项目 400 行硬线。
//!
//! ## 表里的 74 行 = 规则表 64 条 + 引擎内置结论 10 条
//!
//! 前 64 行顺序与 `rules.d/` 的 category 顺序一致（注释分段）。`hint` **64 条全有**：
//! M392 只给 YAML 里已有 `detail` 的 43 条接了线，其余 21 条当时是 `None`（「不编造」）；
//! M400 `DIAGHINT` 把这 21 条补齐——YAML 的 `cause.detail` 与目录的 zh **逐字相等**（同一句
//! 话存两份，只改一边就红），en 是译文。
//!
//! 后 10 行是引擎内置结论（`signal.*` ×6 + `hs_err.*` ×4，M395 `BUILTIN10`）：它们的文案不在
//! YAML 里，是 `core/src/diagnose/synthetic.rs` 拼的。从 M395 起 core 把动态片段提成
//! `Finding::groups` 的具名参数，界面于是能按 `diagnose.title.<rule id>` 取模板渲染——
//! 在这一格之前，这 10 条在英文界面只能显示「未翻译的系统文案」。它们**一律没有 hint**
//! （core 只有一句结论，没有第二段建议，缺的不补编）。
//!
//! 表与规则表的**逐条对齐**由 `crates/app-iced/src/pages/logs/diagnostic_tests.rs` 的
//! `diagnose_catalog_matches_the_rule_table`（64 条规则）与
//! `builtin_conclusions_read_the_same_in_chinese`（10 条内置结论）守着（那里同时看得见
//! `unlucky_core` 与 `unlucky_i18n`；这两个 crate 之间没有依赖边）。

use super::Key;

/// 一条规则的文案键。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnoseKeys {
    /// 规则 id（`cause.key` 去前缀 `diagnose.cause.` 之后的那一段）。
    pub rule: &'static str,
    /// 结论标题（对应 YAML 的 `cause.summary`）。
    pub title: Key,
    /// 处理建议（对应 YAML 的 `cause.detail`；64 条规则现在都有，M400 补齐了最后 21 条）。
    pub hint: Option<Key>,
}

impl DiagnoseKeys {
    /// 标题之于 `cause.key` 的镜像：`diagnose.title.<rule id>`。
    #[must_use]
    pub fn title_catalog_key(self) -> String {
        format!("diagnose.title.{}", self.rule)
    }

    /// 提示之于 `cause.key` 的镜像：`diagnose.hint.<rule id>`。
    #[must_use]
    pub fn hint_catalog_key(self) -> String {
        format!("diagnose.hint.{}", self.rule)
    }
}

/// 表构造助手（`const` 上下文里用，只为让 64 行各自一行）。
const fn row(rule: &'static str, title: Key, hint: Option<Key>) -> DiagnoseKeys {
    DiagnoseKeys { rule, title, hint }
}

/// 全部 64 条规则（顺序 = `rules.d/` 的规则顺序）。
///
/// `#[rustfmt::skip]` 只为**一行一条**：这张表是数据，不是代码。按 rustfmt 的 100 列折开要 467 行
/// （本仓同类先例：`core/src/addon/import/plan/layer/real_pack.rs`、
/// `design/src/color/critical_planes.rs`）。逐字节的准确性不靠格式，靠
/// `catalog/golden/tail4.rs` / `tail5.rs` 与 `app-iced/src/pages/logs/diagnostic.rs` 的
/// `diagnose_catalog_matches_the_rule_table`。
#[rustfmt::skip]
pub(super) const DIAGNOSE_KEYS: &[DiagnoseKeys] = &[
    // ── fabric ──
    row("fabric.mod_resolution", Key::DiagnoseTitleFabricModResolution, Some(Key::DiagnoseHintFabricModResolution)),
    row("fabric.mod_resolution_conflict", Key::DiagnoseTitleFabricModResolutionConflict, Some(Key::DiagnoseHintFabricModResolutionConflict)),
    row("fabric.mod_resolution_missing", Key::DiagnoseTitleFabricModResolutionMissing, Some(Key::DiagnoseHintFabricModResolutionMissing)),
    row("fabric.mod_resolution_missing_minecraft", Key::DiagnoseTitleFabricModResolutionMissingMinecraft, Some(Key::DiagnoseHintFabricModResolutionMissingMinecraft)),
    row("fabric.mod_resolution_collection", Key::DiagnoseTitleFabricModResolutionCollection, Some(Key::DiagnoseHintFabricModResolutionCollection)),
    row("fabric.mod_resolution_details", Key::DiagnoseTitleFabricModResolutionDetails, Some(Key::DiagnoseHintFabricModResolutionDetails)),
    row("fabric.loader_0_12", Key::DiagnoseTitleFabricLoader012, Some(Key::DiagnoseHintFabricLoader012)),
    row("fabric.warnings", Key::DiagnoseTitleFabricWarnings, Some(Key::DiagnoseHintFabricWarnings)),
    // ── forge ──
    row("forge.missing_dependencies", Key::DiagnoseTitleForgeMissingDependencies, Some(Key::DiagnoseHintForgeMissingDependencies)),
    row("forge.duplicate_mods", Key::DiagnoseTitleForgeDuplicateMods, Some(Key::DiagnoseHintForgeDuplicateMods)),
    row("forge.duplicated_mod", Key::DiagnoseTitleForgeDuplicatedMod, Some(Key::DiagnoseHintForgeDuplicatedMod)),
    row("forge.repeat_installation", Key::DiagnoseTitleForgeRepeatInstallation, Some(Key::DiagnoseHintForgeRepeatInstallation)),
    row("forge.optifine_repeat_installation", Key::DiagnoseTitleForgeOptifineRepeatInstallation, Some(Key::DiagnoseHintForgeOptifineRepeatInstallation)),
    row("forge.too_many_mods", Key::DiagnoseTitleForgeTooManyMods, Some(Key::DiagnoseHintForgeTooManyMods)),
    row("forge.module_name_invalid", Key::DiagnoseTitleForgeModuleNameInvalid, Some(Key::DiagnoseHintForgeModuleNameInvalid)),
    row("forge.bootstrap_failed", Key::DiagnoseTitleForgeBootstrapFailed, Some(Key::DiagnoseHintForgeBootstrapFailed)),
    // ── graphics ──
    row("graphics.gl_operation_failure", Key::DiagnoseTitleGraphicsGlOperationFailure, Some(Key::DiagnoseHintGraphicsGlOperationFailure)),
    row("graphics.opengl_not_supported", Key::DiagnoseTitleGraphicsOpenglNotSupported, Some(Key::DiagnoseHintGraphicsOpenglNotSupported)),
    row("graphics.graphics_driver", Key::DiagnoseTitleGraphicsGraphicsDriver, Some(Key::DiagnoseHintGraphicsGraphicsDriver)),
    row("graphics.macos_display_port", Key::DiagnoseTitleGraphicsMacosDisplayPort, Some(Key::DiagnoseHintGraphicsMacosDisplayPort)),
    row("graphics.resolution_too_high", Key::DiagnoseTitleGraphicsResolutionTooHigh, Some(Key::DiagnoseHintGraphicsResolutionTooHigh)),
    row("graphics.rtss_sodium", Key::DiagnoseTitleGraphicsRtssSodium, Some(Key::DiagnoseHintGraphicsRtssSodium)),
    row("graphics.vulkan_loader_missing", Key::DiagnoseTitleGraphicsVulkanLoaderMissing, Some(Key::DiagnoseHintGraphicsVulkanLoaderMissing)),
    row("graphics.vulkan_device_version_too_low", Key::DiagnoseTitleGraphicsVulkanDeviceVersionTooLow, Some(Key::DiagnoseHintGraphicsVulkanDeviceVersionTooLow)),
    row("graphics.vulkan_missing_extension", Key::DiagnoseTitleGraphicsVulkanMissingExtension, Some(Key::DiagnoseHintGraphicsVulkanMissingExtension)),
    row("graphics.vulkan_missing_feature", Key::DiagnoseTitleGraphicsVulkanMissingFeature, Some(Key::DiagnoseHintGraphicsVulkanMissingFeature)),
    row("graphics.no_graphics_backend", Key::DiagnoseTitleGraphicsNoGraphicsBackend, Some(Key::DiagnoseHintGraphicsNoGraphicsBackend)),
    // ── integrity ──
    row("integrity.file_changed", Key::DiagnoseTitleIntegrityFileChanged, Some(Key::DiagnoseHintIntegrityFileChanged)),
    row("integrity.no_such_method", Key::DiagnoseTitleIntegrityNoSuchMethod, Some(Key::DiagnoseHintIntegrityNoSuchMethod)),
    row("integrity.no_class_def_found", Key::DiagnoseTitleIntegrityNoClassDefFound, Some(Key::DiagnoseHintIntegrityNoClassDefFound)),
    row("integrity.illegal_access", Key::DiagnoseTitleIntegrityIllegalAccess, Some(Key::DiagnoseHintIntegrityIllegalAccess)),
    row("integrity.file_already_exists", Key::DiagnoseTitleIntegrityFileAlreadyExists, Some(Key::DiagnoseHintIntegrityFileAlreadyExists)),
    row("integrity.mod_files_decompressed", Key::DiagnoseTitleIntegrityModFilesDecompressed, Some(Key::DiagnoseHintIntegrityModFilesDecompressed)),
    row("integrity.incomplete_forge", Key::DiagnoseTitleIntegrityIncompleteForge, Some(Key::DiagnoseHintIntegrityIncompleteForge)),
    row("integrity.unsatisfied_link", Key::DiagnoseTitleIntegrityUnsatisfiedLink, Some(Key::DiagnoseHintIntegrityUnsatisfiedLink)),
    // ── jvm ──
    row("jvm.openj9", Key::DiagnoseTitleJvmOpenj9, Some(Key::DiagnoseHintJvmOpenj9)),
    row("jvm.need_jdk11", Key::DiagnoseTitleJvmNeedJdk11, Some(Key::DiagnoseHintJvmNeedJdk11)),
    row("jvm.too_old_java", Key::DiagnoseTitleJvmTooOldJava, Some(Key::DiagnoseHintJvmTooOldJava)),
    row("jvm.32bit", Key::DiagnoseTitleJvm32bit, Some(Key::DiagnoseHintJvm32bit)),
    row("jvm.jdk_9", Key::DiagnoseTitleJvmJdk9, Some(Key::DiagnoseHintJvmJdk9)),
    row("jvm.version_too_high", Key::DiagnoseTitleJvmVersionTooHigh, Some(Key::DiagnoseHintJvmVersionTooHigh)),
    row("jvm.modlauncher_8", Key::DiagnoseTitleJvmModlauncher8, Some(Key::DiagnoseHintJvmModlauncher8)),
    row("jvm.mac_jdk8u261", Key::DiagnoseTitleJvmMacJdk8u261, Some(Key::DiagnoseHintJvmMacJdk8u261)),
    // ── memory ──
    row("memory.out_of_memory", Key::DiagnoseTitleMemoryOutOfMemory, Some(Key::DiagnoseHintMemoryOutOfMemory)),
    row("memory.memory_exceeded", Key::DiagnoseTitleMemoryMemoryExceeded, Some(Key::DiagnoseHintMemoryMemoryExceeded)),
    // ── mod_conflict ──
    row("conflict.optifine_forge", Key::DiagnoseTitleConflictOptifineForge, Some(Key::DiagnoseHintConflictOptifineForge)),
    row("conflict.shaders_mod", Key::DiagnoseTitleConflictShadersMod, Some(Key::DiagnoseHintConflictShadersMod)),
    row("conflict.multipart_optifine", Key::DiagnoseTitleConflictMultipartOptifine, Some(Key::DiagnoseHintConflictMultipartOptifine)),
    row("conflict.performant_optifine", Key::DiagnoseTitleConflictPerformantOptifine, Some(Key::DiagnoseHintConflictPerformantOptifine)),
    row("conflict.twilight_optifine", Key::DiagnoseTitleConflictTwilightOptifine, Some(Key::DiagnoseHintConflictTwilightOptifine)),
    row("conflict.jade_optifine", Key::DiagnoseTitleConflictJadeOptifine, Some(Key::DiagnoseHintConflictJadeOptifine)),
    row("conflict.neoforge_optifine", Key::DiagnoseTitleConflictNeoforgeOptifine, Some(Key::DiagnoseHintConflictNeoforgeOptifine)),
    row("conflict.optifine_world_load", Key::DiagnoseTitleConflictOptifineWorldLoad, Some(Key::DiagnoseHintConflictOptifineWorldLoad)),
    // ── mod_runtime ──
    row("mod.loading_crashed_forge", Key::DiagnoseTitleModLoadingCrashedForge, Some(Key::DiagnoseHintModLoadingCrashedForge)),
    row("mod.loading_crashed_fabric", Key::DiagnoseTitleModLoadingCrashedFabric, Some(Key::DiagnoseHintModLoadingCrashedFabric)),
    row("mod.config_failed", Key::DiagnoseTitleModConfigFailed, Some(Key::DiagnoseHintModConfigFailed)),
    row("mod.mixin_failure", Key::DiagnoseTitleModMixinFailure, Some(Key::DiagnoseHintModMixinFailure)),
    row("mod.mixin_apply_failed", Key::DiagnoseTitleModMixinApplyFailed, Some(Key::DiagnoseHintModMixinApplyFailed)),
    row("mod.forge_error", Key::DiagnoseTitleModForgeError, Some(Key::DiagnoseHintModForgeError)),
    row("mod.night_config_fixes", Key::DiagnoseTitleModNightConfigFixes, Some(Key::DiagnoseHintModNightConfigFixes)),
    row("mod.install_mixinbootstrap", Key::DiagnoseTitleModInstallMixinbootstrap, Some(Key::DiagnoseHintModInstallMixinbootstrap)),
    // ── other ──
    row("other.debug_crash", Key::DiagnoseTitleOtherDebugCrash, Some(Key::DiagnoseHintOtherDebugCrash)),
    // ── world ──
    row("world.entity_tick", Key::DiagnoseTitleWorldEntityTick, Some(Key::DiagnoseHintWorldEntityTick)),
    row("world.block_model", Key::DiagnoseTitleWorldBlockModel, Some(Key::DiagnoseHintWorldBlockModel)),
    // ── 引擎内置结论（M395 BUILTIN10，10 条 / 0 hint）──
    // 顺序 = `core/src/diagnose/synthetic.rs` 的产出顺序（先退出信号 6 条，再 hs_err 4 条）。
    // 参数（`{exit_code}` 之类）由 core 的 `Finding::groups` 给，界面不猜。
    row("signal.access_violation", Key::DiagnoseTitleSignalAccessViolation, None),
    row("signal.dll_not_found", Key::DiagnoseTitleSignalDllNotFound, None),
    row("signal.stack_buffer_overrun", Key::DiagnoseTitleSignalStackBufferOverrun, None),
    row("signal.sigkill", Key::DiagnoseTitleSignalSigkill, None),
    row("signal.jvm_error", Key::DiagnoseTitleSignalJvmError, None),
    row("signal.silent_failure", Key::DiagnoseTitleSignalSilentFailure, None),
    row("hs_err.access_violation", Key::DiagnoseTitleHsErrAccessViolation, None),
    row("hs_err.out_of_memory", Key::DiagnoseTitleHsErrOutOfMemory, None),
    row("hs_err.stack_overflow", Key::DiagnoseTitleHsErrStackOverflow, None),
    row("hs_err.internal_error", Key::DiagnoseTitleHsErrInternalError, None),
];

/// 按规则 id 取文案键（`None` = 目录里没有条目：未来新加的、还没接线的结论）。
///
/// 线性扫 74 条：调用点在渲染路径上（每条结论一帧一次），实测无感——比对的是长度不同的
/// `&str`，前缀就分出胜负；真要更快得做哈希，而这张表的值是「顺序可读」。
#[must_use]
pub fn diagnose_keys(rule_id: &str) -> Option<&'static DiagnoseKeys> {
    DIAGNOSE_KEYS.iter().find(|entry| entry.rule == rule_id)
}

/// 按 `cause.key`（`diagnose.cause.<rule id>`）取文案键。
///
/// 界面拿到的就是这个字段（`presenter::dto::Diagnosis::cause_key`）：它是 core 规则表里的稳定
/// 标识，比「从规则 id 拼出来」更硬——拼字符串的地方一多，总有一处忘了更新前缀。
#[must_use]
pub fn diagnose_keys_from_cause(cause_key: &str) -> Option<&'static DiagnoseKeys> {
    diagnose_keys(cause_key.strip_prefix("diagnose.cause.")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表里没有重复 id，且条数是 74（64 条规则 + 10 条引擎内置结论）。
    #[test]
    fn the_table_has_one_row_per_rule() {
        assert_eq!(DIAGNOSE_KEYS.len(), 74);
        for (index, entry) in DIAGNOSE_KEYS.iter().enumerate() {
            assert!(
                DIAGNOSE_KEYS[..index]
                    .iter()
                    .all(|done| done.rule != entry.rule),
                "重复的规则 id：{}",
                entry.rule
            );
        }
        assert_eq!(
            DIAGNOSE_KEYS.iter().filter(|e| e.hint.is_some()).count(),
            64,
            "带 hint 的规则应是 64 条（21 条规则在 M400 补齐；10 条内置结论仍不编）"
        );
    }

    /// 查找函数真的按 id 命中，且 `cause.key` 的前缀与那两段键同名。
    #[test]
    fn lookup_finds_the_documented_example() {
        let entry = diagnose_keys("integrity.file_changed").expect("完整性那条规则必须在表里");
        assert_eq!(
            entry.title_catalog_key(),
            "diagnose.title.integrity.file_changed"
        );
        assert_eq!(
            entry.hint_catalog_key(),
            "diagnose.hint.integrity.file_changed"
        );
        assert_eq!(entry.title, Key::DiagnoseTitleIntegrityFileChanged);
        assert_eq!(entry.hint, Some(Key::DiagnoseHintIntegrityFileChanged));
        assert!(diagnose_keys("integrity").is_none(), "前缀不是规则 id");
        // 内置结论也在表里（M395）：它们的规则 id 就是 `signal.*` / `hs_err.*`。
        assert_eq!(
            diagnose_keys("signal.silent_failure").map(|keys| keys.title),
            Some(Key::DiagnoseTitleSignalSilentFailure)
        );
        assert!(
            diagnose_keys("signal.exit_code").is_none(),
            "不存在的内置结论 id 不许凭空命中"
        );
    }

    /// 每一条规则的 `cause.key` 都等于 `diagnose.cause.<rule id>`（键名族的第三段同形）。
    #[test]
    fn every_row_mirrors_the_cause_key_family() {
        for entry in DIAGNOSE_KEYS {
            assert_eq!(
                entry.title_catalog_key(),
                format!("diagnose.title.{}", entry.rule)
            );
            if let Some(hint) = entry.hint {
                assert_eq!(
                    hint.name(),
                    format!("DiagnoseHint{}", pascal(entry.rule)),
                    "hint 变体名必须是机械推导的"
                );
            }
            assert_eq!(
                entry.title.name(),
                format!("DiagnoseTitle{}", pascal(entry.rule)),
                "title 变体名必须是机械推导的"
            );
        }
    }

    /// 与 [`super::super::diagnose`] 用的同一条命名规则（`PascalCase`，`.` 与 `_` 都是边界）。
    fn pascal(rule: &str) -> String {
        rule.split(['.', '_'])
            .filter(|part| !part.is_empty())
            .map(|part| part[..1].to_uppercase() + &part[1..])
            .collect()
    }
}
