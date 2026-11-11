//! 路径与版本号的小判据（纯函数）：`.disabled` / `.old` 后缀变体与注记、CF 类别目录、MC 版本前缀。
//!
//! 拆出来是因为它们是**格式事实**而非管线逻辑：`.disabled` 只按路径后缀（tower 定案，不用
//! `env.client=optional`）、`.old` 只识别与保留（不清不删）、CF 的 `classId` 认不出就不猜。

use super::super::report::{keys, ImportIssue};

/// 路径后缀变体（`.disabled` / `.old` 的四种组合）。
///
/// M224 收敛：类型本体只有 [`crate::addon::mods::suffix`] 一处，这里原样再导出，于是
/// `paths::SuffixVariant` 与启停用的是**同一个类型**（零行为变化，调用点不用改）。
pub use crate::addon::mods::SuffixVariant;

/// 识别四种路径后缀变体。
///
/// 委派到唯一判据（见模块文档）；保留函数名与签名。
#[must_use]
pub fn suffix_variant(path: &str) -> SuffixVariant {
    crate::addon::mods::suffix_variant(path)
}

/// 后缀注记（`.disabled` / `.old`）：进报告，告诉用户"这个名字我们是认得的"。
#[must_use]
pub fn suffix_note(variant: SuffixVariant, path: &str) -> Option<ImportIssue> {
    match variant {
        SuffixVariant::Plain => None,
        SuffixVariant::Disabled => Some(
            ImportIssue::new(
                keys::NOTE_DISABLED_SUFFIX,
                "路径以 .disabled 结尾：按禁用态安装，后缀原样保留".to_string(),
            )
            .with_path(path),
        ),
        SuffixVariant::Old => Some(
            ImportIssue::new(
                keys::NOTE_OLD_SUFFIX,
                "路径以 .old 结尾：识别并原样保留（不清除、不改名）".to_string(),
            )
            .with_path(path),
        ),
        SuffixVariant::DisabledOld => Some(
            ImportIssue::new(
                keys::NOTE_DISABLED_SUFFIX,
                "路径以 .disabled.old 结尾：禁用态的旧文件，原样保留".to_string(),
            )
            .with_path(path),
        ),
    }
}

/// CurseForge `classId` → 实例内目录（**认不出的类别返回 `None`**，由调用方告警）。
///
/// 依据：CF 官方 `classId` 枚举（`6` mods / `12` resource packs / `6945` shaders /
/// `17` worlds / `4546` customization / `4471` modpacks）。未知类别**不猜**：
/// 计划层返回 `None`，导入时进 `missing`/告警，而不是"随手塞进 mods/"。
#[must_use]
pub const fn curse_class_dir(class_id: i64) -> Option<&'static str> {
    match class_id {
        6 => Some("mods"),
        12 => Some("resourcepacks"),
        6945 => Some("shaderpacks"),
        17 => Some("saves"),
        4546 => Some("customize"),
        _ => None,
    }
}

/// 剥掉 CF 在版本段里又写一遍的 MC 前缀（`neoforge-1.20.1-47.1.106` 的坑）。
///
/// 只在"版本段以 `<mc>-` 开头"时剥——这是 CF 导出器在 NeoForge/Forge 上会写出的形状
/// （A 级来源 `FlamePackExportTask.cpp:404-410`）。不按 `-` 硬切第三段：`47.1.106` 本身
/// 也有不同风格，切错会把版本号喂成 `1.20.1`。
#[must_use]
pub fn strip_mc_prefix(mc_version: &str, loader_version: &str) -> String {
    let prefix = format!("{mc_version}-");
    loader_version
        .strip_prefix(&prefix)
        .filter(|rest| !rest.is_empty())
        .unwrap_or(loader_version)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_four_suffix_variants() {
        assert_eq!(suffix_variant("mods/x.jar"), SuffixVariant::Plain);
        assert_eq!(
            suffix_variant("mods/x.jar.disabled"),
            SuffixVariant::Disabled
        );
        assert_eq!(suffix_variant("mods/x.jar.old"), SuffixVariant::Old);
        assert_eq!(
            suffix_variant("mods/x.jar.disabled.old"),
            SuffixVariant::DisabledOld
        );
        assert!(suffix_variant("mods/x.jar.disabled").is_disabled());
        assert!(!suffix_variant("mods/x.jar.old").is_disabled());
        assert!(suffix_variant("mods/x.jar.old").is_old());
    }

    #[test]
    fn suffix_notes_only_for_recognised_variants() {
        assert!(suffix_note(SuffixVariant::Plain, "mods/x.jar").is_none());
        let disabled = suffix_note(SuffixVariant::Disabled, "mods/x.jar.disabled").expect("有注记");
        assert_eq!(disabled.message_key, keys::NOTE_DISABLED_SUFFIX);
        assert_eq!(disabled.path.as_deref(), Some("mods/x.jar.disabled"));
        let old = suffix_note(SuffixVariant::Old, "mods/x.jar.old").expect("有注记");
        assert_eq!(old.message_key, keys::NOTE_OLD_SUFFIX);
    }

    #[test]
    fn strips_mc_prefix_from_loader_version() {
        // NeoForge 在 1.20.1 上的真实形状。
        assert_eq!(strip_mc_prefix("1.20.1", "1.20.1-47.1.106"), "47.1.106");
        // 没有前缀的照原样（Fabric / 正常 Forge）。
        assert_eq!(strip_mc_prefix("1.20.1", "0.15.11"), "0.15.11");
        // 只剥一层，且剥完不能是空串。
        assert_eq!(strip_mc_prefix("1.20.1", "1.20.1-"), "1.20.1-");
    }

    #[test]
    fn knows_curse_class_directories() {
        assert_eq!(curse_class_dir(6), Some("mods"));
        assert_eq!(curse_class_dir(12), Some("resourcepacks"));
        assert_eq!(curse_class_dir(6945), Some("shaderpacks"));
        // 认不出的类别不猜。
        assert_eq!(curse_class_dir(999_999), None);
    }
}
